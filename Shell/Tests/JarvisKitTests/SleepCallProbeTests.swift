import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

// Probes for one toolchain fault: on the Mac (Xcode 26.6) a workflow Wait step aborted the
// process with "freed pointer was not the last allocation" when `WorkflowRunner.drive`
// came back from its injected sleep closure. Each test is one way of making that call, so
// a run names the forms that are safe. A probe that aborts takes the whole process down;
// `scripts/host-test.sh` reruns every test alone and lists them.

protocol ProbeSleeper: Sendable {
    func sleep(_ duration: Duration) async throws
}

struct TaskProbeSleeper: ProbeSleeper {
    func sleep(_ duration: Duration) async throws { try await Task.sleep(for: duration) }
}

@MainActor
final class SleepProbe {
    enum Step { case wait(UInt64), done }

    private let sleep: @Sendable (Duration) async throws -> Void
    private let sleepNoThrow: @Sendable (Duration) async -> Bool
    private let sleeper: any ProbeSleeper
    private(set) var log: [String] = []

    init(sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) },
         sleepNoThrow: @escaping @Sendable (Duration) async -> Bool = { (try? await Task.sleep(for: $0)) != nil },
         sleeper: any ProbeSleeper = TaskProbeSleeper()) {
        self.sleep = sleep
        self.sleepNoThrow = sleepNoThrow
        self.sleeper = sleeper
    }

    /// The stored throwing closure, called plainly.
    func closurePlain() async -> Bool {
        do {
            try await sleep(.milliseconds(5))
            return true
        } catch {
            return false
        }
    }

    /// The shape of `WorkflowRunner.drive`: a loop, a switch, `if … { try await sleep(…) }`.
    func closureInLoop() async -> Bool {
        var step: Step = .wait(5)
        var slept = false
        while true {
            switch step {
            case .done:
                return slept
            case .wait(let untilMs):
                let now: UInt64 = 0
                do {
                    if untilMs > now { try await sleep(.milliseconds(Int64(clamping: untilMs - now))) }
                    log.append("waited")
                    slept = true
                    step = .done
                } catch {
                    step = .done
                }
            }
        }
    }

    /// The same loop with the duration bound to a constant before the call.
    func closureInLoopBound() async -> Bool {
        var step: Step = .wait(5)
        var slept = false
        while true {
            switch step {
            case .done:
                return slept
            case .wait(let untilMs):
                let duration = Duration.milliseconds(Int64(clamping: untilMs))
                let ok = await waitThrowing(duration)
                log.append("waited")
                slept = ok
                step = .done
            }
        }
    }

    private func waitThrowing(_ duration: Duration) async -> Bool {
        do {
            try await sleep(duration)
            return true
        } catch {
            return false
        }
    }

    func closureNoThrow() async -> Bool { await sleepNoThrow(.milliseconds(5)) }

    func existential() async -> Bool {
        do {
            try await sleeper.sleep(.milliseconds(5))
            return true
        } catch {
            return false
        }
    }

    func direct() async -> Bool {
        do {
            try await Task.sleep(for: .milliseconds(5))
            return true
        } catch {
            return false
        }
    }
}

@Suite("Probe: calling a sleep from the main actor", .timeLimit(.minutes(1)))
@MainActor
struct SleepCallProbeTests {
    /// Runs `body` the way `WorkflowRunner.start` runs `drive`: in an unstructured main-actor task.
    private func inTask(_ body: @escaping @MainActor (SleepProbe) async -> Bool, probe: SleepProbe = SleepProbe()) async -> Bool {
        await Task { @MainActor in await body(probe) }.value
    }

    @Test("stored throwing closure from a default argument, plain call")
    func defaultClosurePlain() async { #expect(await inTask { await $0.closurePlain() }) }

    @Test("stored throwing closure from a default argument, in the loop shape")
    func defaultClosureInLoop() async { #expect(await inTask { await $0.closureInLoop() }) }

    @Test("stored throwing closure from a default argument, loop with the call in a helper")
    func defaultClosureInLoopBound() async { #expect(await inTask { await $0.closureInLoopBound() }) }

    @Test("stored throwing closure passed by the caller, plain call")
    func passedClosurePlain() async {
        let probe = SleepProbe(sleep: { try await Task.sleep(for: $0) })
        #expect(await inTask({ await $0.closurePlain() }, probe: probe))
    }

    @Test("stored throwing closure passed by the caller, in the loop shape")
    func passedClosureInLoop() async {
        let probe = SleepProbe(sleep: { try await Task.sleep(for: $0) })
        #expect(await inTask({ await $0.closureInLoop() }, probe: probe))
    }

    @Test("stored throwing closure that sleeps by nanoseconds, in the loop shape")
    func nanosecondClosureInLoop() async {
        let probe = SleepProbe(sleep: { try await Task.sleep(nanoseconds: UInt64($0.components.seconds) * 1_000_000_000 + 5_000_000) })
        #expect(await inTask({ await $0.closureInLoop() }, probe: probe))
    }

    @Test("stored non-throwing closure")
    func nonThrowingClosure() async { #expect(await inTask { await $0.closureNoThrow() }) }

    @Test("a protocol value instead of a closure")
    func protocolValue() async { #expect(await inTask { await $0.existential() }) }

    @Test("Task.sleep called directly")
    func directCall() async { #expect(await inTask { await $0.direct() }) }

    @Test("the plain call without the extra task")
    func plainWithoutTask() async { #expect(await SleepProbe().closurePlain()) }
}

@Suite("Probe: a workflow that only waits", .timeLimit(.minutes(1)))
@MainActor
struct WorkflowWaitProbeTests {
    private func waitOnly(_ name: String, sleep: (@Sendable (Duration) async throws -> Void)?) async throws {
        let h = AppHarness(name, sleep: sleep)
        defer { h.cleanUp() }
        let w = try h.library.save(WorkflowDraft(name: "Wait", trigger: TriggerDraft(), steps: [StepDraft(kind: .wait, waitSeconds: 1)], onFailure: .stop))
        guard case .success(let task) = h.runner.start(id: w.id) else {
            Issue.record("refused")
            return
        }
        let outcome = try await value(of: task, "the workflow run to end")
        #expect(outcome.status == .succeeded, "\(outcome)")
    }

    @Test("the runner's own default sleep")
    func defaultSleep() async throws { try await waitOnly("probe-default", sleep: nil) }

    @Test("a sleep closure passed by the caller")
    func passedSleep() async throws { try await waitOnly("probe-passed", sleep: { try await Task.sleep(for: $0) }) }

    @Test("a passed closure that returns at once")
    func passedNoSleep() async throws { try await waitOnly("probe-nosleep", sleep: { _ in }) }

    @Test("a passed closure that only yields")
    func passedYield() async throws { try await waitOnly("probe-yield", sleep: { _ in await Task.yield() }) }
}
