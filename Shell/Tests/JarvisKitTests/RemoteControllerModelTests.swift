import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// The controller screen's model (`RemoteControllerModel`) against the REAL phone side:
// `RemoteControlService` with the real hub, the real `JarvisRemoteClient`, over the
// in-memory transport, on the demo backend (DemoPhone + scripted model). The phone is the
// authority: every permission test checks both ends (the model does not offer the function,
// and the phone refuses it when the controller sends it anyway).

/// Reconnects within a few tens of milliseconds, a handful of times.
private let fastPolicy = ControllerReconnectPolicy(initialDelayMs: 20, maxDelayMs: 80, maxAttempts: 4, maxRefusals: 2)

@MainActor
private func makeController(_ r: RemoteHarness, secrets: any SecretStore = InMemorySecretStore(),
                            policy: ControllerReconnectPolicy = fastPolicy) -> RemoteControllerModel {
    RemoteControllerModel(secrets: secrets, random: GeneratorRandomSource(), dialer: InMemoryRemoteDialer(transport: r.transport),
                          deviceName: "Controller", policy: policy, keepAlive: .seconds(3_600), handshakeTimeout: .seconds(5))
}

/// Remote control on, a pairing offer open, the controller paired with its code and told
/// what it may do. Returns the controller and its device id on the phone.
@MainActor
private func pairedController(_ r: RemoteHarness, secrets: any SecretStore = InMemorySecretStore()) async throws -> (RemoteControllerModel, String) {
    if !r.service.isOn { r.service.setEnabled(true) }
    try await eventually("the listener") { r.transport.isListening }
    let offer = try r.service.openPairing()
    let c = makeController(r, secrets: secrets)
    try await c.pair(code: offer.code, address: .inProcess)
    #expect(c.state.phase == .connected)
    try await eventually("the device to be kept") { !r.service.devices.isEmpty }
    try await eventually("the welcome") { c.state.permissions != nil }
    let id = try #require(r.service.devices.first?.id)
    return (c, id)
}

/// Sets the device's permissions on the phone and waits until the controller was told.
@MainActor
private func allow(_ r: RemoteHarness, _ c: RemoteControllerModel, _ id: String, _ p: RemotePermissions) async throws {
    try r.service.setPermissions(deviceId: id, p)
    try await eventually("the new permissions on the controller") { c.state.permissions == p }
}

private func perms(start: Bool = true, progress: Bool = true, text: Bool = false, approve: Bool = false) -> RemotePermissions {
    RemotePermissions(startTasks: start, seeProgress: progress, seeScreenText: text, approve: approve)
}

@MainActor
private func waitEnded(_ c: RemoteControllerModel, seconds: Double = 20) async throws -> RemoteControllerModel.Ended {
    try await eventually("the task to end on the controller", seconds: seconds) {
        if case .ended = c.state.activity { return true }
        return false
    }
    guard case .ended(let e) = c.state.activity else { throw WaitTimeout() }
    return e
}

private func isProgress(_ m: ServerMessage) -> Bool {
    if case .progress = m { return true }
    return false
}

/// The phone's Live view as the app keeps it (a fresh viewer per session, every run event
/// through the service), as in `RemoteLiveViewTests`.
@MainActor
private final class PhoneViewer {
    var viewer = LiveViewModel()
    private var observer: UUID?

    init(_ r: RemoteHarness) {
        let service = r.service
        observer = r.base.center.observe { [weak self] event in
            guard let self else { return }
            switch event {
            case .sessionStarted: self.viewer = LiveViewModel()
            case .session(_, let runEvent): service.feedLiveView(self.viewer, runEvent)
            default: break
            }
        }
    }
}

