import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// The confirm step's break-it review, Swift side: the controller keeps its check string
// after a welcome until the owner says the numbers matched; a failed store write on the
// phone keeps the pending question on the phone; typed IPv6 literals must be local.

@MainActor
private func controller(_ r: RemoteHarness, secrets: any SecretStore) -> RemoteControllerModel {
    RemoteControllerModel(secrets: secrets, random: GeneratorRandomSource(), dialer: InMemoryRemoteDialer(transport: r.transport),
                          deviceName: "Controller",
                          policy: ControllerReconnectPolicy(initialDelayMs: 20, maxDelayMs: 80, maxAttempts: 4, maxRefusals: 2),
                          keepAlive: .seconds(3_600), handshakeTimeout: .seconds(5))
}

@Suite("Remote control: confirm-step review fixes", .timeLimit(.minutes(2)))
@MainActor
struct ConfirmReviewFixTests {
    @Test("a phone that lets the device in at once: the controller keeps its check string, not verified, across reconnects and relaunches, until the owner says the numbers matched")
    func checkStringKeptAfterWelcome() async throws {
        // The scripted owner confirms at once, as a fake phone that saw the code would
        // welcome the controller at once: from the controller's side the two look alike.
        let r = RemoteHarness("fix-check")
        defer { r.cleanUp() }
        r.autoConfirm = true
        r.service.setEnabled(true)
        try await eventually("the listener") { r.transport.isListening }
        let secrets = InMemorySecretStore()
        let offer = try r.service.openPairing()
        let c = controller(r, secrets: secrets)
        try await c.pair(code: offer.code, address: .inProcess)
        try await eventually("let in") { c.state.permissions != nil && c.state.isConnected }
        let check = try #require(c.state.pairingCheck)
        #expect(check.count == 7)
        #expect(c.state.checkUnverified)
        #expect(c.state.awaitingConfirmation == nil)
        #expect(c.state.checkReminder == "Connected. Check that the phone showed \(check) when you confirmed this device. "
                + "If it showed other numbers or none, remove this pairing on both devices.")
        #expect(!c.state.entries.contains { $0.text.contains("Confirmed") }, "a welcome was worded as proof: \(c.state.entries)")
        #expect(c.state.entries.contains { $0.text.contains(check) })
        #expect(try PairedPhone.decode(try #require(try secrets.read(RemoteControllerModel.secretKey))).check == check)

        // Across a reconnect.
        c.disconnect()
        #expect(c.state.pairingCheck == check)
        #expect(c.state.checkReminder?.hasPrefix("Check that the phone showed \(check)") == true)
        c.reconnect()
        try await eventually("reconnected") { c.state.isConnected && c.state.permissions != nil }
        #expect(c.state.pairingCheck == check && c.state.checkUnverified)
        c.disconnect()

        // Across a relaunch (a new model on the same stored pairing).
        let again = controller(r, secrets: secrets)
        again.activate()
        #expect(again.state.pairingCheck == check)
        try await eventually("connected after the relaunch") { again.state.isConnected && again.state.permissions != nil }
        #expect(again.state.checkReminder?.contains(check) == true)

