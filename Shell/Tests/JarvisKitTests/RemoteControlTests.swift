import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

private func isProgress(_ m: ServerMessage) -> Bool {
    if case .progress = m { return true }
    return false
}

private func isFinished(_ m: ServerMessage) -> Bool {
    if case .finished = m { return true }
    return false
}

private func isDenied(_ m: ServerMessage) -> Bool {
    if case .denied = m { return true }
    return false
}

@Suite("Remote control through the real hub and client", .timeLimit(.minutes(1)))
@MainActor
struct RemoteControlTests {
    @Test("pair, reconnect, start a task, watch it, a never-remote approval is refused, decline stops it, revoke ends it")
    func fullFlow() async throws {
        let r = RemoteHarness("remote-flow")
        defer { r.cleanUp() }
        #expect(!r.service.isOn, "remote control must be off by default")
        let paired = try await r.pairDevice("Test Mac")
        #expect(r.service.devices.map(\.name) == ["Test Mac"])
        #expect(r.service.pairing == nil, "the offer was used and must be dropped")
        #expect(r.secrets.writes.withLock { $0 } > 0, "the paired device was not persisted")
        let deviceId = try #require(paired.deviceId)
        let credentials = try #require(paired.credentials)
        #expect("\(credentials)" == "<redacted>")
        #expect(r.service.devices.first?.permissions == (try newDeviceRemotePermissions()))

        paired.close()
        try await eventually("the closed connection") { !r.service.isConnected(deviceId) }
        let remote = try await InProcessRemote.connect(credentials: credentials, over: r.transport)
        try await eventually("the reconnected device") { r.service.isConnected(deviceId) }

        // Even with "approve for you" on, a delete can never be approved remotely.
        var permissions = try newDeviceRemotePermissions()
        permissions.approve = true
        try r.service.setPermissions(deviceId: deviceId, permissions)
        try remote.send(.subscribe)
        try await Task.sleep(for: .milliseconds(50))
        let mark = remote.messages.count
        try remote.send(.startTask(text: "Delete my account"))
        try await remote.waitForMessage(after: mark) { $0 == .taskStarted }
        let progress = try await remote.waitForMessage(after: mark, where: isProgress)
        if case .progress(_, _, _, _, let screenText, _) = progress {
            #expect(screenText == nil, "screen text went to a device without that permission")
        }
        #expect(r.base.center.session?.origin.remoteTaskId != nil, "the task did not start through TaskCenter")
        let pending = try await remote.waitForMessage(after: mark) { m in
            if case .confirmPending(_, .delete, _, _) = m { return true }
            return false
        }
        guard case .confirmPending(let requestId, _, _, let canApprove) = pending else { return }
        #expect(!canApprove)
        #expect(r.base.owner.confirmCategories.contains(.firstUse), "first use was not asked on the phone")

        let beforeApprove = remote.messages.count
        try remote.send(.decision(requestId: requestId, approved: true))
        try await remote.waitForMessage(after: beforeApprove, where: isDenied)
        #expect(r.base.center.broker.pendingConfirmation?.request.category == .delete, "the approval went through")

        try remote.send(.decision(requestId: requestId, approved: false))
        let finished = try await remote.waitForMessage(after: mark, where: isFinished)
        if case .finished(let status, _) = finished {
            #expect(status != .success, "the declined delete finished as a success")
        }
        try await eventually("the session to end") { r.base.center.session == nil }
        #expect(r.base.history.first?.requestedBy == "Test Mac")
        #expect(r.service.audit.entries.contains { $0.text.hasPrefix("Declined") })

        try r.service.revoke(deviceId: deviceId)
        try await eventually("the revoked connection to close") { remote.isClosed }
        #expect(r.service.devices.isEmpty)
        await #expect(throws: (any Error).self) {
            _ = try await InProcessRemote.connect(credentials: credentials, over: r.transport, timeout: .seconds(3))
        }
    }

    @Test("storeChanged is written before any later effect; the real pairing follows the rule")
    func persistBeforeEffects() async throws {
        let r = RemoteHarness("remote-order")
        defer { r.cleanUp() }
        let paired = try await r.pairDevice()
        // Every list the hub returned during pairing: a send after storeChanged leaves only
        // after a write that came after the sends before it.
        let log = r.log.all
        var checked = 0
        for effects in r.probe?.returned.withLock({ $0 }) ?? [] {
            guard let i = effects.firstIndex(of: .storeChanged) else { continue }
            let later = effects[(i + 1)...].compactMap { e -> IOLog.Entry? in
                if case .send(let c, let b) = e { return .send(c, b) }
                return nil
            }
            let earlier = effects[..<i].compactMap { e -> IOLog.Entry? in
                if case .send(let c, let b) = e { return .send(c, b) }
                return nil
            }
            guard let first = later.first, let at = log.firstIndex(of: first) else { continue }
            let from = earlier.last.flatMap { log.firstIndex(of: $0) }.map { $0 + 1 } ?? 0
            #expect(log[from..<at].contains(.write), "a frame left before the store was written")
            checked += 1
        }
        #expect(checked > 0, "pairing produced no storeChanged followed by a frame")
        _ = paired

        // Injected: [send A, storeChanged, send B] runs as send A, write, send B.
        let a = Data("A".utf8), b = Data("B".utf8)
        let start = r.log.all.count
        r.probe?.injectOnTick.withLock { $0 = [.send(connId: 999, bytes: a), .storeChanged, .send(connId: 999, bytes: b)] }
        r.service.tick()
        #expect(Array(r.log.all[start...]) == [.send(999, a), .write, .send(999, b)])

        // A failed write: nothing after it goes out, and every connection is closed.
        r.secrets.failWrites.withLock { $0 = true }
        let start2 = r.log.all.count
        r.probe?.injectOnTick.withLock { $0 = [.send(connId: 999, bytes: a), .storeChanged, .send(connId: 999, bytes: b)] }
        r.service.tick()
        let after = Array(r.log.all[start2...])
        #expect(after.first == .send(999, a))
        #expect(!after.contains(.send(999, b)), "a frame went out after a failed write")
        #expect(after.contains { if case .close = $0 { true } else { false } }, "the connections stayed open")
        #expect(r.service.lastError != nil)
        try await eventually("the device's connection to close") { paired.isClosed }
    }

    @Test("weak entropy is surfaced, never replaced")
    func weakEntropy() async throws {
        let r = RemoteHarness("remote-entropy", random: ZeroRandomSource())
        defer { r.cleanUp() }
        r.service.setEnabled(true)
        try await eventually("the listener") { r.transport.isListening }
        do {
            try r.service.openPairing()
            Issue.record("a pairing offer opened with all-zero entropy")
        } catch let error as RemoteControlError {
            guard case .randomSource = error else {
                Issue.record("wrong error: \(error)")
                return
            }
        }
        #expect(r.service.pairing == nil)
        #expect(r.service.lastError?.contains("random") == true)
        let endpoint = try #require(r.transport.connect())
        try await eventually("the refused connection to close") { endpoint.isClosed }
        #expect(r.service.connected.isEmpty)
    }

    @Test("a remote runSkill with bad arguments is refused by the shell; nothing runs")
    func runSkillBadArguments() async throws {
        let r = RemoteHarness("remote-skill")
        defer { r.cleanUp() }
        try r.base.skills.save(SkillDraft(name: "Text someone", triggers: ["text {who} hello"]))
        let remote = try await r.pairDevice()
        try remote.send(.subscribe)
        try await Task.sleep(for: .milliseconds(50))
        let mark = remote.messages.count
        try remote.send(.runSkill(id: "text-someone", args: [:]))
        let finished = try await remote.waitForMessage(after: mark, where: isFinished)
        if case .finished(let status, _) = finished { #expect(status == .failed) }
        #expect(r.base.history.isEmpty, "a session ran")
        #expect(r.service.audit.entries.contains { $0.text.hasPrefix("Refused a skill") })
    }

    @Test("the shell re-checks a decision against the prompt on screen")
    func decideRechecked() async throws {
        let r = RemoteHarness("remote-decide")
        defer { r.cleanUp() }
        r.service.setEnabled(true)
        try await eventually("the listener") { r.transport.isListening }
        guard case .success(let task) = r.base.center.start("Delete my account") else {
            Issue.record("refused")
            return
        }
        try await eventually("the delete confirmation") { r.base.center.broker.pendingConfirmation?.request.category == .delete }
        // Wait for the hub's request for THIS confirmation. "The hub has a pending request"
        // is not enough: the first-use question (approved by the scripted owner just
        // before) can still be the hub's pending request while the phone already shows the
        // delete; every decision below would then name the wrong request, all of them
        // would be ignored (rightly), and the run would wait for the owner until
        // `humanTimeout` (600 s). That was the intermittent hang of the whole test run.
        try await eventually("the hub to know the delete confirmation") {
            guard let id = r.probe?.requestId(for: .delete) else { return false }
            return (try? r.probe?.pendingRequestId()) == id
        }
        let id = try #require(r.probe?.requestId(for: .delete))
        let device = String(repeating: "0", count: 32)

        // An approval of a never-remote category, even if the hub asked for it: refused.
        r.probe?.injectOnTick.withLock { $0 = [.decide(requestId: id, approved: true, category: .delete, deviceId: device)] }
        r.service.tick()
        #expect(r.base.center.broker.pendingConfirmation?.request.category == .delete)
        // A decision whose category does not match the screen: ignored.
        r.probe?.injectOnTick.withLock { $0 = [.decide(requestId: id, approved: false, category: .payment, deviceId: device)] }
        r.service.tick()
        // A stale request id: ignored.
        r.probe?.injectOnTick.withLock { $0 = [.decide(requestId: id + 1, approved: false, category: .delete, deviceId: device)] }
        r.service.tick()
        #expect(r.base.center.broker.pendingConfirmation?.request.category == .delete, "a mismatched decision was applied")
        // The matching decline is applied.
        r.probe?.injectOnTick.withLock { $0 = [.decide(requestId: id, approved: false, category: .delete, deviceId: device)] }
        r.service.tick()
        let outcome = try await value(of: task, "the declined run to end")
        #expect(outcome.outcome.status != .success)
    }

    @Test("revoking a device never stops the owner's own task")
    func revokeKeepsOwnersTask() async throws {
        let r = RemoteHarness("remote-revoke")
        defer { r.cleanUp() }
        let remote = try await r.pairDevice()
        let deviceId = try #require(remote.deviceId)
        guard case .success(let task) = r.base.center.start("Delete my account") else {
            Issue.record("refused")
            return
        }
        try await eventually("the delete confirmation") { r.base.center.broker.pendingConfirmation?.request.category == .delete }
        try r.service.revoke(deviceId: deviceId)
        try await eventually("the connection to close") { remote.isClosed }
        try await Task.sleep(for: .milliseconds(100))
        #expect(r.base.center.session?.origin == .owner, "revoking a device stopped the owner's task")
        r.base.center.cancelSession()
        _ = try await value(of: task, "the cancelled run to end")
    }

    @Test("a poisoned hub is rebuilt from the secret store; devices come back and can reconnect")
    func poisonedHubRebuilt() async throws {
        let r = RemoteHarness("remote-poison")
        defer { r.cleanUp() }
        let remote = try await r.pairDevice()
        let credentials = try #require(remote.credentials)
        #expect(r.hubsMade == 1)
        r.probe?.poisonNext.withLock { $0 = true }
        r.service.tick()
        #expect(r.hubsMade == 2, "the hub was not rebuilt")
        #expect(r.service.isOn)
        #expect(r.service.devices.map(\.name) == ["Test Mac"])
        try await eventually("the old connection to close") { remote.isClosed }
        let again = try await InProcessRemote.connect(credentials: credentials, over: r.transport)
        #expect(again.deviceId == remote.deviceId)
    }

    @Test("off stops the listener; the catalog lists only enabled skills and workflows")
    func lifecycleAndCatalog() async throws {
        let r = RemoteHarness("remote-life")
        defer { r.cleanUp() }
        try r.base.skills.save(SkillDraft(name: "Lights out", triggers: ["lights out"]))
        try r.base.skills.save(SkillDraft(name: "Hidden", triggers: ["hidden"], enabled: false))
        try r.base.library.save(WorkflowDraft(name: "Evening", steps: [StepDraft(kind: .notify, text: "x")]))
        let remote = try await r.pairDevice()
        let mark = remote.messages.count
        try remote.send(.listSkills)
        let list = try await remote.waitForMessage(after: mark) { m in
            if case .skills = m { return true }
            return false
        }
        if case .skills(let skills, let workflows) = list {
            #expect(skills.map(\.id) == ["lights-out"])
            #expect(workflows.map(\.id) == ["evening"])
        }
        r.service.setEnabled(false)
        #expect(!r.transport.isListening)
        #expect(remote.isClosed)
        #expect(r.service.status == .off)
    }

    @Test("pairing offers and credentials never print")
    func redaction() {
        let offer = RemotePairingOffer(code: Redacted("ABCD-EFGH"), qrPayload: Redacted("jarvis-remote:2:ABCDEFGH"), expiresAtMs: 5)
        var dumped = ""
        dump(offer, to: &dumped)
        for text in ["\(offer)", String(reflecting: offer), dumped, "\(offer.code)", String(reflecting: offer.qrPayload)] {
            #expect(!text.contains("ABCD"), "\(text)")
        }
        #expect(offer.code.reveal() == "ABCD-EFGH")
    }

    @Test("audit words for the hub's events")
    func auditText() {
        #expect(RemoteAuditText.describe(.paired(name: "Mac")).text == "Paired \u{201C}Mac\u{201D}")
        #expect(RemoteAuditText.describe(.denied(request: "approve", reason: .notRemotelyApprovable)).warning)
        #expect(RemoteAuditText.describe(RemotePermissions()) == "nothing")
    }
}

