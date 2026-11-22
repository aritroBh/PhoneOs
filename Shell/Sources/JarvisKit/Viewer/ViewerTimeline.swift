import Foundation

// The Live view's data: what Jarvis did, step by step, on which screen. Plain values,
// `Sendable` and `Codable` with explicit, stable keys (snake_case, enums tagged by "kind"),
// so the timeline can later be streamed as is to the paired remote controller. Nothing in
// here is raw: screen text went through the core's redaction, typed text is shown only when
// the core's rules allow it (`ViewerPrivacy`), links are hosts or schemes only and element
// values are never kept.
//
// Forward compatible: a gesture, step state, activity or screen source this reader does not
// know (from a newer sender) decodes as `.unknown(<its tag>)`, re-encodes with the same tag,
// and is drawn as neutrally as possible, so one new value never fails the whole timeline.

/// How the viewer tints an element.
public enum ViewerElementKind: String, Sendable, Equatable, Codable, CaseIterable {
    /// Buttons, links, cells, switches, sliders, pickers, tabs, keys.
    case control
    /// Text fields, secure fields, text views, search fields.
    case input
    /// Static text.
    case text
    case image
    /// Bars, tables, scroll and web views and other containers.
    case container
    /// Alerts and sheets.
    case alert
    case keyboard

    /// Unknown values (from a newer sender) read as `.container`, the quietest tint.
    public init(from decoder: any Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = ViewerElementKind(rawValue: raw) ?? .container
    }
}

/// One rectangle of the wireframe backdrop (and the target of a gesture).
public struct ViewerElement: Sendable, Equatable, Codable {
    /// Frame in screen points, clipped to the screen.
    public var frame: ViewerRect
    /// The role as the core names it (`Button`, `SecureTextField`, …).
    public var role: String
    public var kind: ViewerElementKind
    /// The accessibility label, else the placeholder; never the element's value. Capped.
    public var label: String?
    /// A secure (password) field.
    public var secure: Bool
    /// Depth in the tree (0 = top level shown); deeper elements draw on top.
    public var depth: Int

    public init(frame: ViewerRect, role: String, kind: ViewerElementKind, label: String?, secure: Bool, depth: Int) {
        self.frame = frame
        self.role = role
        self.kind = kind
        self.label = label
        self.secure = secure
        self.depth = depth
    }

    enum CodingKeys: String, CodingKey { case frame, role, kind, label, secure, depth }
}

/// One observed screen.
public struct ViewerScreen: Sendable, Equatable, Codable, Identifiable {
    public enum Source: Sendable, Equatable, Codable {
        /// A typed tree: the wireframe is available.
        case tree
        /// The runner's text format: no frames, so no wireframe (cursor and caption only).
        case runnerText
        /// The phone was locked: nothing of the screen is kept.
        case locked
        /// A source from a newer sender (its tag); drawn like `runnerText`.
        case unknown(String)

        /// The coding tag.
        public var tag: String {
            switch self {
            case .tree: "tree"
            case .runnerText: "runner_text"
            case .locked: "locked"
            case .unknown(let tag): tag
            }
        }

        public init(tag: String) {
            switch tag {
            case "tree": self = .tree
            case "runner_text": self = .runnerText
            case "locked": self = .locked
            default: self = .unknown(tag)
            }
        }

        public init(from decoder: any Decoder) throws {
            self.init(tag: try decoder.singleValueContainer().decode(String.self))
        }

        public func encode(to encoder: any Encoder) throws {
            var c = encoder.singleValueContainer()
            try c.encode(tag)
        }
    }

    /// Unique within a timeline.
    public var id: Int
    /// Screen size in points (as reported; draw with `ViewerSize.usable`).
    public var size: ViewerSize
    public var appName: String?
    public var bundleId: String
    public var source: Source
    /// The wireframe, parents before children (draw in order). Empty for runner text and
    /// locked screens.
    public var elements: [ViewerElement]
    /// More elements were shown than `PointerTimeline.maxElements`.
    public var elementsTruncated: Bool
    /// Where a real screenshot of this observation may be (`ViewerFrameStore`); `nil` when
    /// the option was off or the phone was locked. The frame may have been evicted since.
    public var frameKey: ViewerFrameKey?
    /// Whether the focused element is a secure field (`nil`: no focused input, or the tree
    /// cannot tell). Used to hide typed text.
    public var focusedInputSecure: Bool?

