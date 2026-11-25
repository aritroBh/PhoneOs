import Foundation
import Synchronization
import UIKit
import XCTest
import JarvisCore
import JarvisKit

/// One observation as the runner saw it, for QA artifacts.
struct RunnerObservationRecord {
    /// 1-based observation number in this runner's life.
    var number: Int
    var bundleId: String
    var raw: RawElement
    var built: UiTreeBuilder.Result
    var screenshot: XCUIScreenshot?
    var notes: [String]
    /// Milliseconds the XCUITest snapshot took.
    var snapshotMs: Int
}

/// `PhoneRunner` over public XCUITest API, running inside a UI-test process: the
/// simulator QA harness (JarvisUITests), and the JarvisRunner bundle that serves it to the
/// app over the authenticated loopback protocol (docs/JARVIS-RUNNER.md).
///
/// ## Threading
/// XCUITest calls are synchronous and must run on the main thread: they block while
/// XCTest talks to the accessibility server and waits for the app to go idle. The class is
/// `@MainActor`; the two `PhoneRunner` requirements are `nonisolated async` and hop to the
/// main actor for the XCUITest part only. The `SessionDriver` actor awaits them, so its
/// executor is never blocked. This works because the test method that awaits
/// `SessionDriver.run()` is itself `@MainActor async`: while it is suspended, the main
/// actor is free to run these hops. Pauses (settle time, `wait`) are `Task.sleep` outside
/// the main actor, never a blocking sleep on it.
@MainActor
final class XCUITestPhoneRunner: PhoneRunner {
    struct Configuration: Sendable {
        /// Apps checked (in order) when the last activated app is no longer in front and the
        /// private path (`FrontProcess`) is off or does not tell: the public reading of
        /// before, kept so that a broken private path degrades to it (any other app in front
        /// is then the unidentified app, `FrontApp`).
        var knownBundleIds: [String]
        /// Ask the test framework's accessibility client which app is in front
        /// (`FrontProcess`, private API). Off: the public readings and the fail-closed rule
        /// only (the tests of part 1).
        var identifyFrontApp: Bool
        /// Pause after a command before the next snapshot (animations, keyboard).
        var settleAfterAction: Duration
        /// Longest wait for the software keyboard before typing.
        var keyboardWait: TimeInterval
        /// Longest wait for a launched app to reach the foreground.
        var launchWait: TimeInterval
        /// Put both raw lock readings in every observation's notes and in
        /// `lockSignalHistory` (the lock tests), not only when they change.
        var noteLockSignalsEveryObservation: Bool
        /// How long a gesture asked for while a notification banner is up waits for it to go
        /// before the stale answer (`BannerGate`; never more than `BannerGate.maxWait`). No
        /// event is sent either way; zero answers at once (the banner test checks the banner
        /// is still up after the refusal).
        var bannerWait: Duration

        init(
            knownBundleIds: [String] = XCUITestPhoneRunner.defaultKnownApps,
            identifyFrontApp: Bool = true,
            settleAfterAction: Duration = .milliseconds(700),
            keyboardWait: TimeInterval = 3,
            launchWait: TimeInterval = 15,
            noteLockSignalsEveryObservation: Bool = false,
            bannerWait: Duration = BannerGate.maxWait
        ) {
            self.knownBundleIds = knownBundleIds
            self.identifyFrontApp = identifyFrontApp
            self.settleAfterAction = settleAfterAction
            self.keyboardWait = keyboardWait
            self.launchWait = launchWait
            self.noteLockSignalsEveryObservation = noteLockSignalsEveryObservation
            self.bannerWait = min(max(bannerWait, .zero), BannerGate.maxWait)
        }
    }

    static let springboard = "com.apple.springboard"
    static let spotlight = TopLayer.spotlight
    nonisolated static let defaultKnownApps = [
        "dev.jarvis.app", "com.apple.Preferences", "com.apple.mobilesafari", "com.apple.MobileSMS",
        "com.apple.mobilenotes", "com.apple.mobilecal", "com.apple.reminders", "com.apple.mobileslideshow",
        "com.apple.Maps", "com.apple.DocumentsApp", "com.apple.MobileAddressBook", "com.apple.Health",
    ]

    nonisolated let configuration: Configuration
    /// Observations taken so far (readable from any thread: the QA harness ties prompts to
    /// the observation they were built from).
    nonisolated let observationCounter = Mutex(0)
    /// Called on the main actor after every observation.
    var onObservation: ((RunnerObservationRecord) -> Void)?
    /// Called on the main actor after every command with its result.
    var onCommand: ((RunnerCommand, RunnerResult) -> Void)?

