import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

@Suite("Waiting for a task within a limit", .timeLimit(.minutes(1)))
struct TaskWaitTests {
    @Test("the task finishes before the limit: its value")
    func finishesFirst() async {
        let task = Task<Int, Never> { 42 }
        let result = await value(of: task, within: .seconds(10))
        #expect(result == 42)
    }

    @Test("the limit passes first: nil in about the limit, the task keeps running and its result is not lost")
    func limitFirst() async {
        let task = Task<Int, Never> {
            // Returns -1 if it was cancelled (the sleep then ends at once).
            do { try await Task.sleep(for: .milliseconds(1_500)) } catch { return -1 }
            return 7
        }
        let started = ContinuousClock.now
        let result = await value(of: task, within: .milliseconds(100))
        let waited = ContinuousClock.now - started
        #expect(result == nil)
        #expect(waited >= .milliseconds(90))
        #expect(waited < .milliseconds(1_200), "it must not wait for the task (waited \(waited))")
        #expect(await task.value == 7, "the task must not be cancelled and its result must still arrive")
    }

    @Test("the caller is cancelled: nil at once, the task is not cancelled")
    func callerCancelled() async throws {
        let task = Task<Int, Never> {
            do { try await Task.sleep(for: .milliseconds(2_000)) } catch { return -1 }
            return 9
        }
        let started = ContinuousClock.now
        let caller = Task { await value(of: task, within: .seconds(30)) }
        try await Task.sleep(for: .milliseconds(50))
        caller.cancel()
        let result = await caller.value
        #expect(result == nil)
        #expect(ContinuousClock.now - started < .milliseconds(1_500), "a cancelled caller must return at once")
        #expect(await task.value == 9)
    }

    @Test("a caller cancelled before it waits: nil")
    func cancelledBefore() async {
        let task = Task<Int, Never> {
            do { try await Task.sleep(for: .milliseconds(300)) } catch { return -1 }
            return 3
        }
        let caller = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            return await value(of: task, within: .seconds(30))
        }
        #expect(await caller.value == nil)
        #expect(await task.value == 3)
    }
}

@Suite("Listener restart policy", .timeLimit(.minutes(1)))
struct ListenerRestartPolicyTests {
    @Test("delays double from the first step up to the cap")
    func doubling() {
        let policy = ListenerRestartPolicy(initialDelayMs: 1_000, maxDelayMs: 60_000, healthyAfterMs: 30_000)
        #expect((0..<9).map { policy.delayMs(forAttempt: $0) } == [1_000, 2_000, 4_000, 8_000, 16_000, 32_000, 60_000, 60_000, 60_000])
        #expect(policy.delayMs(forAttempt: 10_000) == 60_000)
        #expect(policy.delayMs(forAttempt: -1) == 1_000)
    }

    @Test("failures in a row grow the delay; a short ready period does not reset it")
    func inARow() {
        var policy = ListenerRestartPolicy(initialDelayMs: 500, maxDelayMs: 4_000, healthyAfterMs: 10_000)
        #expect(policy.listenerFailed(nowMs: 0) == 500)
        #expect(policy.listenerFailed(nowMs: 600) == 1_000)
        policy.listenerReady(nowMs: 2_000)
        #expect(policy.listenerFailed(nowMs: 3_000) == 2_000, "ready for 1 s is not healthy")
        #expect(policy.listenerFailed(nowMs: 6_000) == 4_000)
        #expect(policy.listenerFailed(nowMs: 11_000) == 4_000, "capped")
        #expect(policy.attempts == 5)
    }

    @Test("ready long enough before failing starts from the first step again")
    func healthyResets() {
        var policy = ListenerRestartPolicy(initialDelayMs: 1_000, maxDelayMs: 60_000, healthyAfterMs: 30_000)
        _ = policy.listenerFailed(nowMs: 0)
        _ = policy.listenerFailed(nowMs: 1_000)
        _ = policy.listenerFailed(nowMs: 3_000)
        policy.listenerReady(nowMs: 7_000)
        #expect(policy.listenerFailed(nowMs: 37_000) == 1_000)
        #expect(policy.attempts == 1)
        // A failure with no ready in between keeps counting.
        #expect(policy.listenerFailed(nowMs: 100_000) == 2_000)
    }
}

