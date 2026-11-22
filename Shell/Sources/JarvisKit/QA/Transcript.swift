import Foundation
import JarvisCore
import Synchronization

/// A timed record of one run for QA artifacts: prompts, model replies, effects, events,
/// observations and the result, written as one JSON file a person without a Mac can read.
public final class Transcript: Sendable {
    /// One record.
    public struct Entry: Codable, Sendable, Equatable {
        /// Order of recording, from 1.
        public var seq: Int
        /// Milliseconds since the transcript was created.
        public var tMs: Int
        /// `task`, `observation`, `prompt`, `reply`, `script`, `effect`, `event`, `progress`, `lock`,
        /// `confirm`, `question`, `note`, `io_failed`, `owner`, `result`, `error`.
        public var kind: String
        /// Short title.
        public var title: String
        /// Full text, when any.
        public var text: String?

        enum CodingKeys: String, CodingKey {
            case seq
            case tMs = "t_ms"
            case kind, title, text
        }
    }

    private struct State {
        var entries: [Entry] = []
        var lastSystemPrompt: String?
        var prompts = 0
    }

    /// Name of the run (used in the file name).
    public let name: String
    private let origin = ContinuousClock.now
    private let state = Mutex(State())

    public init(name: String) {
        self.name = name
    }

    /// Every entry so far.
    public var entries: [Entry] { state.withLock { $0.entries } }

    /// Milliseconds since creation.
    public func elapsedMs() -> Int {
        let (s, atto) = origin.duration(to: ContinuousClock.now).components
        return Int(s * 1_000 + atto / 1_000_000_000_000_000)
    }

    /// Adds an entry.
    public func record(_ kind: String, _ title: String, _ text: String? = nil) {
        let t = elapsedMs()
        state.withLock { s in
            s.entries.append(Entry(seq: s.entries.count + 1, tMs: t, kind: kind, title: title, text: text))
        }
    }

    /// Records a model request: the system prompt in full the first time and whenever it
    /// changes, the user part always.
    public func recordPrompt(_ request: ModelRequest) {
        let (number, systemChanged) = state.withLock { s -> (Int, Bool) in
            s.prompts += 1
            let changed = s.lastSystemPrompt != request.system
            s.lastSystemPrompt = request.system
            return (s.prompts, changed)
        }
        if systemChanged {
            record("prompt", "prompt \(number): system", request.system)
        }
        var header = "prompt \(number): user (kind \(Self.kindName(request.kind)), max_tokens \(request.maxTokens)"
        if let image = request.image { header += ", image \(image.id) \(image.width)x\(image.height), marks \(request.marks.count)" }
        header += systemChanged ? ")" : ", system prompt unchanged)"
        record("prompt", header, request.user)
    }

    /// Records one driver update (effects and events summarised; prompts are recorded by
    /// `RecordingModel`).
    public func record(_ update: DriverUpdate) {
        switch update {
        case .started(let task):
            record("task", "started \(task.id)", task.text)
        case .effect(let effect, let number):
            record("effect", "#\(number) \(Self.describe(effect))")
        case .progress(let p):
            record("progress", "step \(p.step) phase \(p.phase) model_calls \(p.modelCalls) parse_retries \(p.parseRetries) app \(p.app ?? "-")")
        case .confirmationPending(let request):
            record("confirm", "confirmation pending: \(request.category) in \(request.app)", request.summary)
        case .questionPending(let question):
            record("question", "question pending", question)
        case .ioFailed(let kind, let message, let willRetry):
            record("io_failed", "\(kind.rawValue) failed\(willRetry ? ", retrying" : "")", message)
        case .note(let note):
            record("note", "note", note)
        case .awaitingUnlock(let reason, let waited):
            record("lock", "awaiting unlock (waited \(waited) ms)", reason)
        case .lockDetected(let kind):
            record("lock", "lock detected while waiting for \(kind.rawValue)")
        case .unlockWaitEnded:
            record("lock", "unlock wait ended")
        case .effectCompleted(let effect, let event):
            record("event", "\(Self.describe(event)) (for \(Self.effectName(effect)))")
        case .finished(let result):
            record("result", "finished: \(result.outcome.status)", Self.describe(result))
        }
    }

