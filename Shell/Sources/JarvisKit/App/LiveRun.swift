import Foundation
import JarvisCore

/// What the run view listens to: the driver's updates plus each prompt and reply (the
/// coordinator taps the model for those, so the view can show what Jarvis sees).
public enum RunEvent: Sendable, Equatable {
    case update(DriverUpdate)
    /// The user part of a model request.
    case prompt(String)
    /// The model's reply text.
    case reply(String)
}

/// The state of the run view, reduced from `RunEvent`s. Plain value, tested on Linux; the
/// SwiftUI view only renders it.
public struct LiveRun: Sendable, Equatable {
    public enum Status: String, Sendable, Equatable {
        case standingBy, driving, replaying, needsYou, locked, done, stopped

        public var title: String {
            switch self {
            case .standingBy: "Standing by"
            case .driving: "Driving"
            case .replaying: "Replaying"
            case .needsYou: "Needs you"
            case .locked: "Unlock to continue"
            case .done: "Done"
            case .stopped: "Stopped"
            }
        }
    }

    /// Where a step came from.
    public enum Source: String, Sendable, Equatable {
        case model, replay, tool
    }

    public enum StepState: Sendable, Equatable {
        case running, done, failed(String)
    }

    /// One executed step in the timeline.
    public struct Step: Sendable, Equatable, Identifiable {
        public var id: Int
        public var label: String
        public var source: Source
        public var state: StepState
    }

    public var task: String = ""
    /// The skill the run uses ("Using skill: …"), `nil` for a plain task.
    public var skillName: String?
    public var status: Status = .standingBy
    /// The core's step counter.
    public var stepNumber: Int = 0
    /// The step budget (for "STEP 2 OF 40").
    public var maxSteps: Int = 0
    /// The app the session is in.
    public var app: String?
    /// The element list of the latest prompt ("What Jarvis sees").
    public var sight: [PromptScreen.Element] = []
    /// The `App:` line of that prompt.
    public var sightApp: String?
    /// The element the model chose on it, and the verb (`tap`, `type`, …).
    public var chosen: UInt32?
    public var chosenVerb: String?
    public var steps: [Step] = []
    public var result: RunResult?
    /// Notes and IO failures, newest last.
    public var notes: [String] = []
    /// While `status == .locked`: why the session waits, and for how long so far.
    public var lockReason: String?
    public var lockedWaitMs: UInt64 = 0
    /// Lock waits in this run so far.
    public var lockWaits: Int = 0

    private var modelChoseSinceLastAct = false
    private var lastReplyAction: String?

    public init(task: String = "", maxSteps: Int = 0, skillName: String? = nil) {
        self.task = task
        self.maxSteps = maxSteps
        self.skillName = skillName
    }

    public var isRunning: Bool {
        switch status {
        case .driving, .replaying, .needsYou, .locked: true
        case .standingBy, .done, .stopped: false
        }
    }

    /// `SETTINGS · STEP 2 OF 40`.
    public var caption: String {
        let app = (sightApp.flatMap(Self.appName) ?? app ?? "Jarvis").uppercased()
        guard stepNumber > 0 else { return app }
        return maxSteps > 0 ? "\(app) · STEP \(stepNumber) OF \(maxSteps)" : "\(app) · STEP \(stepNumber)"
    }

    /// `Settings (com.apple.Preferences)` → `Settings`.
    static func appName(_ header: String) -> String {
        if let paren = header.range(of: " (", options: .backwards), header.hasSuffix(")") {
            return String(header[..<paren.lowerBound])
        }
        return header
    }

    public mutating func apply(_ event: RunEvent) {
        switch event {
        case .prompt(let user):
            let screen = PromptScreen(prompt: user)
            sight = screen.elements
            sightApp = screen.app
            chosen = nil
            chosenVerb = nil
        case .reply(let text):
            modelChoseSinceLastAct = true
            if let json = Self.firstJSONObject(text) {
                chosen = json["element"]?.numberValue.map(\.clampedUInt32)
                chosenVerb = json["action"]?.stringValue
                lastReplyAction = chosenVerb
            }
        case .update(let update):
            apply(update)
        }
    }

