import Foundation
import Network
import JarvisKit

/// The controller's dialing side: TCP to a phone that runs Jarvis, mirroring the phone's
/// listener (`NetworkRemoteTransport`): local network only (cellular is prohibited, no
/// peer-to-peer, no relay), and the peer is judged by `PeerAddressPolicy` once the
/// connection is ready, before any byte reaches the core. Same framing: bytes as they
/// arrive, the core frames them.
struct NetworkRemoteDialer: RemoteDialer {
    /// A connection that is not ready this long after it was started fails.
    static let readyTimeoutMs = 10_000

    func dial(_ address: RemotePhoneAddress) async throws -> any RemoteConnection {
        let endpoint: NWEndpoint
        switch address {
        case .host(let host, let port):
            guard let nwPort = NWEndpoint.Port(rawValue: port) else { throw RemoteDialError.unreachable("not a port") }
            endpoint = .hostPort(host: NWEndpoint.Host(host), port: nwPort)
        case .service(let name):
            endpoint = .service(name: name, type: NetworkRemoteTransport.serviceType, domain: "local.", interface: nil)
        case .inProcess:
            throw RemoteDialError.unreachable("there is no phone in this app")
        }
        let connection = NetworkRemoteConnection(endpoint: endpoint)
        try await connection.open()
        return connection
    }
}

/// One `NWConnection` to a phone. Every state is touched on `queue` only.
final class NetworkRemoteConnection: RemoteConnection, @unchecked Sendable {
    let incoming: AsyncStream<Data>
    private let continuation: AsyncStream<Data>.Continuation
    private let queue = DispatchQueue(label: "dev.jarvis.remote.controller")
    private let connection: NWConnection
    /// Bytes handed to the network and not processed yet (a phone that stops reading).
    private let outbound = ByteBudget(limit: RemoteTransportLimits.maxUnsentOutboundBytes)
    // On `queue`:
    private var opening: CheckedContinuation<Void, any Error>?
    private var ready = false
    private var finished = false

    init(endpoint: NWEndpoint) {
        let parameters = NWParameters.tcp
        parameters.includePeerToPeer = false
        parameters.prohibitedInterfaceTypes = [.cellular]
        connection = NWConnection(to: endpoint, using: parameters)
        // At most 256 chunks of up to 64 KiB wait for the controller: a peer that floods
        // loses bytes, which the core then refuses (the connection fails closed).
        (incoming, continuation) = AsyncStream.makeStream(of: Data.self, bufferingPolicy: .bufferingOldest(256))
    }

    /// Starts the connection; returns once it is ready and judged local, throws otherwise.
    func open() async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
            queue.async { self.start(continuation) }
        }
    }

    private func start(_ continuation: CheckedContinuation<Void, any Error>) {
        opening = continuation
        connection.stateUpdateHandler = { [weak self] state in self?.changed(state) }
        connection.start(queue: queue)
        queue.asyncAfter(deadline: .now() + .milliseconds(NetworkRemoteDialer.readyTimeoutMs)) { [weak self] in
            guard let self, !self.ready else { return }
            self.shutdown(RemoteDialError.unreachable("no answer within \(NetworkRemoteDialer.readyTimeoutMs / 1_000) s"))
        }
    }

    private func changed(_ state: NWConnection.State) {
        switch state {
        case .ready:
            let path = connection.currentPath
            let cellular = path.map { $0.usesInterfaceType(.cellular) }
            // A Bonjour dial's own endpoint is the service name: the address it reached is
            // the path's remote endpoint.
            let remote = connection.currentPath?.remoteEndpoint ?? connection.endpoint
            let address = NetworkRemoteTransport.addressBytes(remote)
            guard !address.isEmpty else {
                shutdown(RemoteDialError.refused("the phone's IP address could not be read from the connection, so it cannot be checked to be on this network"))
                return
            }
            if case .refuse(let why) = PeerAddressPolicy.decide(address: address, pathIsCellular: cellular) {
                shutdown(RemoteDialError.refused(why))
                return
            }
            ready = true
            opening?.resume()
            opening = nil
            receive()
        case .waiting:
            // Not final: iOS reports waiting while it shows the local-network prompt (the
            // first pairing) and while a route comes up. The ready timeout ends it.
            break
        case .failed(let error):
            shutdown(RemoteDialError.unreachable("\(error)"))
        case .cancelled:
            shutdown(RemoteDialError.unreachable("cancelled"))
        default:
            break
        }
    }

    private func receive() {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1_024) { [weak self] data, _, isComplete, error in
            guard let self, !self.finished else { return }
            if let data, !data.isEmpty { self.continuation.yield(data) }
            if isComplete || error != nil {
                self.shutdown(nil)
                return
            }
            self.receive()
        }
    }

    func write(_ bytes: Data) {
        queue.async {
            guard self.ready, !self.finished else { return }
            let count = bytes.count
            guard self.outbound.add(count) else {
                self.shutdown(nil)
                return
            }
            let outbound = self.outbound
            self.connection.send(content: bytes, completion: .contentProcessed { _ in outbound.remove(count) })
        }
    }

    func close() {
        queue.async { self.shutdown(nil) }
    }

    /// Ends the connection once: the stream finishes, a pending `open` throws.
    private func shutdown(_ error: (any Error)?) {
        if let opening {
            self.opening = nil
            opening.resume(throwing: error ?? RemoteDialError.unreachable("closed"))
        }
        guard !finished else { return }
        finished = true
        connection.stateUpdateHandler = nil
        connection.cancel()
        continuation.finish()
    }
}

/// Phones on this Wi-Fi that advertise remote control (`_jarvis-remote._tcp`, the phone's
/// listener). Names only; the connection to one is judged like any other.
@MainActor
final class RemotePhoneBrowser: ObservableObject {
    @Published private(set) var names: [String] = []
    private var browser: NWBrowser?
    private let queue = DispatchQueue(label: "dev.jarvis.remote.browser")

    func start() {
        guard browser == nil else { return }
        let parameters = NWParameters()
        parameters.includePeerToPeer = false
        parameters.prohibitedInterfaceTypes = [.cellular]
        let browser = NWBrowser(for: .bonjour(type: NetworkRemoteTransport.serviceType, domain: nil), using: parameters)
        // Called on `queue`, not on the main actor: the closure is `@Sendable` and hops to
        // the main actor explicitly.
        browser.browseResultsChangedHandler = { @Sendable [weak self] results, _ in
            let found = results.compactMap { result -> String? in
                if case .service(let name, _, _, _) = result.endpoint { return name }
                return nil
            }
            let sorted = Array(Set(found)).sorted()
            Task { @MainActor [weak self] in self?.names = sorted }
        }
        browser.start(queue: queue)
        self.browser = browser
    }

    func stop() {
        browser?.cancel()
        browser = nil
        names = []
    }
}
