import Foundation
import JarvisCore
import Synchronization

/// Something the owner must answer: a confirmation or a question.
public enum HumanPrompt: Sendable, Equatable, Identifiable {
    case confirm(id: Int, request: ConfirmRequest)
    case question(id: Int, text: String)

    public var id: Int {
        switch self {
        case .confirm(let id, _), .question(let id, _): id
        }
    }
}

/// Why a prompt ended without the owner's answer.
public enum HumanPromptError: Error, Equatable, Sendable {
    /// `cancelAll()` was called (the run ended or the UI went away).
    case withdrawn
}

/// The app's `HumanGate`: each request becomes a `HumanPrompt` that the UI shows (via
/// `onChange`) and resolves with `approve` / `decline` / `reply`. One prompt at a time (the
/// driver asks one thing at a time); a newer prompt replaces an older one, which is
/// withdrawn (a withdrawn confirmation is a decline for the driver).
public final class HumanPromptBroker: HumanGate {
    private enum Waiter {
        case confirm(CheckedContinuation<Bool, any Error>)
        case answer(CheckedContinuation<String, any Error>)
    }

    private struct State {
        var nextID = 1
        var prompt: HumanPrompt?
        var waiter: Waiter?
    }

    private let state = Mutex(State())
    private let onChange: @Sendable (HumanPrompt?) -> Void

    /// `onChange` is called with the new pending prompt (`nil` when none), from any thread.
    public init(onChange: @escaping @Sendable (HumanPrompt?) -> Void = { _ in }) {
        self.onChange = onChange
    }

    /// The prompt waiting for the owner.
    public var pending: HumanPrompt? { state.withLock { $0.prompt } }

    public func confirm(_ request: ConfirmRequest) async throws -> Bool {
        try await withCheckedThrowingContinuation { (c: CheckedContinuation<Bool, any Error>) in
            install { id in (.confirm(id: id, request: request), .confirm(c)) }
        }
    }

    public func answer(_ question: String) async throws -> String {
        try await withCheckedThrowingContinuation { (c: CheckedContinuation<String, any Error>) in
            install { id in (.question(id: id, text: question), .answer(c)) }
        }
    }

    /// Approves confirmation `id`. Returns false when it is no longer pending.
    @discardableResult
    public func approve(_ id: Int) -> Bool { decide(id, true) }

    /// Declines confirmation `id`.
    @discardableResult
    public func decline(_ id: Int) -> Bool { decide(id, false) }

    /// Answers question `id`.
    @discardableResult
    public func reply(_ id: Int, _ text: String) -> Bool {
        guard let waiter = take(id, confirmation: false), case .answer(let c) = waiter else { return false }
        c.resume(returning: text)
        return true
    }

    /// Withdraws whatever is pending (a confirmation counts as declined).
    public func cancelAll() {
        let waiter = state.withLock { s -> Waiter? in
            defer { s.prompt = nil; s.waiter = nil }
            return s.waiter
        }
        guard let waiter else { return }
        Self.withdraw(waiter)
        onChange(nil)
    }

    // MARK: - Internals

    private func install(_ make: (Int) -> (HumanPrompt, Waiter)) {
        let (prompt, previous) = state.withLock { s -> (HumanPrompt, Waiter?) in
            let id = s.nextID
            s.nextID += 1
            let (prompt, waiter) = make(id)
            let previous = s.waiter
            s.prompt = prompt
            s.waiter = waiter
            return (prompt, previous)
        }
        if let previous { Self.withdraw(previous) }
        onChange(prompt)
    }

    private func decide(_ id: Int, _ approved: Bool) -> Bool {
        guard let waiter = take(id, confirmation: true), case .confirm(let c) = waiter else { return false }
        c.resume(returning: approved)
        return true
    }

    /// Removes and returns the waiter of prompt `id` when it is of the asked kind.
    private func take(_ id: Int, confirmation: Bool) -> Waiter? {
        let waiter = state.withLock { s -> Waiter? in
            guard let prompt = s.prompt, prompt.id == id else { return nil }
            let isConfirmation = if case .confirm = prompt { true } else { false }
            guard isConfirmation == confirmation else { return nil }
            defer { s.prompt = nil; s.waiter = nil }
            return s.waiter
        }
        if waiter != nil { onChange(nil) }
        return waiter
    }

    private static func withdraw(_ waiter: Waiter) {
        switch waiter {
        case .confirm(let c): c.resume(throwing: HumanPromptError.withdrawn)
        case .answer(let c): c.resume(throwing: HumanPromptError.withdrawn)
        }
    }
}