@Suite("The controller screen's model through the real phone service, hub and client", .timeLimit(.minutes(2)))
@MainActor
struct RemoteControllerModelTests {
    @Test("pair, connect, start a task, follow its status, answer an approvable question, see it end; the pairing is stored")
    func fullFlow() async throws {
        let r = RemoteHarness("ctl-flow", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: true))
        defer { r.cleanUp() }
        let secrets = InMemorySecretStore()
        let (c, id) = try await pairedController(r, secrets: secrets)
        #expect(c.state.phoneName == "Test phone")
        #expect(c.state.permissions == (try newDeviceRemotePermissions()), "a new device: start tasks and see progress")
        #expect(c.state.canStartTask)
        #expect(c.state.sharingNote == "This phone shares only that a task is running, when it waits, and how it ended.")
        #expect(r.service.devices.first?.name == "Controller")
        // The pairing is stored, never printed.
        let stored = try #require(try secrets.read(RemoteControllerModel.secretKey))
        let record = try PairedPhone.decode(stored)
        #expect(record.deviceId == id)
        #expect("\(record)" == "PairedPhone(<redacted>)")
        var dumped = ""
        dump(record, to: &dumped)
        #expect(!dumped.contains(record.credentials))

        try await allow(r, c, id, perms(text: true, approve: true))
        #expect(c.state.sharingNote == nil)
        try c.startTask("Turn off Wi-Fi")
        try await eventually("the task to run") { c.state.isBusy }
        #expect(c.state.canStop)
        #expect(!c.state.canStartTask, "one task at a time")
        // The first use in Settings is asked; this device may approve it.
        try await eventually("the question") { c.state.question != nil }
        let q = try #require(c.state.question)
        #expect(q.category == .firstUse)
        #expect(q.canApprove)
        #expect(c.state.canAnswer)
        #expect(c.state.statusTitle == "Waiting for an answer")
        try c.answer(approved: true)
        let ended = try await waitEnded(c)
        #expect(ended.status == .success)
        #expect(c.state.statusTitle == "Ended \u{00B7} Done")
        #expect(c.state.question == nil)
        #expect(r.base.history.first?.requestedBy == "Controller")
        #expect(r.service.audit.entries.contains { $0.text.hasPrefix("Approved") })
        // With "See what Jarvis sees" the progress carries the redacted screen text.
        let texts = c.received.compactMap { m -> String? in
            if case .progress(_, _, _, _, let t, _) = m { return t }
            return nil
        }
        #expect(texts.contains { $0.contains("Wi-Fi") }, "\(texts)")
        let lines = c.state.entries.map(\.text)
        #expect(lines.contains("Task started"))
        #expect(lines.contains("Approved from this device"))
        #expect(lines.contains { $0.hasPrefix("Done: ") })

