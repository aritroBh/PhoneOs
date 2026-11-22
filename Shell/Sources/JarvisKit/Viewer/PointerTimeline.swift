import Foundation
import JarvisCore

/// Reduces the run's `RunEvent`s (the same stream `LiveRun` reads: driver updates, which
/// carry every observation and every runner command, plus the prompts) into the Live view's
/// `ViewerTimeline`: one step per runner command, the screen it acted on, the element under
/// the pointer and a short caption.
///
/// The target element is a **display aid only**: the deepest shown element whose frame
/// contains the point, so the viewer can outline it and name it in the caption. It is never
/// used to decide anything; the core alone decides what to do and what needs the owner's OK.
///
/// Privacy (each rule is tested):
/// - Typed text is never taken from the runner command (which carries the owner's stored
///   values filled in). The caption uses the text the model or the learned flow asked for
///   (`SessionProgress.lastAction`), with `{{profile.…}}` placeholders masked, and only when
///   the field is known not to be secure; a secure field, or any doubt, shows `••••` or no
///   text at all.
/// - The wireframe keeps labels and placeholders, never values.
/// - `openUrl` keeps the host only.
/// - A locked observation keeps nothing of the screen.
/// - Runner error texts are not kept (they may quote what was typed).
public struct PointerTimeline: Sendable, Equatable {
    /// Steps kept by default (older steps are dropped from the front).
    public static let defaultMaxSteps = 200
    /// Wireframe rectangles kept per screen.
    public static let maxElements = 400
    /// Tree nodes looked at per screen (a pathological tree cannot stall the view).
    static let maxVisitedNodes = 20_000

    public private(set) var timeline = ViewerTimeline()
    /// The step cap (at least 1).
    public let maxSteps: Int
    /// The driver keeps Live view frames for this run (`DriverConfiguration.viewerScreenshots`):
    /// unlocked screens get a `frameKey`.
    public let framesEnabled: Bool

    /// The number of the effect being handled (`DriverUpdate.effect(_, number:)`).
    private var effectNumber = 0
    /// The current screen's typed tree (target lookup); not part of the timeline.
    private var tree: UiNode?
    /// The element list of the latest prompt; cleared when the command it led to completes.
    private var sight: [PromptScreen.Element] = []
    /// The core's step counter from the latest progress.
    private var coreStep: Int?
    /// The step the current `act` effect created.
    private var actStep: Int?
    /// A typing step waiting for the model's text (the `.progress` right after its effect).
    private var pendingType: PendingType?
    private var nextScreenID = 1

    private struct PendingType: Sendable, Equatable {
        var number: Int
        var secure: Bool
        var plain: Bool
        /// Length and in-process hash of the runner's text: the model's text is shown only if
        /// it is exactly what was typed (or the core filled placeholders into it).
        var rawCount: Int
        var rawHash: Int
    }

    public init(maxSteps: Int = PointerTimeline.defaultMaxSteps, framesEnabled: Bool = false) {
        self.maxSteps = max(1, maxSteps)
        self.framesEnabled = framesEnabled
    }

    public var steps: [ViewerStep] { timeline.steps }

    // MARK: - Reducer

    public mutating func apply(_ event: RunEvent) {
        switch event {
        case .prompt(let user):
            sight = PromptScreen(prompt: user).elements
        case .reply:
            break
        case .update(let update):
            apply(update)
        }
    }

