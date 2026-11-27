import Foundation
import JarvisCore

/// What the owner edits in the workflow form. Becomes a core `Workflow` in
/// `WorkflowLibrary.save(_:)`; every rule about it is checked by the core there.
public struct WorkflowDraft: Sendable, Equatable {
    /// `nil` for a new workflow (an id is made from the name).
    public var id: String?
    public var name: String
    public var enabled: Bool
    public var trigger: TriggerDraft
    public var steps: [StepDraft]
    public var onFailure: OnFailure

    public init(id: String? = nil, name: String = "", enabled: Bool = true, trigger: TriggerDraft = TriggerDraft(),
                steps: [StepDraft] = [StepDraft()], onFailure: OnFailure = .stop) {
        self.id = id
        self.name = name
        self.enabled = enabled
        self.trigger = trigger
        self.steps = steps
        self.onFailure = onFailure
    }

    /// The draft for editing `workflow`.
    public init(workflow: Workflow) {
        self.init(id: workflow.id, name: workflow.name, enabled: workflow.enabled,
                  trigger: TriggerDraft(workflow.trigger), steps: workflow.steps.map(StepDraft.init),
                  onFailure: workflow.onFailure)
    }

    /// Moves the step at `index` up (`offset` -1) or down (+1); out of range does nothing.
    public mutating func moveStep(at index: Int, by offset: Int) {
        let target = index + offset
        guard steps.indices.contains(index), steps.indices.contains(target) else { return }
        steps.swapAt(index, target)
    }

    public var coreSteps: [Step] { steps.map(\.core) }
}

/// The trigger part of the form: every kind's fields are kept while the owner switches kinds.
public struct TriggerDraft: Sendable, Equatable {
    public enum Kind: String, Sendable, CaseIterable, Identifiable {
        case manual, phrase, schedule, external

        public var id: String { rawValue }

        public var title: String {
            switch self {
            case .manual: "By hand"
            case .phrase: "A phrase"
            case .schedule: "A schedule"
            case .external: "Shortcuts"
            }
        }
    }

    public var kind: Kind
    public var phrase: String
    /// Monday first.
    public var days: Set<WeekdayOption>
    public var hour: Int
    public var minute: Int
    public var key: String

    public init(kind: Kind = .manual, phrase: String = "", days: Set<WeekdayOption> = Set(WeekdayOption.weekdays),
                hour: Int = 8, minute: Int = 0, key: String = "") {
        self.kind = kind
        self.phrase = phrase
        self.days = days
        self.hour = hour
        self.minute = minute
        self.key = key
    }

    public init(_ trigger: Trigger) {
        self.init()
        switch trigger {
        case .manual:
            kind = .manual
        case .phrase(let text):
            kind = .phrase
            phrase = text
        case .schedule(let s):
            kind = .schedule
            days = Set(s.days.map(WeekdayOption.init))
            hour = Int(s.hour)
            minute = Int(s.minute)
        case .external(let k):
            kind = .external
            key = k
        }
    }

    /// The core trigger. Hours and minutes outside a byte are passed as 255 so the core
    /// refuses them with its own message (never clamped here).
    public var core: Trigger {
        switch kind {
        case .manual: .manual
        case .phrase: .phrase(text: phrase)
        case .schedule:
            .schedule(schedule: Schedule(days: WeekdayOption.allCases.filter(days.contains).map(\.core),
                                         hour: UInt8(exactly: hour) ?? .max, minute: UInt8(exactly: minute) ?? .max))
        case .external: .external(key: key.trimmingCharacters(in: .whitespaces))
        }
    }
}

/// Codable, ordered mirror of `Weekday` (Monday first).
public enum WeekdayOption: Int, Sendable, CaseIterable, Identifiable, Comparable, Codable {
    case mon, tue, wed, thu, fri, sat, sun

    public var id: Int { rawValue }

    public static let weekdays: [WeekdayOption] = [.mon, .tue, .wed, .thu, .fri]

    public static func < (a: Self, b: Self) -> Bool { a.rawValue < b.rawValue }

    public init(_ day: Weekday) {
        switch day {
        case .mon: self = .mon
        case .tue: self = .tue
        case .wed: self = .wed
        case .thu: self = .thu
        case .fri: self = .fri
        case .sat: self = .sat
        case .sun: self = .sun
        }
    }

    public var core: Weekday {
        switch self {
        case .mon: .mon
        case .tue: .tue
        case .wed: .wed
        case .thu: .thu
        case .fri: .fri
        case .sat: .sat
        case .sun: .sun
        }
    }

    public var short: String {
        switch self {
        case .mon: "Mon"
        case .tue: "Tue"
        case .wed: "Wed"
        case .thu: "Thu"
        case .fri: "Fri"
        case .sat: "Sat"
        case .sun: "Sun"
        }
    }
}

