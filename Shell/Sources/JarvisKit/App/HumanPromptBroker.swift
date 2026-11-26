import Foundation
import JarvisCore
import Synchronization

/// Something the owner must answer: a confirmation, a question, or a yes/no question from
/// a workflow's "Ask you" step.
public enum HumanPrompt: Sendable, Equatable, Identifiable {
    case confirm(id: Int, request: ConfirmRequest)
    case question(id: Int, text: String)
    case yesNo(id: Int, text: String)

    public var id: Int {
        switch self {
        case .confirm(let id, _), .question(let id, _), .yesNo(let id, _): id
        }
    }
}

/// Where a decision about the confirmation on screen can be applied (remote control's
/// `decide` goes through this, so it can only answer the prompt the owner sees).
public protocol ConfirmationDesk: Sendable {
    /// The confirmation waiting for the owner, if the pending prompt is one.
    var pendingConfirmation: (id: Int, request: ConfirmRequest)? { get }
    /// The highest prompt id handed out so far (0 before the first prompt).
    var latestPromptId: Int { get }
    /// The recent confirmation prompts with an id above `id`, oldest first (whether or not
    /// they are still pending). Remote control binds each request it announces to exactly
    /// one of these.
    func confirmations(after id: Int) -> [(id: Int, request: ConfirmRequest)]
    /// Answers confirmation `id` for a paired device (review pass 35: an answer given here
    /// is remote, never kept); false when it is no longer pending.
    @discardableResult
    func resolveConfirmation(_ id: Int, approved: Bool) -> Bool
}

/// A yes/no question for the owner (a workflow's "Ask you" step).
public protocol OwnerQuestioner: Sendable {
    /// Throws when the question was withdrawn (the run ended, the UI went away).
    func askYesNo(_ question: String) async throws -> Bool
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
///
/// A prompt whose asking task is cancelled is withdrawn by its own id, and only that one:
/// when the driver abandons a confirmation or a question because the phone locked (or the
/// wait timed out), its prompt leaves the screen at that moment, and a newer prompt is never
/// touched by it. `withdraw(_:)` does the same for a caller that holds an id.
public final class HumanPromptBroker: HumanGate, ConfirmationDesk, OwnerQuestioner {
    private enum Waiter {
        case confirm(CheckedContinuation<OwnerDecision, any Error>)
        case answer(CheckedContinuation<String, any Error>)
    }

    private struct State {
        var nextID = 1
        var prompt: HumanPrompt?
        var waiter: Waiter?
        /// Ids handed out whose prompt is not installed yet; an id cancelled meanwhile is
        /// removed, and its install then withdraws at once.
        var reserved: Set<Int> = []
        /// The latest confirmation prompts (bounded), for `confirmations(after:)`.
        var recentConfirmations: [(id: Int, request: ConfirmRequest)] = []
    }

    /// How many confirmation prompts `confirmations(after:)` remembers.
    static let confirmationHistory = 32

    private let state = Mutex(State())
    private let onChange: @Sendable (HumanPrompt?) -> Void

    /// `onChange` is called with the new pending prompt (`nil` when none), from any thread.
    public init(onChange: @escaping @Sendable (HumanPrompt?) -> Void = { _ in }) {
        self.onChange = onChange
    }

    /// The prompt waiting for the owner.
    public var pending: HumanPrompt? { state.withLock { $0.prompt } }

    public func confirm(_ request: ConfirmRequest) async throws -> Bool {
        try await decide(request).approved
    }