    private mutating func apply(_ update: DriverUpdate) {
        switch update {
        case .started(let spec):
            self = PointerTimeline(maxSteps: maxSteps, framesEnabled: framesEnabled)
            timeline.taskId = spec.id
        case .effect(let effect, let number):
            effectNumber = number
            actStep = nil
            pendingType = nil
            switch effect {
            case .observe:
                timeline.activity = .looking
            case .infer:
                timeline.activity = .thinking
            case .act(let command):
                addStep(command)
                timeline.activity = .acting
            case .callTool(let call):
                timeline.activity = .usingTool(ViewerPrivacy.label(call.name) ?? "a tool")
            case .confirm(let request):
                timeline.activity = .waitingForApproval(riskCategoryTitle(request.category))
            case .askUser:
                timeline.activity = .waitingForAnswer
            case .awaitUnlock:
                timeline.activity = .locked
            case .finished:
                break
            }
        case .progress(let progress):
            coreStep = Int(progress.step)
            // The progress right after an `act` effect counts the step that sent it.
            if let number = actStep, let i = timeline.steps.lastIndex(where: { $0.number == number }) {
                timeline.steps[i].coreStep = coreStep
            }
            if let action = progress.lastAction { resolvePending(with: action) }
        case .confirmationPending(let request):
            timeline.activity = .waitingForApproval(riskCategoryTitle(request.category))
        case .questionPending:
            timeline.activity = .waitingForAnswer
        case .awaitingUnlock:
            timeline.activity = .locked
        case .lockDetected, .unlockWaitEnded, .ioFailed, .note:
            break
        case .effectCompleted(let effect, let event):
            switch event {
            case .observed(let observation):
                ingest(observation)
            case .observedRunnerText(let snapshot):
                ingest(snapshot)
            case .actResult(let result):
                if case .act = effect { complete(result) }
            default:
                break
            }
            // The prompt's element list belongs to the decision that led to this command.
            if case .act = effect { sight = [] }
        case .finished(let result):
            timeline.activity = .finished(Self.outcome(result.outcome.status))
            for i in timeline.steps.indices where timeline.steps[i].state == .running {
                timeline.steps[i].state = .failed("Not finished")
            }
            pendingType = nil
        }
    }

    static func outcome(_ status: OutcomeStatus) -> ViewerOutcome {
        switch status {
        case .success: .success
        case .failed: .failed
        case .cancelled: .cancelled
        case .blocked: .blocked
        case .interrupted: .interrupted
        }
    }

    // MARK: - Screens

    private mutating func ingest(_ observation: ScreenObservation) {
        let screen = observation.screen
        let id = nextScreenID
        nextScreenID += 1
        let size = ViewerSize(width: screen.size.width, height: screen.size.height)
        if observation.deviceLocked {
            tree = nil
            addScreen(ViewerScreen(id: id, size: size, appName: nil, bundleId: "", source: .locked, elements: [],
                                   elementsTruncated: false, frameKey: nil, focusedInputSecure: nil))
            return
        }
        let wire = Self.wireframe(screen.root, size: size)
        tree = screen.root
        addScreen(ViewerScreen(
            id: id, size: size, appName: ViewerPrivacy.label(screen.appName), bundleId: screen.appBundleId, source: .tree,
            elements: wire.elements, elementsTruncated: wire.truncated, frameKey: frameKey(),
            focusedInputSecure: Self.focusedInputSecure(screen.root)
        ))
    }

    private mutating func ingest(_ snapshot: RunnerSnapshot) {
        let id = nextScreenID
        nextScreenID += 1
        tree = nil
        let size = ViewerSize(width: snapshot.size.width, height: snapshot.size.height)
        if snapshot.deviceLocked {
            addScreen(ViewerScreen(id: id, size: size, appName: nil, bundleId: "", source: .locked, elements: [],
                                   elementsTruncated: false, frameKey: nil, focusedInputSecure: nil))
            return
        }
        addScreen(ViewerScreen(id: id, size: size, appName: ViewerPrivacy.label(snapshot.appName), bundleId: snapshot.appBundleId,
                               source: .runnerText, elements: [], elementsTruncated: false, frameKey: frameKey(),
                               focusedInputSecure: nil))
    }

    private func frameKey() -> ViewerFrameKey? {
        framesEnabled ? ViewerFrameKey(taskId: timeline.taskId, observation: effectNumber) : nil
    }

    private mutating func addScreen(_ screen: ViewerScreen) {
        timeline.screens.append(screen)
        timeline.currentScreenID = screen.id
        prune()
    }

    /// Keeps the screens the kept steps refer to and the current one.
    private mutating func prune() {
        var used = Set(timeline.steps.compactMap(\.screenID))
        if let current = timeline.currentScreenID { used.insert(current) }
        timeline.screens.removeAll { !used.contains($0.id) }
    }

