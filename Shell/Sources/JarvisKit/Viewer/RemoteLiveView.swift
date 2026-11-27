import Foundation
import JarvisCore
import Synchronization

/// A paired controller's Live view of the phone: the stream from `JarvisRemoteClient`
/// (`receiveAll` → `RemoteViewerEvent`s) applied to a `LiveViewModel` of its own (a second
/// `JarvisViewer`, fed with `applyDelta`), so the controller draws exactly what the phone's
/// Run screen draws: wireframes, the cursor, captions; never pixels.
///
/// - **Start / stop**: `start(_:)` and `stop(_:)` return the bytes to write to the phone.
///   The phone answers with a snapshot (`watching`) or refuses (`denied`: "Watch the screen
///   layout" is off for this device on the phone).
/// - **Automatic resync**: the client asks for a snapshot by itself on a gap it sees; the
///   view's own check (`applyDelta` → `needsResync`) asks through
///   `JarvisRemoteClient.requestResync()`, at most once per gap. The bytes to write come
///   back from `receive(_:client:)`.
/// - **Bounded**: the core bounds the timeline (200 steps, only referenced screens, every
///   delta ≤ 48,000 bytes); the model caches at most `LiveViewModel.maxCachedScreens`
///   wireframes; this type keeps counters only.
/// - **Ended**: the phone stopped the stream (`ended(reason)`): the owner turned the
///   permission off, or the controller asked. The last picture stays until the next start.
///
/// What a snapshot does not carry is documented in the core: the screens of older steps
/// (those steps draw as "not available") and, for a very large wireframe, its least
/// relevant elements (flagged truncated).
public final class RemoteLiveView: Sendable {
    public enum State: Sendable, Equatable {
        /// Not asked yet (or stopped by this side).
        case idle
        /// Asked; waiting for the phone's snapshot.
        case starting
        case watching
        /// The phone refused: the device may not watch the screen layout.
        case denied
        /// The phone ended the stream.
        case ended(EndReason)
        /// The phone streams a newer format (its version): update this app.
        case incompatible(UInt32)
    }

    public enum EndReason: Sendable, Equatable {
        case stopped
        case permissionRevoked
        case other

        init(_ r: RemoteViewerEndReason) {
            switch r {
            case .stopped: self = .stopped
            case .permissionRevoked: self = .permissionRevoked
            case .other: self = .other
            }
        }

        /// Owner-facing words.
        public var text: String {
            switch self {
            case .stopped: "Stopped"
            case .permissionRevoked: "The phone's owner turned off \u{201C}Watch the screen layout\u{201D}"
            case .other: "The phone ended it"
            }
        }
    }

    /// What the stream has done so far (numbers only).
    public struct Counters: Sendable, Equatable {
        public var deltas = 0
        public var snapshotParts = 0
        /// Snapshots asked for after a gap (by the client or by the view).
        public var resyncs = 0
        /// Delta bytes received (decoded payloads).
        public var bytes = 0
    }

    public let model: LiveViewModel

    private struct Inner {
        var state: State = .idle
        var counters = Counters()
        /// Test hook: deltas still to leave unapplied (a lossy path).
        var dropDeltas = 0
    }

    private let inner = Mutex(Inner())

    /// A controller view backed by its own `JarvisViewer` (no real-screen frames).
    public convenience init() {
        self.init(model: LiveViewModel(framesEnabled: false))
    }

    public init(model: LiveViewModel) {
        self.model = model
    }

    public var state: State { inner.withLock { $0.state } }
    public var counters: Counters { inner.withLock { $0.counters } }

    /// The bytes asking the phone for the Live view (write them). Throws before the client
    /// is connected.
    public func start(_ client: any JarvisRemoteClientProtocol) throws -> Data {
        let bytes = try client.startWatching()
        inner.withLock { $0.state = .starting }
        return bytes
    }

    /// The bytes stopping the Live view (write them). The picture stays as it was.
    public func stop(_ client: any JarvisRemoteClientProtocol) throws -> Data {
        let bytes = try client.stopWatching()
        inner.withLock { $0.state = .idle }
        return bytes
    }

    /// Applies one `receiveAll`'s viewer events, in order. Returns bytes to write to the
    /// phone (a snapshot request after a gap the view found).
    public func receive(_ events: [RemoteViewerEvent], client: any JarvisRemoteClientProtocol) -> [Data] {
        var out: [Data] = []
        for event in events {
            switch event {
            case .delta(let bytes):
                let drop = inner.withLock { s -> Bool in
                    s.counters.deltas += 1
                    s.counters.bytes += bytes.count
                    guard s.dropDeltas > 0 else { return false }
                    s.dropDeltas -= 1
                    return true
                }
                if !drop { out += apply(bytes, client: client) }
            case .snapshot(let bytes, _, _):
                inner.withLock { s in
                    s.counters.snapshotParts += 1
                    s.counters.bytes += bytes.count
                    if s.state == .starting { s.state = .watching }
                }
                out += apply(bytes, client: client)
            case .resyncRequested:
                inner.withLock { $0.counters.resyncs += 1 }
            case .ended(let reason):
                inner.withLock { $0.state = .ended(EndReason(reason)) }
            case .denied:
                inner.withLock { $0.state = .denied }
            case .incompatible(let version):
                inner.withLock { $0.state = .incompatible(version) }
            }
        }
        return out
    }

    /// Test hook: leaves the next `count` deltas unapplied (as if lost on the way).
    public func dropNextDeltas(_ count: Int) {
        inner.withLock { $0.dropDeltas = max(0, count) }
    }

    private func apply(_ bytes: Data, client: any JarvisRemoteClientProtocol) -> [Data] {
        switch model.applyDelta(bytes) {
        case .needsResync?:
            guard let request = (try? client.requestResync()) ?? nil else { return [] }
            inner.withLock { $0.counters.resyncs += 1 }
            return [request]
        case .incompatible(let version)?:
            inner.withLock { $0.state = .incompatible(version) }
            return []
        default:
            return []
        }
    }
}