/// A device id for injected decisions.
private func deviceId(_ digit: Character) -> String { String(repeating: digit, count: 32) }

@Suite("Remote decisions are bound to one prompt id", .timeLimit(.minutes(1)))
@MainActor
struct RemoteDecisionBindingTests {
    /// Remote control on, "Turn off Wi-Fi" started with an owner who answers nothing, and
    /// the first-use confirmation on the phone and known to the hub. Returns the hub's
    /// request id, the prompt and the run.
    private func firstUsePending(_ r: RemoteHarness) async throws -> (UInt64, (id: Int, request: ConfirmRequest), Task<TaskRunOutcome, Never>) {
        r.service.setEnabled(true)
        try await eventually("the listener") { r.transport.isListening }
        let broker = r.base.center.broker
        guard case .success(let task) = r.base.center.start("Turn off Wi-Fi") else {
            Issue.record("refused")
            throw WaitTimeout()
        }
        try await eventually("first use") { broker.pendingConfirmation?.request.category == .firstUse }
        try await eventually("the hub to know it") {
            guard let id = r.probe?.requestId(for: .firstUse) else { return false }
            return (try? r.probe?.pendingRequestId()) == id
        }
        return (try #require(r.probe?.requestId(for: .firstUse)), try #require(broker.pendingConfirmation), task)
    }

    private func audit(_ r: RemoteHarness, _ prefix: String) -> Int {
        r.service.audit.entries.filter { $0.text.hasPrefix(prefix) }.count
    }

    /// Blocks the main actor (so remote control sees none of the run's events) until the
    /// broker shows a prompt other than `old`.
    private func waitForNewPrompt(_ broker: HumanPromptBroker, after old: Int) throws -> (id: Int, request: ConfirmRequest) {
        let deadline = Date().addingTimeInterval(5)
        while broker.pendingConfirmation?.id == old || broker.pendingConfirmation == nil, Date() < deadline {
            usleep(1_000)
        }
        return try #require(broker.pendingConfirmation)
    }

    @Test("a decision for hub request h1 never answers a newer prompt with an equal request")
    func staleDecisionNotMisdirected() async throws {
        let r = RemoteHarness("probe-decide", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: nil))
        defer { r.cleanUp() }
        let broker = r.base.center.broker
        let (h1, p1, _) = try await firstUsePending(r)

        // From here on the main actor is not yielded: the phone shows a new prompt (same
        // request, as when the core asks the same thing again) before the run's events reach
        // remote control.
        let request = p1.request
        let second = Task.detached { try await broker.confirm(request) }
        let p2 = try waitForNewPrompt(broker, after: p1.id)
        #expect(p2.id != p1.id)

        r.probe?.injectOnTick.withLock { $0 = [.decide(requestId: h1, approved: true, category: .firstUse, deviceId: deviceId("0"))] }
        r.service.tick()
        #expect(broker.pendingConfirmation?.id == p2.id, "the decision for request \(h1) (prompt \(p1.id)) answered prompt \(p2.id)")
        broker.cancelAll()
        let approved = try? await second.value
        #expect(approved != true, "prompt \(p2.id), never shown to the device, was approved by its stale decision")
        #expect(audit(r, "Ignored a stale decision") == 1)
    }