    public init(id: Int, size: ViewerSize, appName: String?, bundleId: String, source: Source, elements: [ViewerElement],
                elementsTruncated: Bool, frameKey: ViewerFrameKey?, focusedInputSecure: Bool?) {
        self.id = id
        self.size = size
        self.appName = appName
        self.bundleId = bundleId
        self.source = source
        self.elements = elements
        self.elementsTruncated = elementsTruncated
        self.frameKey = frameKey
        self.focusedInputSecure = focusedInputSecure
    }

    /// The name to show: the app name, else the bundle id.
    public var displayName: String {
        if let appName, !appName.isEmpty { return appName }
        return bundleId
    }

    enum CodingKeys: String, CodingKey {
        case id, size
        case appName = "app_name"
        case bundleId = "bundle_id"
        case source, elements
        case elementsTruncated = "elements_truncated"
        case frameKey = "frame_key"
        case focusedInputSecure = "focused_input_secure"
    }
}

/// What a step did, in display form.
public enum ViewerGesture: Sendable, Equatable {
    case tap(ViewerPoint)
    case longPress(ViewerPoint, durationMs: UInt64)
    case swipe(from: ViewerPoint, to: ViewerPoint, durationMs: UInt64)
    /// Typing; `text` is already redacted (`••••` for secure fields), `nil` when it is not
    /// shown at all.
    case type(text: String?)
    /// A key press (`Return`, `Delete`, …).
    case key(String)
    /// Opening an app (its name, else its bundle id).
    case launch(String)
    /// Opening a URL: the host of an `http`/`https` URL, only `<scheme>:` for any other
    /// scheme, empty when there is neither.
    case openURL(host: String)
    case wait(ms: UInt64)
    /// A gesture from a newer sender (its kind): no cursor movement, a parked step.
    case unknown(String)

    /// Where the pointer goes down, for pointer gestures.
    public var pressPoint: ViewerPoint? {
        switch self {
        case .tap(let p), .longPress(let p, _): p
        case .swipe(let from, _, _): from
        default: nil
        }
    }

    /// Where the pointer ends up, for pointer gestures.
    public var endPoint: ViewerPoint? {
        switch self {
        case .tap(let p), .longPress(let p, _): p
        case .swipe(_, let to, _): to
        default: nil
        }
    }

    /// Whether the cursor moves for this gesture.
    public var isPointer: Bool { pressPoint != nil }

    /// A short stable name (also the coding tag).
    public var kind: String {
        switch self {
        case .tap: "tap"
        case .longPress: "long_press"
        case .swipe: "swipe"
        case .type: "type"
        case .key: "key"
        case .launch: "launch"
        case .openURL: "open_url"
        case .wait: "wait"
        case .unknown(let kind): kind
        }
    }
}

extension ViewerGesture: Codable {
    enum CodingKeys: String, CodingKey {
        case kind, point, from, to
        case durationMs = "duration_ms"
        case text, key, app, host, ms
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(kind, forKey: .kind)
        switch self {
        case .tap(let p):
            try c.encode(p, forKey: .point)
        case .longPress(let p, let ms):
            try c.encode(p, forKey: .point)
            try c.encode(ms, forKey: .durationMs)
        case .swipe(let from, let to, let ms):
            try c.encode(from, forKey: .from)
            try c.encode(to, forKey: .to)
            try c.encode(ms, forKey: .durationMs)
        case .type(let text):
            try c.encodeIfPresent(text, forKey: .text)
        case .key(let key):
            try c.encode(key, forKey: .key)
        case .launch(let app):
            try c.encode(app, forKey: .app)
        case .openURL(let host):
            try c.encode(host, forKey: .host)
        case .wait(let ms):
            try c.encode(ms, forKey: .ms)
        case .unknown:
            break
        }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "tap": self = .tap(try c.decode(ViewerPoint.self, forKey: .point))
        case "long_press":
            self = .longPress(try c.decode(ViewerPoint.self, forKey: .point), durationMs: try c.decode(UInt64.self, forKey: .durationMs))
        case "swipe":
            self = .swipe(from: try c.decode(ViewerPoint.self, forKey: .from), to: try c.decode(ViewerPoint.self, forKey: .to),
                          durationMs: try c.decode(UInt64.self, forKey: .durationMs))
        case "type": self = .type(text: try c.decodeIfPresent(String.self, forKey: .text))
        case "key": self = .key(try c.decode(String.self, forKey: .key))
        case "launch": self = .launch(try c.decode(String.self, forKey: .app))
        case "open_url": self = .openURL(host: try c.decode(String.self, forKey: .host))
        case "wait": self = .wait(ms: try c.decode(UInt64.self, forKey: .ms))
        case let other:
            self = .unknown(other)
        }
    }
}