    /// The run in order, one compact line per model reply, script reason, executed command,
    /// owner decision and IO failure, for failure messages in the job log. Longer runs keep
    /// the last `maxLines` lines and say how many were left out.
    public func timeline(maxLines: Int = 40) -> String {
        var lines: [String] = []
        for e in entries {
            switch e.kind {
            case "reply": lines.append("model: \(Self.oneLine(e.text ?? e.title, 200))")
            case "script": lines.append("  why: \(Self.oneLine(e.text ?? "", 200))")
            case "command": lines.append("  act: \(Self.oneLine(e.title, 200))")
            case "owner": lines.append("owner: \(Self.oneLine(e.title, 160))")
            case "io_failed": lines.append("io failed: \(Self.oneLine(e.title + " " + (e.text ?? ""), 200))")
            case "interruption": lines.append("interruption: \(Self.oneLine(e.text ?? "", 160))")
            case "lock": lines.append("lock: \(Self.oneLine(e.title, 160))")
            default: break
            }
        }
        let cap = max(1, maxLines)
        guard lines.count > cap else { return lines.joined(separator: "\n") }
        return (["(\(lines.count - cap) earlier lines omitted)"] + lines.suffix(cap)).joined(separator: "\n")
    }

    static func oneLine(_ text: String, _ limit: Int) -> String {
        let flat = text.split(whereSeparator: \.isNewline).joined(separator: " ")
        return flat.count > limit ? String(flat.prefix(limit - 1)) + "…" : flat
    }

    /// The transcript as pretty-printed JSON (`{"name": …, "entries": […]}`).
    public func jsonData() -> Data {
        struct File: Encodable {
            var name: String
            var entries: [Entry]
        }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        return (try? encoder.encode(File(name: name, entries: entries))) ?? Data("{}".utf8)
    }

    // MARK: - Descriptions

    public static func describe(_ result: RunResult) -> String {
        """
        status: \(result.outcome.status)
        summary: \(result.outcome.summary)
        steps: \(result.outcome.steps)
        duration_ms: \(result.outcome.durationMs)
        end_reason: \(result.endReason)
        memory: \(result.memory)
        effects: \(result.effectCount)
        model_calls: \(result.modelCalls)
        """
    }

    public static func effectName(_ effect: Effect) -> String {
        switch effect {
        case .observe: "observe"
        case .infer: "infer"
        case .act: "act"
        case .callTool: "call_tool"
        case .confirm: "confirm"
        case .askUser: "ask_user"
        case .finished: "finished"
        case .awaitUnlock: "await_unlock"
        }
    }

    public static func describe(_ effect: Effect) -> String {
        switch effect {
        case .observe(let screenshot): "observe(screenshot: \(screenshot))"
        case .infer(let r):
            "infer(kind: \(kindName(r.kind)), user: \(r.user.count) chars, image: \(r.image?.id ?? "none"), marks: \(r.marks.count))"
        case .act(let command): "act(\(describe(command)))"
        case .callTool(let call): "call_tool(\(call.name), \(call.argsJson))"
        case .confirm(let r): "confirm(\(r.category), app: \(r.app), \"\(r.summary)\")"
        case .askUser(let q): "ask_user(\"\(q)\")"
        case .finished(let o): "finished(\(o.status), \"\(o.summary)\", steps: \(o.steps), \(o.durationMs) ms)"
        case .awaitUnlock(let reason, let waited): "await_unlock(\"\(reason)\", waited: \(waited) ms)"
        }
    }

    public static func describe(_ command: RunnerCommand) -> String {
        switch command {
        case .tap(let x, let y): "tap(\(x.compactText), \(y.compactText))"
        case .longPress(let x, let y, let ms): "long_press(\(x.compactText), \(y.compactText), \(ms) ms)"
        case .swipe(let from, let to, let ms):
            "swipe((\(from.x.compactText), \(from.y.compactText)) -> (\(to.x.compactText), \(to.y.compactText)), \(ms) ms)"
        case .typeText(let text): "type_text(\(text.count) chars)"
        case .pressKey(let key): "press_key(\(key))"
        case .launchApp(let id): "launch_app(\(id))"
        case .openUrl(let url): "open_url(\(url))"
        case .wait(let ms): "wait(\(ms) ms)"
        }
    }