    @Test("two devices race: the first decision wins, the second is stale and changes nothing")
    func twoDevicesRace() async throws {
        let r = RemoteHarness("decide-race", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: nil))
        defer { r.cleanUp() }
        let (h1, _, task) = try await firstUsePending(r)
        r.probe?.injectOnTick.withLock {
            $0 = [.decide(requestId: h1, approved: false, category: .firstUse, deviceId: deviceId("0")),
                  .decide(requestId: h1, approved: true, category: .firstUse, deviceId: deviceId("1"))]
        }
        r.service.tick()
        #expect(audit(r, "Declined") == 1)
        #expect(audit(r, "Ignored a stale decision") == 1, "\(r.service.audit.entries.map(\.text))")
        #expect(audit(r, "Approved") == 0, "the second device's approval was applied")
        let outcome = try await value(of: task, "the declined run to end")
        if case .finished(_, let result) = outcome {
            #expect(result.outcome.status != .success, "the declined first use ran anyway")
        }
    }

    @Test("a decision that arrives after the owner answered on the phone is stale")
    func afterOwnerAnswered() async throws {
        let r = RemoteHarness("decide-owner", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: nil))
        defer { r.cleanUp() }
        let broker = r.base.center.broker
        let (h1, p1, task) = try await firstUsePending(r)
        // The owner declines on the phone; the device's approval arrives before remote
        // control has seen the run's answer.
        #expect(broker.decline(p1.id))
        r.probe?.injectOnTick.withLock { $0 = [.decide(requestId: h1, approved: true, category: .firstUse, deviceId: deviceId("0"))] }
        r.service.tick()
        #expect(audit(r, "Ignored a stale decision") == 1)
        #expect(audit(r, "Approved") == 0)
        let outcome = try await value(of: task, "the declined run to end")
        if case .finished(_, let result) = outcome {
            #expect(result.outcome.status != .success, "the owner's decline was overridden")
        }
    }

    @Test("a decision for a prompt withdrawn by a lock and replaced by an equal one is stale")
    func replacedAfterLock() async throws {
        let r = RemoteHarness("decide-lock", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: nil))
        defer { r.cleanUp() }
        let broker = r.base.center.broker
        let (h1, p1, _) = try await firstUsePending(r)
        // The lock withdraws p1 (by its id); after the unlock the core asks the same again.
        #expect(broker.withdraw(p1.id))
        let request = p1.request
        let again = Task.detached { try await broker.confirm(request) }
        let p2 = try waitForNewPrompt(broker, after: p1.id)
        #expect(p2.request == p1.request && p2.id != p1.id)

        r.probe?.injectOnTick.withLock { $0 = [.decide(requestId: h1, approved: true, category: .firstUse, deviceId: deviceId("0"))] }
        r.service.tick()
        #expect(broker.pendingConfirmation?.id == p2.id, "the new prompt was answered by a decision about the old one")
        #expect(audit(r, "Ignored a stale decision") == 1)
        broker.withdraw(p2.id)
        let approved = try? await again.value
        #expect(approved != true)
    }

    @Test("the matching decision for the bound prompt is applied")
    func boundDecisionApplies() async throws {
        let r = RemoteHarness("decide-bound", rules: ScriptedOwner.Rules(approve: [], decline: [], yes: nil))
        defer { r.cleanUp() }
        let (h1, _, task) = try await firstUsePending(r)
        r.probe?.injectOnTick.withLock { $0 = [.decide(requestId: h1, approved: false, category: .firstUse, deviceId: deviceId("0"))] }
        r.service.tick()
        #expect(audit(r, "Declined") == 1)
        #expect(audit(r, "Ignored") == 0)
        _ = try await value(of: task, "the declined run to end")
    }
}
