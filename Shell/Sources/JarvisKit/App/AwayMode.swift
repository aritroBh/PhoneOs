import Foundation

/// The screen side of Away mode: brightness, the idle timer, proximity blanking. On iOS
/// this is `UIScreen.main.brightness`, `UIApplication.shared.isIdleTimerDisabled` and
/// `UIDevice.current.isProximityMonitoringEnabled`; tests use a fake.
@MainActor
public protocol AwayDisplay: AnyObject {
    /// 0…1.
    var brightness: Double { get set }
    var isIdleTimerDisabled: Bool { get set }
    var isProximityMonitoringEnabled: Bool { get set }
}

/// Away mode (PLAN §3.1): the phone stays unlocked and awake but looks off — a black
/// screen at brightness 0 with the idle timer off, optionally blanked by the proximity
/// sensor when covered.
///
/// Rules:
/// - First use needs the owner's acknowledgement of the costs (`phase == .explaining`
///   until `acknowledge()`); it is remembered (`acknowledged`, persisted by the app).
/// - Entering saves the brightness and the idle-timer setting, then dims to 0.
/// - The saved values are put back on `exit()`, when the app leaves the foreground and
///   when it terminates; exiting twice, or leaving the foreground twice, never writes 0
///   over the owner's brightness.
/// - Back in the foreground while still in Away mode, the current brightness is saved
///   again and the screen dims again.
/// - Proximity blanking is off unless the owner turned it on; it is only on while dimmed.
@MainActor
public final class AwayModeController {
    public enum Phase: Sendable, Equatable {
        /// Not in Away mode.
        case off
        /// Showing the first-use explanation, waiting for the acknowledgement.
        case explaining
        /// The black screen is up.
        case on
    }

    public private(set) var phase: Phase = .off
    /// The owner has read and accepted the costs.
    public private(set) var acknowledged: Bool
    /// Use the proximity sensor while dimmed (off by default).
    public private(set) var proximity: Bool
    /// Whether the screen is dimmed right now (Away mode on and the app in front).
    public private(set) var dimmed = false
    /// The brightness to put back, while dimmed.
    public private(set) var savedBrightness: Double?

    private let display: any AwayDisplay
    private var savedIdleTimerDisabled: Bool?
    private var inForeground = true

    public init(display: any AwayDisplay, acknowledged: Bool, proximity: Bool = false) {
        self.display = display
        self.acknowledged = acknowledged
        self.proximity = proximity
    }

    /// The owner asked for Away mode: the explanation first time, else straight in.
    @discardableResult
    public func request() -> Phase {
        guard phase == .off else { return phase }
        if acknowledged {
            enter()
        } else {
            phase = .explaining
        }
        return phase
    }

    /// The owner accepted the costs: remembered, and Away mode starts.
    public func acknowledge() {
        acknowledged = true
        if phase == .explaining { enter() }
    }

    /// The owner closed the explanation without accepting.
    public func dismissExplanation() {
        if phase == .explaining { phase = .off }
    }

    /// Leaves Away mode and puts the screen back. Safe to call more than once.
    public func exit() {
        restore()
        phase = .off
    }

    /// Changes the proximity preference; applied at once while dimmed.
    public func setProximity(_ on: Bool) {
        proximity = on
        if dimmed { display.isProximityMonitoringEnabled = on }
    }

    /// The owner's saved choices, read again (the rules file became readable after the first
    /// unlock). The phase does not change: nothing starts or ends here.
    public func applySaved(acknowledged: Bool, proximity: Bool) {
        self.acknowledged = acknowledged
        setProximity(proximity)
    }

    /// The app left the foreground (resign active / background): the screen is put back,
    /// Away mode stays chosen.
    public func appLeftForeground() {
        inForeground = false
        restore()
    }

    /// The app is in front again: dims again when Away mode is still on.
    public func appReturnedToForeground() {
        inForeground = true
        if phase == .on && !dimmed { dim() }
    }

    /// The app is about to terminate: the screen is put back.
    public func appWillTerminate() {
        restore()
        phase = .off
    }

    // MARK: - Internals

    private func enter() {
        phase = .on
        if inForeground { dim() }
    }

    private func dim() {
        guard !dimmed else { return }
        savedBrightness = min(max(display.brightness, 0), 1)
        savedIdleTimerDisabled = display.isIdleTimerDisabled
        display.isIdleTimerDisabled = true
        display.brightness = 0
        display.isProximityMonitoringEnabled = proximity
        dimmed = true
    }

    private func restore() {
        guard dimmed else { return }
        if let savedBrightness { display.brightness = savedBrightness }
        display.isIdleTimerDisabled = savedIdleTimerDisabled ?? false
        display.isProximityMonitoringEnabled = false
        savedBrightness = nil
        savedIdleTimerDisabled = nil
        dimmed = false
    }
}

/// What Away mode costs, in the owner's words (PLAN §3.1 "Costs"). Shown before first use.
public enum AwayModeCopy {
    public static let title = "Away mode"
    public static let summary = "The screen goes black and dim, and the phone stays unlocked and awake so Jarvis can keep working."
    public static let costs: [(title: String, detail: String)] = [
        ("The phone stays unlocked", "Anyone who picks it up has everything on it. A swipe home reaches an unlocked phone, and iOS has no way to stop that while Jarvis works."),
        ("Home or desk only", "Not a pocket, bag or public place: with the screen dim but unlocked, touches still register."),
        ("Keep it on a charger", "The screen stays awake and Jarvis keeps working, so the phone uses battery and can run warm."),
        ("Auto-Lock", "If you set Auto-Lock to Never for this, it applies to all use of the phone until you switch it back."),
    ]
    public static let acknowledgement = "I understand: the phone stays unlocked"
    public static let proximityTitle = "Turn the screen off when covered"
    public static let proximityDetail = "Uses the proximity sensor: face down or covered, the display goes off and ignores touches. The phone is not locked, and this only works while Jarvis is in front."
    public static let exitHint = "Touch and hold for 2 seconds to leave"
}