    /// Review pass 35 (F4): `approve`/`decline` (Jarvis's own sheet) answer on the phone,
    /// `resolveConfirmation` (remote control's desk) for a paired device.
    public func decide(_ request: ConfirmRequest) async throws -> OwnerDecision {
        let id = reserve()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (c: CheckedContinuation<OwnerDecision, any Error>) in
                install(id, .confirm(id: id, request: request), .confirm(c))
            }
        } onCancel: {
            self.withdraw(id)
        }
    }

    public func answer(_ question: String) async throws -> String {
        let id = reserve()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (c: CheckedContinuation<String, any Error>) in
                install(id, .question(id: id, text: question), .answer(c))
            }
        } onCancel: {
            self.withdraw(id)
        }
    }

    public func askYesNo(_ question: String) async throws -> Bool {
        let id = reserve()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (c: CheckedContinuation<OwnerDecision, any Error>) in
                install(id, .yesNo(id: id, text: question), .confirm(c))
            }
        } onCancel: {
            self.withdraw(id)
        }.approved
    }

    public var latestPromptId: Int { state.withLock { $0.nextID - 1 } }

    public func confirmations(after id: Int) -> [(id: Int, request: ConfirmRequest)] {
        state.withLock { s in s.recentConfirmations.filter { $0.id > id } }
    }

    public var pendingConfirmation: (id: Int, request: ConfirmRequest)? {
        guard case .confirm(let id, let request)? = pending else { return nil }
        return (id, request)
    }

    public func resolveConfirmation(_ id: Int, approved: Bool) -> Bool {
        decide(id, OwnerDecision(approved: approved, byPhone: false), kind: .confirm)
    }

    /// Approves confirmation `id` on the phone. Returns false when it is no longer pending.
    @discardableResult
    public func approve(_ id: Int) -> Bool { decide(id, OwnerDecision(approved: true, byPhone: true), kind: .confirm) }

    /// Declines confirmation `id` on the phone.
    @discardableResult
    public func decline(_ id: Int) -> Bool { decide(id, OwnerDecision(approved: false, byPhone: true), kind: .confirm) }

    /// Answers yes/no question `id`.
    @discardableResult
    public func answerYesNo(_ id: Int, yes: Bool) -> Bool {
        decide(id, OwnerDecision(approved: yes, byPhone: true), kind: .yesNo)
    }

    /// Answers question `id`.
    @discardableResult
    public func reply(_ id: Int, _ text: String) -> Bool {
        guard let waiter = take(id, kind: .question), case .answer(let c) = waiter else { return false }
        c.resume(returning: text)
        return true
    }

    /// Withdraws prompt `id` (a confirmation counts as declined) if it is still pending, or
    /// before it is shown if it is not yet. Any other prompt stays. Returns whether `id` was
    /// pending or about to be.
    @discardableResult
    public func withdraw(_ id: Int) -> Bool {
        let (waiter, wasReserved) = state.withLock { s -> (Waiter?, Bool) in
            if s.reserved.remove(id) != nil { return (nil, true) }
            guard let prompt = s.prompt, prompt.id == id else { return (nil, false) }
            defer { s.prompt = nil; s.waiter = nil }
            return (s.waiter, false)
        }
        guard let waiter else { return wasReserved }
        Self.withdraw(waiter)
        onChange(nil)
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

    /// Hands out the id of the next prompt.
    private func reserve() -> Int {
        state.withLock { s in
            let id = s.nextID
            s.nextID += 1
            s.reserved.insert(id)
            return id
        }
    }

    /// Shows `prompt` (id `id`, reserved), replacing and withdrawing the pending one. When
    /// `id` was withdrawn before it was shown, `waiter` is withdrawn at once instead.
    private func install(_ id: Int, _ prompt: HumanPrompt, _ waiter: Waiter) {
        let (shown, previous) = state.withLock { s -> (Bool, Waiter?) in
            guard s.reserved.remove(id) != nil else { return (false, nil) }
            let previous = s.waiter
            s.prompt = prompt
            s.waiter = waiter
            if case .confirm(let id, let request) = prompt {
                s.recentConfirmations.append((id, request))
                if s.recentConfirmations.count > Self.confirmationHistory {
                    s.recentConfirmations.removeFirst(s.recentConfirmations.count - Self.confirmationHistory)
                }
            }
            return (true, previous)
        }
        guard shown else {
            Self.withdraw(waiter)
            return
        }
        if let previous { Self.withdraw(previous) }
        onChange(prompt)
    }

    private enum Kind { case confirm, question, yesNo }

    private func decide(_ id: Int, _ decision: OwnerDecision, kind: Kind) -> Bool {
        guard let waiter = take(id, kind: kind), case .confirm(let c) = waiter else { return false }
        c.resume(returning: decision)
        return true
    }

    /// Removes and returns the waiter of prompt `id` when it is of the asked kind.
    private func take(_ id: Int, kind: Kind) -> Waiter? {
        let waiter = state.withLock { s -> Waiter? in
            guard let prompt = s.prompt, prompt.id == id else { return nil }
            let promptKind: Kind = switch prompt {
            case .confirm: .confirm
            case .question: .question
            case .yesNo: .yesNo
            }
            guard promptKind == kind else { return nil }
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