        // A second model on the same store connects with the stored pairing, no code.
        c.disconnect()
        #expect(c.state.phase == .disconnected(.init(reason: .byOwner, retrying: false, attempts: 0)))
        let again = makeController(r, secrets: secrets)
        again.activate()
        try await eventually("the stored pairing to connect") { again.state.phase == .connected }
        try await eventually("the welcome") { again.state.permissions == perms(text: true, approve: true) }
        #expect(again.state.phoneName == "Test phone")
        #expect(r.service.devices.count == 1, "no second device")
    }

    @Test("start tasks off: not offered; the phone refuses it when sent anyway, nothing runs")
    func startTasksOff() async throws {
        let r = RemoteHarness("ctl-nostart")
        defer { r.cleanUp() }
        let (c, id) = try await pairedController(r)
        try await allow(r, c, id, perms(start: false))
        #expect(!c.state.canStartTask)
        #expect(throws: RemoteControllerError.notAllowed("The phone does not allow this device to start tasks.")) {
            try c.startTask("Turn off Wi-Fi")
        }
        try c.sendUnchecked(.startTask(text: "Turn off Wi-Fi"))
        try await eventually("the refusal") { c.state.lastRefusal != nil }
        #expect(c.state.lastRefusal == "The phone refused: this device is not allowed to do that.")
        #expect(c.state.phase == .connected, "a refusal is not fatal")
        try await Task.sleep(for: .milliseconds(100))
        #expect(r.base.center.session == nil, "a task started")
        #expect(r.base.history.isEmpty)
        #expect(r.service.audit.entries.contains { $0.isWarning })
    }

    @Test("see progress off: no progress reaches the device; subscribing anyway is refused; its own task still ends visibly")
    func seeProgressOff() async throws {
        let r = RemoteHarness("ctl-noprogress")
        defer { r.cleanUp() }
        let (c, id) = try await pairedController(r)
        try await allow(r, c, id, perms(progress: false))
        #expect(c.state.sharingNote?.hasPrefix("This phone does not share its tasks") == true)
        let mark = c.received.count
        try c.startTask("Turn off Wi-Fi")
        let ended = try await waitEnded(c)
        #expect(ended.status == .success)
        #expect(!c.received.dropFirst(mark).contains(where: isProgress), "progress went to a device without see progress")
        #expect(c.state.progress == nil)
        try c.sendUnchecked(.subscribe)
        try await eventually("the refusal") { c.state.lastRefusal != nil }
        #expect(c.state.lastRefusal == "The phone refused: this device is not allowed to do that.")
    }

    @Test("approve off: the question is shown as answered on the phone; an approval sent anyway is refused; stop works")
    func approveOff() async throws {
        let r = RemoteHarness("ctl-noapprove", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: true))
        defer { r.cleanUp() }
        let (c, _) = try await pairedController(r)
        #expect(c.state.permissions?.approve == false, "off by default")
        try c.startTask("Turn off Wi-Fi")
        try await eventually("the question") { c.state.question != nil }
        let q = try #require(c.state.question)
        #expect(!q.canApprove)
        #expect(!c.state.canAnswer)
        #expect(q.summary == "Waiting for confirmation (first use)", "no detail without screen text: \(q.summary)")
        #expect(throws: RemoteControllerError.notAllowed("Answer this on the phone.")) { try c.answer(approved: true) }
        try c.sendUnchecked(.decision(requestId: q.requestId, approved: true))
        try await eventually("the refusal") { c.state.lastRefusal != nil }
        #expect(r.base.center.broker.pendingConfirmation?.request.category == .firstUse, "the approval went through")
        try c.stop()
        let ended = try await waitEnded(c)
        #expect(ended.status != .success)
        try await eventually("the phone's session to end") { r.base.center.session == nil }
    }

    @Test("a never-remote question (delete) is never answerable, even with approve on; the phone refuses an approval")
    func phoneOnlyQuestion() async throws {
        let r = RemoteHarness("ctl-phoneonly")
        defer { r.cleanUp() }
        let (c, id) = try await pairedController(r)
        try await allow(r, c, id, perms(text: true, approve: true))
        try c.startTask("Delete my account")
        try await eventually("the delete question") { c.state.question?.category == .delete }
        let q = try #require(c.state.question)
        #expect(!q.canApprove, "a delete must be answered on the phone")
        #expect(!c.state.canAnswer)
        #expect(throws: RemoteControllerError.notAllowed("Answer this on the phone.")) { try c.answer(approved: true) }
        try c.sendUnchecked(.decision(requestId: q.requestId, approved: true))
        try await eventually("the refusal") { c.state.lastRefusal != nil }
        #expect(c.state.lastRefusal == "The phone refused: that can only be approved on the phone.")
        #expect(r.base.center.broker.pendingConfirmation?.request.category == .delete, "the delete was approved remotely")
        try c.stop()
        _ = try await waitEnded(c)
    }

    @Test("without screen text nothing model-chosen in detail reaches the device: only that a task runs, waits, and how it ended")
    func noDetailWithoutScreenText() async throws {
        let r = RemoteHarness("ctl-nodetail")
        defer { r.cleanUp() }
        let (c, id) = try await pairedController(r)
        try await allow(r, c, id, perms(text: false, approve: true))
        let mark = c.received.count
        try c.startTask("Turn off Wi-Fi")
        _ = try await waitEnded(c)
        let frames = Array(c.received.dropFirst(mark))
        #expect(frames.contains(where: isProgress))
        let generic: Set<String> = ["Task completed", "Task failed", "Task cancelled", "Task stopped by the safety policy",
                                    "Task interrupted (the phone stayed locked)"]
        for m in frames {
            switch m {
            case .progress(let step, _, let app, let action, let text, _):
                #expect(step == 0 && app == nil && action == nil && text == nil, "detail in \(m)")
            case .finished(_, let summary):
                #expect(generic.contains(summary), "the finish summary is the phone's own words: \(summary)")
            case .confirmPending(_, _, let summary, let canApprove):
                // A device that may approve this question is shown its summary (the gate's
                // words, redacted); otherwise a generic line.
                if !canApprove { #expect(summary.hasPrefix("Waiting for confirmation (")) }
            default:
                break
            }
            if case .confirmPending(_, _, _, true) = m { continue }
            #expect(!String(describing: m).contains("Wi-Fi"), "screen detail in \(m)")
        }
        #expect(c.state.progress.map(\.coarse) ?? true)
    }

    @Test("Live view: refused without \u{201C}Watch the screen layout\u{201D} (no stream); with it the controller draws the phone's run")
    func liveViewByPermission() async throws {
        let r = RemoteHarness("ctl-live")
        defer { r.cleanUp() }
        let phone = PhoneViewer(r)
        let (c, id) = try await pairedController(r)
        #expect(!r.service.watchScreen(id), "off by default")
        try c.startWatching()
        try await eventually("the refusal") { c.state.live == .denied }
        #expect(!c.state.watchWanted, "a refused stream is not asked for again")
        try c.startTask("Turn off Wi-Fi")
        _ = try await waitEnded(c)
        #expect(c.live.model.timeline.steps.isEmpty, "the stream reached a device without the permission")
        #expect(c.live.counters.deltas == 0 && c.live.counters.snapshotParts == 0)

        try r.service.setWatchScreen(deviceId: id, true)
        try c.startWatching()
        try await eventually("the snapshot") { c.state.live == .watching }
        try c.startTask("Turn off Wi-Fi")
        _ = try await waitEnded(c)
        try await eventually("the controller's timeline to match the phone's") {
            !c.live.model.timeline.steps.isEmpty
                && c.live.model.timeline.steps.count == phone.viewer.timeline.steps.count
        }
        #expect(c.live.model.timeline.screens.allSatisfy { $0.frameKey == nil }, "frame keys (pixels) reached the controller")

        // Off on the phone ends the stream at once.
        try r.service.setWatchScreen(deviceId: id, false)
        try await eventually("the end") { c.state.live == .ended(.permissionRevoked) }
        #expect(!c.state.watchWanted)
    }

    @Test("a dropped connection reconnects by itself and the Live view resyncs by a snapshot")
    func reconnectResyncs() async throws {
        let r = RemoteHarness("ctl-reconnect")
        defer { r.cleanUp() }
        let phone = PhoneViewer(r)
        let (c, id) = try await pairedController(r)
        try r.service.setWatchScreen(deviceId: id, true)
        try c.startWatching()
        try await eventually("the snapshot") { c.state.live == .watching }
        try c.startTask("Turn off Wi-Fi")
        _ = try await waitEnded(c)
        let parts = c.live.counters.snapshotParts
        c.dropConnectionForTesting()
        try await eventually("the drop to be seen") { c.state.phase != .connected }
        try await eventually("the reconnect") { c.state.phase == .connected }
        #expect(c.state.entries.contains { $0.text == "The connection to the phone was lost" })
        try await eventually("a snapshot after the reconnect") {
            c.state.live == .watching && c.live.counters.snapshotParts > parts
                && c.live.model.timeline.steps.count == phone.viewer.timeline.steps.count
        }
        #expect(r.service.connected.count == 1, "the old connection is gone on the phone")
        #expect(r.service.devices.count == 1)
    }

    @Test("a removed device is disconnected and cannot reconnect; the retries are bounded")
    func revokedCannotReconnect() async throws {
        let r = RemoteHarness("ctl-revoke")
        defer { r.cleanUp() }
        let (c, id) = try await pairedController(r)
        try r.service.revoke(deviceId: id)
        try await eventually("the refusal") {
            if case .disconnected(let d) = c.state.phase { return d.reason == .refused && !d.retrying }
            return false
        }
        let connectedLines = c.state.entries.filter { $0.text == "Connected" }.count
        #expect(r.service.devices.isEmpty)
        #expect(r.service.connected.isEmpty)
        #expect(!c.state.canStartTask && !c.state.canStop)
        // Reconnect asks once more, and is refused again: never connected.
        c.reconnect()
        try await eventually("the second refusal") {
            if case .disconnected(let d) = c.state.phase { return d.reason == .refused && !d.retrying }
            return false
        }
        #expect(r.service.connected.isEmpty)
        #expect(c.state.entries.filter { $0.text == "Connected" }.count == connectedLines, "a revoked device connected again")
        #expect(connectedLines == 1)
    }

    @Test("a wrong code fails closed; after 5 the phone pauses pairing and even the right code is refused; a malformed code sends nothing")
    func wrongCode() async throws {
        let r = RemoteHarness("ctl-wrongcode")
        defer { r.cleanUp() }
        r.service.setEnabled(true)
        try await eventually("the listener") { r.transport.isListening }
        let offer = try r.service.openPairing()
        let right = offer.code.reveal()
        let first = try #require(right.first)
        let wrong = String(first == "A" ? "B" : "A") + right.dropFirst()
        let secrets = InMemorySecretStore()
        let c = makeController(r, secrets: secrets)

        // Not a code at all: refused before anything is sent (no attempt is counted).
        await #expect(throws: RemoteControllerError.self) { try await c.pair(code: Redacted("1234"), address: .inProcess) }
        #expect(c.state.phase == .notPaired)
        #expect(c.state.lastError?.contains("not a pairing code") == true)

        for attempt in 1...Int(LOCKOUT_THRESHOLD_FOR_TESTS) {
            await #expect(throws: RemoteControllerError.self, "wrong code \(attempt)") {
                try await c.pair(code: Redacted(wrong), address: .inProcess)
            }
            #expect(c.state.phase == .notPaired)
            #expect(c.state.lastError?.contains("did not accept the code") == true)
        }
        #expect(try secrets.read(RemoteControllerModel.secretKey) == nil, "a failed pairing was stored")
        #expect(r.service.devices.isEmpty)
        #expect(r.service.pairingLockedUntilMs > r.service.nowMs(), "five wrong codes did not pause pairing")
        // Locked: even the right code fails, closed.
        await #expect(throws: RemoteControllerError.self) { try await c.pair(code: Redacted(right), address: .inProcess) }
        #expect(c.state.phase == .notPaired)
        #expect(r.service.devices.isEmpty)
    }

    @Test("no lock-screen content reaches the controller while the phone is locked (status or Live view)")
    func noLockScreenContent() async throws {
        let phoneRunner = LockedBannerPhone(lockedPolls: 3)
        let r = RemoteHarness("ctl-lock", rules: ScriptedOwner.Rules(approve: [.firstUse], decline: [], yes: true),
                              demoRunner: { _, _ in phoneRunner })
        defer { r.cleanUp() }
        _ = PhoneViewer(r)
        let (c, id) = try await pairedController(r)
        try await allow(r, c, id, perms(text: true, approve: true))
        try r.service.setWatchScreen(deviceId: id, true)
        try c.startWatching()
        try await eventually("the snapshot") { c.state.live == .watching }
        try c.startTask("Turn off Wi-Fi")
        _ = try await waitEnded(c)
        #expect(phoneRunner.lockedSnapshots > 0, "the run never observed the lock screen")
        #expect(c.received.contains { if case .progress(_, _, _, _, _, true) = $0 { true } else { false } },
                "the controller was not told the task waits for an unlock")
        for m in c.received {
            #expect(!String(describing: m).contains(LockedBannerPhone.secret), "lock-screen text reached the controller: \(m)")
        }
        for entry in c.state.entries { #expect(!entry.text.contains(LockedBannerPhone.secret)) }
        for info in c.live.model.timeline.screens {
            let elements = c.live.model.screen(id: info.id)?.elements ?? []
            #expect(!elements.contains { ($0.label ?? "").contains(LockedBannerPhone.secret) }, "lock screen in the Live view")
            if info.source == .locked { #expect(elements.isEmpty) }
        }
        for step in c.live.model.timeline.steps {
            #expect(!String(describing: step).contains(LockedBannerPhone.secret))
        }
    }

    @Test("forget deletes the stored pairing and resets the screen")
    func forget() async throws {
        let r = RemoteHarness("ctl-forget")
        defer { r.cleanUp() }
        let secrets = InMemorySecretStore()
        let (c, _) = try await pairedController(r, secrets: secrets)
        c.forget()
        #expect(c.state == RemoteControllerModel.State())
        #expect(try secrets.read(RemoteControllerModel.secretKey) == nil)
        try await eventually("the phone to see the connection close") { r.service.connected.isEmpty }
    }

    @Test("the phone shows its connection details while listening")
    func connectionDetails() async throws {
        let r = RemoteHarness("ctl-details")
        defer { r.cleanUp() }
        #expect(r.service.connectionDetails.isEmpty)
        r.service.setEnabled(true)
        try await eventually("listening") { if case .listening = r.service.status { true } else { false } }
        #expect(r.service.connectionDetails == [.inProcess])
        r.service.setEnabled(false)
        #expect(r.service.connectionDetails.isEmpty)
    }
}

