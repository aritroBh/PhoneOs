import Foundation
import Synchronization
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// The answer to one JSON-RPC call: a result object, or an error the runner reported.
enum RPCReply: Equatable, Sendable {
    case result([String: JSONValue])
    case failure(code: Int, message: String)
}

/// JSON-RPC 2.0 over `POST /rpc`, with sealing. One request per call, ids counting up.
final class LoopCtrlRPCClient: Sendable {
    let endpoint: URL
    let timeout: TimeInterval
    let transport: any HTTPTransport
    let sealer: any RequestSealer
    private let nextID = Mutex<Int>(1)

    init(baseURL: URL, timeout: TimeInterval, transport: any HTTPTransport, sealer: any RequestSealer) {
        self.endpoint = baseURL.appendingPathComponent(String(LoopCtrlAPI.rpcPath.dropFirst()))
        self.timeout = timeout
        self.transport = transport
        self.sealer = sealer
    }

    /// Calls `method`; throws for transport, HTTP, envelope and authentication problems.
    func call(_ method: String, _ params: [String: JSONValue], extraTimeout: TimeInterval = 0) async throws -> RPCReply {
        let id = nextID.withLock { id -> Int in
            defer { id += 1 }
            return id
        }
        typealias E = LoopCtrlAPI.Envelope
        let body: Data
        do {
            body = try JSONValue.object([
                E.jsonrpc: .string(E.version),
                E.id: .number(Double(id)),
                E.method: .string(method),
                E.params: .object(params),
            ]).encoded()
        } catch {
            throw LoopCtrlError.malformedResponse(method: method, reason: "request not encodable: \(error)")
        }
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.timeoutInterval = timeout + extraTimeout
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let sealed = try await sealer.seal(body: body, into: request)

        let data: Data
        let response: HTTPURLResponse
        do {
            (data, response) = try await transport.send(sealed)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            throw LoopCtrlError.transport(String(describing: error))
        }
        let plain: Data
        do {
            plain = try await sealer.unseal(body: data, response: response)
        } catch {
            throw LoopCtrlError.malformedResponse(method: method, reason: "could not unseal: \(error)")
        }
        let ok = (200..<300).contains(response.statusCode)
        guard let json = try? JSONValue(parsing: plain), case .object(let envelope) = json else {
            if !ok { throw LoopCtrlError.httpStatus(response.statusCode, body: plain.snippet()) }
            throw LoopCtrlError.malformedResponse(method: method, reason: "not a JSON object: \(plain.snippet(120))")
        }
        if let error = envelope[E.error], error != .null {
            let code = error[E.errorCode]?.numberValue.map(\.clampedInt)
            let message = error[E.errorMessage]?.stringValue ?? "no message"
            guard let code else {
                throw LoopCtrlError.malformedResponse(method: method, reason: "error without a numeric code: \(message)")
            }
            if code == LoopCtrlAPI.ErrorCode.authentication {
                throw LoopCtrlError.authenticationRequired(message)
            }
            return .failure(code: code, message: message)
        }
        guard ok else { throw LoopCtrlError.httpStatus(response.statusCode, body: plain.snippet()) }
        switch envelope[E.result] {
        case .object(let result):
            return .result(result)
        case nil, .null?:
            // `result` is optional in LoopCtrl's response type [source].
            return .result([:])
        case let other?:
            // A non-object result is still a success for commands; snapshot calls report
            // the fields they miss.
            return .result([E.result: other])
        }
    }

    /// Calls `method` and requires a result (a runner-reported error throws `rpcError`).
    func result(_ method: String, _ params: [String: JSONValue] = [:]) async throws -> [String: JSONValue] {
        switch try await call(method, params) {
        case .result(let result):
            return result
        case .failure(let code, let message):
            throw LoopCtrlError.rpcError(method: method, code: code, message: message)
        }
    }
}