@Suite("Peer address policy", .timeLimit(.minutes(1)))
struct PeerAddressPolicyTests {
    /// Parses dotted IPv4 or IPv6 text (with `::`, and a dotted IPv4 tail) into bytes.
    static func bytes(_ text: String) -> [UInt8] {
        if !text.contains(":") { return text.split(separator: ".").map { UInt8($0)! } }
        var tail: [UInt8] = []
        var head = text
        if let last = text.split(separator: ":").last, last.contains(".") {
            tail = bytes(String(last))
            head = String(text.dropLast(last.count))
            if head.hasSuffix(":") && !head.hasSuffix("::") { head.removeLast() }
        }
        func groups(_ s: Substring) -> [UInt8] {
            s.split(separator: ":").flatMap { g -> [UInt8] in
                let v = UInt16(g, radix: 16)!
                return [UInt8(v >> 8), UInt8(v & 0xFF)]
            }
        }
        let parts = head.components(separatedBy: "::")
        let left = groups(Substring(parts[0]))
        let right = parts.count > 1 ? groups(Substring(parts[1])) : []
        let fill = 16 - tail.count - left.count - right.count
        return left + [UInt8](repeating: 0, count: max(0, fill)) + right + tail
    }

    @Test("the text helper parses what the table uses")
    func parser() {
        #expect(Self.bytes("192.168.1.2") == [192, 168, 1, 2])
        #expect(Self.bytes("::1") == [UInt8](repeating: 0, count: 15) + [1])
        #expect(Self.bytes("::ffff:10.0.0.1") == [UInt8](repeating: 0, count: 10) + [0xFF, 0xFF, 10, 0, 0, 1])
        #expect(Self.bytes("2001:db8::1").count == 16)
        #expect(Self.bytes("fe80::1:2")[0...1] == [0xFE, 0x80])
    }

    static let table: [(address: String, cellular: Bool?, accept: Bool)] = [
        // IPv4
        ("127.0.0.1", false, true),
        ("10.1.2.3", false, true),
        ("172.16.0.1", false, true),
        ("172.31.255.254", false, true),
        ("172.15.0.1", false, false),
        ("172.32.0.1", false, false),
        ("192.168.0.20", false, true),
        ("192.169.0.20", false, false),
        ("169.254.10.10", false, true),
        ("8.8.8.8", false, false),
        ("100.64.0.1", false, false),
        ("0.0.0.0", false, false),
        ("192.168.0.20", nil, true),
        ("192.168.0.20", true, false),
        ("10.1.2.3", true, false),
        // IPv6
        ("::1", false, true),
        ("fe80::1c2a:3bff:fe4d:5e6f", false, true),
        ("febf::1", false, true),
        ("fec0::1", false, false),
        ("fd12:3456:789a::1", false, true),
        ("fc00::1", false, true),
        ("2a02:810d:4b3f:e100:1c2a:3bff:fe4d:5e6f", false, true),
        ("2001:db8::1", false, true),
        ("3fff::1", false, true),
        ("2a02:810d:4b3f:e100::1", nil, false),
        ("2a02:810d:4b3f:e100::1", true, false),
        ("fd12::1", true, false),
        ("fe80::1", true, false),
        ("ff02::1", false, false),
        ("::", false, false),
        ("64:ff9b::808:808", false, false),
        // IPv4-mapped IPv6 follows the IPv4 rule
        ("::ffff:192.168.1.5", false, true),
        ("::ffff:127.0.0.1", false, true),
        ("::ffff:8.8.8.8", false, false),
        ("::ffff:192.168.1.5", true, false),
    ]

    @Test("the table", arguments: table)
    func decide(_ row: (address: String, cellular: Bool?, accept: Bool)) {
        let decision = PeerAddressPolicy.decide(address: Self.bytes(row.address), pathIsCellular: row.cellular)
        if row.accept {
            #expect(decision == .accept, "\(row.address) cellular: \(String(describing: row.cellular))")
        } else {
            guard case .refuse(let why) = decision else {
                Issue.record("\(row.address) cellular: \(String(describing: row.cellular)) must be refused")
                return
            }
            #expect(!why.isEmpty)
        }
    }

