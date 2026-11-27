import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// The confirm step, both ends: a device paired with the code holds nothing until the owner
// confirms it on the phone (`RemoteControlService.confirm`), after comparing the check
// string the phone lists with the one the controller shows. Plus the reconnect and buffer
// bounds that came with the controller review.

private let confirmPolicy = ControllerReconnectPolicy(initialDelayMs: 20, maxDelayMs: 80, maxAttempts: 4, maxRefusals: 2)

@MainActor
private func controller(_ r: RemoteHarness, secrets: any SecretStore = InMemorySecretStore(),
                        policy: ControllerReconnectPolicy = confirmPolicy, name: String = "Controller") -> RemoteControllerModel {
    RemoteControllerModel(secrets: secrets, random: GeneratorRandomSource(), dialer: InMemoryRemoteDialer(transport: r.transport),
                          deviceName: name, policy: policy, keepAlive: .seconds(3_600), handshakeTimeout: .seconds(5))
}

/// Remote control on, the scripted owner NOT confirming, a controller paired and waiting.
@MainActor
private func pendingController(_ r: RemoteHarness, secrets: any SecretStore = InMemorySecretStore(),
                               policy: ControllerReconnectPolicy = confirmPolicy) async throws -> (RemoteControllerModel, PendingDevice) {
    r.autoConfirm = false
    if !r.service.isOn { r.service.setEnabled(true) }
    try await eventually("the listener") { r.transport.isListening }
    let offer = try r.service.openPairing()
    let c = controller(r, secrets: secrets, policy: policy)
    try await c.pair(code: offer.code, address: .inProcess, name: "  Work Mac  ")
    try await eventually("the waiting notice") { c.state.awaitingConfirmation != nil }
    try await eventually("the pending row on the phone") { r.service.pendingDevices.count == 1 }
    return (c, try #require(r.service.pendingDevices.first))
}

@Suite("Remote control: confirm a new device on the phone", .timeLimit(.minutes(2)))
@MainActor
struct RemoteConfirmStepTests {
    @Test("a paired device waits: same check string on both ends, no permission, every function refused by the phone")
    func pendingGetsNothing() async throws {
        let r = RemoteHarness("confirm-pending", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: true))
        defer { r.cleanUp() }
        let (c, pending) = try await pendingController(r)
        #expect(pending.device.name == "Work Mac", "the name the owner typed, cleaned")
        #expect(c.state.deviceName == "Work Mac")
        #expect(c.state.awaitingConfirmation == pending.check)
        #expect(c.state.confirmationPrompt == "Confirm on the phone that it shows \(pending.check)")
        #expect(c.state.statusTitle == "Waiting for confirmation on the phone")
        #expect(pending.device.permissions == RemotePermissions(startTasks: false, seeProgress: false, seeScreenText: false, approve: false))
        #expect(r.service.devices.isEmpty)
        #expect(c.state.permissions == nil)
        #expect(!c.state.canStartTask && !c.state.canStop && !c.state.canAnswer)
        #expect(c.state.phoneName == nil, "the phone's name is not shared with a pending device")
        #expect(r.service.audit.entries.contains { $0.text.contains("waiting for you to confirm it on this phone") })

        // Sent anyway: refused by the phone, nothing runs, nothing is subscribed.
        let mark = c.received.count
        try c.sendUnchecked(.startTask(text: "Turn off Wi-Fi"))
        try c.sendUnchecked(.subscribe)
        try c.sendUnchecked(.cancel)
        try c.sendUnchecked(.listSkills)
        try await eventually("four refusals") {
            c.received.dropFirst(mark).filter { if case .denied(.notPermitted) = $0 { true } else { false } }.count == 4
        }
        try await Task.sleep(for: .milliseconds(200))
        #expect(r.base.center.session == nil, "a pending device started a task")
        #expect(c.state.question == nil)
        // A task the owner runs on the phone: the pending device hears nothing of it.
        let before = c.received.count
        _ = r.base.center.start("Turn off Wi-Fi")
        try await Task.sleep(for: .milliseconds(600))
        let heard = c.received.dropFirst(before).filter {
            switch $0 {
            case .taskStarted, .progress, .confirmPending, .finished: true
            default: false
            }
        }
        #expect(heard.isEmpty, "a pending device heard: \(heard)")
        #expect(r.service.pendingDevices.count == 1 && r.service.devices.isEmpty)
        c.disconnect()
    }

    @Test("Confirm on the phone grants exactly the new-device permissions; the controller is welcomed and keeps the check until the owner says it matched")
    func confirmGrantsNewDevicePermissions() async throws {
        let r = RemoteHarness("confirm-grant")
        defer { r.cleanUp() }
        let secrets = InMemorySecretStore()
        let (c, pending) = try await pendingController(r, secrets: secrets)
        let stored = try PairedPhone.decode(try #require(try secrets.read(RemoteControllerModel.secretKey)))
        #expect(stored.check == pending.check, "the check is kept with the pairing until confirmed")
        #expect(stored.deviceName == "Work Mac")
        try r.service.confirm(deviceId: pending.device.id)
        try await eventually("the welcome") { c.state.permissions != nil }
        #expect(c.state.permissions == (try newDeviceRemotePermissions()))
        #expect(r.service.devices.map(\.id) == [pending.device.id])
        #expect(r.service.devices.first?.permissions == (try newDeviceRemotePermissions()))
        #expect(r.service.watchScreen(pending.device.id) == false)
        #expect(r.service.pendingDevices.isEmpty)
        #expect(c.state.awaitingConfirmation == nil)
        #expect(c.state.phoneName == "Test phone")
        // Confirm-step break-it review: a welcome is not proof that the phone's owner
        // confirmed THIS device, so it is not worded as one and the check string is kept
        // until the owner says the numbers matched (this test used to expect "Confirmed on
        // the phone" and the check deleted at the welcome).
        #expect(!c.state.entries.contains { $0.text == "Confirmed on the phone" })
        #expect(c.state.entries.contains { $0.text.contains("Check that it showed \(pending.check)") })
        #expect(c.state.pairingCheck == pending.check && c.state.checkUnverified)
        #expect(r.service.audit.entries.contains { $0.text.contains("Confirmed \u{201C}Work Mac\u{201D}") })
        let kept = try PairedPhone.decode(try #require(try secrets.read(RemoteControllerModel.secretKey)))
        #expect(kept.check == pending.check)
        c.confirmCheckMatched()
        let after = try PairedPhone.decode(try #require(try secrets.read(RemoteControllerModel.secretKey)))
        #expect(after.check == nil)
        #expect(c.state.pairingCheck == nil)
        #expect(c.state.canStartTask)
        // Confirming again is refused (it is no longer pending).
        #expect(throws: RemoteControlError.self) { try r.service.confirm(deviceId: pending.device.id) }
        c.disconnect()
    }

    @Test("Remove on the phone deletes a pending device; the controller is told (authenticated) and stops for good")
    func removePending() async throws {
        let r = RemoteHarness("confirm-remove")
        defer { r.cleanUp() }
        let (c, pending) = try await pendingController(r)
        try r.service.revoke(deviceId: pending.device.id)
        try await eventually("the removal on the controller") {
            if case .disconnected(let d) = c.state.phase { return d.reason == .refused && !d.retrying && !d.slowRetry }
            return false
        }
        #expect(r.service.pendingDevices.isEmpty && r.service.devices.isEmpty)
        #expect(c.state.awaitingConfirmation == nil)
        #expect(r.service.audit.entries.contains { $0.text.contains("before it was confirmed") })
        // Nothing to confirm any more.
        #expect(throws: RemoteControlError.self) { try r.service.confirm(deviceId: pending.device.id) }
    }

    @Test("two devices paired one after the other show different check strings; the code works once")
    func checkStringsDiffer() async throws {
        let r = RemoteHarness("confirm-two")
        defer { r.cleanUp() }
        let (first, pendingA) = try await pendingController(r)
        let offer = try r.service.openPairing()
        let second = controller(r, name: "Other Mac")
        try await second.pair(code: offer.code, address: .inProcess)
        try await eventually("the second pending row") { r.service.pendingDevices.count == 2 }
        try await eventually("the second waiting notice") { second.state.awaitingConfirmation != nil }
        let pendingB = try #require(r.service.pendingDevices.first { $0.device.id != pendingA.device.id })
        #expect(second.state.awaitingConfirmation == pendingB.check)
        #expect(pendingA.check != pendingB.check)
        // The same code again: refused, no third row.
        let third = controller(r, name: "Third")
        await #expect(throws: RemoteControllerError.self) { try await third.pair(code: offer.code, address: .inProcess) }
        #expect(r.service.pendingDevices.count == 2)
        first.disconnect()
        second.disconnect()
    }

    @Test("a pending device stays pending when the phone's hub is rebuilt (remote control off and on); stores from before load confirmed")
    func pendingSurvivesRebuild() async throws {
        let r = RemoteHarness("confirm-rebuild")
        defer { r.cleanUp() }
        let (c, pending) = try await pendingController(r)
        r.service.setEnabled(false)
        try await eventually("the drop") { c.state.phase != .connected }
        r.service.setEnabled(true)
        #expect(r.hubsMade == 2)
        #expect(r.service.pendingDevices.map(\.device.id) == [pending.device.id])
        #expect(r.service.pendingDevices.first?.check == pending.check)
        #expect(r.service.devices.isEmpty, "a rebuild confirmed a pending device")
        c.reconnect()
        try await eventually("the reconnect") { c.state.phase == .connected }
        try await eventually("still waiting") { c.state.awaitingConfirmation == pending.check }
        #expect(c.state.permissions == nil)
        c.disconnect()

        // A hub store written before the confirm step: its devices load as confirmed.
        let old = #"{"schema_version":2,"nonce_lease":64,"devices":[{"id":"0102030405060708090a0b0c0d0e0f10","name":"Old Mac","key":"0909090909090909090909090909090909090909090909090909090909090909","permissions":{"start_tasks":true,"see_progress":true,"see_screen_text":false,"approve":false},"paired_at_ms":5}]}"#
        let hub = try JarvisRemoteHub(storeJson: old, phoneName: "Phone")
        #expect(try hub.pendingDevices().isEmpty)
        #expect(try hub.devices().map(\.name) == ["Old Mac"])
        // And a controller record from before the confirm step reads as confirmed.
        let record = #"{"version":1,"deviceId":"01","credentials":"{}","address":{"inProcess":{}}}"#
        let decoded = try PairedPhone.decode(record)
        #expect(decoded.check == nil && decoded.deviceName == nil)
    }

    @Test("wrong codes from the network are counted on the pairing sheet; a new offer starts clean")
    func wrongCodesCounted() async throws {
        let r = RemoteHarness("confirm-wrong")
        defer { r.cleanUp() }
        r.service.setEnabled(true)
        try await eventually("the listener") { r.transport.isListening }
        let offer = try r.service.openPairing()
        let right = offer.code.reveal()
        let wrong = String(right.first == "A" ? "B" : "A") + right.dropFirst()
        for _ in 0..<3 {
            let c = controller(r)
            await #expect(throws: RemoteControllerError.self) { try await c.pair(code: Redacted(wrong), address: .inProcess) }
        }
        try await eventually("the count") { r.service.pairingFailedAttempts == 3 }
        try r.service.openPairing()
        #expect(r.service.pairingFailedAttempts == 0)
        #expect(r.service.pairingLockedUntilMs == 0)
    }

    @Test("the phone's lockout never refuses the device's key; after refused handshakes the controller keeps trying slowly and comes back")
    func slowRetryAfterLockout() async throws {
        let r = RemoteHarness("confirm-slow")
        defer { r.cleanUp() }
        let secrets = InMemorySecretStore()
        let policy = ControllerReconnectPolicy(initialDelayMs: 20, maxDelayMs: 80, maxAttempts: 4, maxRefusals: 2,
                                               notAcceptedRetryDelayMs: 500, maxNotAcceptedRetries: 40)
        r.autoConfirm = true
        r.service.setEnabled(true)
        try await eventually("the listener") { r.transport.isListening }
        let offer = try r.service.openPairing()
        let c = controller(r, secrets: secrets, policy: policy)
        try await c.pair(code: offer.code, address: .inProcess)
        try await eventually("confirmed") { c.state.permissions != nil }
        let id = try #require(r.service.devices.first?.id)
        // A host that knows the device id (it travels in clear) arms the phone's lockout.
        let record = try PairedPhone.decode(try #require(try secrets.read(RemoteControllerModel.secretKey)))
        var doc = try #require(try JSONSerialization.jsonObject(with: Data(record.credentials.utf8)) as? [String: Any])
        doc["device_key"] = String(repeating: "ab", count: 32)
        let forged = String(decoding: try JSONSerialization.data(withJSONObject: doc), as: UTF8.self)
        c.disconnect()
        for _ in 0..<5 {
            _ = try? await InProcessRemote.connect(credentials: Redacted(forged), over: r.transport, timeout: .milliseconds(500))
        }
        #expect(r.service.deviceLockedUntilMs(id) > r.service.nowMs())
        // Confirm-step break-it review F1: the lockout never refuses the device's own key
        // (this test used to expect the refusal here and the slow retries below from it).
        c.reconnect()
        try await eventually("connected despite the lockout") { c.state.phase == .connected }
        c.disconnect()
        // A phone that does not accept this device (removed there, or refusing for any
        // other reason): the controller retries slowly.
        let probe = try #require(r.probe)
        probe.refuseConnections.withLock { $0 = true }
        c.reconnect()
        try await eventually("not accepted, retrying slowly") {
            if case .disconnected(let d) = c.state.phase { return d.reason == .notAccepted && d.slowRetry && !d.retrying }
            return false
        }
        #expect(c.state.statusTitle == "Disconnected")
        #expect(!(c.state.phase.disconnectText ?? "").contains("removed there; pair again"))
        // Once the phone accepts again, a slow retry connects by itself.
        probe.refuseConnections.withLock { $0 = false }
        try await eventually("back by itself", seconds: 20) { c.state.phase == .connected }
        c.disconnect()
    }

    @Test("the received tail stays bounded in count and bytes")
    func receivedBounded() async throws {
        let r = RemoteHarness("confirm-bound")
        defer { r.cleanUp() }
        let c = controller(r)
        for i in 0..<1_000 {
            c.keep(.finished(status: .success, summary: String(repeating: "x", count: 4_000) + "\(i)"))
        }
        #expect(c.received.count <= RemoteControllerModel.maxReceived)
        let bytes = c.received.reduce(0) { $0 + String(describing: $1).utf8.count }
        #expect(bytes <= RemoteControllerModel.maxReceivedBytes)
        #expect(c.received.count >= 2, "the newest are kept")
        if case .finished(_, let summary) = c.received.last { #expect(summary.hasSuffix("999")) }
    }
}

private extension RemoteControllerModel.Phase {
    var disconnectText: String? {
        if case .disconnected(let d) = self { return d.reason.text }
        return nil
    }
}