    /// The shown elements of `root` in drawing order (parents first), clipped to the screen:
    /// visible, with a finite positive area on the screen; the application and window nodes
    /// (the whole screen) are left out. At most `maxElements`.
    static func wireframe(_ root: UiNode, size: ViewerSize) -> (elements: [ViewerElement], truncated: Bool) {
        let bounds = ViewerRect(x: 0, y: 0, width: size.usable.width, height: size.usable.height)
        var out: [ViewerElement] = []
        var truncated = false
        var visited = 0
        // Iterative pre-order: a deep tree cannot overflow the stack.
        var stack: [(node: UiNode, depth: Int)] = [(root, 0)]
        while let (node, depth) = stack.popLast() {
            visited += 1
            if visited > maxVisitedNodes {
                truncated = true
                break
            }
            var childDepth = depth
            if let element = element(node, depth: depth, bounds: bounds) {
                if out.count < maxElements {
                    out.append(element)
                } else {
                    truncated = true
                }
                childDepth = depth + 1
            }
            for child in node.children.reversed() {
                stack.append((child, childDepth))
            }
        }
        return (out, truncated)
    }

    /// `node` as a wireframe element, `nil` when it is not shown on the screen.
    static func element(_ node: UiNode, depth: Int, bounds: ViewerRect) -> ViewerElement? {
        switch node.role {
        case .application, .window: return nil
        default: break
        }
        guard node.visible else { return nil }
        let raw = ViewerRect(x: node.frame.x, y: node.frame.y, width: node.frame.width, height: node.frame.height)
        guard let clipped = raw.intersection(bounds) else { return nil }
        let secure = node.secure || node.role == .secureTextField
        // Label, else placeholder; never the value (whatever the role).
        let label = ViewerPrivacy.label(node.label) ?? ViewerPrivacy.label(node.placeholder)
        return ViewerElement(frame: clipped, role: roleName(node.role), kind: kind(node.role), label: label, secure: secure,
                             depth: depth)
    }

    /// Whether the focused input is secure; `nil` when no input is focused.
    static func focusedInputSecure(_ root: UiNode) -> Bool? {
        var found: Bool?
        var visited = 0
        var stack = [root]
        while let node = stack.popLast(), visited < maxVisitedNodes {
            visited += 1
            if node.focused {
                let secure = node.secure || node.role == .secureTextField
                if secure { return true }
                if kind(node.role) == .input { found = false }
            }
            stack.append(contentsOf: node.children)
        }
        return found
    }

    /// The deepest shown element (topmost among equals) whose frame contains `point`, or
    /// `nil`. Display only.
    static func target(at point: ViewerPoint, in root: UiNode, size: ViewerSize) -> ViewerElement? {
        guard point.isFinite else { return nil }
        let bounds = ViewerRect(x: 0, y: 0, width: size.usable.width, height: size.usable.height)
        var best: ViewerElement?
        var visited = 0
        var stack: [(node: UiNode, depth: Int)] = [(root, 0)]
        while let (node, depth) = stack.popLast(), visited < maxVisitedNodes {
            visited += 1
            var childDepth = depth
            if let element = element(node, depth: depth, bounds: bounds) {
                childDepth = depth + 1
                let frame = ViewerRect(x: node.frame.x, y: node.frame.y, width: node.frame.width, height: node.frame.height)
                // Pre-order visits later siblings later, so `>=` keeps the one drawn on top.
                if frame.contains(point), element.depth >= (best?.depth ?? -1) {
                    best = element
                }
            }
            for child in node.children.reversed() {
                stack.append((child, childDepth))
            }
        }
        return best
    }

    // MARK: - Steps