@Suite("Controller addresses and reconnect policy")
struct ControllerPureTests {
    @Test("typed addresses: local only, a port, no URL")
    func addresses() {
        #expect(RemotePhoneAddress.parse(host: " 192.168.1.20 ", port: "51234") == .success(.host("192.168.1.20", port: 51_234)))
        #expect(RemotePhoneAddress.parse(host: "iPhone.local", port: "8000") == .success(.host("iPhone.local", port: 8_000)))
        #expect(RemotePhoneAddress.parse(host: "[fe80::1%en0]", port: "1") == .success(.host("fe80::1%en0", port: 1)))
        #expect(RemotePhoneAddress.parse(host: "", port: "1") == .failure(.emptyHost))
        #expect(RemotePhoneAddress.parse(host: "http://10.0.0.2", port: "1") == .failure(.notAHost))
        #expect(RemotePhoneAddress.parse(host: "10.0.0.2", port: "0") == .failure(.badPort))
        #expect(RemotePhoneAddress.parse(host: "10.0.0.2", port: "70000") == .failure(.badPort))
        guard case .failure(.notLocal) = RemotePhoneAddress.parse(host: "8.8.8.8", port: "80") else {
            Issue.record("a public address was accepted")
            return
        }
        #expect(RemotePhoneAddress.host("fe80::1", port: 5).description == "[fe80::1]:5")
    }