    @Test("anything that is not 4 or 16 bytes is refused")
    func wrongLength() {
        for length in [0, 1, 3, 5, 15, 17] {
            #expect(PeerAddressPolicy.decide(address: [UInt8](repeating: 10, count: length), pathIsCellular: false)
                    == .refuse("not an IP address"))
        }
    }
}

@Suite("Byte budget", .timeLimit(.minutes(1)))
struct ByteBudgetTests {
    @Test("add, remove, over the limit")
    func counting() {
        let budget = ByteBudget(limit: 100)
        #expect(budget.add(60))
        #expect(budget.add(40), "exactly the limit is within it")
        #expect(!budget.isOverBudget)
        #expect(!budget.add(1))
        #expect(budget.isOverBudget)
        budget.remove(50)
        #expect(budget.outstanding == 51)
        #expect(!budget.isOverBudget)
        budget.remove(1_000)
        #expect(budget.outstanding == 0, "never below zero")
        budget.add(-5)
        #expect(budget.outstanding == 0)
    }

    @Test("concurrent adds and removes balance out")
    func concurrent() async {
        let budget = ByteBudget(limit: 1_000_000)
        await withTaskGroup(of: Void.self) { group in
            for _ in 0..<8 {
                group.addTask {
                    for _ in 0..<1_000 {
                        budget.add(7)
                        budget.remove(7)
                    }
                }
            }
        }
        #expect(budget.outstanding == 0)
    }

    @Test("the limits mirror the core's MAX_BUFFERED_BYTES")
    func limits() {
        // 4 + MAX_RECORD_LEN, MAX_RECORD_LEN = 2 + 8 + 64 KiB + 16 (core/jarvis-remote/src/codec.rs).
        #expect(RemoteTransportLimits.coreMaxBufferedBytes == 65_566)
        #expect(RemoteTransportLimits.maxUnconsumedInboundBytes == 131_132)
        #expect(RemoteTransportLimits.maxUnsentOutboundBytes > RemoteTransportLimits.coreMaxBufferedBytes)
        #expect(RemoteTransportLimits.connectionReadyTimeoutMs == 10_000)
    }
}

@Suite("Remote control: the listener comes back", .timeLimit(.minutes(1)))
@MainActor
struct RemoteListenerRecoveryTests {
    @Test("a failed listener is restarted by ensureListening, and a paired device connects again")
    func restartsAfterFailure() async throws {
        let r = RemoteHarness("remote-listener-restart")
        defer { r.cleanUp() }
        let paired = try await r.pairDevice("Test Mac")
        let credentials = try #require(paired.credentials)
        let hubs = r.hubsMade
        #expect(r.transport.startCount == 1)
        #expect(!r.service.ensureListening(), "a listening service is left alone")
        #expect(r.transport.startCount == 1)

        r.transport.failListener("the listener stopped (suspended)")
        try await eventually("the failed status") {
            if case .failed = r.service.status { return true }
            return false
        }
        #expect(r.transport.connect() == nil, "a failed listener takes no connections")
        #expect(r.service.ensureListening())
        try await eventually("listening again") {
            if case .listening = r.service.status { return true }
            return false
        }
        #expect(r.transport.startCount == 2)
        #expect(r.hubsMade == hubs, "only the transport restarts; the hub and its devices stay")
        #expect(r.service.connected.isEmpty, "the old connections were closed and the hub told")
        #expect(paired.isClosed)

        let again = try await InProcessRemote.connect(credentials: credentials, over: r.transport)
        defer { again.close() }
        try await eventually("the device to connect again") { r.service.isConnected(again.deviceId ?? "") }
        #expect(!r.service.ensureListening())
    }

