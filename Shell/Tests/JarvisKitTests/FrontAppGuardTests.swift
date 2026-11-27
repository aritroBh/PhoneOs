import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

/// A runner whose front app is scripted: each `frontBundleId()` takes the next reading
/// (`.some(nil)` = cannot tell), then `steady` for ever. Everything else is a `FakePhone`.
final class FrontScriptedPhone: PhoneRunner {
    let inner = FakePhone()
    private let readings: Mutex<[String?]>
    private let reads = Mutex(0)
    let steady: String?

    init(_ readings: [String?], steady: String? = settingsBundle) {
        self.readings = Mutex(readings)
        self.steady = steady
    }

    var frontReads: Int { reads.withLock { $0 } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        try await inner.snapshot(includeScreenshot: includeScreenshot)
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        try await inner.perform(command)
    }

    func frontBundleId() async throws -> String? {
        reads.withLock { $0 += 1 }
        return readings.withLock { r -> String? in r.isEmpty ? steady : r.removeFirst() }
    }
}

@Suite("Stale-screen guard: the front-app check before a command (FrontAppGuard)")
struct FrontAppGuardTests {
    let expectSettings = CommandExpect(bundleId: settingsBundle)

    @Test("another app in front: refused as stale, nothing sent")
    func differs() async throws {
        let phone = FrontScriptedPhone(["com.apple.springboard"])
        let out = try await FrontAppGuard().perform(.tap(x: 10, y: 20), expect: expectSettings, on: phone)
        #expect(!out.result.ok)
        #expect(out.result.failure == .stale)
        #expect(out.result.error?.contains("com.apple.springboard") == true)
        #expect(phone.inner.commands.isEmpty, "the tap reached the phone")
        #expect(out.note == nil)
    }

    @Test("the expected app in front (any letter case): performed")
    func same() async throws {
        let phone = FrontScriptedPhone([" COM.APPLE.Preferences "])
        let out = try await FrontAppGuard().perform(.tap(x: 10, y: 20), expect: expectSettings, on: phone)
        #expect(out.result.ok)
        #expect(phone.inner.commands.count == 1)
        #expect(out.note == nil)
    }

    @Test("a runner that cannot tell: performed, and noted once per guard")
    func unknown() async throws {
        let phone = FrontScriptedPhone([], steady: nil)
        let guardian = FrontAppGuard()
        let first = try await guardian.perform(.tap(x: 10, y: 20), expect: expectSettings, on: phone)
        let second = try await guardian.perform(.typeText(text: "x"), expect: expectSettings, on: phone)
        #expect(first.result.ok && second.result.ok)
        #expect(phone.inner.commands.count == 2)
        #expect(first.note == FrontAppGuard.unknownNote)
        #expect(second.note == nil, "noted once")
    }

    @Test("a runner without the requirement (the protocol's default) counts as unknown")
    func defaultIsUnknown() async throws {
        let phone = FakePhone()
        #expect(try await phone.frontBundleId() == nil)
        let out = try await FrontAppGuard().perform(.tap(x: 10, y: 20), expect: expectSettings, on: phone)
        #expect(out.result.ok)
        #expect(out.note == FrontAppGuard.unknownNote)
    }

    @Test("a command without an expectation is sent unchecked")
    func noExpect() async throws {
        let phone = FrontScriptedPhone(["com.apple.springboard"])
        let out = try await FrontAppGuard().perform(.launchApp(bundleId: settingsBundle), expect: nil, on: phone)
        #expect(out.result.ok)
        #expect(phone.frontReads == 0)
    }

    @Test("LockCheckingRunner passes the front app through")
    func lockCheckingForwards() async throws {
        let phone = FrontScriptedPhone(["com.example.other"])
        #expect(try await LockCheckingRunner(phone).frontBundleId() == "com.example.other")
    }

    @Test("the core's defaults carry the plan-age limit to the shell")
    func defaultPlanAge() throws {
        var policy = try defaultPolicy()
        #expect(policy.maxPlanAgeMs == 1_000)
        JarvisSettings().apply(to: &policy)
        #expect(policy.maxPlanAgeMs == 1_000, "the settings keep the core's limit")
    }
}

@Suite("Stale-screen guard end to end (SessionDriver, real core)", .timeLimit(.minutes(1)))
struct FrontAppDriverTests {
    func run(_ phone: FrontScriptedPhone) async throws -> (RunResult, [DriverUpdate]) {
        let model = FakeModel()
        let config = try SessionConfigBuilder.make(
            capabilities: model.capabilities,
            tools: [],
            knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")],
            adjustPolicy: { $0.firstUseApproval = false }
        )
        let deps = DriverDependencies(
            runner: phone, model: model, tools: ToolRegistry(), human: FakeGate(), memory: RecordingMemory(),
            screenshots: ScreenshotStore(), clock: SteppingClock()
        )
        let driver = try SessionDriver(
            config: config, task: TaskSpec(id: "t1", text: "Turn off Wi-Fi"), memoryJSON: "",
            dependencies: deps, configuration: DriverConfiguration(observeRetryDelay: .milliseconds(10))
        )
        let collector = Task { () -> [DriverUpdate] in
            var all: [DriverUpdate] = []
            for await update in driver.updates { all.append(update) }
            return all
        }
        let result = await driver.run()
        return (result, await collector.value)
    }

    @Test("the front app changes before the first tap: refused, re-observed, chosen again, done")
    func frontChanges() async throws {
        let phone = FrontScriptedPhone(["com.apple.springboard"])
        let (result, updates) = try await run(phone)
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        let stale = updates.filter {
            if case .effectCompleted(_, .actResult(let r)) = $0 { return r.failure == .stale && !r.ok }
            return false
        }
        #expect(stale.count == 1, "\(updates)")
        // The refused tap never reached the phone: the cell and the switch, once each.
        let taps = phone.inner.commands.filter { if case .tap = $0 { return true } else { return false } }
        #expect(taps.count == 2, "\(phone.inner.commands)")
        #expect(phone.inner.page == .wifiOff)
        // After the refusal the next effect is an observation.
        guard let refused = updates.firstIndex(where: {
            if case .effectCompleted(_, .actResult(let r)) = $0 { return r.failure == .stale }
            return false
        }) else { return }
        let next = updates[(refused + 1)...].compactMap { u -> Effect? in if case .effect(let e, _) = u { return e } else { return nil } }
        #expect({ if case .observe? = next.first { return true } else { return false } }(), "\(next.prefix(2))")
    }

    @Test("the expected app stays in front: every gesture carries it and is performed")
    func carriesExpect() async throws {
        let phone = FrontScriptedPhone([])
        let (result, updates) = try await run(phone)
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        let expects = updates.compactMap { u -> CommandExpect?? in
            if case .effect(.act(.tap, let expect), _) = u { return expect }
            return nil
        }
        #expect(!expects.isEmpty)
        #expect(expects.allSatisfy { $0?.bundleId == settingsBundle }, "\(expects)")
        #expect(phone.frontReads == expects.count)
    }

    @Test("a runner that cannot tell: the run works and says so once")
    func unknownNotedOnce() async throws {
        let phone = FrontScriptedPhone([], steady: nil)
        let (result, updates) = try await run(phone)
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(updates.filter { $0 == .note(FrontAppGuard.unknownNote) }.count == 1)
    }
}