/// Where a step stands.
public enum ViewerStepState: Sendable, Equatable {
    case running
    case done
    case failed(String)
    /// The run was stopped while the command was in flight: whether the phone did it is not
    /// known, so nothing is claimed ("Stopped").
    case stopped
    /// A state from a newer sender (its tag): neither running nor failed.
    case unknown(String)

    public var isFailed: Bool {
        if case .failed = self { return true }
        return false
    }

    /// A short word for the state ("Stopped" for a step stopped mid-command), `nil` when
    /// there is nothing to say (running, done, unknown).
    public var title: String? {
        switch self {
        case .running, .done, .unknown: nil
        case .failed(let why): why
        case .stopped: "Stopped"
        }
    }
}

extension ViewerStepState: Codable {
    enum CodingKeys: String, CodingKey { case kind, error }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .running: try c.encode("running", forKey: .kind)
        case .done: try c.encode("done", forKey: .kind)
        case .stopped: try c.encode("stopped", forKey: .kind)
        case .failed(let error):
            try c.encode("failed", forKey: .kind)
            try c.encode(error, forKey: .error)
        case .unknown(let tag):
            try c.encode(tag, forKey: .kind)
        }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "running": self = .running
        case "done": self = .done
        case "stopped": self = .stopped
        case "failed": self = .failed(try c.decodeIfPresent(String.self, forKey: .error) ?? "failed")
        case let other:
            self = .unknown(other)
        }
    }
}

/// One executed runner command, as the viewer shows it.
public struct ViewerStep: Sendable, Equatable, Codable, Identifiable {
    /// 1, 2, 3… in the order the commands were sent; stable (never reused after the cap drops
    /// old steps).
    public var number: Int
    /// The core's step counter when the command was sent (several commands can belong to
    /// one core step).
    public var coreStep: Int?
    /// The screen it acted on (`ViewerTimeline.screen(for:)`).
    public var screenID: Int?
    public var gesture: ViewerGesture
    /// The element under the pointer, for display only (see `PointerTimeline`).
    public var target: ViewerElement?
    /// "Tap “Send”", "Swipe up", "Type “hello”".
    public var caption: String
    public var state: ViewerStepState

    public var id: Int { number }

    public init(number: Int, coreStep: Int?, screenID: Int?, gesture: ViewerGesture, target: ViewerElement?, caption: String,
                state: ViewerStepState) {
        self.number = number
        self.coreStep = coreStep
        self.screenID = screenID
        self.gesture = gesture
        self.target = target
        self.caption = caption
        self.state = state
    }

    enum CodingKeys: String, CodingKey {
        case number
        case coreStep = "core_step"
        case screenID = "screen_id"
        case gesture, target, caption, state
    }
}

/// How a run ended, for the viewer.
public enum ViewerOutcome: String, Sendable, Equatable, Codable {
    case success, failed, cancelled, blocked, interrupted

    public var title: String {
        switch self {
        case .success: "Done"
        case .failed: "Failed"
        case .cancelled: "Stopped"
        case .blocked: "Blocked"
        case .interrupted: "Phone stayed locked"
        }
    }
}

/// What Jarvis is doing right now (the caption pill while following live).
public enum ViewerActivity: Sendable, Equatable {
    case idle
    case looking
    case thinking
    case acting
    case usingTool(String)
    /// A confirmation waits for the owner; the text names the kind of action.
    case waitingForApproval(String)
    case waitingForAnswer
    case locked
    case finished(ViewerOutcome)
    /// An activity from a newer sender (its tag, or `finished` with an outcome this reader
    /// does not know): a neutral caption, the cursor idles.
    case unknown(String)

    /// The pill text (`.acting` shows the step's caption instead).
    public var caption: String {
        switch self {
        case .idle: "Standing by"
        case .looking: "Looking at the screen…"
        case .thinking: "Thinking…"
        case .acting: "Acting…"
        case .usingTool(let name): "Using \(name)"
        case .waitingForApproval(let what): what.isEmpty ? "Waiting for your OK" : "Waiting for your OK · \(what)"
        case .waitingForAnswer: "Waiting for your answer"
        case .locked: "Unlock to continue"
        case .finished(let outcome): outcome.title
        case .unknown: "Working…"
        }
    }

