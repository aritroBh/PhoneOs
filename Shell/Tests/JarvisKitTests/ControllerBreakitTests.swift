import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// Break-it tests for the controller screen's model (`RemoteControllerModel`, commit
// a129f14), against the REAL phone side where the phone matters (RemoteControlService,
// hub, client, in-memory transport). Tests named `finding…` fail on a129f14 for the reason
// in their name; tests named `sound…` pin a property that was found to hold.

private let breakPolicy = ControllerReconnectPolicy(initialDelayMs: 20, maxDelayMs: 80, maxAttempts: 4, maxRefusals: 2)

@MainActor
private func makeBreakController(_ r: RemoteHarness, secrets: any SecretStore = InMemorySecretStore(),
                                 dialer: (any RemoteDialer)? = nil,
                                 handshakeTimeout: Duration = .seconds(5)) -> RemoteControllerModel {
    RemoteControllerModel(secrets: secrets, random: GeneratorRandomSource(),
                          dialer: dialer ?? InMemoryRemoteDialer(transport: r.transport),
                          deviceName: "Controller", policy: breakPolicy, keepAlive: .seconds(3_600),
                          handshakeTimeout: handshakeTimeout)
}

@MainActor
private func breakPaired(_ r: RemoteHarness, secrets: any SecretStore = InMemorySecretStore()) async throws -> (RemoteControllerModel, String) {
    if !r.service.isOn { r.service.setEnabled(true) }
    try await eventually("the listener") { r.transport.isListening }
    let offer = try r.service.openPairing()
    let c = makeBreakController(r, secrets: secrets)
    try await c.pair(code: offer.code, address: .inProcess)
    try await eventually("the device to be kept") { !r.service.devices.isEmpty }
    try await eventually("the welcome") { c.state.permissions != nil }
    let id = try #require(r.service.devices.first?.id)
    return (c, id)
}

@MainActor
private func welcomes(_ c: RemoteControllerModel) -> Int {
    c.received.filter { if case .welcome = $0 { true } else { false } }.count
}

/// A connection that is accepted but never says anything (a phone that hangs in the
/// handshake, a slow Wi-Fi, a captive host on the stored address).
private final class SilentConnection: RemoteConnection, @unchecked Sendable {
    let incoming: AsyncStream<Data>
    private let continuation: AsyncStream<Data>.Continuation
    init() { (incoming, continuation) = AsyncStream.makeStream(of: Data.self) }
    func write(_ bytes: Data) {}
    func close() { continuation.finish() }
}

/// Dials the real phone until `silent` is set, then hands out silent connections.
private final class SwitchableDialer: RemoteDialer, @unchecked Sendable {
    let inner: InMemoryRemoteDialer
    let silent = Mutex(false)
    let dials = Mutex(0)
    init(_ transport: InMemoryRemoteTransport) { inner = InMemoryRemoteDialer(transport: transport) }
    func dial(_ address: RemotePhoneAddress) async throws -> any RemoteConnection {
        dials.withLock { $0 += 1 }
        if silent.withLock({ $0 }) { return SilentConnection() }
        return try await inner.dial(address)
    }
}

@Suite("Controller screen: break-it", .timeLimit(.minutes(2)))
@MainActor
struct ControllerBreakitTests {
    // MARK: - State confusion

    @Test("FINDING: a question answered on the phone while the controller was away is still offered after the reconnect")
    func findingStaleQuestionSurvivesReconnect() async throws {
        // A slow demo phone keeps the task running after the question is answered.
        let r = RemoteHarness("brk-stale", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: true),
                              demoRunner: { _, _ in DemoPhone(delay: .milliseconds(400)) })
        defer { r.cleanUp() }
        let (c, id) = try await breakPaired(r)
        try r.service.setPermissions(deviceId: id, RemotePermissions(startTasks: true, seeProgress: true,
                                                                     seeScreenText: true, approve: true))
        try await eventually("approve on the controller") { c.state.permissions?.approve == true }
        try c.startTask("Turn off Wi-Fi")
        try await eventually("the question", seconds: 20) { c.state.question?.canApprove == true }
        let stale = try #require(c.state.question)