    @Test("reconnect delays double, cap, and stop")
    func policy() {
        let p = ControllerReconnectPolicy(initialDelayMs: 1_000, maxDelayMs: 5_000, maxAttempts: 5)
        #expect((1...5).map { p.delayMs(forAttempt: $0) } == [1_000, 2_000, 4_000, 5_000, 5_000])
        #expect(p.delayMs(forAttempt: 6) == nil)
        #expect(p.delayMs(forAttempt: 0) == nil)
    }

    @Test("a stored pairing of an unknown version is refused, not guessed")
    func storedVersion() throws {
        var record = PairedPhone(deviceId: "00", credentials: "secret", phoneName: nil, address: .inProcess)
        #expect(try PairedPhone.decode(record.encoded()) == record)
        record.version = 2
        #expect(throws: (any Error).self) { try PairedPhone.decode(record.encoded()) }
    }
}

/// The core's pairing lockout threshold (`LOCKOUT_THRESHOLD` in jarvis-remote's hub).
private let LOCKOUT_THRESHOLD_FOR_TESTS: UInt32 = 5

/// A demo phone whose lock screen shows a code while it is locked.
private final class LockedBannerPhone: PhoneRunner, Sendable {
    static let secret = "739104"
    private let inner: DemoPhone
    private let locked = Mutex(0)

    init(lockedPolls: Int) { inner = DemoPhone(lockedPolls: lockedPolls) }

    var lockedSnapshots: Int { locked.withLock { $0 } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        guard inner.isLocked else { return try await inner.snapshot(includeScreenshot: includeScreenshot) }
        locked.withLock { $0 += 1 }
        let size = DemoPhone.screenSize
        let banner = UiNode(role: .staticText, label: "Bank: your code is \(Self.secret)", value: nil, identifier: nil,
                            placeholder: nil, frame: Rect(x: 16, y: 200, width: 361, height: 40), enabled: true,
                            visible: true, hittable: false, focused: false, selected: false, secure: false, children: [])
        let root = UiNode(role: .application, label: "SpringBoard", value: nil, identifier: nil, placeholder: nil,
                          frame: Rect(x: 0, y: 0, width: size.width, height: size.height), enabled: true, visible: true,
                          hittable: false, focused: false, selected: false, secure: false, children: [banner])
        return RunnerObservation(root: root, appBundleId: "com.apple.springboard", appName: nil, screenSize: size,
                                 deviceLocked: true)
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult { try await inner.perform(command) }

    func lockState() async throws -> DeviceLockState { try await inner.lockState() }
}
