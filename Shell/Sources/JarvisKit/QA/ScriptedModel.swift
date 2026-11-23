import Foundation
import JarvisCore
import Synchronization

/// Which on-screen element a script step means, matched against the prompt's element lines
/// the way a model reads them: by label (or placeholder), optionally by role and value.
public struct ElementQuery: Sendable, Equatable, CustomStringConvertible {
    /// How `text` is compared (always case-insensitive).
    public enum Match: Sendable, Equatable {
        /// The whole label.
        case exact
        /// The label starts with `text` followed by a non-letter/digit or the end
        /// (`"Wi-Fi"` matches `"Wi-Fi HomeNet"`, not `"Wi-Fiber"`).
        case prefix
        /// `text` anywhere in the label.
        case contains
    }

    /// The text to find.
    public var text: String
    /// How to compare it.
    public var match: Match
    /// Accepted roles as rendered (`Button`, `Cell`, …); empty accepts any.
    public var roles: [String]
    /// The element's rendered `value=` must equal this, when set (text fields, sliders;
    /// not switches, which the core renders as `(on)` / `(off)`).
    public var value: String?
    /// The element must be a switch that is on (`true`) or off (`false`), when set.
    public var isOn: Bool?

    public init(_ text: String, match: Match = .prefix, roles: [String] = [], value: String? = nil, isOn: Bool? = nil) {
        self.text = text
        self.match = match
        self.roles = roles
        self.value = value
        self.isOn = isOn
    }

    /// Whether the query states a precondition on the element's state.
    var hasStateCondition: Bool { value != nil || isOn != nil }

    public var description: String {
        var s = roles.isEmpty ? "" : roles.joined(separator: "|") + " "
        s += "\"\(text)\""
        switch match {
        case .exact: s += " (exact)"
        case .prefix: break
        case .contains: s += " (contains)"
        }
        if let value { s += " value=\"\(value)\"" }
        if let isOn { s += isOn ? " (on)" : " (off)" }
        return s
    }

    /// Whether `element` matches.
    public func matches(_ element: PromptScreen.Element) -> Bool {
        if !roles.isEmpty && !roles.contains(element.role) { return false }
        if let value, element.value != value { return false }
        if let isOn, element.isOn != isOn { return false }
        let needle = text.lowercased()
        return [element.label, element.placeholder ?? ""].contains { candidate in
            let hay = candidate.lowercased()
            guard !hay.isEmpty else { return false }
            switch match {
            case .exact:
                return hay == needle
            case .prefix:
                guard hay.hasPrefix(needle) else { return false }
                guard let next = hay.dropFirst(needle.count).first else { return true }
                return !(next.isLetter || next.isNumber)
            case .contains:
                return hay.contains(needle)
            }
        }
    }

    /// Roles a model would rather act on than plain text with the same words.
    static let interactiveRoles: Set<String> = [
        "Button", "Link", "Cell", "Switch", "TextField", "SecureTextField", "SearchField",
        "TextView", "Slider", "Picker", "Tab",
    ]

    /// The element this query picks on `screen`: interactive matches first, then any.
    public func find(in screen: PromptScreen) -> PromptScreen.Element? {
        let all = screen.elements.filter(matches)
        return all.first { Self.interactiveRoles.contains($0.role) } ?? all.first
    }
}

/// One step of a scripted task (see `ScriptedModel`).
public enum ScriptStep: Sendable, Equatable, CustomStringConvertible {
    /// Tap the element.
    case tap(ElementQuery)
    /// Type into the element (`submit` presses Return).
    case type(ElementQuery, text: String, submit: Bool)
    /// Scroll in `direction` until the element is listed (no action once it is).
    case scrollTo(ElementQuery, direction: String, maxScrolls: Int)
    /// `open_app` by name or bundle id.
    case openApp(String)
    /// `open_url`.
    case openURL(String)
    /// `press_home`.
    case pressHome
    /// `wait`.
    case wait(ms: Int)
    /// `done` once any of `expecting` is listed (at once when empty).
    case done(summary: String, expecting: [ElementQuery])
    /// `fail`.
    case fail(reason: String)

    // Builders that read like the step they are.