    private var lastActivated: String?
    private var applications: [String: XCUIApplication] = [:]
    private var settlePending = false
    private var focusNoteGiven = false
    private var notifyToken: Int32?
    private var lastLockEvidence: String?
    private var pendingLockNotes: [String] = []
    /// Notes about the last commands for the next observation (a held gesture, `BannerGate`).
    private var pendingCommandNotes: [String] = []
    /// When the current command's XCUITest event call began (after the keyboard wait for
    /// typing), for the held-gesture check (`BannerGate.heldNote`).
    private var eventStarted = ContinuousClock.now
    private let started = Date()
    /// The one judge for commands and for resume (JarvisKit, Linux-tested): staleness of the
    /// notify reading is decided by time, here, nowhere else.
    private let lockJudge = LockJudge()
    /// The banners (screen points) the last observation added to the front app's tree as
    /// covering nodes (`BannerCover`): what the next touch was planned with.
    private var reportedBanners: [Rect] = []
    /// Lock readings with the time since the runner was made: every change, and every
    /// reading with `noteLockSignalsEveryObservation` (for failure messages).
    private(set) var lockSignalHistory: [String] = []
    /// How the last foreground reading was made: the last launched app still in front, the
    /// private path (`FrontProcess.lastPath`), the known apps, or SpringBoard (for the notes
    /// and the QA records).
    private(set) var frontAppPath = "not read yet"
    /// The `frontAppPath` last put in an observation's notes (each change is noted once).
    private var notedFrontAppPath: String?
    /// The screen's size from the last tree read (for an observation that reads no tree).
    private var lastScreenSize: Size?

    init(configuration: Configuration = Configuration()) {
        self.configuration = configuration
    }

    /// Tells the runner which app the test launched itself.
    func noteActivated(_ bundleId: String) {
        lastActivated = bundleId
    }

    // MARK: - PhoneRunner

