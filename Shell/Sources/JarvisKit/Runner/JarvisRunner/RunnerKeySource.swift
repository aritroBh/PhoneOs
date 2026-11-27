import Foundation

/// Where the 32-byte key shared by the Jarvis app and its runner comes from. The key is a
/// secret: never logged, never in a URL, a file, `UserDefaults` or an error message.
public protocol RunnerKeySource: Sendable {
    /// The key, or `nil` when none is set up yet (the Jarvis runner is then unusable and
    /// the app says so; it never falls back to an unauthenticated runner).
    func runnerKey() throws -> Redacted<Data>?
}

/// A fixed key (tests, and the simulator UI test that runs both ends in one process).
public struct FixedRunnerKeySource: RunnerKeySource {
    private let key: Redacted<Data>

    public init(_ key: Data) {
        self.key = Redacted(key)
    }

    public func runnerKey() throws -> Redacted<Data>? { key }
}

/// The key size of the runner protocol.
public enum RunnerKey {
    public static let length = 32

    /// A fresh key from `random`.
    public static func generate(_ random: any RandomSource) throws -> Redacted<Data> {
        let key = try random.bytes(count: length)
        guard key.count == length else { throw RunnerKeyError.wrongLength }
        return Redacted(key)
    }
}

public enum RunnerKeyError: Error, Equatable, CustomStringConvertible {
    /// A provisioned key is not 32 bytes of base64 (its value is never repeated here).
    case wrongLength

    public var description: String {
        switch self {
        case .wrongLength: "the runner key is not 32 bytes"
        }
    }
}

/// The first provisioning candidate (unverified on a device; `docs/JARVIS-RUNNER.md`, spike
/// R2): the runner process makes a fresh key each time it starts and launches the Jarvis
/// app with it in the launch environment (`JARVIS_RUNNER_KEY`, base64). XCUITest can set a
/// launched app's environment; another app cannot. The app adopts the key at launch into
/// its secret store (the Keychain, this device only) and reads it from there afterwards,
/// so a later launch by the owner (no environment) still finds it while that runner lives.
///
/// Why not a shared Keychain group or App Group: whether a free Apple ID can give the app
/// and the UI-test runner bundle one is a device fact not checked yet; this needs neither.
public struct ProvisionedRunnerKeySource: RunnerKeySource {
    /// The launch-environment variable the runner sets.
    public static let environmentVariable = "JARVIS_RUNNER_KEY"
    /// The secret-store entry.
    public static let secretKey = "jarvis-runner-key"

    private let store: any SecretStore

    public init(store: any SecretStore) {
        self.store = store
    }

    /// Adopts a key from the launch environment when there is one: stores it, replacing
    /// the previous one. Returns whether a key was adopted. A malformed value is refused
    /// (the stored key stays) and reported without its content.
    @discardableResult
    public func adopt(environment: [String: String]) throws -> Bool {
        guard let encoded = environment[Self.environmentVariable] else { return false }
        guard let key = Data(base64Encoded: encoded), key.count == RunnerKey.length else {
            throw RunnerKeyError.wrongLength
        }
        try store.write(Self.secretKey, key.base64EncodedString())
        return true
    }

    public func runnerKey() throws -> Redacted<Data>? {
        guard let stored = try store.read(Self.secretKey) else { return nil }
        guard let key = Data(base64Encoded: stored), key.count == RunnerKey.length else {
            throw RunnerKeyError.wrongLength
        }
        return Redacted(key)
    }

    /// The launch environment entry for `key` (the runner side).
    public static func environment(for key: Redacted<Data>) -> [String: String] {
        [environmentVariable: key.reveal().base64EncodedString()]
    }
}