    public static func tap(_ text: String, role: String? = nil, match: ElementQuery.Match = .prefix, value: String? = nil, isOn: Bool? = nil) -> ScriptStep {
        .tap(ElementQuery(text, match: match, roles: role.map { [$0] } ?? [], value: value, isOn: isOn))
    }

    public static func type(_ text: String, into label: String, role: String? = nil, submit: Bool = false) -> ScriptStep {
        .type(ElementQuery(label, roles: role.map { [$0] } ?? []), text: text, submit: submit)
    }

    public static func scroll(to label: String, role: String? = nil, direction: String = "down", maxScrolls: Int = 20) -> ScriptStep {
        .scrollTo(ElementQuery(label, roles: role.map { [$0] } ?? []), direction: direction, maxScrolls: maxScrolls)
    }

    public static func done(_ summary: String, when labels: [String] = [], match: ElementQuery.Match = .prefix) -> ScriptStep {
        .done(summary: summary, expecting: labels.map { ElementQuery($0, match: match) })
    }

    public var description: String {
        switch self {
        case .tap(let q): "tap \(q)"
        case .type(let q, let text, let submit): "type \"\(text)\" into \(q)\(submit ? " +return" : "")"
        case .scrollTo(let q, let d, let n): "scroll \(d) to \(q) (max \(n))"
        case .openApp(let app): "open_app \"\(app)\""
        case .openURL(let url): "open_url \"\(url)\""
        case .pressHome: "press_home"
        case .wait(let ms): "wait \(ms)ms"
        case .done(let summary, let expecting):
            expecting.isEmpty ? "done \"\(summary)\"" : "done \"\(summary)\" when any of \(expecting.map(\.description).joined(separator: ", "))"
        case .fail(let reason): "fail \"\(reason)\""
        }
    }
}

/// A prompt that is not part of the task (first-launch sheet, onboarding card) and the
/// button that dismisses it. Narrow on purpose: it fires only while every `context`
/// element is listed too (so "Close" on an onboarding sheet is not "Close" in a toolbar),
/// at most `maxUses` times, and never while the current step's target is listed.
public struct DismissRule: Sendable, Equatable, CustomStringConvertible {
    /// The element to tap.
    public var tap: ElementQuery
    /// Elements that identify the surface; all must be listed.
    public var context: [ElementQuery]
    /// Times this rule may fire in one run.
    public var maxUses: Int

    public init(tap: ElementQuery, when context: [ElementQuery], maxUses: Int = 1) {
        self.tap = tap
        self.context = context
        self.maxUses = max(0, maxUses)
    }

    public var description: String {
        "tap \(tap) when " + (context.isEmpty ? "(always)" : context.map(\.description).joined(separator: " and "))
    }

    /// The element to tap on `screen`, when the rule applies there.
    public func match(_ screen: PromptScreen) -> PromptScreen.Element? {
        guard context.allSatisfy({ $0.find(in: screen) != nil }) else { return nil }
        return tap.find(in: screen)
    }
}

/// A deterministic `ModelProvider` for QA: it answers each request by reading the rendered
/// screen in the prompt (`PromptScreen`) and picking the element whose label matches the
/// current script step, exactly as a model would choose an index. It never sees the tree.
///
/// Behaviour per request, in order:
/// 1. When the last step was declined by the owner, reply `options.onDeclined`.
/// 2. When a `DismissRule` matches (its element and its whole context are listed, it has
///    uses left, and the current step's target is not listed), tap it; at most
///    `options.maxDismissals` dismissals per run.
/// 3. A tap/type step counts as done only from the prompt: the next request's last
///    "Steps so far" line must say `ok` (`ok; changed: …`, `ok; now in …`, … — the
///    suffix is ignored) or `done, but …` while the step's stated precondition (such as
///    "the switch is on") no longer holds. When it says the action failed,
///    was not done, was dropped or its result is unknown (`interrupted: …` after a lock),
///    the step is retried, unless its precondition no longer holds on screen (it took
///    effect) or its target is gone. A step the core reports as never run (`not done`,
///    `not answered`) whose target is hidden while the next step's target is not listed
///    either (a sheet or tip over the screen) is gone back to: it waits for its target,
///    and dismiss rules may clear the cover. At most `options.maxRetries` retries per step, then the
///    script fails with the reason.
/// 4. Run the current step. A missing target waits (`wait` actions) up to
///    `options.patience` times, then fails with a reason that lists what was on screen.
///
/// `decisions` records why each reply was given (for transcripts).
public final class ScriptedModel: ModelProvider {
    /// Tunables.
    public struct Options: Sendable, Equatable {
        /// Prompts that are not part of the task, and how to dismiss them.
        public var dismiss: [DismissRule]
        /// Most dismiss taps per run.
        public var maxDismissals: Int
        /// Waits for a missing target before failing.
        public var patience: Int
        /// Length of each such wait.
        public var waitMs: Int
        /// The reply after a declined confirmation.
        public var onDeclined: ScriptStep
        /// Retries of one tap/type step whose result the prompt reports as not done.
        public var maxRetries: Int