        // The owner says the numbers matched: gone, here and in the store.
        again.confirmCheckMatched()
        #expect(again.state.pairingCheck == nil && !again.state.checkUnverified && again.state.checkReminder == nil)
        #expect(try PairedPhone.decode(try #require(try secrets.read(RemoteControllerModel.secretKey))).check == nil)
        again.disconnect()
        let third = controller(r, secrets: secrets)
        third.activate()
        #expect(third.state.pairingCheck == nil)
        third.disconnect()
    }

    @Test("the numbers stay when the stored pairing cannot be written")
    func checkMatchedNeedsTheStore() async throws {
        let r = RemoteHarness("fix-check-store")
        defer { r.cleanUp() }
        r.autoConfirm = true
        r.service.setEnabled(true)
        try await eventually("the listener") { r.transport.isListening }
        let secrets = RecordingSecretStore(log: IOLog())
        let offer = try r.service.openPairing()
        let c = controller(r, secrets: secrets)
        try await c.pair(code: offer.code, address: .inProcess)
        try await eventually("let in") { c.state.permissions != nil }
        let check = try #require(c.state.pairingCheck)
        secrets.failWrites.withLock { $0 = true }
        c.confirmCheckMatched()
        #expect(c.state.pairingCheck == check)
        #expect(c.state.lastError != nil)
        c.disconnect()
    }

    @Test("a failed store write on the phone: the pending question stays the phone's own and is neither offered nor answerable remotely")
    func failedStoreWriteKeepsTheQuestionOnThePhone() async throws {
        let r = RemoteHarness("fix-store", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: nil))
        defer { r.cleanUp() }
        let device = try await r.pairDevice("Test Mac")
        let id = try #require(device.deviceId)
        let credentials = try #require(device.credentials)
        var permissions = try newDeviceRemotePermissions()
        permissions.approve = true
        try r.service.setPermissions(deviceId: id, permissions)
        try device.send(.subscribe)
        try await Task.sleep(for: .milliseconds(50))
        let probe = try #require(r.probe)

        // The question's lease cannot be stored.
        r.secrets.failWrites.withLock { $0 = true }
        let broker = r.base.center.broker
        guard case .success(let task) = r.base.center.start("Turn off Wi-Fi") else {
            Issue.record("refused")
            return
        }
        try await eventually("first use on the phone") { broker.pendingConfirmation?.request.category == .firstUse }
        try await eventually("the hub to know it") {
            guard let hid = probe.requestId(for: .firstUse) else { return false }
            return (try? probe.pendingRequestId()) == hid
        }
        let hid = try #require(probe.requestId(for: .firstUse))
        #expect(probe.storeWriteFailures.withLock { $0 } > 0, "the hub was not told the write failed")
        try await eventually("every connection closed") { device.isClosed }
        #expect(!device.messages.contains { if case .confirmPending = $0 { true } else { false } },
                "the question went out after the failed write")
        #expect(r.service.lastError != nil)

        // The device comes back (the store works again): the question is not offered, and
        // an answer naming its id is refused.
        r.secrets.failWrites.withLock { $0 = false }
        let again = try await InProcessRemote.connect(credentials: credentials, over: r.transport)
        try again.send(.subscribe)
        try await Task.sleep(for: .milliseconds(200))
        #expect(!again.messages.contains { if case .confirmPending = $0 { true } else { false } },
                "a question whose id was never stored was offered: \(again.messages)")
        let mark = again.messages.count
        try again.send(.decision(requestId: hid, approved: true))
        let refused = try await again.waitForMessage(after: mark) { if case .denied = $0 { true } else { false } }
        #expect(refused == .denied(reason: .staleRequest))
        // The phone still asks its owner.
        #expect(broker.pendingConfirmation?.request.category == .firstUse)
        r.base.center.cancelSession()
        _ = await task.value
        again.close()
    }

    @Test("a typed IPv6 literal is local only if loopback, link-local or unique-local (as a peer on an unknown path)")
    func typedIPv6MustBeLocal() {
        for host in ["::1", "[::1]", "fe80::1", "fe80::1%en0", "[fe80::1%en0]", "febf::1", "fc00::1", "fd12:3456:789a::1",
                     "::ffff:192.168.1.20"] {
            guard case .success = RemotePhoneAddress.parse(host: host, port: "51234") else {
                Issue.record("a local address was refused: \(host)")
                continue
            }
        }
        for host in ["2001:4860:4860::8888", "[2001:4860:4860::8888]", "2002::1", "2002:c0a8:0101::1", "2606:4700::1111",
                     "3fff::1", "fec0::1", "ff02::1", "::", "::ffff:8.8.8.8", "64:ff9b::808:808"] {
            guard case .failure(.notLocal) = RemotePhoneAddress.parse(host: host, port: "51234") else {
                Issue.record("a non-local IPv6 literal passed the local-only check: \(host)")
                continue
            }
        }
    }
}
