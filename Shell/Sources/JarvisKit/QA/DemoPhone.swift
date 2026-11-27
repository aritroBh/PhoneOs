import Foundation
import JarvisCore
import Synchronization

/// A small simulated phone that returns typed trees (`ObservedUI.tree`): a "Demo Settings"
/// app with a Wi-Fi row, a Wi-Fi switch and a "Delete account" button. It backs the app's
/// demo mode (no runner, no model server) and the Linux tests of the typed observation path.
///
/// Taps hit by frame; nothing else changes state.
public final class DemoPhone: PhoneRunner {
    /// The pages.
    public enum Page: Sendable, Equatable {
        case home
        case wifi(on: Bool)
        case deleted
    }

    public static let bundleId = "dev.jarvis.demo.settings"
    public static let appName = "Demo Settings"
    public static let screenSize = Size(width: 393, height: 852)

    private struct State {
        var page = Page.home
        var commands: [RunnerCommand] = []
        var snapshots = 0
        /// Lock-state polls left before the simulated phone unlocks (0: unlocked).
        var lockedPolls = 0
    }

    private let state = Mutex(State())
    private let delay: Duration

    /// `delay` is spent in every call, so the demo is watchable. With `lockedPolls > 0` the
    /// phone starts locked and unlocks itself after that many lock-state polls (the app's
    /// "Unlock to continue" demo and its UI test).
    public init(delay: Duration = .zero, lockedPolls: Int = 0) {
        self.delay = delay
        state.withLock { $0.lockedPolls = max(0, lockedPolls) }
    }

    public var isLocked: Bool { state.withLock { $0.lockedPolls > 0 } }

    public func lockState() async throws -> DeviceLockState {
        state.withLock { s in
            guard s.lockedPolls > 0 else { return .unlocked }
            s.lockedPolls -= 1
            return s.lockedPolls > 0 ? .locked : .unlocked
        }
    }

    public var page: Page { state.withLock { $0.page } }
    public var commands: [RunnerCommand] { state.withLock { $0.commands } }
    public var snapshotCount: Int { state.withLock { $0.snapshots } }

    public func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        if delay > .zero { try await Task.sleep(for: delay) }
        let (page, locked) = state.withLock { s -> (Page, Bool) in
            s.snapshots += 1
            return (s.page, s.lockedPolls > 0)
        }
        if locked {
            return RunnerObservation(
                root: Self.node(.application, "SpringBoard", Rect(x: 0, y: 0, width: Self.screenSize.width, height: Self.screenSize.height)),
                appBundleId: "com.apple.springboard", appName: nil, screenSize: Self.screenSize, deviceLocked: true
            )
        }
        return RunnerObservation(
            root: Self.tree(for: page),
            appBundleId: Self.bundleId,
            appName: Self.appName,
            screenSize: Self.screenSize,
            lockState: .unlocked
        )
    }

    public func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        if delay > .zero { try await Task.sleep(for: delay) }
        return state.withLock { s in
            if s.lockedPolls > 0 { return .deviceLocked() }
            s.commands.append(command)
            guard case .tap(let x, let y) = command else {
                if case .wait = command { return RunnerResult(ok: true, error: nil) }
                return RunnerResult(ok: false, error: "the demo phone only supports taps and waits")
            }
            switch s.page {
            case .home where Self.wifiRow.contains(x, y):
                s.page = .wifi(on: true)
            case .home where Self.deleteButton.contains(x, y):
                s.page = .deleted
            case .wifi(let on) where Self.wifiSwitch.contains(x, y):
                s.page = .wifi(on: !on)
            case .wifi where Self.backButton.contains(x, y):
                s.page = .home
            default:
                break
            }
            return RunnerResult(ok: true, error: nil)
        }
    }

    // MARK: - Screens

    static let wifiRow = Rect(x: 16, y: 200, width: 361, height: 44)
    static let deleteButton = Rect(x: 16, y: 400, width: 361, height: 44)
    static let wifiSwitch = Rect(x: 312, y: 166, width: 51, height: 31)
    static let backButton = Rect(x: 8, y: 70, width: 90, height: 44)

    static func node(_ role: Role, _ label: String? = nil, value: String? = nil, _ frame: Rect, _ children: [UiNode] = []) -> UiNode {
        UiNode(role: role, label: label, value: value, identifier: nil, placeholder: nil, frame: frame,
               enabled: true, visible: true, hittable: true, focused: false, selected: false, secure: false,
               children: children)
    }

    /// The typed tree of `page`.
    public static func tree(for page: Page) -> UiNode {
        let full = Rect(x: 0, y: 0, width: screenSize.width, height: screenSize.height)
        let content: [UiNode]
        switch page {
        case .home:
            content = [
                node(.navigationBar, "Demo Settings", Rect(x: 0, y: 59, width: 393, height: 81)),
                node(.table, nil, Rect(x: 0, y: 140, width: 393, height: 712), [
                    node(.cell, nil, wifiRow, [node(.staticText, "Wi-Fi", Rect(x: 60, y: 212, width: 200, height: 20))]),
                    node(.button, "Delete account", deleteButton),
                ]),
            ]
        case .wifi(let on):
            content = [
                node(.navigationBar, "Wi-Fi", Rect(x: 0, y: 59, width: 393, height: 81), [
                    node(.button, "Demo Settings", backButton),
                ]),
                node(.table, nil, Rect(x: 0, y: 140, width: 393, height: 712), [
                    node(.cell, nil, Rect(x: 16, y: 160, width: 361, height: 44), [
                        node(.switch, "Wi-Fi", value: on ? "1" : "0", wifiSwitch),
                    ]),
                ]),
            ]
        case .deleted:
            content = [node(.staticText, "Account deleted", Rect(x: 16, y: 200, width: 361, height: 44))]
        }
        return node(.application, appName, full, [node(.window, nil, full, content)])
    }

    /// The script the demo model runs for "Turn off Wi-Fi" (tap the row, tap the switch
    /// while it is on, done when it shows off).
    public static let wifiOffScript: [ScriptStep] = [
        .tap("Wi-Fi", role: "Cell"),
        .tap(ElementQuery("Wi-Fi", match: .exact, roles: ["Switch"], isOn: true)),
        .done(summary: "Wi-Fi is off", expecting: [ElementQuery("Wi-Fi", match: .exact, roles: ["Switch"], isOn: false)]),
    ]

    /// The script for "Delete my account" (the tap is a `delete`-category confirmation).
    public static let deleteAccountScript: [ScriptStep] = [
        .tap("Delete account", role: "Button"),
        .done("The account is deleted", when: ["Account deleted"]),
    ]

    /// Known apps for the demo.
    public static let knownApps = [AppInfo(bundleId: bundleId, name: appName)]
}

private extension Rect {
    func contains(_ px: Double, _ py: Double) -> Bool {
        px >= x && px < x + width && py >= y && py < y + height
    }
}