        public init(
            dismiss: [DismissRule] = [],
            maxDismissals: Int = 3,
            patience: Int = 4,
            waitMs: Int = 700,
            onDeclined: ScriptStep = .fail(reason: "the owner declined"),
            maxRetries: Int = 3
        ) {
            self.dismiss = dismiss
            self.maxDismissals = maxDismissals
            self.patience = patience
            self.waitMs = waitMs
            self.onDeclined = onDeclined
            self.maxRetries = max(0, maxRetries)
        }
    }

    /// Why one reply was given.
    public struct Decision: Sendable, Equatable {
        /// 1-based call number.
        public var call: Int
        /// 0-based step index the reply belongs to (`nil` for dismissals and declines).
        public var step: Int?
        /// The reply JSON.
        public var reply: String
        /// Plain-language reason.
        public var reason: String
    }

    private struct State {
        var cursor = 0
        var waits = 0
        var scrolls = 0
        var dismissals = 0
        var ruleUses: [Int: Int] = [:]
        /// Retries per step index.
        var retries: [Int: Int] = [:]
        /// The tap/type reply whose result the next prompt reports.
        var pending: (step: Int, query: ElementQuery)?
        /// Extra scrolls made because the core said a target was covered, per step index.
        var coverScrolls: [Int: Int] = [:]
        /// Consecutive replies found never to have run (the prompt's history did not grow).
        var dropStreak = 0
        /// The reply issued last: the step number the prompt had when it was chosen, and the
        /// state before it (decisions left out), to rewind to if the core dropped it.
        var lastIssued: [Issued] = []   // zero or one (an array, as State holds a State)
        var decisions: [Decision] = []
    }

    private struct Issued {
        var historyNumber: Int
        var before: State
    }

    /// Most extra scrolls per step when the core says the target is covered.
    static let maxCoverScrolls = 3
    /// Most times one reply is issued again after the core dropped it.
    static let maxReissues = 3

    public let capabilities: ModelCapabilities
    /// The script.
    public let steps: [ScriptStep]
    /// The tunables.
    public let options: Options
    private let state = Mutex(State())

    public init(
        steps: [ScriptStep],
        options: Options = Options(),
        capabilities: ModelCapabilities = ModelCapabilities(contextTokens: 8_192, supportsVision: false)
    ) {
        self.steps = steps
        self.options = options
        self.capabilities = capabilities
    }

    /// Every reply so far with its reason.
    public var decisions: [Decision] { state.withLock { $0.decisions } }
    /// Model calls so far.
    public var callCount: Int { state.withLock { $0.decisions.count } }
    /// Steps completed (the cursor).
    public var completedSteps: Int { state.withLock { $0.cursor } }