    private mutating func addStep(_ command: RunnerCommand) {
        let number = timeline.droppedSteps + timeline.steps.count + 1
        let screen = timeline.currentScreen
        let size = screen?.size ?? ViewerSize.fallbackPhone
        let gesture: ViewerGesture
        var target: ViewerElement?
        let caption: String
        switch command {
        case .tap(let x, let y):
            let p = ViewerPoint(x: x, y: y)
            gesture = .tap(p)
            target = tree.flatMap { Self.target(at: p, in: $0, size: size) }
            caption = Self.pointerCaption("Tap", target)
        case .longPress(let x, let y, let ms):
            let p = ViewerPoint(x: x, y: y)
            gesture = .longPress(p, durationMs: ms)
            target = tree.flatMap { Self.target(at: p, in: $0, size: size) }
            caption = Self.pointerCaption("Press and hold", target)
        case .swipe(let from, let to, let ms):
            let a = ViewerPoint(x: from.x, y: from.y), b = ViewerPoint(x: to.x, y: to.y)
            gesture = .swipe(from: a, to: b, durationMs: ms)
            target = tree.flatMap { Self.target(at: a, in: $0, size: size) }
            caption = Self.swipeCaption(from: a, to: b)
        case .typeText(let raw):
            let evidence = typingEvidence(screen: screen)
            pendingType = PendingType(number: number, secure: evidence.secure, plain: evidence.plain,
                                      rawCount: raw.count, rawHash: raw.hashValue)
            let shown: String? = evidence.secure ? ViewerPrivacy.mask : nil
            gesture = .type(text: shown)
            caption = Self.typeCaption(shown)
        case .pressKey(let key):
            let name = Self.keyName(key)
            gesture = .key(name)
            caption = key == .home ? "Go home" : "Press \(name)"
        case .launchApp(let bundleId):
            let name = Self.appName(bundleId)
            gesture = .launch(name)
            caption = "Open \(name)"
        case .openUrl(let url):
            let host = ViewerPrivacy.host(of: url)
            gesture = .openURL(host: host ?? "")
            caption = host.map { "Open \($0)" } ?? "Open a link"
        case .wait(let ms):
            gesture = .wait(ms: ms)
            caption = Self.waitCaption(ms)
        }
        timeline.steps.append(ViewerStep(number: number, coreStep: coreStep, screenID: screen?.id, gesture: gesture,
                                         target: target, caption: caption, state: .running))
        actStep = number
        if timeline.steps.count > maxSteps {
            let drop = timeline.steps.count - maxSteps
            timeline.steps.removeFirst(drop)
            timeline.droppedSteps += drop
            prune()
        }
    }

    /// What is known about the field text goes into: secure evidence wins; "plain" needs
    /// positive evidence of a non-secure input.
    private func typingEvidence(screen: ViewerScreen?) -> (secure: Bool, plain: Bool) {
        var secure = false
        var plain = false
        if let focused = screen?.focusedInputSecure {
            if focused { secure = true } else { plain = true }
        }
        // A tap on a field just before, on the same screen, puts the focus there.
        if let screen, let previous = timeline.steps.last, previous.screenID == screen.id, previous.gesture.isPointer,
           let target = previous.target {
            if target.secure {
                secure = true
            } else if target.kind == .input {
                plain = true
            }
        }
        if sight.contains(where: { Self.isSecureRole($0.role) && $0.flags.contains("focused") }) {
            secure = true
        }
        if screen == nil || screen?.source == .locked { plain = false }
        return (secure, plain)
    }

    /// Sets a pending typing step's text from the model's (or flow's) `type` action.
    private mutating func resolvePending(with action: Action) {
        guard let pending = pendingType else { return }
        pendingType = nil
        guard case .type(let element, let text, _) = action,
              let i = timeline.steps.lastIndex(where: { $0.number == pending.number }) else { return }
        var secure = pending.secure
        var plain = pending.plain
        if let element, let e = sight.first(where: { $0.index == element }) {
            if Self.isSecureRole(e.role) {
                secure = true
            } else if Self.inputRoles.contains(e.role) {
                plain = true
            }
        }
        // Shown only when it is exactly what was typed, or the core filled stored values
        // into placeholders (which are then masked).
        let consistent = ViewerPrivacy.hasPlaceholder(text) || (text.count == pending.rawCount && text.hashValue == pending.rawHash)
        let shown: String?
        if secure {
            shown = ViewerPrivacy.mask
        } else if plain && consistent {
            shown = ViewerPrivacy.typedText(text, secure: false)
        } else {
            shown = nil
        }
        timeline.steps[i].gesture = .type(text: shown)
        timeline.steps[i].caption = Self.typeCaption(shown)
    }