    nonisolated func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        if await takeSettle() {
            try await Task.sleep(for: configuration.settleAfterAction)
        }
        let first = try await observe(includeScreenshot: includeScreenshot)
        // An app Jarvis cannot identify, read right after a command, may be a transition (an
        // app launching or closing while SpringBoard animates): look once more after a short
        // pause (off the main actor) before reporting it. The second look decides.
        guard FrontApp.isUnidentified(first.appBundleId) else { return first }
        try await Task.sleep(for: .milliseconds(700))
        return try await observe(includeScreenshot: includeScreenshot)
    }

    nonisolated func lockState() async throws -> DeviceLockState {
        await lockStateOnMain()
    }

    /// Stale-screen guard (`FrontAppGuard`): the top layer, by the same decision the
    /// observations and the check next to a gesture use (`TopLayer`: a SpringBoard alert,
    /// else Spotlight in front, else the last activated app while it is in front, else the
    /// app the private path names (`FrontProcess`), else a known app in front, else
    /// SpringBoard while it shows its Home Screen or the lock screen, else
    /// `FrontApp.unidentified`). No snapshot is taken (queries only).
    nonisolated func frontBundleId() async throws -> String? {
        await frontOnMain()
    }

    /// The runner checks a command's `expect` itself, in the same main-actor step as the
    /// gesture, and can always read the app in front (`FrontAppGuard`).
    nonisolated var verifiesFrontApp: Bool { true }

    nonisolated func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        try await perform(command, expect: nil)
    }

    nonisolated func perform(_ command: RunnerCommand, expect: CommandExpect?) async throws -> RunnerResult {
        if case .wait(let ms) = command {
            try await Task.sleep(for: .milliseconds(Int(min(ms, 30_000)))) // capped: no UI-test wait over 30 s
            return RunnerResult(ok: true, error: nil)
        }
        if case .typeText = command {
            // Give a keyboard that is still animating in time to appear (off the main actor).
            if !(await keyboardIsUp()) { try await Task.sleep(for: .milliseconds(500)) }
        }
        if await commandLockState() == .locked {
            await noteRefusal(command)
            return .deviceLocked("the device is locked; \(Transcript.describe(command)) was not performed")
        }
        // No event while a notification banner is up (`BannerGate`, run 138: XCUITest holds
        // the event until the banner goes, up to 15 s, then sends it onto whatever is on the
        // screen): wait for the banner to go (bounded, off the main actor), then answer stale
        // either way; the caller looks again.
        if BannerGate.synthesizesEvent(command), await bannerIsUp() {
            let clock = ContinuousClock()
            let start = clock.now
            var left = false
            while clock.now - start < configuration.bannerWait {
                try await Task.sleep(for: BannerGate.pollInterval)
                if !(await bannerIsUp()) {
                    left = true
                    break
                }
            }
            let refusal = BannerGate.refusal(waited: clock.now - start, bannerLeft: left)
            await noteCommand(command, refusal)
            return refusal
        }
        return await act(command, expect: expect)
    }

    /// Whether SpringBoard shows a notification banner now (`.exists` on the banner's
    /// identifier query on SpringBoard; no snapshot).
    private func bannerIsUp() -> Bool {
        application(Self.springboard).descendants(matching: .any)
            .matching(identifier: BannerCover.identifier).firstMatch.exists
    }

    private func noteCommand(_ command: RunnerCommand, _ result: RunnerResult) {
        onCommand?(command, result)
    }

    /// After a command: the notify state alone (cheap, and it flips about 0.3 s before the
    /// front app does). A command that "succeeded" while the phone locked under it is
    /// reported as locked (`RunnerResult.checkedAfterCommand`), so the core re-observes
    /// after the unlock instead of trusting it.
    private func lockedAfterCommand() -> DeviceLockState {
        // Same judge as everywhere else. While the notify state says unlocked (or cannot be
        // read) no lock screen reading is taken, which is what keeps this cheap; once it says
        // locked, the full reading (with an app in front, SpringBoard's tree for the lock
        // screen: an unchecked reading never lets the notify state go stale, so without it a
        // stale notify state would report every command as locked).
        let notify = notifyLockState()
        if notify == true { return judgedLockState() }
        let signals = LockSignals(notify: notify, lockScreen: nil, front: .app(lastActivated ?? Self.springboard))
        let verdict = lockJudge.judge(signals, at: lockClockNow())
        record(signals, verdict, everyTime: false)
        return verdict.state
    }

    // MARK: - Observation (main actor)

    private func takeSettle() -> Bool {
        defer { settlePending = false }
        return settlePending
    }

    private func observe(includeScreenshot: Bool) throws -> RunnerObservation {
        var notes: [String] = []
        let foreground = foregroundBundleId(notes: &notes)
        noteFrontAppPath(&notes)
        let alertUp = systemAlertUp()
        let spotlightUp = !alertUp && spotlightInFront()
        // SpringBoard's tree (the OS's process, not a look at any app), taken at most once per
        // observation: the observation itself when SpringBoard is the top layer, the lock
        // screen check, and whether it shows its Home Screen (`FrontApp`, part 1).
        var board: RawElement?
        var boardMs = 0
        func springBoardTree() throws -> RawElement {
            if let board { return board }
            let start = Date()
            let copy = SnapshotCopy.copy(try application(Self.springboard).snapshot())
            boardMs = (Date().timeIntervalSince(start) * 1_000).clampedInt
            board = copy
            return copy
        }

        // 1. The lock readings keep their meaning: they are of the foreground reading (the app
        // in front, else SpringBoard), whatever the top layer is. With SpringBoard the
        // foreground reading, SpringBoard's tree is checked for the lock screen; with an app
        // in front, also once the notify state says locked (audit C6: the locked app still
        // reads as in front for seconds after the lock).
        let notify = notifyLockState()
        let lockScreen: Bool?
        if foreground == Self.springboard || notify == true {
            lockScreen = (try? springBoardTree()).map(LockScreenSignature.matches)
        } else {
            lockScreen = nil
        }
        let signals = LockSignals(
            notify: notify,
            lockScreen: lockScreen,
            front: foreground == Self.springboard ? .springBoard : .app(foreground)
        )
        let verdict = lockJudge.judge(signals, at: lockClockNow())
        record(signals, verdict, everyTime: configuration.noteLockSignalsEveryObservation)
        // One verdict for commands and resume (`LockJudge`).
        if verdict.state == .locked {
            // Nothing behind the lock screen is read or passed on: no snapshot of the app, no
            // tree, no pixels (the lock screen shows notifications).
            notes.append("the phone is locked: no tree and no screenshot were taken (nothing behind the lock screen is read)")
            return emptyObservation(bundleId: Self.springboard, screen: board?.frame, notes: notes, locked: true,
                                    includeScreenshot: false)
        }

        // 2. The top layer (`TopLayer.decide`): with SpringBoard the foreground reading and
        // neither an alert nor Spotlight up, SpringBoard must show its Home Screen; else an app
        // Jarvis cannot identify is in front.
        var shows = TopLayer.SpringBoardShows.other
        if foreground == Self.springboard, !alertUp, !spotlightUp {
            shows = FrontApp.homeScreenShowing(try springBoardTree()) ? .homeScreen : .other
        }
        let top = TopLayer.decide(systemAlertUp: alertUp, spotlightInFront: spotlightUp,
                                  foreground: foreground, springBoard: shows)
        if let note = top.note { notes.append(note) }
        let bundleId = top.bundleId
        if FrontApp.isUnidentified(bundleId) {
            // Never SpringBoard's tree, never "the Home Screen": the application node only.
            reportedBanners = []
            return emptyObservation(bundleId: FrontApp.unidentified, screen: board?.frame, notes: notes,
                                    locked: false, includeScreenshot: includeScreenshot)
        }

        // 3. The top layer's tree: SpringBoard's own snapshot reused when it is the top layer.
        let raw: RawElement
        let snapshotMs: Int
        if bundleId == Self.springboard {
            raw = try springBoardTree()
            snapshotMs = boardMs
        } else {
            let start = Date()
            raw = SnapshotCopy.copy(try application(bundleId).snapshot())
            snapshotMs = (Date().timeIntervalSince(start) * 1_000).clampedInt
        }
        let rootFrame = raw.frame.sanitized
        var size = Size(width: rootFrame.width, height: rootFrame.height)
        if size.width <= 0 || size.height <= 0 {
            size = screenshotSize()
            notes.append("the application element has no frame; screen size taken from a screenshot (\(size.width.compactText)x\(size.height.compactText))")
        }
        lastScreenSize = size
        var built = UiTreeBuilder.build(raw, screenSize: size)
        // A notification banner over an app or Spotlight is SpringBoard's: one covering node
        // per banner, with no text (`BannerCover`). On SpringBoard the banner is in the tree.
        reportedBanners = []
        if bundleId != Self.springboard {
            let banners = bannerFrames()
            if !banners.isEmpty {
                built.root = BannerCover.covering(built.root, banners: banners)
                reportedBanners = banners
                notes.append("a notification banner (SpringBoard's) is up: \(banners.count) covering node(s) with no text added at its frame")
            }
        }
        notes += pendingLockNotes
        pendingLockNotes = []
        notes += pendingCommandNotes
        pendingCommandNotes = []
        if built.sanitizedFrames > 0 {
            notes.append("\(built.sanitizedFrames) element frame(s) were non-finite, huge or negative and were sanitised (raw values in the tree dump)")
        }
        if raw.subtreeFocus == nil, !focusNoteGiven {
            focusNoteGiven = true
            notes.append("keyboard focus is not readable from snapshots on this iOS; focused flags are false")
        }
        let shot = XCUIScreen.main.screenshot()
        let number = nextObservationNumber()
        onObservation?(RunnerObservationRecord(
            number: number, bundleId: bundleId, raw: raw, built: built, screenshot: shot,
            notes: notes, snapshotMs: snapshotMs
        ))
        return RunnerObservation(
            root: built.root,
            appBundleId: bundleId,
            appName: raw.label.isEmpty ? nil : raw.label,
            screenSize: size,
            keyboardVisible: built.keyboardVisible,
            orientation: orientation(),
            screenshot: includeScreenshot ? captured(shot) : nil,
            deviceLocked: false,
            notes: notes
        )
    }

    /// An observation that reads no app's tree: the application node only (`FrontApp.emptyRaw`)
    /// at `screen` (SpringBoard's frame when it was read, else the last size seen, else a
    /// screenshot's), for a locked phone or an app Jarvis cannot identify.
    private func emptyObservation(bundleId: String, screen: Rect?, notes: [String], locked: Bool,
                                  includeScreenshot: Bool) -> RunnerObservation {
        var notes = notes
        let size: Size
        if let screen, screen.sanitized.width > 0, screen.sanitized.height > 0 {
            size = Size(width: screen.sanitized.width, height: screen.sanitized.height)
        } else if let last = lastScreenSize {
            size = last
        } else {
            size = screenshotSize()
        }
        lastScreenSize = size
        let raw = FrontApp.emptyRaw(screen: Rect(x: 0, y: 0, width: size.width, height: size.height))
        let built = UiTreeBuilder.build(raw, screenSize: size)
        notes += pendingLockNotes
        pendingLockNotes = []
        notes += pendingCommandNotes
        pendingCommandNotes = []
        let shot = includeScreenshot ? XCUIScreen.main.screenshot() : nil
        let number = nextObservationNumber()
        onObservation?(RunnerObservationRecord(
            number: number, bundleId: bundleId, raw: raw, built: built, screenshot: shot,
            notes: notes, snapshotMs: 0
        ))
        return RunnerObservation(
            root: built.root,
            appBundleId: bundleId,
            appName: nil,
            screenSize: size,
            keyboardVisible: false,
            orientation: orientation(),
            screenshot: shot.map { captured($0) },
            deviceLocked: locked,
            notes: notes
        )
    }

    private func nextObservationNumber() -> Int {
        observationCounter.withLock { n -> Int in
            n += 1
            return n
        }
    }

    private func captured(_ shot: XCUIScreenshot) -> CapturedScreenshot {
        let image = shot.image
        return CapturedScreenshot(
            data: shot.pngRepresentation,
            mimeType: "image/png",
            pixelWidth: Double(image.size.width * image.scale).clampedUInt32,
            pixelHeight: Double(image.size.height * image.scale).clampedUInt32,
            scale: Double(image.scale)
        )
    }

    /// The screen's size in points from a screenshot (pixels / scale; UIScreen.main is
    /// deprecated): no app's code runs for it.
    private func screenshotSize() -> Size {
        let image = XCUIScreen.main.screenshot().image
        return Size(width: Double(image.size.width).finiteClamped(Rect.coordinateLimit),
                    height: Double(image.size.height).finiteClamped(Rect.coordinateLimit))
    }

    /// Puts `frontAppPath` in the notes when it changed since it was last noted.
    private func noteFrontAppPath(_ notes: inout [String]) {
        guard frontAppPath != notedFrontAppPath else { return }
        notedFrontAppPath = frontAppPath
        notes.append("front app: \(frontAppPath)")
    }

    /// The app in front: the last activated one while it is in front; else the private path
    /// (`FrontProcess`, when `identifyFrontApp`); else the first known app in front; else
    /// SpringBoard (which counts as "in front" under every app, so it is checked last — and
    /// then must show its Home Screen, `TopLayer.decide`).
    private func foregroundBundleId(notes: inout [String]) -> String {
        if let id = lastActivated, application(id).state == .runningForeground {
            frontAppPath = "the last launched app (\(id)) is in front (its application state)"
            return id
        }
        if configuration.identifyFrontApp {
            let answer = FrontProcess.bundleId()
            if let id = answer, !FrontApp.isUnidentified(id) {
                frontAppPath = FrontProcess.lastPath
                if TopLayer.isSpringBoard(id) {
                    if let gone = lastActivated {
                        notes.append("\(gone) is no longer in front; reading SpringBoard")
                    }
                    lastActivated = nil
                    return Self.springboard
                }
                if let previous = lastActivated, previous != id { notes.append("foreground changed to \(id)") }
                lastActivated = id
                return id
            }
            frontAppPath = FrontProcess.lastPath + "; then the known apps"
        } else {
            frontAppPath = "identification off: the known apps"
        }
        for id in configuration.knownBundleIds where id != Self.springboard && id != lastActivated {
            if application(id).state == .runningForeground {
                if lastActivated != nil { notes.append("foreground changed to \(id)") }
                frontAppPath += " (\(id) is in front)"
                lastActivated = id
                return id
            }
        }
        frontAppPath += " (none in front: SpringBoard, which must show its Home Screen)"
        if let id = lastActivated {
            notes.append("\(id) is no longer in front (state \(application(id).state.rawValue)); reading SpringBoard")
        }
        lastActivated = nil
        return Self.springboard
    }

    /// The top layer now, read cheaply (queries on the OS's processes, no snapshot): what
    /// `frontBundleId()` reports and what the check next to a gesture compares `expect` with.
    private func frontOnMain() -> String {
        var notes: [String] = []
        let foreground = foregroundBundleId(notes: &notes)
        let alertUp = systemAlertUp()
        let spotlightUp = !alertUp && spotlightInFront()
        var shows = TopLayer.SpringBoardShows.other
        if foreground == Self.springboard, !alertUp, !spotlightUp {
            shows = springBoardShowsByQuery()
        }
        return TopLayer.decide(systemAlertUp: alertUp, spotlightInFront: spotlightUp,
                               foreground: foreground, springBoard: shows).bundleId
    }

    /// Whether SpringBoard shows an alert (`.exists` on its alerts query).
    private func systemAlertUp() -> Bool {
        application(Self.springboard).alerts.firstMatch.exists
    }

    /// Whether Spotlight runs in the foreground showing its search field (its application
    /// state, then `.exists` on the field's query, made only while it is in the foreground).
    private func spotlightInFront() -> Bool {
        let spotlight = application(Self.spotlight)
        return spotlight.state == .runningForeground
            && spotlight.textFields[TopLayer.spotlightFieldIdentifier].firstMatch.exists
    }

    /// What SpringBoard shows, by queries on SpringBoard (the same identifiers
    /// `FrontApp.homeScreenShowing` and `LockScreenSignature` read from its tree): its Home
    /// Screen; else the lock screen when the lock verdict (notify state and the lock screen
    /// query, `LockJudge`) is locked; else neither (an app Jarvis cannot identify is in front).
    private func springBoardShowsByQuery() -> TopLayer.SpringBoardShows {
        let board = application(Self.springboard)
        let home = NSPredicate(format: "identifier == %@ OR identifier BEGINSWITH %@",
                               FrontApp.homeScreenIdentifier, FrontApp.homeScreenIdentifierPrefix)
        if board.descendants(matching: .any).matching(home).firstMatch.exists { return .homeScreen }
        let lockParts = LockScreenSignature.identifierFragments.map { NSPredicate(format: "identifier CONTAINS %@", $0) }
            + LockScreenSignature.labels.map { NSPredicate(format: "label == %@", $0) }
        let lock = NSCompoundPredicate(orPredicateWithSubpredicates: lockParts)
        let lockScreen = board.descendants(matching: .any).matching(lock).firstMatch.exists
        let signals = LockSignals(notify: notifyLockState(), lockScreen: lockScreen, front: .springBoard)
        let verdict = lockJudge.judge(signals, at: lockClockNow())
        record(signals, verdict, everyTime: false)
        return verdict.state == .locked ? .lockScreen : .other
    }

    /// The frames (screen points) of the notification banners SpringBoard shows now: a query
    /// for the banner's identifier on SpringBoard, and only when one exists, SpringBoard's
    /// snapshot for the frames (`BannerCover.frames`: some SpringBoard windows report pixels).
    private func bannerFrames() -> [Rect] {
        let board = application(Self.springboard)
        guard board.descendants(matching: .any).matching(identifier: BannerCover.identifier).firstMatch.exists,
              let snapshot = try? board.snapshot() else { return [] }
        return BannerCover.frames(inSpringBoard: SnapshotCopy.copy(snapshot))
    }

    private func application(_ bundleId: String) -> XCUIApplication {
        if let app = applications[bundleId] { return app }
        let app = XCUIApplication(bundleIdentifier: bundleId)
        applications[bundleId] = app
        return app
    }

    private func orientation() -> Orientation {
        switch XCUIDevice.shared.orientation {
        case .portraitUpsideDown: return .portraitUpsideDown
        case .landscapeLeft: return .landscapeLeft
        case .landscapeRight: return .landscapeRight
        default: return .portrait
        }
    }

    private func keyboardIsUp() -> Bool {
        let top = frontOnMain()
        // An app Jarvis cannot identify has no application to ask (typing there is refused).
        if FrontApp.isUnidentified(top) { return false }
        return application(top).keyboards.count > 0
    }

    // MARK: - Lock state (main actor)
    //
    // Two readings, combined by `LockSignals` (JarvisKit, Linux-tested):
    // 1. Darwin notify state of "com.apple.springboard.lockstate" (non-zero = locked),
    //    looked up with dlsym so nothing here depends on how the SDK exposes notify.h;
    //    whether the simulator's notifyd publishes it to the test runner is [unverified].
    // 2. SpringBoard's tree showing the lock screen (`LockScreenSignature`, from the
    //    run-16 probe trees): checked whenever SpringBoard is the foreground reading, and with
    //    an app in front once the notify state says locked (audit C6, run 137: right after
    //    the lock the locked app still read as in front for about 8 s).
    // `LockJudge` turns them into ONE verdict (the notify reading counts as stale after 12 s,
    // and only on readings whose check showed no lock screen), used for refusing commands,
    // for `lockState()` (unlock polls and the lock watch) and for observations: the answers
    // cannot contradict.
    // Every change of the readings goes into the next observation's notes.

    private func lockStateOnMain() -> DeviceLockState {
        judgedLockState()
    }

    /// Both readings now, judged (the same verdict `lockState()` gives). For test
    /// preconditions ("did the simulator lock?"); recorded in `lockSignalHistory` on change.
    func commandLockState() -> DeviceLockState {
        judgedLockState()
    }

    /// Time for the judge: seconds since the runner was made.
    private func lockClockNow() -> Duration {
        Duration.seconds(Date().timeIntervalSince(started))
    }

    /// Reads both signals now and judges them.
    private func judgedLockState() -> DeviceLockState {
        let signals = lockSignals()
        let verdict = lockJudge.judge(signals, at: lockClockNow())
        record(signals, verdict, everyTime: false)
        return verdict.state
    }

    /// Reads both signals now.
    private func lockSignals() -> LockSignals {
        var notes: [String] = []
        let front = foregroundBundleId(notes: &notes)
        let notify = notifyLockState()
        // SpringBoard's tree for the lock screen: with SpringBoard in front, and with an app in
        // front once the notify state says locked (the OS's process, not a look at the app).
        var lockScreen: Bool?
        if front == Self.springboard || notify == true {
            lockScreen = (try? application(Self.springboard).snapshot()).map { LockScreenSignature.matches(SnapshotCopy.copy($0)) }
        }
        return LockSignals(notify: notify, lockScreen: lockScreen,
                           front: front == Self.springboard ? .springBoard : .app(front))
    }

    private func record(_ signals: LockSignals, _ verdict: LockVerdict, everyTime: Bool) {
        let evidence = "\(signals) \(verdict)"
        // Change detection ignores the running "notify true for" time.
        let key = "\(signals) \(verdict.state.rawValue)"
        let changed = key != lastLockEvidence
        lastLockEvidence = key
        if let note = verdict.staleNote {
            pendingLockNotes.append(note)
            let seconds = String(format: "%.1f", Date().timeIntervalSince(started))
            lockSignalHistory.append("t+\(seconds)s \(note)")
        }
        guard changed || everyTime else { return }
        pendingLockNotes.append(evidence)
        let seconds = String(format: "%.1f", Date().timeIntervalSince(started))
        lockSignalHistory.append("t+\(seconds)s \(evidence)")
    }

    private func notifyLockState() -> Bool? {
        typealias Register = @convention(c) (UnsafePointer<CChar>?, UnsafeMutablePointer<Int32>?) -> UInt32
        typealias GetState = @convention(c) (Int32, UnsafeMutablePointer<UInt64>?) -> UInt32
        guard let handle = dlopen(nil, RTLD_NOW),
              let registerSymbol = dlsym(handle, "notify_register_check"),
              let getSymbol = dlsym(handle, "notify_get_state") else { return nil }
        let register = unsafeBitCast(registerSymbol, to: Register.self)
        let getState = unsafeBitCast(getSymbol, to: GetState.self)
        if notifyToken == nil {
            var token: Int32 = 0
            let status = "com.apple.springboard.lockstate".withCString { register($0, &token) }
            guard status == 0 else { return nil }
            notifyToken = token
        }
        guard let token = notifyToken else { return nil }
        var state: UInt64 = 0
        guard getState(token, &state) == 0 else { return nil }
        return state != 0
    }

    private func noteRefusal(_ command: RunnerCommand) {
        onCommand?(command, .deviceLocked())
    }

    // MARK: - Commands (main actor)

    private func act(_ command: RunnerCommand, expect: CommandExpect?) -> RunnerResult {
        // Stale-screen check next to the gesture: the top layer is read in this same
        // main-actor step, with no suspension before `execute` acts on it.
        let top = frontOnMain()
        if let refusal = FrontAppGuard.refusalNextToGesture(expect, front: top) {
            onCommand?(command, refusal)
            return refusal
        }
        // Banners, in this same main-actor step as the event: read once (a query on
        // SpringBoard, and its snapshot only when one is up) for a command that synthesizes
        // an event.
        let banners = BannerGate.synthesizesEvent(command) ? bannerFrames() : []
        // A banner that came up since the look and holds the point the command touches (the
        // special case, with its own message).
        if let expect, top != Self.springboard, let p = BannerCover.touchPoint(command),
           BannerCover.refuses(x: p.x, y: p.y, banners: banners, planned: reportedBanners) {
            let refusal = RunnerResult.staleBanner(expected: expect.bundleId)
            onCommand?(command, refusal)
            return refusal
        }
        // Any banner up: no event at all (`BannerGate`; the wait was in `perform`, this is the
        // check next to the event).
        if !BannerGate.mayProceed(command, bannersUp: !banners.isEmpty) {
            let refusal = BannerGate.refusal(waited: .zero, bannerLeft: false)
            onCommand?(command, refusal)
            return refusal
        }
        settlePending = true
        eventStarted = ContinuousClock.now
        let executed = execute(command, in: top)
        // The window XCUITest leaves open: a banner or an alert that came up after the checks
        // above makes XCUITest hold the event; a call that took longer than a held event
        // explains is said in the next observation's notes (the core looks again after every
        // touch).
        if executed.ok, let held = BannerGate.heldNote(command, elapsed: ContinuousClock.now - eventStarted) {
            pendingCommandNotes.append("\(Transcript.describe(command)): \(held)")
        }
        let result = executed.checkedAfterCommand(command, lockedAfter: lockedAfterCommand())
        onCommand?(command, result)
        return result
    }

    /// The last check before XCUITest types into `app` (after the keyboard wait, which can
    /// take seconds): no SpringBoard alert over it and no banner up, else no event.
    private func typingBlocked(_ app: XCUIApplication) -> RunnerResult? {
        if app !== application(Self.springboard), systemAlertUp() {
            return RunnerResult(ok: false, error: "not carried out: a system alert (SpringBoard's) came up before typing; nothing was typed", failure: .stale)
        }
        if bannerIsUp() { return BannerGate.refusal(waited: .zero, bannerLeft: false) }
        return nil
    }

    /// Carries out `command` on the top layer `bundleId` (the app whose coordinates and
    /// keyboard it uses: Spotlight's keyboard is in Spotlight's tree).
    private func execute(_ command: RunnerCommand, in bundleId: String) -> RunnerResult {
        // An app Jarvis cannot identify has no application of its own here: touches use
        // SpringBoard's coordinate space (the screen's), typing and keys other than Home are
        // refused (XCUITest types only into an application it can name).
        let unidentified = FrontApp.isUnidentified(bundleId)
        if unidentified {
            switch command {
            case .typeText:
                return fail("not typed: an app Jarvis cannot identify is in front, and XCUITest types only into an app it can name")
            case .pressKey(let key):
                if case .home = key { break }
                return fail("not typed: an app Jarvis cannot identify is in front, and XCUITest types only into an app it can name")
            default:
                break
            }
        }
        let app = application(unidentified ? Self.springboard : bundleId)
        switch command {
        case .tap(let x, let y):
            point(app, x, y).tap()
            return ok
        case .longPress(let x, let y, let ms):
            point(app, x, y).press(forDuration: Double(ms) / 1_000)
            return ok
        case .swipe(let from, let to, _):
            // press(forDuration:thenDragTo:) drags at XCTest's own speed; the requested
            // duration is not honoured (a short hold keeps it a drag, not a long press).
            point(app, from.x, from.y).press(forDuration: 0.05, thenDragTo: point(app, to.x, to.y))
            return ok
        case .typeText(let text):
            if let refusal = typingRefusal(app) { return refusal }
            eventStarted = ContinuousClock.now
            app.typeText(text)
            return ok
        case .pressKey(let key):
            return press(key, in: app)
        case .launchApp(let id):
            let target = application(id)
            target.activate()
            lastActivated = id
            guard target.wait(for: .runningForeground, timeout: configuration.launchWait) else {
                return fail("\(id) did not reach the foreground within \(configuration.launchWait) s (state \(target.state.rawValue))")
            }
            return ok
        case .openUrl(let text):
            guard let url = URL(string: text) else { return fail("not a URL: \(text)") }
            XCUIDevice.shared.system.open(url)
            // The handling app is found on the next observation.
            lastActivated = nil
            return ok
        case .wait:
            return ok // handled before the main-actor hop
        }
    }

    private func press(_ key: Key, in app: XCUIApplication) -> RunnerResult {
        switch key {
        case .home:
            XCUIDevice.shared.press(.home)
            lastActivated = nil
            return ok
        case .`return`:
            return typeKey("\n", app)
        case .delete:
            return typeKey(XCUIKeyboardKey.delete.rawValue, app)
        case .tab:
            return typeKey("\t", app)
        case .space:
            return typeKey(" ", app)
        case .escape:
            return typeKey(XCUIKeyboardKey.escape.rawValue, app)
        }
    }

    private func typeKey(_ text: String, _ app: XCUIApplication) -> RunnerResult {
        if let refusal = typingRefusal(app) { return refusal }
        eventStarted = ContinuousClock.now
        app.typeText(text)
        return ok
    }

    /// `typeText` without keyboard focus is an XCTest failure, not an error, so check first:
    /// refuse when there is neither a keyboard nor a focused element. When focus cannot be
    /// read at all, typing is attempted (a hardware-keyboard simulator shows no keyboard).
    private func typingRefusal(_ app: XCUIApplication) -> RunnerResult? {
        if app.keyboards.firstMatch.waitForExistence(timeout: configuration.keyboardWait) { return typingBlocked(app) }
        guard let snapshot = try? app.snapshot() else { return typingBlocked(app) }
        let raw = SnapshotCopy.copy(snapshot)
        switch raw.subtreeFocus {
        case true?, nil:
            return typingBlocked(app)
        case false?:
            return fail("no software keyboard and no focused element in \(app.identifierDescription); tap a text field first")
        }
    }

    private func point(_ app: XCUIApplication, _ x: Double, _ y: Double) -> XCUICoordinate {
        // The core sends absolute screen points; the app's frame origin is the screen origin
        // in portrait, so offset from the normalised (0, 0) corner.
        // Never hand XCTest a non-finite or absurd offset (the core computes points from
        // sanitised frames, but this is the last line before the event synthesiser).
        let limit = Rect.coordinateLimit
        return app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
            .withOffset(CGVector(dx: x.finiteClamped(limit), dy: y.finiteClamped(limit)))
    }

    private var ok: RunnerResult { RunnerResult(ok: true, error: nil) }

    private func fail(_ why: String) -> RunnerResult { RunnerResult(ok: false, error: why) }
}

private extension XCUIApplication {
    /// A short description for messages (XCUIApplication has no public bundle id).
    var identifierDescription: String {
        let label = (try? snapshot().label) ?? ""
        return label.isEmpty ? "the foreground app" : "\"\(label)\""
    }
}