    public func generate(_ request: ModelRequest) async throws -> String {
        let screen = PromptScreen(prompt: request.user)
        return state.withLock { s in
            let call = s.decisions.count + 1
            let number = screen.historyNumber
            var note = ""
            // Every reply the core executes (or refuses, or sets aside) adds a "Steps so far"
            // line. When none was added the reply never ran: the phone locked while the model
            // was answering and the core dropped the late reply, to ask again on the fresh
            // screen (Locked run: the Switch tap was dropped, the script had already moved on
            // and went on to `done`). Rewind to the state before that reply and decide again.
            // A prompt without any "Steps so far" says nothing (nothing ran yet, or the history
            // is not shown): never judged.
            if let issued = s.lastIssued.first, number > 0, number <= issued.historyNumber {
                let streak = s.dropStreak + 1
                guard streak <= Self.maxReissues else {
                    let at = issued.before.cursor < steps.count ? "step \(issued.before.cursor + 1)/\(steps.count) (\(steps[issued.before.cursor]))" : "the script"
                    let reply = failJSON("\(at): the reply was issued \(Self.maxReissues) times and never ran (the prompt's steps did not grow past \(number))", screen)
                    s.lastIssued = []
                    s.decisions.append(Decision(call: call, step: nil, reply: reply, reason: "\(at): giving up after \(Self.maxReissues) re-issues"))
                    return reply
                }
                let kept = s.decisions
                s = issued.before
                s.decisions = kept
                s.dropStreak = streak
                note = "re-issued (\(streak)/\(Self.maxReissues)): the previous reply never ran; "
            } else {
                s.dropStreak = 0
            }
            var before = s
            before.decisions = []
            before.lastIssued = []
            let (step, reply, reason) = decide(screen, &s)
            s.lastIssued = [Issued(historyNumber: number, before: before)]
            s.decisions.append(Decision(call: call, step: step, reply: reply, reason: note + reason))
            return reply
        }
    }

    /// Whether `result` is the core's refusal of an action whose target is under another
    /// control, a bar or the keyboard. The core's wording (`covered_message` in
    /// `core/jarvis-agent/src/session.rs`): `not done: element 26 (Item 60) is covered by the
    /// tab bar: a tap there could land on it instead, so nothing was sent; choose another
    /// control or scroll first`.
    static func isCoveredRefusal(_ result: String) -> Bool {
        let r = result.lowercased()
        return StepOutcome(result: result) == .notDone && r.contains("is covered by") && r.contains("scroll first")
    }

    /// The direction to scroll for a covered target of step `index`: that of the nearest
    /// `scroll(to:)` step before it, else down.
    private func coverScrollDirection(for index: Int) -> String {
        var i = min(index, steps.count) - 1
        while i >= 0 {
            if case .scrollTo(_, let direction, _) = steps[i] { return direction }
            i -= 1
        }
        return "down"
    }

    // MARK: - Deciding