    private mutating func complete(_ result: RunnerResult) {
        guard let number = actStep, let i = timeline.steps.lastIndex(where: { $0.number == number }) else { return }
        if result.ok {
            timeline.steps[i].state = .done
        } else if result.failure == .deviceLocked {
            timeline.steps[i].state = .failed("The phone was locked")
        } else {
            // The runner's own words are not kept: they may quote typed text.
            timeline.steps[i].state = .failed("The phone did not do it")
        }
    }

    // MARK: - Captions

    static let inputRoles: Set<String> = ["TextField", "SecureTextField", "TextView", "SearchField"]

    static func isSecureRole(_ role: String) -> Bool {
        role == "SecureTextField"
    }

    static func pointerCaption(_ verb: String, _ target: ViewerElement?) -> String {
        guard let label = target?.label else { return verb }
        return "\(verb) \u{201C}\(label)\u{201D}"
    }

    static func typeCaption(_ shown: String?) -> String {
        guard let shown else { return "Type text" }
        return "Type \u{201C}\(shown)\u{201D}"
    }

    static func swipeCaption(from a: ViewerPoint, to b: ViewerPoint) -> String {
        let dx = b.x - a.x, dy = b.y - a.y
        guard dx.isFinite, dy.isFinite, abs(dx) > 0.5 || abs(dy) > 0.5 else { return "Swipe" }
        if abs(dy) >= abs(dx) { return dy < 0 ? "Swipe up" : "Swipe down" }
        return dx < 0 ? "Swipe left" : "Swipe right"
    }

    static func waitCaption(_ ms: UInt64) -> String {
        if ms < 1_000 { return "Wait \(ms) ms" }
        if ms % 1_000 == 0 { return "Wait \(ms / 1_000) s" }
        return "Wait " + String(format: "%.1f", Double(ms) / 1_000) + " s"
    }

    static func keyName(_ key: Key) -> String {
        switch key {
        case .`return`: "Return"
        case .delete: "Delete"
        case .tab: "Tab"
        case .space: "Space"
        case .escape: "Escape"
        case .home: "Home"
        }
    }

    /// The app's display name when Jarvis knows it, else the bundle id.
    static func appName(_ bundleId: String) -> String {
        let known = KnownApps.common + DemoPhone.knownApps
        if let app = known.first(where: { $0.bundleId == bundleId }), !app.name.isEmpty { return app.name }
        return ViewerPrivacy.capped(bundleId, 64)
    }

    static func roleName(_ role: Role) -> String {
        switch role {
        case .button: "Button"
        case .link: "Link"
        case .textField: "TextField"
        case .secureTextField: "SecureTextField"
        case .textView: "TextView"
        case .searchField: "SearchField"
        case .staticText: "StaticText"
        case .image: "Image"
        case .cell: "Cell"
        case .`switch`: "Switch"
        case .slider: "Slider"
        case .picker: "Picker"
        case .tab: "Tab"
        case .navigationBar: "NavigationBar"
        case .tabBar: "TabBar"
        case .toolbar: "Toolbar"
        case .alert: "Alert"
        case .sheet: "Sheet"
        case .keyboard: "Keyboard"
        case .key: "Key"
        case .scrollView: "ScrollView"
        case .table: "Table"
        case .collectionView: "CollectionView"
        case .webView: "WebView"
        case .window: "Window"
        case .application: "Application"
        case .other(let name): ViewerPrivacy.capped(name.isEmpty ? "Other" : name, 32)
        }
    }

    static func kind(_ role: Role) -> ViewerElementKind {
        switch role {
        case .button, .link, .cell, .`switch`, .slider, .picker, .tab, .key: .control
        case .textField, .secureTextField, .textView, .searchField: .input
        case .staticText: .text
        case .image: .image
        case .alert, .sheet: .alert
        case .keyboard: .keyboard
        case .navigationBar, .tabBar, .toolbar, .scrollView, .table, .collectionView, .webView, .window, .application, .other:
            .container
        }
    }
}