    private mutating func apply(_ update: DriverUpdate) {
        switch update {
        case .started(let spec):
            let budget = maxSteps
            let skill = skillName
            self = LiveRun(task: spec.text, maxSteps: budget, skillName: skill)
            status = .driving
        case .effect(let effect, _):
            switch effect {
            case .act, .callTool:
                let source: Source
                if case .callTool = effect { source = .tool } else { source = modelChoseSinceLastAct ? .model : .replay }
                modelChoseSinceLastAct = false
                steps.append(Step(id: steps.count + 1, label: label(for: effect), source: source, state: .running))
                status = source == .replay ? .replaying : .driving
            case .confirm, .askUser:
                status = .needsYou
            case .observe, .infer, .finished, .awaitUnlock:
                break
            }
        case .progress(let p):
            stepNumber = Int(p.step)
            if let a = p.app { app = a }
            if let action = p.lastAction, let last = steps.indices.last, steps[last].state == .running {
                steps[last].label = describe(action, source: steps[last].source)
            }
        case .confirmationPending, .questionPending:
            status = .needsYou
        case .awaitingUnlock(let reason, let waited):
            if status != .locked { lockWaits += 1 }
            status = .locked
            lockReason = reason
            lockedWaitMs = waited
        case .lockDetected(let kind):
            notes.append("the phone locked while waiting for \(kind.rawValue)")
        case .unlockWaitEnded:
            if status == .locked { status = .driving }
            lockReason = nil
            lockedWaitMs = 0
        case .ioFailed(let kind, let message, _):
            notes.append("\(kind.rawValue): \(message)")
        case .note(let note):
            notes.append(note)
        case .effectCompleted(let effect, let event):
            if case .actResult(let r) = event, let last = steps.indices.last {
                steps[last].state = r.ok ? .done : .failed(r.error ?? "failed")
            }
            if case .toolResult(let r) = event, let last = steps.indices.last {
                steps[last].state = r.ok ? .done : .failed(r.error ?? "failed")
            }
            if case .confirm = effect { status = .driving }
            if case .askUser = effect { status = .driving }
        case .finished(let r):
            result = r
            status = r.outcome.status == .success ? .done : .stopped
            for i in steps.indices where steps[i].state == .running {
                steps[i].state = .failed("not finished")
            }
        }
    }

    private func label(for effect: Effect) -> String {
        if case .callTool(let call) = effect { return "Call \(call.name)" }
        if let verb = lastReplyAction, let index = chosen, let e = sight.first(where: { $0.index == index }) {
            return "\(verb.capitalized) \"\(e.label.isEmpty ? e.role : e.label)\""
        }
        return "Step \(steps.count + 1)"
    }

    /// A short, value-free description of an action (typed text is never shown).
    func describe(_ action: Action, source: Source) -> String {
        func target(_ index: UInt32?) -> String {
            guard source == .model, let index, let e = sight.first(where: { $0.index == index }) else {
                return index.map { "[\($0)]" } ?? ""
            }
            return "\"\(e.label.isEmpty ? e.role : e.label)\""
        }
        switch action {
        case .tap(let e): return "Tap \(target(e))"
        case .longPress(let e, _): return "Long-press \(target(e))"
        case .type(let e, _, let submit): return "Type into \(target(e))" + (submit ? " and submit" : "")
        case .clear(let e): return "Clear \(target(e))"
        case .scroll(let d, _): return "Scroll \(Self.name(d))"
        case .swipe(let d, _): return "Swipe \(Self.name(d))"
        case .tapPoint: return "Tap a point"
        case .pressHome: return "Go home"
        case .openApp(let app): return "Open \(app)"
        case .openUrl(let url): return "Open \(url)"
        case .wait(let ms): return "Wait \(ms) ms"
        case .callTool(let name, _): return "Call \(name)"
        case .askUser: return "Ask you"
        case .look: return "Look at the screen"
        case .done: return "Finish"
        case .fail: return "Give up"
        }
    }

    static func name(_ d: Direction) -> String {
        switch d {
        case .up: "up"
        case .down: "down"
        case .left: "left"
        case .right: "right"
        }
    }

    /// The first `{…}` object in a reply (models may wrap it in prose or fences).
    static func firstJSONObject(_ text: String) -> JSONValue? {
        guard let start = text.firstIndex(of: "{"), let end = text.lastIndex(of: "}"), start < end else { return nil }
        return try? JSONValue(parsing: String(text[start...end]))
    }
}