    private func decide(_ screen: PromptScreen, _ s: inout State) -> (Int?, String, String) {
        let last = screen.lastResult ?? ""
        let pending = s.pending
        s.pending = nil
        let outcome = StepOutcome(result: last)
        if outcome == .notDone || outcome == .failed,
           last.contains("owner declined") || last.contains("does not want Jarvis to act") || last.contains("did not let Jarvis act") {
            let (reply, _) = render(options.onDeclined, on: screen, thought: "the owner said no")
            return (nil, reply, "last step was declined (\(last)); replying \(options.onDeclined)")
        }

        var current = s.cursor < steps.count ? steps[s.cursor] : nil
        var currentTargetListed = current.map(Self.queries)?.contains { $0.find(in: screen) != nil } ?? false

        // The core refused the tap/type because its target sits under a bar (it was listed
        // at the very edge of the screen): scroll once more, then tap again.
        if let pending, Self.isCoveredRefusal(last) {
            let label = "step \(pending.step + 1)/\(steps.count) (\(steps[pending.step]))"
            let extra = s.coverScrolls[pending.step, default: 0]
            guard extra < Self.maxCoverScrolls else {
                return (pending.step, failJSON("\(label): still covered after \(extra) extra scrolls (last result: \(last))", screen),
                        "\(label): giving up after \(extra) extra scrolls")
            }
            s.coverScrolls[pending.step] = extra + 1
            s.cursor = pending.step
            s.waits = 0
            let direction = coverScrollDirection(for: pending.step)
            return (pending.step, Self.json(["action": "scroll", "direction": .string(direction)], thought: "uncover the target"),
                    "\(label): the target is covered; extra scroll \(extra + 1)/\(Self.maxCoverScrolls) \(direction), then tap again")
        }

        // Did the last tap/type take effect? Only the prompt says.
        if let pending, Self.needsRetry(result: last, query: pending.query, on: screen, nextTargetListed: currentTargetListed) {
            let label = "step \(pending.step + 1)/\(steps.count) (\(steps[pending.step]))"
            let done = s.retries[pending.step, default: 0]
            guard done < options.maxRetries else {
                return (pending.step, failJSON("\(label): still not done after \(done) retries (last result: \(last))", screen),
                        "\(label): giving up after \(done) retries")
            }
            s.retries[pending.step] = done + 1
            s.cursor = pending.step
            s.waits = 0
            current = steps[pending.step]
            currentTargetListed = true
        } else if let pending, Self.mustWaitForTarget(result: last, query: pending.query, on: screen, nextTargetListed: currentTargetListed) {
            // The core says the step never ran, and something (a sheet, a tip) covers its
            // target: the step is still to do. Go back to it, so it waits for its target
            // (and dismiss rules may clear what covers it) instead of moving on with the
            // step unfinished (Safari form run: a tip appeared between the first-use
            // approval and the typing, the core dropped the typing, the script went on to
            // the email and the form was sent without a name).
            let label = "step \(pending.step + 1)/\(steps.count) (\(steps[pending.step]))"
            let done = s.retries[pending.step, default: 0]
            guard done < options.maxRetries else {
                return (pending.step, failJSON("\(label): still not done after \(done) retries (last result: \(last))", screen),
                        "\(label): giving up after \(done) retries")
            }
            s.retries[pending.step] = done + 1
            s.cursor = pending.step
            s.waits = 0
            current = steps[pending.step]
            currentTargetListed = false
        }
        if !currentTargetListed, s.dismissals < options.maxDismissals {
            for (i, rule) in options.dismiss.enumerated() where s.ruleUses[i, default: 0] < rule.maxUses {
                if let e = rule.match(screen) {
                    s.dismissals += 1
                    s.ruleUses[i, default: 0] += 1
                    return (nil, Self.json(["action": "tap", "element": .number(Double(e.index))], thought: "dismiss \(e.label)"),
                            "dismiss rule \(rule) matched \(e.line)")
                }
            }
        }

        while s.cursor < steps.count {
            let index = s.cursor
            let step = steps[index]
            let label = "step \(index + 1)/\(steps.count) (\(step))"
            switch step {
            case .scrollTo(let query, let direction, let maxScrolls):
                // Listed is not enough while the core says the last target was covered.
                let covered = Self.isCoveredRefusal(last)
                if query.find(in: screen) != nil, !(covered && s.scrolls < maxScrolls) {
                    s.cursor += 1
                    s.scrolls = 0
                    continue
                }
                guard s.scrolls < maxScrolls else {
                    return (index, failJSON("\(label): not listed after \(maxScrolls) scrolls", screen),
                            "\(label): giving up after \(maxScrolls) scrolls")
                }
                s.scrolls += 1
                return (index, Self.json(["action": "scroll", "direction": .string(direction)], thought: "look further \(direction)"),
                        "\(label): not listed yet, scroll \(s.scrolls)/\(maxScrolls)")

            case .tap(let query), .type(let query, _, _):
                guard let element = query.find(in: screen) else {
                    return waitOrFail(index, label, "no element matches \(query)", screen, &s)
                }
                s.cursor += 1
                s.waits = 0
                s.pending = (index, query)
                var object: [String: JSONValue] = ["element": .number(Double(element.index))]
                if case .type(_, let text, let submit) = step {
                    object["action"] = "type"
                    object["text"] = .string(text)
                    object["submit"] = .bool(submit)
                } else {
                    object["action"] = "tap"
                }
                return (index, Self.json(object, thought: "\(step)"), "\(label): matched \(element.line)")

            case .done(_, let expecting):
                let found = expecting.lazy.compactMap { $0.find(in: screen) }.first
                guard expecting.isEmpty || found != nil else {
                    return waitOrFail(index, label, "none of \(expecting.map(\.description).joined(separator: ", ")) is listed", screen, &s)
                }
                s.cursor += 1
                let (reply, _) = render(step, on: screen, thought: "finished")
                return (index, reply, "\(label): " + (found.map { "found \($0.line)" } ?? "nothing to check"))

            default:
                s.cursor += 1
                s.waits = 0
                let (reply, _) = render(step, on: screen, thought: "\(step)")
                return (index, reply, label)
            }
        }
        return (nil, failJSON("the script has no step left", screen), "script exhausted")
    }