    public static func describe(_ event: Event) -> String {
        switch event {
        case .start: "start"
        case .observed(let o):
            "observed(app: \(o.screen.appBundleId), nodes: \(countNodes(o.screen.root)), keyboard: \(o.screen.keyboardVisible), locked: \(o.deviceLocked), screenshot: \(o.screenshot?.id ?? "none"))"
        case .observedRunnerText(let s):
            "observed_runner_text(app: \(s.appBundleId), \(s.source.count) chars, locked: \(s.deviceLocked), screenshot: \(s.screenshot?.id ?? "none"))"
        case .modelOutput(let output): "model_output(\(output.text))"
        case .modelFailed(let error): "model_failed(\(error))"
        case .actResult(let r):
            r.ok ? "act_result(ok)" : "act_result(failed\(r.failure == .deviceLocked ? " [device locked]" : ""): \(r.error ?? "?"))"
        case .toolResult(let r): r.ok ? "tool_result(ok, \(r.output))" : "tool_result(failed: \(r.error ?? "?"))"
        case .userDecision(let approved): "user_decision(\(approved ? "approved" : "declined"))"
        case .userAnswer(let text): "user_answer(\(text))"
        case .cancel: "cancel"
        case .deviceLocked: "device_locked"
        case .deviceUnlocked: "device_unlocked"
        case .tick: "tick"
        case .observeFailed(let reason): "observe_failed(\(reason))"
        }
    }

    static func kindName(_ kind: RequestKind) -> String {
        switch kind {
        case .text: "text"
        case .vision: "vision"
        }
    }

    static func countNodes(_ node: UiNode) -> Int {
        node.children.reduce(1) { $0 + countNodes($1) }
    }
}

/// A `ModelProvider` wrapper that records every request and reply (and the scripted
/// model's reason, when the wrapped model is a `ScriptedModel`) into a `Transcript`, and
/// tells `onPrompt` about each request (QA harnesses attach the prompt's screen block to
/// the observation it was built from).
public final class RecordingModel: ModelProvider {
    private let inner: any ModelProvider
    private let transcript: Transcript
    private let onPrompt: @Sendable (ModelRequest) -> Void
    private let requestLog = Mutex<[(request: ModelRequest, reply: String?)]>([])

    public init(_ inner: any ModelProvider, transcript: Transcript, onPrompt: @escaping @Sendable (ModelRequest) -> Void = { _ in }) {
        self.inner = inner
        self.transcript = transcript
        self.onPrompt = onPrompt
    }

    public var capabilities: ModelCapabilities { inner.capabilities }

    /// Requests so far.
    public var requests: [ModelRequest] { requestLog.withLock { $0.map(\.request) } }
    /// Replies so far (in request order; `nil` where the model threw).
    public var replies: [String?] { requestLog.withLock { $0.map(\.reply) } }

    public func generate(_ request: ModelRequest) async throws -> String {
        transcript.recordPrompt(request)
        onPrompt(request)
        let index = requestLog.withLock { log -> Int in
            log.append((request, nil))
            return log.count - 1
        }
        let start = transcript.elapsedMs()
        do {
            let reply = try await inner.generate(request)
            requestLog.withLock { $0[index].reply = reply }
            let elapsed = transcript.elapsedMs() - start
            transcript.record("reply", "reply \(index + 1) (\(elapsed) ms)", reply)
            if let scripted = inner as? ScriptedModel, let decision = scripted.decisions.last {
                transcript.record("script", "script decision \(decision.call)", decision.reason)
            }
            return reply
        } catch {
            transcript.record("reply", "reply \(index + 1) threw", String(describing: error))
            throw error
        }
    }
}