    @Test("ensureListening does nothing while remote control is off")
    func offStaysOff() async throws {
        let r = RemoteHarness("remote-listener-off")
        defer { r.cleanUp() }
        #expect(!r.service.ensureListening())
        #expect(r.service.status == .off)
        #expect(r.transport.startCount == 0)
        r.service.setEnabled(true)
        try await eventually("listening") { r.transport.isListening }
        r.service.setEnabled(false)
        r.transport.failListener()
        #expect(!r.service.ensureListening())
        #expect(r.transport.startCount == 1)
    }

    @Test("every received byte is acknowledged as consumed")
    func consumedBytes() async throws {
        let r = RemoteHarness("remote-consumed")
        defer { r.cleanUp() }
        let remote = try await r.pairDevice("Test Mac")
        defer { remote.close() }
        try remote.send(.ping)
        try await eventually("every byte consumed") { r.transport.unconsumedBytes == 0 }
    }
}

@Suite("Notifications, saved settings and intents", .timeLimit(.minutes(1)))
@MainActor
struct OwnerStateTests {
    @Test("the Control warning shows only when notifications are denied")
    func notificationWarning() {
        #expect(NotificationAccess.denied.warning == "Notifications are off for Jarvis, so it cannot tell you when it needs an unlock.")
        for access in [NotificationAccess.unknown, .notDetermined, .allowed] {
            #expect(access.warning == nil)
        }
    }

    @Test("a workflow with a Notify step is recognised")
    func notifyStep() {
        func workflow(_ steps: [StepDraft]) -> Workflow {
            let draft = WorkflowDraft(name: "W", steps: steps)
            return Workflow(id: "w", name: draft.name, enabled: true, trigger: draft.trigger.core, steps: draft.coreSteps,
                            onFailure: draft.onFailure, createdMs: 0, updatedMs: 0, runs: 0, lastRunMs: nil, lastOutcome: nil)
        }
        #expect(workflow([StepDraft(kind: .runTask, text: "x"), StepDraft(kind: .notify, text: "done")]).hasNotifyStep)
        #expect(!workflow([StepDraft(kind: .runTask, text: "x"), StepDraft(kind: .askOwner, text: "ok?")]).hasNotifyStep)
        #expect(workflow(WorkflowSeeds.scheduled.steps).hasNotifyStep)
    }

    @Test("a failed save is worded for the owner")
    func unsaved() {
        let text = SettingsGate.unsavedMessage(StoredFileError.unreadable(file: "settings.json", reason: "locked"))
        #expect(text.hasPrefix("Your change could not be saved ("))
    }

    @Test("an automation is answered with the real reason when workflows or rules cannot be used")
    func externalUnavailable() {
        #expect(WorkflowLibrary.externalUnavailable(settingsRefusal: nil, workflowsBlocked: false) == nil)
        #expect(WorkflowLibrary.externalUnavailable(settingsRefusal: nil, workflowsBlocked: true)
                == "Jarvis cannot read its workflows until the phone has been unlocked once.")
        #expect(WorkflowLibrary.externalUnavailable(settingsRefusal: "Rules unreadable.", workflowsBlocked: true) == "Rules unreadable.")
        #expect(WorkflowLibrary.externalUnavailable(settingsRefusal: "Rules unreadable.", workflowsBlocked: false) == "Rules unreadable.")
    }

    @Test("Away mode takes the reloaded choices without changing phase")
    func awayApplySaved() {
        final class Display: AwayDisplay {
            var brightness = 0.7
            var isIdleTimerDisabled = false
            var isProximityMonitoringEnabled = false
        }
        let display = Display()
        let away = AwayModeController(display: display, acknowledged: false, proximity: false)
        away.applySaved(acknowledged: true, proximity: true)
        #expect(away.acknowledged && away.proximity)
        #expect(away.phase == .off)
        #expect(!display.isProximityMonitoringEnabled, "nothing is applied to the screen while not dimmed")
        #expect(away.request() == .on, "acknowledged: straight in")
        #expect(display.isProximityMonitoringEnabled)
        away.applySaved(acknowledged: true, proximity: false)
        #expect(!display.isProximityMonitoringEnabled)
        #expect(away.phase == .on)
        away.exit()
    }
}
