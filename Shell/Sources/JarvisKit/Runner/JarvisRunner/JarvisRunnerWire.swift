import Foundation
import JarvisCore

/// Failures of `JarvisRunnerClient` (thrown; a command the runner refused is a
/// `RunnerResult`, as with every runner). The driver reports a throw as an unreadable
/// screen or a failed command; none of these is ever turned into a screen.
public enum JarvisRunnerError: Error, Equatable, Sendable, CustomStringConvertible {
    /// No runner key is set up on this phone.
    case notProvisioned
    /// Nothing answered, or the connection broke.
    case unreachable(String)
    /// Something answered on the runner's port but could not prove the key: another app
    /// may hold the port. Treated as no runner at all; nothing it said is used.
    case unauthenticated
    /// The runner did not answer within the deadline (the connection is dropped).
    case timedOut(operation: String, after: Duration)
    /// The runner answered that it could not do it (its own call threw).
    case runnerFailed(String)
    /// The runner's answer broke the protocol (the connection is dropped).
    case protocolError(String)

    public var description: String {
        switch self {
        case .notProvisioned:
            "Jarvis's runner has no key on this phone yet (start the Jarvis runner once)"
        case .unreachable(let why):
            "Jarvis's runner is unreachable: \(why)"
        case .unauthenticated:
            "the program on the runner's port could not prove it is Jarvis's runner; it was ignored"
        case .timedOut(let operation, let after):
            "Jarvis's runner did not answer \(operation) within \(after)"
        case .runnerFailed(let why):
            "Jarvis's runner could not do it: \(why)"
        case .protocolError(let why):
            "Jarvis's runner broke the protocol: \(why)"
        }
    }
}

/// Lets one async caller in at a time, in arrival order (an actor alone would interleave
/// callers at every `await`).
actor RunnerAsyncGate {
    private var busy = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func acquire() async {
        if !busy {
            busy = true
            return
        }
        await withCheckedContinuation { waiters.append($0) }
    }

    func release() {
        if waiters.isEmpty {
            busy = false
        } else {
            waiters.removeFirst().resume()
        }
    }
}

// MARK: - Conversions between the shell's runner types and the wire records

extension RunnerWireLockState {
    init(_ state: DeviceLockState) {
        switch state {
        case .locked: self = .locked
        case .unlocked: self = .unlocked
        case .unknown: self = .unknown
        }
    }

    var deviceLockState: DeviceLockState {
        switch self {
        case .locked: .locked
        case .unlocked: .unlocked
        case .unknown: .unknown
        }
    }
}

extension RunnerWireScreenshot {
    init(_ shot: CapturedScreenshot) {
        self.init(
            data: shot.data, mimeType: shot.mimeType, pixelWidth: shot.pixelWidth,
            pixelHeight: shot.pixelHeight, scale: shot.scale
        )
    }

    var captured: CapturedScreenshot {
        CapturedScreenshot(data: data, mimeType: mimeType, pixelWidth: pixelWidth, pixelHeight: pixelHeight, scale: scale)
    }
}

extension RunnerWireSnapshot {
    /// The wire form of `observation`; `nil` for a text-format observation (this protocol
    /// carries typed trees only).
    init?(_ observation: RunnerObservation) {
        guard let root = observation.root else { return nil }
        self.init(
            root: root,
            appBundleId: observation.appBundleId,
            appName: observation.appName,
            screenSize: observation.screenSize,
            keyboardVisible: observation.keyboardVisible,
            orientation: observation.orientation,
            deviceLocked: observation.deviceLocked,
            lockState: RunnerWireLockState(observation.lockState),
            notes: observation.notes,
            screenshot: observation.screenshot.map(RunnerWireScreenshot.init),
            treeCut: observation.treeCut
        )
    }

    var observation: RunnerObservation {
        RunnerObservation(
            root: root,
            appBundleId: appBundleId,
            appName: appName,
            screenSize: screenSize,
            keyboardVisible: keyboardVisible,
            orientation: orientation,
            screenshot: screenshot?.captured,
            deviceLocked: deviceLocked,
            notes: notes,
            lockState: lockState.deviceLockState,
            treeCut: treeCut
        )
    }
}

extension RunnerWireRequest {
    /// A short name for messages and the runner's log: the kind only, never text, a URL or
    /// coordinates.
    var operationName: String {
        switch self {
        case .hello: "hello"
        case .snapshot: "snapshot"
        case .perform(let command, _):
            switch command {
            case .tap: "tap"
            case .longPress: "long_press"
            case .swipe: "swipe"
            case .typeText: "type_text"
            case .pressKey: "press_key"
            case .launchApp: "launch_app"
            case .openUrl: "open_url"
            case .wait: "wait"
            }
        case .lockState: "lock state"
        case .viewerScreenshot: "screenshot"
        case .frontBundleId: "front app"
        }
    }
}

/// Fixed facts of Jarvis's own runner.
public enum JarvisRunnerDefaults {
    /// The loopback port the runner listens on (choice and collisions: spike R4 in
    /// `docs/JARVIS-RUNNER.md`).
    public static let port: UInt16 = 47_831
    /// What the runner calls itself in its hello.
    public static let runnerName = "JarvisRunner"
}