/// One row of the steps list in the form. Each kind's fields are kept while switching.
public struct StepDraft: Sendable, Equatable, Identifiable {
    public enum Kind: String, Sendable, CaseIterable, Identifiable {
        case runSkill, runTask, stopUnless, askOwner, notify, wait

        public var id: String { rawValue }

        /// The node label on the workflow page ("Run skill", "Only continue if", …).
        public var title: String {
            switch self {
            case .runSkill: "Run skill"
            case .runTask: "Task"
            case .stopUnless: "Only continue if"
            case .askOwner: "Ask you"
            case .notify: "Notify"
            case .wait: "Wait"
            }
        }
    }

    public enum ConditionKind: String, Sendable, CaseIterable, Identifiable {
        case lastSucceeded, ownerSaidYes, lastSummaryContains

        public var id: String { rawValue }

        public var title: String {
            switch self {
            case .lastSucceeded: "That worked"
            case .ownerSaidYes: "You said yes"
            case .lastSummaryContains: "The result mentions…"
            }
        }
    }

    public var id = UUID()
    public var kind: Kind
    public var skillId: String
    public var args: [String: String]
    /// Task text, question or notification text (one field, kept across kinds).
    public var text: String
    public var waitSeconds: Int
    public var condition: ConditionKind
    public var conditionText: String

    public init(kind: Kind = .runTask, skillId: String = "", args: [String: String] = [:], text: String = "",
                waitSeconds: Int = 60, condition: ConditionKind = .lastSucceeded, conditionText: String = "") {
        self.kind = kind
        self.skillId = skillId
        self.args = args
        self.text = text
        self.waitSeconds = waitSeconds
        self.condition = condition
        self.conditionText = conditionText
    }

    public init(_ step: Step) {
        self.init()
        switch step {
        case .runSkill(let id, let a):
            kind = .runSkill
            skillId = id
            args = a
        case .runTask(let t):
            kind = .runTask
            text = t
        case .askOwner(let q):
            kind = .askOwner
            text = q
        case .notify(let t):
            kind = .notify
            text = t
        case .wait(let ms):
            kind = .wait
            waitSeconds = Int(clamping: ms / 1_000)
            if ms % 1_000 != 0 { waitSeconds += 1 }
        case .stopUnless(let c):
            kind = .stopUnless
            switch c {
            case .lastSucceeded: condition = .lastSucceeded
            case .ownerSaidYes: condition = .ownerSaidYes
            case .lastSummaryContains(let t):
                condition = .lastSummaryContains
                conditionText = t
            }
        }
    }

    /// Equality ignores the row identity.
    public static func == (a: Self, b: Self) -> Bool {
        a.kind == b.kind && a.skillId == b.skillId && a.args == b.args && a.text == b.text
            && a.waitSeconds == b.waitSeconds && a.condition == b.condition && a.conditionText == b.conditionText
    }

    /// The core step. Negative waits go to the core as 0 so it refuses them itself.
    public var core: Step {
        switch kind {
        case .runSkill:
            .runSkill(skillId: skillId, args: args.filter { !$0.value.trimmingCharacters(in: .whitespaces).isEmpty })
        case .runTask: .runTask(text: text)
        case .askOwner: .askOwner(question: text)
        case .notify: .notify(text: text)
        case .wait: .wait(ms: UInt64(max(0, waitSeconds)) * 1_000)
        case .stopUnless:
            switch condition {
            case .lastSucceeded: .stopUnless(condition: .lastSucceeded)
            case .ownerSaidYes: .stopUnless(condition: .ownerSaidYes)
            case .lastSummaryContains: .stopUnless(condition: .lastSummaryContains(text: conditionText))
            }
        }
    }
}

extension Step {
    /// The kind, for the node chain.
    public var kind: StepDraft.Kind {
        switch self {
        case .runSkill: .runSkill
        case .runTask: .runTask
        case .askOwner: .askOwner
        case .notify: .notify
        case .wait: .wait
        case .stopUnless: .stopUnless
        }
    }
}

/// Where a core message about a workflow belongs on the form.
public enum WorkflowField: Sendable, Equatable, Hashable {
    case name, trigger, steps, general
    /// One step row (0-based).
    case step(Int)

    /// Reads which field a core validation message names (`name: …`, `trigger.text: …`,
    /// `steps[2].text: …`). Routing only: the validation is the core's.
    public init(coreMessage message: String) {
        let m = message.lowercased()
        if m.hasPrefix("name") {
            self = .name
        } else if m.hasPrefix("trigger") {
            self = .trigger
        } else if m.hasPrefix("steps["), let close = m.firstIndex(of: "]"),
                  let n = Int(m[m.index(m.startIndex, offsetBy: 6)..<close]) {
            self = .step(n)
        } else if m.hasPrefix("steps") || m.hasPrefix("invalid id") {
            // An id other than the generated one can only be a step's skill.
            self = .steps
        } else {
            self = .general
        }
    }
}