        // The controller goes away; the owner answers on the phone meanwhile.
        c.disconnect()
        let pending = try #require(r.base.center.broker.pendingConfirmation)
        #expect(r.base.center.broker.approve(pending.id))
        try await eventually("the phone to have no question") { r.base.center.broker.pendingConfirmation == nil }
        #expect(r.base.center.session != nil, "the task must still run for this scenario")

        let before = welcomes(c)
        c.reconnect()
        try await eventually("the reconnect") { c.state.phase == .connected }
        try await eventually("the new welcome") { welcomes(c) > before }
        try await Task.sleep(for: .milliseconds(150))
        try #require(r.base.center.session != nil, "the task ended; the scenario needs it running")
        // The phone has nothing pending, yet the controller still offers Approve / Decline
        // for the old question (request id \(stale.requestId)). The request id restarts at 1
        // with every phone restart (`next_request_id: 1` in the hub), so such a stale card
        // can name a NEW question's id.
        #expect(c.state.question == nil, "stale question kept: \(String(describing: c.state.question))")
        #expect(!c.state.canAnswer, "the controller offers to answer a question the phone no longer asks")
        c.disconnect()
    }

    @Test("sound: an answer to a question the phone no longer asks is refused by the phone (it carries the request id)")
    func soundStaleAnswerRefusedByPhone() async throws {
        let r = RemoteHarness("brk-stale-ans", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: true),
                              demoRunner: { _, _ in DemoPhone(delay: .milliseconds(400)) })
        defer { r.cleanUp() }
        let (c, id) = try await breakPaired(r)
        try r.service.setPermissions(deviceId: id, RemotePermissions(startTasks: true, seeProgress: true,
                                                                     seeScreenText: true, approve: true))
        try await eventually("approve on the controller") { c.state.permissions?.approve == true }
        try c.startTask("Turn off Wi-Fi")
        try await eventually("the question", seconds: 20) { c.state.question?.canApprove == true }
        let q = try #require(c.state.question)
        let pending = try #require(r.base.center.broker.pendingConfirmation)
        #expect(r.base.center.broker.approve(pending.id))
        try await eventually("the phone to have no question") { r.base.center.broker.pendingConfirmation == nil }
        let auditBefore = r.service.audit.entries.count
        try c.sendUnchecked(.decision(requestId: q.requestId, approved: false))
        try c.sendUnchecked(.decision(requestId: q.requestId, approved: true))
        try await eventually("the refusal") {
            c.received.contains { if case .denied(.staleRequest) = $0 { true } else { false } }
        }
        try await Task.sleep(for: .milliseconds(200))
        #expect(!r.service.audit.entries.dropFirst(auditBefore).contains { $0.text.hasPrefix("Approved") })
        #expect(c.state.lastRefusal == "The phone refused: that question is no longer open.")
        c.disconnect()
    }

    // MARK: - Reconnect classification and storms

    @Test("FINDING: two handshakes that time out (a silent host, slow Wi-Fi) are reported as 'removed; pair again'")
    func findingHandshakeTimeoutReadsAsRemoved() async throws {
        let r = RemoteHarness("brk-timeout")
        defer { r.cleanUp() }
        r.service.setEnabled(true)
        try await eventually("the listener") { r.transport.isListening }
        let secrets = InMemorySecretStore()
        let dialer = SwitchableDialer(r.transport)
        let offer = try r.service.openPairing()
        let c = makeBreakController(r, secrets: secrets, dialer: dialer, handshakeTimeout: .milliseconds(100))
        try await c.pair(code: offer.code, address: .inProcess)
        try await eventually("connected") { c.state.phase == .connected }
        dialer.silent.withLock { $0 = true }
        c.dropConnectionForTesting()
        try await eventually("the controller to stop retrying", seconds: 10) {
            if case .disconnected(let d) = c.state.phase { return !d.retrying }
            return false
        }
        guard case .disconnected(let d) = c.state.phase else { return }
        // Nothing refused this device: the device is still listed on the phone, the phone
        // never closed a handshake. Telling the owner to re-pair sends them to Forget.
        #expect(!r.service.devices.isEmpty)
        #expect(d.reason != .refused, "a handshake timeout was reported as: \(d.reason.text)")
        c.disconnect()
    }

    @Test("FINDING: the phone's temporary per-device lockout (any LAN host that saw the device id can arm it) ends retries as 'removed; pair again'")
    func findingLockoutReadsAsRemoved() async throws {
        let r = RemoteHarness("brk-lockout")
        defer { r.cleanUp() }
        let secrets = InMemorySecretStore()
        let (c, id) = try await breakPaired(r, secrets: secrets)
        // An attacker knows the device id (sent in clear in every ConnectRequest) but not
        // the key: five bad-MAC connects arm the phone's lockout for this device.
        let stored = try #require(try secrets.read(RemoteControllerModel.secretKey))
        let record = try PairedPhone.decode(stored)
        let doc = try #require(try JSONSerialization.jsonObject(with: Data(record.credentials.utf8)) as? [String: Any])
        var forged = doc
        forged["device_key"] = String(repeating: "ab", count: 32)
        let forgedJson = String(decoding: try JSONSerialization.data(withJSONObject: forged), as: UTF8.self)
        c.disconnect()
        for _ in 0..<5 {
            _ = try? await InProcessRemote.connect(credentials: Redacted(forgedJson), over: r.transport, timeout: .milliseconds(500))
        }
        #expect(r.service.deviceLockedUntilMs(id) > r.service.nowMs(), "the lockout was not armed")
        c.reconnect()
        // Confirm-step break-it review F1: the lockout no longer refuses the device's own
        // key at all (this test used to wait for the refused retries to end), so it cannot
        // read as removed either.
        try await eventually("connected again despite the lockout", seconds: 10) {
            if case .disconnected(let d) = c.state.phase, !d.retrying { return true }
            return c.state.phase == .connected
        }
        #expect(!r.service.devices.isEmpty, "the device is still paired on the phone")
        #expect(c.state.phase == .connected, "the device's own key was refused: \(c.state.phase)")
        if case .disconnected(let d) = c.state.phase {
            #expect(d.reason != .refused,
                    "a lockout of a few seconds became a permanent 'pair again' state: \(d.reason.text)")
        }
        c.disconnect()
    }

    @Test("FINDING: a phone that accepts the handshake and drops at once makes the controller reconnect without bound")
    func findingReconnectStormUnbounded() async throws {
        let r = RemoteHarness("brk-storm")
        defer { r.cleanUp() }
        let (c, _) = try await breakPaired(r)
        var drops = 0
        let deadline = ContinuousClock.now + .seconds(8)
        while drops < 3 * breakPolicy.maxAttempts, ContinuousClock.now < deadline {
            if c.state.phase == .connected {
                c.dropConnectionForTesting()
                drops += 1
            }
            if case .disconnected(let d) = c.state.phase, !d.retrying { break }
            try await Task.sleep(for: .milliseconds(5))
        }
        let connects = c.state.entries.filter { $0.text == "Connected" }.count
        // Every authenticated connection resets `attempts` (and `refusals`), so the policy's
        // maxAttempts never applies: the controller keeps dialing every `initialDelayMs`.
        #expect(connects <= breakPolicy.maxAttempts + 1,
                "\(connects) connections in a row to a phone that drops each one (maxAttempts \(breakPolicy.maxAttempts))")
        c.disconnect()
    }

    // MARK: - Secrets

    @Test("sound: neither the code nor the stored key appears in the model's state, errors or description")
    func soundNoSecretsInStateOrErrors() async throws {
        let r = RemoteHarness("brk-secrets")
        defer { r.cleanUp() }
        r.service.setEnabled(true)
        try await eventually("the listener") { r.transport.isListening }
        let offer = try r.service.openPairing()
        let code = offer.code.reveal()
        let compact = code.replacingOccurrences(of: "-", with: "")
        let secrets = InMemorySecretStore()
        let c = makeBreakController(r, secrets: secrets)
        // A code with one character too many (the core's error must not echo it).
        let tooLong = code + "A"
        var thrown = ""
        do { try await c.pair(code: Redacted(tooLong), address: .inProcess) } catch { thrown = String(describing: error) }
        #expect(!thrown.isEmpty)
        #expect(!thrown.contains(compact.prefix(8)), "the code is echoed in an error")
        #expect(!(c.state.lastError ?? "").contains(compact.prefix(8)))
        try await c.pair(code: offer.code, address: .inProcess)
        try await eventually("connected") { c.state.phase == .connected }
        let stored = try #require(try secrets.read(RemoteControllerModel.secretKey))
        let record = try PairedPhone.decode(stored)
        let doc = try #require(try JSONSerialization.jsonObject(with: Data(record.credentials.utf8)) as? [String: Any])
        let key = try #require(doc["device_key"] as? String)
        var dumped = ""
        dump(c.state, to: &dumped)
        for text in [String(describing: c.state), String(reflecting: c.state), dumped, String(describing: record),
                     String(reflecting: record), String(describing: c.state.entries)] {
            #expect(!text.contains(key), "the device key is printed")
            #expect(!text.contains(compact.prefix(8)), "the code is printed")
        }
        c.forget()
        #expect(try secrets.read(RemoteControllerModel.secretKey) == nil)
    }

    // MARK: - Addresses

    @Test("FINDING: the typed-address check judges dotted literals differently from the system resolver")
    func findingAddressParserDisagreesWithResolver() {
        // inet_aton reads a leading 0 as octal and accepts 1-3 part forms and hex; the
        // parser reads "010" as 10 (local) and does not treat the others as IPv4 literals,
        // so each passes the "local IPv4 only" check while the system dials a public host.
        // (Defence in depth only: the dialer judges the peer again once connected, but the
        // TCP handshake to that host has happened by then.)
        let cases = ["010.8.8.8", "0x08080808", "134744072", "8.8.8.8.", "::ffff:8.8.8.8"]
        for host in cases {
            let parsed = RemotePhoneAddress.parse(host: host, port: "80")
            if case .success = parsed {
                Issue.record("a public address passed the local-only check: \(host)")
            }
        }
    }

    @Test("sound: connection details are empty while remote control is off or not listening")
    func soundConnectionDetailsOnlyWhileListening() async throws {
        let r = RemoteHarness("brk-details")
        defer { r.cleanUp() }
        #expect(r.service.connectionDetails.isEmpty)
        r.service.setEnabled(true)
        try await eventually("listening") { if case .listening = r.service.status { true } else { false } }
        #expect(!r.service.connectionDetails.isEmpty)
        r.service.setEnabled(false)
        #expect(r.service.connectionDetails.isEmpty)
    }

    @Test("sound: the reconnect policy is bounded for any input (no overflow, never past the cap)")
    func soundPolicyBounded() {
        let p = ControllerReconnectPolicy(initialDelayMs: .max / 3, maxDelayMs: .max, maxAttempts: .max, maxRefusals: 0)
        #expect(p.maxRefusals == 1)
        for attempt in [1, 2, 3, 64, 1_000] { #expect((p.delayMs(forAttempt: attempt) ?? 0) <= p.maxDelayMs) }
        #expect(p.delayMs(forAttempt: Int.min) == nil)
        let q = ControllerReconnectPolicy(initialDelayMs: 0, maxDelayMs: 0, maxAttempts: -5)
        #expect(q.maxAttempts == 1)
        #expect(q.delayMs(forAttempt: 1) == 0)
    }
}
