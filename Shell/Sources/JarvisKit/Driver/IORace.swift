import Foundation
import Synchronization

/// Errors the driver's IO wrapper adds.
public enum DriverIOError: Error, Equatable, Sendable, CustomStringConvertible {
    /// The IO did not finish within its deadline.
    case timedOut(IOKind, Duration)

    public var description: String {
        switch self {
        case .timedOut(let kind, let limit): "\(kind.rawValue) timed out after \(limit)"
        }
    }
}

/// Runs one IO operation against a deadline and cancellation, and returns as soon as the
/// first of them happens, even when the operation ignores cancellation.
///
/// The operation runs in its own task. When the deadline passes or `cancel()` is called (or
/// the awaiting task is cancelled) the waiter is resumed at once and the operation's task is
/// cancelled; an operation that does not react to cancellation keeps running detached until
/// it ends on its own, and its result is dropped. Single use.
final class IORace<Value: Sendable>: Sendable {
    private struct State {
        var continuation: CheckedContinuation<Value, any Error>?
        /// A result that arrived before the waiter was installed.
        var early: Result<Value, any Error>?
        var finished = false
        var tasks: [Task<Void, Never>] = []
    }

    private let state = Mutex(State())

    /// Runs `operation`, failing with `DriverIOError.timedOut` after `timeout` (none when
    /// `nil`) and with `CancellationError` on cancellation.
    func run(
        kind: IOKind,
        timeout: Duration?,
        _ operation: @escaping @Sendable () async throws -> Value
    ) async throws -> Value {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Value, any Error>) in
                let early: Result<Value, any Error>? = state.withLock { state in
                    if state.finished {
                        return state.early ?? .failure(CancellationError())
                    }
                    state.continuation = continuation
                    return nil
                }
                if let early {
                    continuation.resume(with: early)
                    return
                }
                var tasks = [Task { [self] in
                    do {
                        finish(.success(try await operation()))
                    } catch {
                        finish(.failure(error))
                    }
                }]
                if let timeout {
                    tasks.append(Task { [self] in
                        do {
                            try await Task.sleep(for: timeout)
                        } catch {
                            return // cancelled: the race is already over
                        }
                        finish(.failure(DriverIOError.timedOut(kind, timeout)))
                    })
                }
                let alreadyOver = state.withLock { state -> Bool in
                    if state.finished { return true }
                    state.tasks = tasks
                    return false
                }
                if alreadyOver {
                    tasks.forEach { $0.cancel() }
                }
            }
        } onCancel: {
            finish(.failure(CancellationError()))
        }
    }

    /// Ends the race with `CancellationError` (no-op once it is over).
    func cancel() {
        finish(.failure(CancellationError()))
    }

    private func finish(_ result: Result<Value, any Error>) {
        let (continuation, tasks) = state.withLock { state -> (CheckedContinuation<Value, any Error>?, [Task<Void, Never>]) in
            if state.finished { return (nil, []) }
            state.finished = true
            guard let continuation = state.continuation else {
                state.early = result
                return (nil, [])
            }
            state.continuation = nil
            let tasks = state.tasks
            state.tasks = []
            return (continuation, tasks)
        }
        tasks.forEach { $0.cancel() }
        continuation?.resume(with: result)
    }
}