    private func waitOrFail(_ index: Int, _ label: String, _ problem: String, _ screen: PromptScreen, _ s: inout State) -> (Int?, String, String) {
        guard s.waits < options.patience else {
            return (index, failJSON("\(label): \(problem) after \(options.patience) waits", screen), "\(label): giving up")
        }
        s.waits += 1
        return (index, Self.json(["action": "wait", "ms": .number(Double(options.waitMs))], thought: "waiting for the screen"),
                "\(label): \(problem); wait \(s.waits)/\(options.patience)")
    }

    /// The reply for a step that needs no element (or the final reply of `done`).
    private func render(_ step: ScriptStep, on screen: PromptScreen, thought: String) -> (String, Bool) {
        switch step {
        case .openApp(let app): (Self.json(["action": "open_app", "app": .string(app)], thought: thought), true)
        case .openURL(let url): (Self.json(["action": "open_url", "url": .string(url)], thought: thought), true)
        case .pressHome: (Self.json(["action": "press_home"], thought: thought), true)
        case .wait(let ms): (Self.json(["action": "wait", "ms": .number(Double(ms))], thought: thought), true)
        case .done(let summary, _): (Self.json(["action": "done", "summary": .string(summary)], thought: thought), true)
        case .fail(let reason): (Self.json(["action": "fail", "reason": .string(reason)], thought: thought), true)
        case .tap, .type, .scrollTo: (failJSON("internal: \(step) needs an element", screen), false)
        }
    }

    /// A `fail` reply whose reason names what was on screen, so a failing run explains itself.
    private func failJSON(_ problem: String, _ screen: PromptScreen) -> String {
        let listed = screen.elements.prefix(40).map(\.line).joined(separator: "; ")
        var reason = "\(problem). Screen: \(screen.app ?? "?") — \(listed.isEmpty ? "(no elements)" : listed)"
        if let last = screen.history.last { reason += ". Last step: \(last)" }
        return Self.json(["action": "fail", "reason": .string(reason)], thought: "cannot continue")
    }

    /// Whether a tap/type whose history result is `result` must run again on `screen`.
    ///
    /// - `ok` (with or without a change summary): done.
    /// - `done, but …` (ran, the lock came before the check): done unless the stated
    ///   precondition (`value=` / on / off) still holds.
    /// - `owner answered: …` (the core gave the step to the owner, who replied): done. What
    ///   the owner did is theirs; typing into a password field again would only ask again.
    /// - anything else (`failed: …`, `not done: …`, `interrupted: …`, `not answered: …`):
    ///   retried while the target is still there and its precondition, if any, still holds.
    ///   A plain failure without a precondition is not retried once the next step's target
    ///   is listed (the screen moved on).
    static func needsRetry(result: String, query: ElementQuery, on screen: PromptScreen, nextTargetListed: Bool) -> Bool {
        if result.trimmingCharacters(in: .whitespaces).isEmpty { return false }
        let targetThere = query.find(in: screen) != nil
        switch StepOutcome(result: result) {
        case .ok, .ownerAnswered: return false
        case .ranUnchecked: return query.hasStateCondition && targetThere
        case .failed:
            if query.hasStateCondition { return targetThere }
            return nextTargetListed ? false : targetThere
        case .notDone, .interrupted, .notAnswered, .unknown:
            return targetThere
        }
    }

    /// Whether a tap/type the core reports as never run (`not done: …`, `not answered: …`)
    /// is still to do although its target is not listed now: neither its target nor the
    /// next step's target is on `screen` (something covers the screen). With the next
    /// step's target listed the screen has moved on and the script moves with it.
    static func mustWaitForTarget(result: String, query: ElementQuery, on screen: PromptScreen, nextTargetListed: Bool) -> Bool {
        switch StepOutcome(result: result) {
        case .notDone, .notAnswered:
            return query.find(in: screen) == nil && !nextTargetListed
        default:
            return false
        }
    }

    /// The elements whose presence means the step can proceed.
    private static func queries(_ step: ScriptStep) -> [ElementQuery] {
        switch step {
        case .tap(let q), .type(let q, _, _), .scrollTo(let q, _, _): [q]
        case .done(_, let expecting): expecting
        default: []
        }
    }

    static func json(_ object: [String: JSONValue], thought: String) -> String {
        var object = object
        object["thought"] = .string(thought)
        guard let data = try? JSONValue.object(object).encoded(), let text = String(data: data, encoding: .utf8) else {
            return #"{"action":"fail","reason":"could not encode the reply"}"#
        }
        return text
    }
}