    /// The cursor idles (pulses) instead of acting.
    public var isIdle: Bool {
        switch self {
        case .acting: false
        default: true
        }
    }
}

extension ViewerActivity: Codable {
    enum CodingKeys: String, CodingKey { case kind, text, outcome }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .idle: try c.encode("idle", forKey: .kind)
        case .looking: try c.encode("looking", forKey: .kind)
        case .thinking: try c.encode("thinking", forKey: .kind)
        case .acting: try c.encode("acting", forKey: .kind)
        case .usingTool(let name):
            try c.encode("using_tool", forKey: .kind)
            try c.encode(name, forKey: .text)
        case .waitingForApproval(let what):
            try c.encode("waiting_for_approval", forKey: .kind)
            try c.encode(what, forKey: .text)
        case .waitingForAnswer: try c.encode("waiting_for_answer", forKey: .kind)
        case .locked: try c.encode("locked", forKey: .kind)
        case .finished(let outcome):
            try c.encode("finished", forKey: .kind)
            try c.encode(outcome, forKey: .outcome)
        case .unknown(let tag):
            try c.encode(tag, forKey: .kind)
        }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "idle": self = .idle
        case "looking": self = .looking
        case "thinking": self = .thinking
        case "acting": self = .acting
        case "using_tool": self = .usingTool(try c.decode(String.self, forKey: .text))
        case "waiting_for_approval": self = .waitingForApproval(try c.decode(String.self, forKey: .text))
        case "waiting_for_answer": self = .waitingForAnswer
        case "locked": self = .locked
        case "finished":
            // An outcome this reader does not know reads as an unknown activity.
            if let outcome = try? c.decode(ViewerOutcome.self, forKey: .outcome) {
                self = .finished(outcome)
            } else {
                self = .unknown("finished")
            }
        case let other:
            self = .unknown(other)
        }
    }
}

/// The run as the Live view shows it: steps (newest last, capped), the screens they acted
/// on, the latest screen and what Jarvis is doing now.
public struct ViewerTimeline: Sendable, Equatable, Codable {
    /// The run's task id (`TaskSpec.id`), empty before a run starts.
    public var taskId: String
    /// Steps, oldest first; at most `PointerTimeline.maxSteps`.
    public var steps: [ViewerStep]
    /// Steps dropped from the front by the cap.
    public var droppedSteps: Int
    /// The screens the kept steps refer to, plus the latest one.
    public var screens: [ViewerScreen]
    /// The latest observed screen.
    public var currentScreenID: Int?
    public var activity: ViewerActivity

    public init(taskId: String = "", steps: [ViewerStep] = [], droppedSteps: Int = 0, screens: [ViewerScreen] = [],
                currentScreenID: Int? = nil, activity: ViewerActivity = .idle) {
        self.taskId = taskId
        self.steps = steps
        self.droppedSteps = droppedSteps
        self.screens = screens
        self.currentScreenID = currentScreenID
        self.activity = activity
    }

    /// The screen with `id`.
    public func screen(id: Int?) -> ViewerScreen? {
        guard let id else { return nil }
        return screens.first { $0.id == id }
    }

    /// The screen `step` acted on.
    public func screen(for step: ViewerStep) -> ViewerScreen? { screen(id: step.screenID) }

    /// The latest observed screen.
    public var currentScreen: ViewerScreen? { screen(id: currentScreenID) }

    /// The step numbered `number`.
    public func step(number: Int?) -> ViewerStep? {
        guard let number, let first = steps.first?.number else { return nil }
        let i = number - first
        // Numbers are consecutive, so this is the index; checked anyway.
        if steps.indices.contains(i), steps[i].number == number { return steps[i] }
        return steps.first { $0.number == number }
    }

    /// The step before the step numbered `number`, `nil` for the first kept step.
    public func step(before number: Int) -> ViewerStep? {
        guard let i = steps.firstIndex(where: { $0.number == number }), i > 0 else { return nil }
        return steps[i - 1]
    }

    /// The step numbers kept, oldest first.
    public var stepNumbers: [Int] { steps.map(\.number) }

    enum CodingKeys: String, CodingKey {
        case taskId = "task_id"
        case steps
        case droppedSteps = "dropped_steps"
        case screens
        case currentScreenID = "current_screen_id"
        case activity
    }
}
