// Swift smoke test for the Jarvis core bridge. Uses only the generated `JarvisCore` module.
//
// Mirrors `core/jarvis-ffi/tests/common/mod.rs`: Settings -> Wi-Fi -> switch off, with the
// Settings screen sent as a typed tree and the Wi-Fi page as runner text, a canned model
// that reads element numbers from the prompt, and approved confirmations (first use of
// Settings, then the setting). Then replays from the saved memory with zero model calls,
// and checks StepByStep mode, a Trusted app rule and a declined first-use approval.
//
// Built and run by `core/jarvis-ffi/scripts/swift-smoke.sh` (needs the `panic-hooks`
// feature for the containment checks). Prints one PASS/FAIL line per check and exits 1 on
// any failure.

import Foundation
import JarvisCore

var failures = 0
var passes = 0

func check(_ name: String, _ ok: Bool, _ detail: @autoclosure () -> String = "") {
    if ok {
        passes += 1
        print("PASS \(name)")
    } else {
        failures += 1
        let d = detail()
        print("FAIL \(name)\(d.isEmpty ? "" : ": \(d)")")
    }
}

// MARK: - The scenario

let settingsBundle = "com.apple.Preferences"
let screenSize = Size(width: 393, height: 852)

func node(_ role: Role, _ label: String?, _ frame: Rect, hittable: Bool = false,
          value: String? = nil, _ children: [UiNode] = []) -> UiNode {
    UiNode(role: role, label: label, value: value, identifier: nil, placeholder: nil,
           frame: frame, enabled: true, visible: true, hittable: hittable, focused: false,
           selected: false, secure: false, children: children)
}

func settingsScreen() -> Screen {
    func cell(_ title: String, _ detail: String, _ y: Double) -> UiNode {
        node(.cell, nil, Rect(x: 16, y: y, width: 361, height: 44), hittable: true, [
            node(.staticText, title, Rect(x: 60, y: y + 12, width: 200, height: 20)),
            node(.staticText, detail, Rect(x: 260, y: y + 12, width: 100, height: 20)),
        ])
    }
    let nav = node(.navigationBar, nil, Rect(x: 0, y: 59, width: 393, height: 81), [
        node(.staticText, "Settings", Rect(x: 16, y: 96, width: 140, height: 41)),
    ])
    let table = node(.table, nil, Rect(x: 0, y: 140, width: 393, height: 712), [
        cell("Wi-Fi", "HomeNet", 200), cell("Bluetooth", "On", 244),
    ])
    let full = Rect(x: 0, y: 0, width: 393, height: 852)
    let root = node(.application, "Settings", full, [node(.window, nil, full, [nav, table])])
    return Screen(appBundleId: settingsBundle, appName: "Settings", size: screenSize, root: root,
                  keyboardVisible: false, orientation: .portrait)
}

func wifiText(on: Bool) -> String {
    """
    Application, label: 'Settings', {{0, 0}, {393, 852}}
      Window, {{0, 0}, {393, 852}}
        NavigationBar, id: 'Wi-Fi', {{0, 59}, {393, 81}}
          Button, label: 'Settings', id: 'BackButton', {{8, 70}, {90, 44}}
        Table, {{0, 140}, {393, 712}}
          Cell, {{16, 160}, {361, 44}}
            Switch, label: 'Wi-Fi', value: '\(on ? 1 : 0)', {{312, 166}, {51, 31}}
    """
}

func snapshot(_ source: String, bundle: String = settingsBundle) -> RunnerSnapshot {
    RunnerSnapshot(source: source, appBundleId: bundle, appName: "Settings", size: screenSize,
                   keyboardVisible: false, orientation: .portrait, screenshot: nil)
}

func makeConfig() throws -> SessionConfig {
    var config = try defaultSessionConfig()
    config.knownApps = [AppInfo(bundleId: settingsBundle, name: "Settings")]
    return config
}

let task = TaskSpec(id: "t1", text: "Turn off Wi-Fi")

/// The element number of the prompt line `[N] <role> "<label>"`.
func indexInPrompt(_ user: String, role: String, label: String) -> UInt32? {
    let needle = "] \(role) \"\(label)\""
    for line in user.split(separator: "\n") where line.contains(needle) {
        let trimmed = line.drop(while: { $0 == " " })
        guard trimmed.first == "[", let close = trimmed.firstIndex(of: "]") else { continue }
        return UInt32(trimmed[trimmed.index(after: trimmed.startIndex)..<close])
    }
    return nil
}

func contains(_ r: Rect, _ x: Double, _ y: Double) -> Bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

enum Page { case settings, wifiOn, wifiOff }

struct Run {
    var outcome: Outcome?
    var infers = 0
    var acts = 0
    var confirms: [ConfirmRequest] = []
    var typedObservations = 0
    var textObservations = 0
    var memoryJson = ""
    var page = Page.settings
    var unlockWaits = 0
    var error: String?
}

func drive(memoryJson: String, config: SessionConfig? = nil,
           decide: (ConfirmRequest) -> Bool = { _ in true }) -> Run {
    var run = Run()
    let session: JarvisSession
    do {
        session = try JarvisSession(config: try config ?? makeConfig(), task: task,
                                    memoryJson: memoryJson)
    } catch {
        run.error = "init threw \(error)"
        return run
    }
    var script: [(String, String)?] = [("Cell", "Wi-Fi, HomeNet"), ("Switch", "Wi-Fi"), nil]
    var now: UInt64 = 1_000
    var event = Event.start
    for _ in 0..<200 {
        let effect = session.step(nowMs: now, event: event)
        now += 50
        switch effect {
        case .observe:
            switch run.page {
            case .settings:
                run.typedObservations += 1
                event = .observed(observation: ScreenObservation(screen: settingsScreen(), screenshot: nil))
            case .wifiOn, .wifiOff:
                run.textObservations += 1
                event = .observedRunnerText(snapshot: snapshot(wifiText(on: run.page == .wifiOn)))
            }
        case .infer(let request):
            run.infers += 1
            let text: String
            if let step = script.isEmpty ? nil : script.removeFirst() {
                guard let i = indexInPrompt(request.user, role: step.0, label: step.1) else {
                    run.error = "no \(step.0) \"\(step.1)\" in prompt:\n\(request.user)"
                    return run
                }
                text = #"{"thought":"tap","action":"tap","element":\#(i)}"#
            } else {
                text = #"{"thought":"done","action":"done","summary":"Wi-Fi is off"}"#
            }
            event = .modelOutput(output: ModelOutput(text: text))
        case .act(let command, let expect):
            run.acts += 1
            if case .tap = command, expect?.bundleId != "com.apple.Preferences" {
                run.error = "a tap without the app it was planned on: \(String(describing: expect))"
                return run
            }
            if case let .tap(x, y) = command {
                switch run.page {
                case .settings where contains(Rect(x: 16, y: 200, width: 361, height: 44), x, y):
                    run.page = .wifiOn
                case .wifiOn where contains(Rect(x: 312, y: 166, width: 51, height: 31), x, y):
                    run.page = .wifiOff
                default: break
                }
            }
            event = .actResult(result: RunnerResult(ok: true, error: nil))
        case .confirm(let request):
            run.confirms.append(request)
            event = .userDecision(approved: decide(request))
        case .callTool(let call):
            run.error = "unexpected tool call \(call)"
            return run
        case .askUser(let question):
            run.error = "unexpected question \(question)"
            return run
        case .awaitUnlock:
            // Not expected in these scenarios; a real shell shows "Unlock to continue".
            run.unlockWaits += 1
            event = .deviceUnlocked
        case .finished(let outcome):
            run.outcome = outcome
            do {
                run.memoryJson = try session.finish()
                let again = try session.finish()
                if again != run.memoryJson { run.error = "second finish() differs" }
            } catch {
                run.error = "finish threw \(error)"
            }
            return run
        }
    }
    run.error = "did not finish in 200 steps"
    return run
}

func jsonObject(_ s: String) -> [String: Any]? {
    guard let data = s.data(using: .utf8) else { return nil }
    return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
}

// MARK: - Checks

print("JarvisCore \(coreVersion()) smoke test")

// 1. Full session from fresh memory.
let first = drive(memoryJson: "")
check("session 1 ran without driver errors", first.error == nil, first.error ?? "")
check("session 1 finished with success", first.outcome?.status == .success,
      "\(String(describing: first.outcome))")
check("session 1 summary", first.outcome?.summary == "Wi-Fi is off", first.outcome?.summary ?? "nil")
check("session 1 used 3 model calls", first.infers == 3, "\(first.infers)")
check("session 1 sent 2 runner commands", first.acts == 2, "\(first.acts)")
check("session 1 observed a typed screen", first.typedObservations >= 1, "\(first.typedObservations)")
check("session 1 observed runner text", first.textObservations >= 2, "\(first.textObservations)")
check("session 1 asked first use of Settings, then the setting",
      first.confirms.count == 2 && first.confirms.first?.category == .firstUse
          && first.confirms.first?.summary == "Let Jarvis act in Settings (com.apple.Preferences)?"
          && first.confirms.last?.category == .systemSetting
          && first.confirms.last?.app == settingsBundle,
      "\(first.confirms)")
check("every question is its rows joined, each row's display text set",
      !first.confirms.isEmpty && first.confirms.allSatisfy { q in
          !q.rows.isEmpty && q.rows.map { $0.text }.joined() == q.summary
              && q.rows.allSatisfy { !$0.display.isEmpty && !$0.display.hasPrefix(". ") }
      },
      "\(first.confirms)")
if let setting = first.confirms.last {
    // The runner-text screen's warning is its own row; the setting tap states its target.
    check("the setting question starts with its warning row and states a tap on a target",
          setting.rows.first?.kind == .warning && setting.rows.contains { $0.kind == .action }
              && setting.stage == .whole && setting.verb == .tap
              && setting.target?.element != nil && setting.target?.frame != nil
              && setting.target?.digest.count == 64,
          "\(setting)")
}
check("a request built from text alone has no rows and no target",
      ConfirmRequest(category: .delete, summary: "Delete", app: "a", phoneOnly: false, stage: .whole, verb: .other).rows.isEmpty
          && ConfirmRequest(category: .delete, summary: "Delete", app: "a", phoneOnly: false, stage: .whole, verb: .other).target == nil)
check("the flow and insert row kinds are their own kinds, not notes",
      QuestionRow(kind: .flow, text: ". x", display: "x").kind == .flow
          && QuestionRow(kind: .insert, text: ". y", display: "y").kind == .insert
          && QuestionRowKind.flow != .note && QuestionRowKind.insert != .note)
check("session 1 left the switch off", first.page == .wifiOff)
let memory = jsonObject(first.memoryJson)
check("finish() returned memory JSON that parses", memory != nil, String(first.memoryJson.prefix(200)))
let flows = (memory?["flows"] as? [String: Any])?["flows"] as? [Any]
check("memory holds one learned flow", flows?.count == 1, "\(String(describing: flows?.count))")
check("memory remembers the approved app",
      (memory?["approved_apps"] as? [String]) == [settingsBundle],
      "\(String(describing: memory?["approved_apps"]))")

// 2. Replay from that memory.
let second = drive(memoryJson: first.memoryJson)
check("session 2 ran without driver errors", second.error == nil, second.error ?? "")
check("session 2 finished with success", second.outcome?.status == .success,
      "\(String(describing: second.outcome))")
check("session 2 replayed with zero Infer effects", second.infers == 0, "\(second.infers)")
check("session 2 replay passed the same confirmation gate (no first-use prompt)",
      second.confirms.count == 1 && second.confirms.first?.category == .systemSetting,
      "\(second.confirms)")
check("session 2 left the switch off", second.page == .wifiOff)

// 2b. The policy model: defaults, StepByStep, a Trusted rule, a declined first use.
do {
    let p = try defaultPolicy()
    check("defaultPolicy() has first-use approval on and Guarded mode",
          p.firstUseApproval && p.mode == .guarded && p.appRules.isEmpty && p.denyCategories.isEmpty)
    check("defaultPolicy() confirms unverified, call and first-use",
          p.confirm.contains(.unverified) && p.confirm.contains(.call) && p.confirm.contains(.firstUse))

    var step = try makeConfig()
    step.policy.mode = .stepByStep
    step.policy.firstUseApproval = false
    let stepRun = drive(memoryJson: "", config: step)
    check("StepByStep run succeeds", stepRun.error == nil && stepRun.outcome?.status == .success,
          stepRun.error ?? "\(String(describing: stepRun.outcome))")
    check("StepByStep asks for the plain row tap, then the setting",
          stepRun.confirms.map { $0.category } == [RiskCategory.none, RiskCategory.systemSetting],
          "\(stepRun.confirms)")

    var trusted = try makeConfig()
    trusted.policy.appRules = [AppRule(bundleIdPrefix: settingsBundle, level: .trusted)]
    // Sixth review: the Wi-Fi page here is read from runner text, and a text screen always
    // confirms whatever the trust level; the typed Settings screen asks nothing.
    let trustedRun = drive(memoryJson: "", config: trusted, decide: { _ in true })
    check("a Trusted app asks only on the runner-text screen",
          trustedRun.outcome?.status == .success && trustedRun.confirms.count == 1
              && trustedRun.confirms[0].summary.hasPrefix("Warning: this screen was read through an unverified text format"),
          "\(trustedRun.confirms) \(String(describing: trustedRun.outcome))")

    let s = try JarvisSession(config: try makeConfig(), task: task, memoryJson: "")
    _ = s.step(nowMs: 1, event: .start)
    var effect = s.step(nowMs: 2, event: .observed(observation: ScreenObservation(screen: settingsScreen(), screenshot: nil)))
    func tapRow(_ e: Effect) -> Event? {
        guard case let .infer(request) = e,
              let i = indexInPrompt(request.user, role: "Cell", label: "Wi-Fi, HomeNet") else { return nil }
        return .modelOutput(output: ModelOutput(text: #"{"action":"tap","element":\#(i)}"#))
    }
    if let tap = tapRow(effect) {
        effect = s.step(nowMs: 3, event: tap)
        if case let .confirm(request) = effect {
            check("first action in Settings asks first-use approval", request.category == .firstUse,
                  "\(request)")
        } else {
            check("first action in Settings asks first-use approval", false, "\(effect)")
        }
        effect = s.step(nowMs: 4, event: .userDecision(approved: false))
        check("progress() counts model calls", (try s.progress()).modelCalls == 2)
        if let again = tapRow(effect) {
            effect = s.step(nowMs: 5, event: again)
        }
        if case let .finished(outcome) = effect {
            check("a declined first use ends the session blocked", outcome.status == .blocked,
                  outcome.summary)
        } else {
            check("a declined first use ends the session blocked", false, "\(effect)")
        }
        let mem = jsonObject(try s.finish())
        check("a declined app is not remembered as approved",
              (mem?["approved_apps"] as? [String]) == [], "\(String(describing: mem?["approved_apps"]))")
    } else {
        check("first-use decline scenario reached the model", false, "\(effect)")
    }
} catch {
    check("policy model checks", false, "\(error)")
}

// 3. Memory errors throw.
for bad in ["{", "not json", "[1]", #"{"schema_version": 999}"#] {
    do {
        _ = try JarvisSession(config: try makeConfig(), task: task, memoryJson: bad)
        check("invalid memory \(bad) throws", false, "accepted")
    } catch let JarvisError.InvalidMemory(message) {
        check("invalid memory \(bad) throws InvalidMemory", !message.isEmpty)
    } catch {
        check("invalid memory \(bad) throws InvalidMemory", false, "\(error)")
    }
}

// 4. Contract strings.
do {
    let schema = try actionJsonSchema()
    check("actionJsonSchema() is non-empty JSON", !schema.isEmpty && jsonObject(schema) != nil)
    check("actionGbnf() is non-empty", try actionGbnf().contains("root ::="))
    check("systemPrompt() is non-empty", !(try systemPrompt()).isEmpty)
    check("coreVersion() is non-empty", !coreVersion().isEmpty)
} catch {
    check("contract functions do not throw", false, "\(error)")
}

// 4b. The Live view's privacy rule, decided by the core.
do {
    let otp = try redactScreenText(text: "Your verification code is 482913.")
    check("redactScreenText masks a one-time code", !otp.contains("482913") && otp.hasPrefix("Your verification code is "), otp)
    check("redactScreenText masks a card number", !(try redactScreenText(text: "Visa 4111 1111 1111 1111")).contains("4111"))
    check("redactScreenText keeps plain text", try redactScreenText(text: "Send") == "Send")
    check("fieldValueIsHidden: a 'Verification code' field",
          try fieldValueIsHidden(role: .textField, label: "Verification code", placeholder: nil, identifier: nil, secure: false))
    check("fieldValueIsHidden: a secure field",
          try fieldValueIsHidden(role: .textField, label: "Name", placeholder: nil, identifier: nil, secure: true))
    check("fieldValueIsHidden: a plain 'Email' field is not hidden",
          !(try fieldValueIsHidden(role: .textField, label: "Email", placeholder: nil, identifier: nil, secure: false)))
} catch {
    check("Live view privacy functions do not throw", false, "\(error)")
}

// 5. Runner text parsing and the parse-failure path.
do {
    let screen = try parseRunnerTextToScreen(snapshot: snapshot(wifiText(on: true)))
    check("parseRunnerTextToScreen builds the tree",
          screen.root.role == .application && screen.root.children.first?.role == .window)
} catch {
    check("parseRunnerTextToScreen builds the tree", false, "\(error)")
}
do {
    _ = try parseRunnerTextToScreen(snapshot: snapshot("garbage"))
    check("parseRunnerTextToScreen throws on garbage", false, "accepted")
} catch JarvisError.RunnerText {
    check("parseRunnerTextToScreen throws on garbage", true)
} catch {
    check("parseRunnerTextToScreen throws on garbage", false, "\(error)")
}
do {
    let s = try JarvisSession(config: try makeConfig(), task: task, memoryJson: "")
    _ = s.step(nowMs: 1, event: .start)
    let effect = s.step(nowMs: 2, event: .observedRunnerText(snapshot: snapshot("<<garbage>>")))
    if case .infer = effect { check("garbage runner text is observed as an empty screen", true) }
    else { check("garbage runner text is observed as an empty screen", false, "\(effect)") }
    check("lastObservationIssue() reports it", (try s.lastObservationIssue())?.contains("empty screen") == true)
    check("progress() works", (try s.progress()).phase == .thinking)
} catch {
    check("runner-text failure path", false, "\(error)")
}

// 6. Panic containment (library built with the `panic-hooks` feature).
do {
    let s = try JarvisSession(config: try makeConfig(), task: task, memoryJson: "")
    _ = s.step(nowMs: 1, event: .start)
    s.debugPanicOnNextStep()
    let effect = s.step(nowMs: 2, event: .cancel)
    if case let .finished(outcome) = effect {
        check("a Rust panic in step() finishes the session failed",
              outcome.status == .failed && outcome.summary.hasPrefix("internal error"), outcome.summary)
    } else {
        check("a Rust panic in step() finishes the session failed", false, "\(effect)")
    }
    if case .finished = s.step(nowMs: 3, event: .start) {
        check("the poisoned session stays finished", true)
    } else {
        check("the poisoned session stays finished", false)
    }
    do {
        _ = try s.finish()
        check("finish() on a poisoned session throws", false, "returned memory")
    } catch JarvisError.Poisoned {
        check("finish() on a poisoned session throws Poisoned", true)
    }
} catch {
    check("panic containment in step", false, "\(error)")
}
do {
    try debugPanic(message: "boom")
    check("a Rust panic in another export throws", false, "did not throw")
} catch let JarvisError.Internal(message) {
    check("a Rust panic in another export throws Internal", message.contains("boom"), message)
} catch {
    check("a Rust panic in another export throws Internal", false, "\(error)")
}

// 7. Two threads stepping one session.
do {
    let s = try JarvisSession(config: try makeConfig(), task: task, memoryJson: "")
    let lock = NSLock()
    var effects: [Effect] = []
    DispatchQueue.concurrentPerform(iterations: 2) { i in
        let e = s.step(nowMs: UInt64(10 + i), event: .start)
        lock.lock(); effects.append(e); lock.unlock()
    }
    let observes = effects.filter { if case .observe = $0 { return true } else { return false } }.count
    check("concurrent step() calls are serialized", observes == 1 && effects.count == 2, "\(effects)")
} catch {
    check("concurrent step() calls", false, "\(error)")
}

// 8. Skills: the store, the matcher and a session running a confined skill.
do {
    // The new `skill` field defaults to nil, so the memberwise init without it compiles.
    let base = try defaultSessionConfig()
    let legacy = SessionConfig(policy: base.policy, tools: [], model: base.model,
                               knownApps: [])
    check("SessionConfig init without skill: defaults to nil", legacy.skill == nil && base.skill == nil)
    // Twenty-fifth review: Jarvis's own ids are an exact list, empty by default.
    check("SessionConfig agentBundleIds defaults to empty", legacy.agentBundleIds.isEmpty && base.agentBundleIds.isEmpty)
    let own = SessionConfig(policy: base.policy, tools: [], model: base.model, knownApps: [],
                            agentBundleIds: ["dev.jarvis.app", "dev.jarvis.app.runner.xctrunner"])
    check("SessionConfig agentBundleIds round-trips", own.agentBundleIds == ["dev.jarvis.app", "dev.jarvis.app.runner.xctrunner"])

    let skills = try JarvisSkills(json: "")
    let wifi = Skill(id: "wifi-off", name: "Wi-Fi off", summary: "", kind: .written,
                     triggers: ["turn off wi-fi", "text {contact} {message}"],
                     instructions: "Use the switch on the Wi-Fi page.",
                     apps: [settingsBundle], confineToApps: true,
                     params: [SkillParam(name: "contact", description: "", required: false),
                              SkillParam(name: "message", description: "", required: false)],
                     flowKeys: [], enabled: true, createdMs: 1, updatedMs: 1, runs: 0, lastRunMs: nil)
    check("JarvisSkills.upsert inserts", try skills.upsert(skill: wifi))
    var bad = wifi
    bad.id = "Not An Id"
    do {
        _ = try skills.upsert(skill: bad)
        check("an invalid skill throws InvalidSkill", false, "accepted")
    } catch let JarvisError.InvalidSkill(message) {
        check("an invalid skill throws InvalidSkill", message.contains("invalid id"), message)
    }
    let result = try skills.matchTask(text: "Please turn off the WiFi")
    check("matchTask finds the skill", result.matches.first?.skillId == "wifi-off"
          && result.matches.first?.score == 1000 && !result.ambiguous, "\(result)")
    check("an exact match may run without asking",
          result.matches.first?.exact == true && !result.needsOwnerChoice, "\(result)")
    let opposite = try skills.matchTask(text: "turn on wi-fi")
    check("the opposite request does not match", opposite.matches.isEmpty, "\(opposite)")
    let slotted = try skills.matchTask(text: "text mom i am on my way")
    check("matchTask captures slots", slotted.matches.first?.args["contact"] == "mom"
          && slotted.matches.first?.args["message"] == "i am on my way", "\(slotted)")
    let json = try skills.toJson()
    let reloaded = try JarvisSkills(json: json)
    check("skills JSON round-trips", try reloaded.list().map { $0.id } == ["wifi-off"]
          && (try reloaded.loadIssues()).isEmpty)
    do {
        _ = try JarvisSkills(json: "{broken")
        check("a corrupt skills snapshot throws InvalidSkill", false, "accepted")
    } catch JarvisError.InvalidSkill {
        check("a corrupt skills snapshot throws InvalidSkill", true)
    }
    let damagedJson = #"{"schema_version":1,"skills":[{"id":"kept","name":"Kept","runs":-1}]}"#
    let damaged = try JarvisSkills(json: damagedJson)
    let resaved = try JarvisSkills(json: try damaged.toJson())
    check("an unloadable skill is reported and survives a save",
          try damaged.hasLoadIssues() && (try resaved.hasLoadIssues())
              && (try resaved.toJson()).contains("\"kept\""))
    do {
        var stale = result.matches.first!
        stale.args["colour"] = "red"
        _ = try skills.contextForMatch(m: stale)
        check("a forged match is refused", false, "accepted")
    } catch JarvisError.InvalidSkill {
        check("a forged match is refused", true)
    }

    if let m = result.matches.first {
        var config = try makeConfig()
        config.skill = try skills.contextForMatch(m: m)
        check("contextForMatch carries confinement", config.skill?.confineToApps == true
              && config.skill?.apps == [settingsBundle])
        // The usual Wi-Fi run inside the skill: the skill adds guidance, keeps both prompts.
        let skillRun = drive(memoryJson: "", config: config)
        check("a session running a confined skill in its own app succeeds",
              skillRun.error == nil && skillRun.outcome?.status == .success,
              skillRun.error ?? "\(String(describing: skillRun.outcome))")
        check("the skill did not skip first-use or the setting confirmation",
              skillRun.confirms.map { $0.category } == [RiskCategory.firstUse, RiskCategory.systemSetting],
              "\(skillRun.confirms)")

        // Escaping the skill's apps is refused, then blocked.
        let s = try JarvisSession(config: config, task: task, memoryJson: "")
        _ = s.step(nowMs: 1, event: .start)
        var effect = s.step(nowMs: 2, event: .observed(observation: ScreenObservation(screen: settingsScreen(), screenshot: nil)))
        if case let .infer(request) = effect {
            check("the prompt carries the owner's guidance",
                  request.user.contains("Owner's guidance for this task (skill: Wi-Fi off):"), request.user)
        }
        let escape = Event.modelOutput(output: ModelOutput(text: #"{"action":"open_app","app":"com.example.shop"}"#))
        effect = s.step(nowMs: 3, event: escape)
        effect = s.step(nowMs: 4, event: escape)
        if case let .finished(outcome) = effect {
            check("leaving a confined skill's apps twice blocks the session",
                  outcome.status == .blocked && outcome.summary.contains("works only in its own apps"),
                  outcome.summary)
        } else {
            check("leaving a confined skill's apps twice blocks the session", false, "\(effect)")
        }
    } else {
        check("skill session checks reached a match", false)
    }
} catch {
    check("skills checks", false, "\(error)")
}

// 9. A locked phone, an unreadable screen, typed reasons on runner results.
do {
    check("session 1 never waited for an unlock", first.unlockWaits == 0)
    let p = try defaultPolicy()
    check("defaultPolicy() waits at most 30 minutes for an unlock", p.maxLockedWaitMs == 1_800_000,
          "\(p.maxLockedWaitMs)")
    check("defaultPolicy() looks again before acting on a plan older than 1 s", p.maxPlanAgeMs == 1_000,
          "\(p.maxPlanAgeMs)")
    // Stale-screen guard: `Effect.act` without `expect` still compiles (default nil), and a
    // runner's stale refusal is a typed reason.
    let bare = Effect.act(command: .wait(ms: 1))
    if case .act(_, let expect) = bare { check("Effect.act init without expect: defaults to nil", expect == nil) }
    let stale = RunnerResult(ok: false, error: "another app is in front", failure: .stale)
    check("RunnerFailure.stale exists", stale.failure == .stale)
    // Initializers written before the new fields still compile (defaults).
    let plain = RunnerResult(ok: true, error: nil)
    check("RunnerResult init without failure: defaults to nil", plain.failure == nil)
    let obs = ScreenObservation(screen: settingsScreen(), screenshot: nil)
    check("ScreenObservation init without deviceLocked: defaults to false", !obs.deviceLocked)
    check("RunnerSnapshot init without deviceLocked: defaults to false", !snapshot("").deviceLocked)

    let s = try JarvisSession(config: try makeConfig(), task: task, memoryJson: "")
    _ = s.step(nowMs: 1_000, event: .start)
    let lockedObs = ScreenObservation(screen: settingsScreen(), screenshot: nil, deviceLocked: true)
    var effect = s.step(nowMs: 1_010, event: .observed(observation: lockedObs))
    if case let .awaitUnlock(reason, waitedMs) = effect {
        check("a locked observation asks the shell to wait for an unlock",
              waitedMs == 0 && reason.contains("Unlock"), reason)
    } else {
        check("a locked observation asks the shell to wait for an unlock", false, "\(effect)")
    }
    let progress = try s.progress()
    check("progress() reports the wait",
          progress.waitingForUnlock && progress.phase == .waitingForUnlock, "\(progress)")
    effect = s.step(nowMs: 61_010, event: .tick)
    if case let .awaitUnlock(_, waitedMs) = effect {
        check("tick re-emits awaitUnlock with the time waited", waitedMs == 60_000, "\(waitedMs)")
    } else {
        check("tick re-emits awaitUnlock with the time waited", false, "\(effect)")
    }
    effect = s.step(nowMs: 62_000, event: .deviceLocked)
    if case .awaitUnlock = effect { check("deviceLocked while waiting keeps waiting", true) }
    else { check("deviceLocked while waiting keeps waiting", false, "\(effect)") }
    effect = s.step(nowMs: 63_000, event: .deviceUnlocked)
    if case .observe(let shot) = effect { check("deviceUnlocked resumes by observing", !shot) }
    else { check("deviceUnlocked resumes by observing", false, "\(effect)") }
    effect = s.step(nowMs: 63_100, event: .observeFailed(reason: "snapshot timed out"))
    if case .observe = effect { check("observeFailed asks to observe again", true) }
    else { check("observeFailed asks to observe again", false, "\(effect)") }
    var locked = snapshot(wifiText(on: true))
    locked.deviceLocked = true
    effect = s.step(nowMs: 63_200, event: .observedRunnerText(snapshot: locked))
    if case .awaitUnlock = effect { check("a locked runner snapshot waits too", true) }
    else { check("a locked runner snapshot waits too", false, "\(effect)") }
    effect = s.step(nowMs: 63_300, event: .observed(observation: obs))
    if case .infer = effect { check("an unlocked observation while waiting resumes", true) }
    else { check("an unlocked observation while waiting resumes", false, "\(effect)") }
    let lockedTotal = (try s.progress()).lockedTotalMs
    check("progress() totals the locked time", lockedTotal == 62_090, "\(lockedTotal)")

    // A refused command and a wait that runs out.
    var short = try makeConfig()
    short.policy.maxLockedWaitMs = 1_000
    short.policy.firstUseApproval = false
    let t = try JarvisSession(config: short, task: task, memoryJson: "")
    _ = t.step(nowMs: 1, event: .start)
    effect = t.step(nowMs: 2, event: .observed(observation: obs))
    if case let .infer(request) = effect,
       let i = indexInPrompt(request.user, role: "Cell", label: "Wi-Fi, HomeNet") {
        effect = t.step(nowMs: 3, event: .modelOutput(output: ModelOutput(text: #"{"action":"tap","element":\#(i)}"#)))
    }
    if case .act = effect {
        let refused = RunnerResult(ok: false, error: "screen_locked", failure: .deviceLocked)
        effect = t.step(nowMs: 4, event: .actResult(result: refused))
        if case .awaitUnlock = effect { check("a command refused as locked waits for an unlock", true) }
        else { check("a command refused as locked waits for an unlock", false, "\(effect)") }
        effect = t.step(nowMs: 5_000, event: .tick)
        if case let .finished(outcome) = effect {
            check("staying locked past maxLockedWaitMs finishes interrupted",
                  outcome.status == .interrupted && outcome.summary.contains("stayed locked"),
                  outcome.summary)
        } else {
            check("staying locked past maxLockedWaitMs finishes interrupted", false, "\(effect)")
        }
        let mem = jsonObject(try t.finish())
        let log = ((mem?["log"] as? [String: Any])?["entries"] as? [[String: Any]])?.last
        check("the task log records the interruption",
              log?["outcome"] as? String == "interrupted" && log?["lock_waits"] as? Int == 1,
              "\(String(describing: log))")
    } else {
        check("lock-refusal scenario reached an act", false, "\(effect)")
    }
} catch {
    check("lock awareness checks", false, "\(error)")
}

// 10. Workflows: the store, utterance resolution and a short run driven end to end.
do {
    let skills = try JarvisSkills(json: "")
    _ = try skills.upsert(skill: Skill(id: "wifi-off", name: "Wi-Fi off", summary: "", kind: .written,
                                       triggers: ["turn off wi-fi"], instructions: "Use the switch.",
                                       apps: [settingsBundle], confineToApps: true, params: [],
                                       flowKeys: [], enabled: true, createdMs: 1, updatedMs: 1,
                                       runs: 0, lastRunMs: nil))
    let workflows = try JarvisWorkflows(json: "")
    let morning = Workflow(id: "morning", name: "Morning", enabled: true,
                           trigger: .phrase(text: "good morning"),
                           steps: [.runSkill(skillId: "wifi-off", args: [:]),
                                   .askOwner(question: "Tell me when done?"),
                                   .stopUnless(condition: .ownerSaidYes),
                                   .notify(text: "Wi-Fi is off")],
                           onFailure: .continue, createdMs: 0, updatedMs: 0, runs: 0,
                           lastRunMs: nil, lastOutcome: nil)
    check("JarvisWorkflows.upsert inserts", try workflows.upsert(workflow: morning, nowMs: 10))
    check("no warnings for a workflow whose skills exist",
          try workflows.warnings(workflow: morning, skills: skills).isEmpty)
    var broken = morning
    broken.steps = [.runSkill(skillId: "missing", args: [:])]
    check("a step naming a missing skill is a warning",
          try workflows.warnings(workflow: broken, skills: skills).count == 1)
    broken.steps = []
    do {
        _ = try workflows.upsert(workflow: broken, nowMs: 11)
        check("a workflow without steps throws InvalidWorkflow", false, "accepted")
    } catch JarvisError.InvalidWorkflow {
        check("a workflow without steps throws InvalidWorkflow", true)
    }
    let resolved = try resolveUtterance(skills: skills, workflows: workflows, text: "Good morning")
    check("resolveUtterance prefers the exact workflow phrase",
          resolved.resolution == .runWorkflow(workflowId: "morning") && !resolved.needsOwnerChoice,
          "\(resolved)")

    let run = try JarvisWorkflowRun(workflow: morning, limits: try defaultRunLimits())
    var effect = try run.step(nowMs: 100, event: .start, skills: skills)
    if case let .startSession(task, skill) = effect {
        check("a skill step starts a session with its SkillContext",
              task == "turn off wi-fi" && skill?.id == "wifi-off" && skill?.confineToApps == true,
              "\(effect)")
    } else {
        check("a skill step starts a session with its SkillContext", false, "\(effect)")
    }
    do {
        _ = try run.step(nowMs: 101, event: .notified, skills: skills)
        check("a wrong answer throws WorkflowProtocol", false, "accepted")
    } catch let JarvisError.WorkflowProtocol(got, expected) {
        check("a wrong answer throws WorkflowProtocol and changes nothing",
              try got == "notified" && expected == "session_finished" && (try run.awaiting()) == .session,
              "\(got) \(expected)")
    }
    effect = try run.step(nowMs: 200, event: .sessionFinished(status: .success, summary: "Wi-Fi off"),
                          skills: skills)
    check("then the question", effect == .askOwner(question: "Tell me when done?"), "\(effect)")
    effect = try run.step(nowMs: 300, event: .ownerAnswered(yes: true, text: ""), skills: skills)
    check("a yes passes stopUnless(ownerSaidYes)", effect == .notify(text: "Wi-Fi is off"), "\(effect)")
    effect = try run.step(nowMs: 400, event: .notified, skills: skills)
    if case let .finished(outcome) = effect {
        check("the run succeeds", outcome.status == .succeeded && outcome.stepsRun == 4, "\(outcome)")
        try workflows.recordRun(id: "morning", nowMs: 400, outcome: outcome)
        let reloaded = try JarvisWorkflows(json: try workflows.toJson())
        check("recordRun survives a JSON round trip",
              try reloaded.get(id: "morning")?.runs == 1 && (try reloaded.loadIssues()).isEmpty)
    } else {
        check("the run succeeds", false, "\(effect)")
    }
    do {
        _ = try weekdayFromIndex(index: 7)
        check("weekday 7 throws InvalidSchedule", false, "accepted")
    } catch JarvisError.InvalidSchedule {
        check("weekday 7 throws InvalidSchedule", true)
    }
} catch {
    check("workflow checks", false, "\(error)")
}

// 11. Remote control: pair, reconnect and start a task between a hub and a client in-process.
do {
    func entropy() -> Data {
        var g = SystemRandomNumberGenerator()
        return Data((0..<32).map { _ in UInt8.random(in: 0...255, using: &g) })
    }
    let hub = try JarvisRemoteHub(storeJson: "", phoneName: "Smoke phone")
    do {
        _ = try hub.openPairing(nowMs: 1, entropy: Data(count: 32))
        check("all-zero entropy throws RemoteEntropy", false, "accepted")
    } catch let JarvisError.RemoteEntropy(problem) {
        check("all-zero entropy throws RemoteEntropy(.notRandom)", problem == .notRandom)
    }
    var keychain = ""
    var log: [HubEffect] = []
    var now: UInt64 = 1_000
    // Carries out hub effects in order; returns the bytes for the client.
    func apply(_ fx: [HubEffect]) throws -> [Data] {
        var out: [Data] = []
        for e in fx {
            switch e {
            case let .send(_, bytes): out.append(bytes)
            case .storeChanged: keychain = try hub.toJson(); log.append(e)
            default: log.append(e)
            }
        }
        return out
    }
    func pump(_ conn: UInt64, _ client: JarvisRemoteClient, _ first: [Data]) throws -> [ClientEvent] {
        var toClient = first
        var seen: [ClientEvent] = []
        for _ in 0..<50 {
            var toPhone: [Data] = []
            for bytes in toClient {
                for ev in try client.receive(bytes: bytes) {
                    if case let .send(b) = ev { toPhone.append(b) } else { seen.append(ev) }
                }
            }
            toClient = []
            if toPhone.isEmpty { break }
            for b in toPhone {
                now += 10
                toClient += try apply(try hub.receive(connId: conn, bytes: b, nowMs: now))
            }
        }
        return seen
    }
    func messages(_ events: [ClientEvent]) -> [ServerMessage] {
        events.compactMap { if case let .message(m) = $0 { return m } else { return nil } }
    }

    let offer = try hub.openPairing(nowMs: now, entropy: entropy())
    check("a pairing offer has a 39-character code", offer.code.count == 39 && offer.qrPayload.hasPrefix("jarvis-remote:2:"))
    let pairing = try JarvisRemoteClient.pair(code: offer.code, deviceName: "Smoke Mac", entropy: entropy())
    now += 10
    var events = try pump(1, pairing, try apply(try hub.connectionOpened(connId: 1, nowMs: now, entropy: entropy())))
    let credentials = events.compactMap { e -> String? in
        if case let .paired(_, json) = e { return json } else { return nil }
    }.first
    let devices = try hub.devices()
    check("pairing over the in-process protocol", try credentials != nil && devices.count == 1
          && devices.first?.name == "Smoke Mac" && (try pairing.isConnected()), "\(devices)")
    check("the store was persisted on storeChanged", keychain.contains(devices.first?.id ?? "?"))
    check("a new device gets the new-device permissions",
          devices.first?.permissions == (try newDeviceRemotePermissions()))

    now += 10
    _ = try apply(try hub.connectionClosed(connId: 1, nowMs: now))
    let client = try JarvisRemoteClient.connect(credentialsJson: credentials ?? "", entropy: entropy())
    now += 10
    events = try pump(2, client, try apply(try hub.connectionOpened(connId: 2, nowMs: now, entropy: entropy())))
    check("reconnecting with stored credentials", try events.contains(.connected) && (try hub.isAuthenticated(connId: 2)))

    func command(_ m: ClientMessage) throws -> [ServerMessage] {
        now += 10
        let fx = try hub.receive(connId: 2, bytes: try client.send(message: m), nowMs: now)
        return messages(try pump(2, client, try apply(fx)))
    }
    func local(_ e: LocalEvent) throws -> [ServerMessage] {
        now += 10
        return messages(try pump(2, client, try apply(try hub.localEvent(nowMs: now, event: e))))
    }
    _ = try command(.subscribe)
    log = []
    _ = try command(.startTask(text: "Turn off Wi-Fi"))
    let taskId = log.compactMap { e -> UInt64? in
        if case let .startTask(id, _, text) = e, text == "Turn off Wi-Fi" { return id } else { return nil }
    }.first
    check("a remote startTask becomes a startTask effect", try taskId != nil && (try hub.isBusy()), "\(log)")
    check("sessionStarted reaches the remote",
          try local(.sessionStarted(taskId: taskId)) == [.taskStarted])
    let finished = try local(.sessionFinished(taskId: taskId,
        outcome: Outcome(status: .success, summary: "Wi-Fi is off", steps: 3, durationMs: 900)))
    if case let .finished(status, _)? = finished.first {
        check("sessionFinished reaches the remote", try status == .success && !(try hub.isBusy()))
    } else {
        check("sessionFinished reaches the remote", false, "\(finished)")
    }
    check("payment can never be approved remotely",
          try neverRemoteApprove().contains(.payment) && !(try remotelyApprovable(category: .payment)))
    let restored = try JarvisRemoteHub(storeJson: keychain, phoneName: "Smoke phone")
    check("the hub restores from the Keychain JSON", try restored.devices() == devices)
    log = []
    _ = try apply(try hub.revokeDevice(nowMs: now + 10, deviceId: devices.first?.id ?? ""))
    check("revoking closes the device's connection",
          try log.contains(.close(connId: 2, reason: .revoked)) && (try hub.devices()).isEmpty, "\(log)")
} catch {
    check("remote checks", false, "\(error)")
}

// 12. Live view: one feed per driver update, one scene per frame, screens on demand, bytes.
do {
    let viewer = try JarvisViewer(config: CoreViewerConfig(
        maxSteps: 200, framesEnabled: true, knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")]))
    func feed(_ u: CoreViewerUpdate) throws -> CoreViewerChange { try viewer.feed(update: u) }
    let progress = { (step: UInt32, action: Action?) in
        SessionProgress(step: step, phase: .acting, lastAction: action, app: settingsBundle, modelCalls: 1,
                        parseRetries: 0, waitingForUnlock: false, lockedWaitMs: 0, lockedTotalMs: 0)
    }
    var started = try feed(.started(taskId: "smoke"))
    check("viewer: started changes the steps", started.steps && started.generation == 1, "\(started)")
    // A typed screen with a one-time code and a card number on it.
    var screen = settingsScreen()
    screen.root.children[0].children.append(UiNode(
        role: .staticText, label: "Your code is 482913", value: nil, identifier: nil,
        placeholder: nil, frame: Rect(x: 16, y: 400, width: 361, height: 44), enabled: true, visible: true,
        hittable: false, focused: false, selected: false, secure: false, children: []))
    _ = try feed(.effect(effect: .observe(screenshot: false), number: 1))
    started = try feed(.effectCompleted(effect: .observe(screenshot: false),
                                        event: .observed(observation: ScreenObservation(screen: screen, screenshot: nil))))
    check("viewer: an observation adds a screen", started.screens)
    let tap = RunnerCommand.tap(x: 196.5, y: 222)
    _ = try feed(.effect(effect: .act(command: tap), number: 2))
    _ = try feed(.progress(progress: progress(1, .tap(element: 2))))
    _ = try feed(.effectCompleted(effect: .act(command: tap), event: .actResult(result: RunnerResult(ok: true, error: nil))))
    _ = try feed(.effect(effect: .observe(screenshot: false), number: 3))
    _ = try feed(.effectCompleted(effect: .observe(screenshot: false),
                                  event: .observedRunnerText(snapshot: snapshot(wifiText(on: true)))))
    let type = RunnerCommand.typeText(text: "hunter2")
    _ = try feed(.effect(effect: .act(command: type), number: 4))
    _ = try feed(.progress(progress: progress(2, .type(element: nil, text: "hunter2", submit: false))))
    _ = try feed(.effectCompleted(effect: .act(command: type), event: .actResult(result: RunnerResult(ok: true, error: nil))))
    _ = try feed(.finished(status: .success))
    let timeline = try viewer.timeline()
    check("viewer: two steps with captions",
          timeline.steps.map(\.caption) == ["Tap \u{201C}Wi-Fi\u{201D}", "Type text"], "\(timeline.steps.map(\.caption))")
    check("viewer: the activity is finished",
          timeline.activity == .finished(outcome: .success) && timeline.activityCaption == "Done")
    let firstScreen = timeline.steps.first?.screenId ?? 0
    let wire = try viewer.screen(id: firstScreen)
    check("viewer: a screen's wireframe on demand",
          wire?.info.source == .tree && (wire?.elements.contains { $0.label == "Wi-Fi" } ?? false)
              && wire?.info.frameKey == CoreViewerFrameKey(taskId: "smoke", observation: 1))
    let bytes = try viewer.encode()
    let text = String(decoding: bytes, as: UTF8.self)
    check("viewer: the encoding is versioned and holds no secret",
          text.contains("\"version\":\(viewerWireVersion())") && !text.contains("482913") && !text.contains("hunter2"))
    let remote = try JarvisViewer.decoded(bytes: bytes, config: CoreViewerConfig(maxSteps: 200, framesEnabled: false, knownApps: []))
    check("viewer: decoded bytes re-encode identically", try remote.encode() == bytes)
    do {
        _ = try JarvisViewer.decoded(bytes: Data("{\"version\":99}".utf8), config: CoreViewerConfig(maxSteps: 1, framesEnabled: false, knownApps: []))
        check("viewer: bad bytes throw InvalidJson", false, "accepted")
    } catch JarvisError.InvalidJson {
        check("viewer: bad bytes throw InvalidJson", true)
    }
    let query = { (now: Double) in CoreSceneQuery(nowMs: now, reduceMotion: false, viewWidth: 196.5, viewHeight: 426) }
    let live = try viewer.scene(query: query(1_000))
    check("viewer: the first frame plays the newest step",
          live.stepNumber == 2 && live.needsFrames && live.following && live.scale == 0.5, "\(live)")
    let settled = try viewer.scene(query: query(60_000))
    check("viewer: a finished run settles", !settled.needsFrames && settled.caption == "Done" && settled.tone == .success)
    let status = try viewer.replay(action: .stepBack, nowMs: 70_000, reduceMotion: false)
    let replayed = try viewer.scene(query: query(80_000))
    check("viewer: replay shows step 1 with its position",
          status.positionText == "Step 1 of 2" && replayed.stepNumber == 1 && replayed.positionIndex == 1
              && replayed.accessibilityValue == "Tap \u{201C}Wi-Fi\u{201D}, Step 1 of 2" && replayed.targetInView != nil,
          "\(status) \(replayed)")
} catch {
    check("viewer checks", false, "\(error)")
}

// 13. Live view over remote control: the phone's one call per update streams to a paired
//     controller that holds "Watch the screen layout"; nothing without it; revoking ends it.
do {
    func entropy() -> Data {
        var g = SystemRandomNumberGenerator()
        return Data((0..<32).map { _ in UInt8.random(in: 0...255, using: &g) })
    }
    let hub = try JarvisRemoteHub(storeJson: "", phoneName: "Smoke phone")
    var keychain = ""
    var audits: [AuditEvent] = []
    var now: UInt64 = 1_000
    var streamBytes = 0
    var sentWhileOff = 0
    func apply(_ fx: [HubEffect]) throws -> [Data] {
        var out: [Data] = []
        for e in fx {
            switch e {
            case let .send(_, bytes): out.append(bytes)
            case .storeChanged: keychain = try hub.toJson()
            case let .audit(entry): audits.append(entry.event)
            default: break
            }
        }
        return out
    }
    let controllerView = try JarvisViewer(config: CoreViewerConfig(maxSteps: 200, framesEnabled: false, knownApps: []))
    var viewerEvents: [RemoteViewerEvent] = []
    var streamText = ""
    var resyncs = 0
    // Delivers phone → controller bytes, applies the Live view, reports writes; until quiet.
    func pump(_ conn: UInt64, _ client: JarvisRemoteClient, _ first: [Data]) throws -> [ClientEvent] {
        var toClient = first
        var seen: [ClientEvent] = []
        for _ in 0..<200 {
            var toPhone: [Data] = []
            var written = 0
            for bytes in toClient {
                written += bytes.count
                let got = try client.receiveAll(bytes: bytes)
                for ev in got.events {
                    if case let .send(b) = ev { toPhone.append(b) } else { seen.append(ev) }
                }
                for ev in got.viewer {
                    viewerEvents.append(ev)
                    switch ev {
                    case let .delta(b), let .snapshot(b, _, _):
                        streamText += String(decoding: b, as: UTF8.self)
                        if case .needsResync = try controllerView.applyDelta(bytes: b),
                           let request = try client.requestResync() {
                            resyncs += 1
                            toPhone.append(request)
                        }
                    default: break
                    }
                }
            }
            toClient = []
            if written > 0 {
                now += 1
                toClient += try apply(try hub.written(connId: conn, bytes: UInt64(written), nowMs: now))
            }
            for b in toPhone {
                now += 1
                toClient += try apply(try hub.receive(connId: conn, bytes: b, nowMs: now))
            }
            if toClient.isEmpty { break }
        }
        return seen
    }

    let offer = try hub.openPairing(nowMs: now, entropy: entropy())
    let client = try JarvisRemoteClient.pair(code: offer.code, deviceName: "Smoke Mac", entropy: entropy())
    _ = try pump(1, client, try apply(try hub.connectionOpened(connId: 1, nowMs: now, entropy: entropy())))
    let deviceId = try client.deviceId() ?? ""
    let mayWatch = try hub.watchScreen(deviceId: deviceId)
    let listedOff = try hub.devices().first?.watchScreen == false
    check("stream: a new device may not watch the screen layout", !mayWatch && listedOff)
    _ = try pump(1, client, try apply(try hub.receive(connId: 1, bytes: try client.startWatching(), nowMs: now)))
    check("stream: start without the permission is denied and audited",
          viewerEvents == [.denied(reason: .notPermitted)]
              && audits.contains(.denied(request: "start_watching", reason: .notPermitted)), "\(viewerEvents)")

    // The phone's view, fed once per update; every update also streams.
    let phoneView = try JarvisViewer(config: CoreViewerConfig(
        maxSteps: 200, framesEnabled: false, knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")]))
    func update(_ u: CoreViewerUpdate) throws {
        now += 300
        let fed = try phoneView.feedAndStream(update: u, hub: hub, nowMs: now)
        if fed.hubError != nil { check("stream: the hub call failed", false, fed.hubError ?? "") }
        let out = try apply(fed.effects)
        if !(try hub.isWatching(connId: 1)) { sentWhileOff += out.reduce(0) { $0 + $1.count } }
        streamBytes += out.reduce(0) { $0 + $1.count }
        _ = try pump(1, client, out)
    }
    var screen = settingsScreen()
    screen.root.children[0].children.append(UiNode(
        role: .staticText, label: "Your code is 482913", value: nil, identifier: nil,
        placeholder: nil, frame: Rect(x: 16, y: 400, width: 361, height: 44), enabled: true, visible: true,
        hittable: false, focused: false, selected: false, secure: false, children: []))
    try update(.started(taskId: "stream"))
    try update(.effect(effect: .observe(screenshot: false), number: 1))
    try update(.effectCompleted(effect: .observe(screenshot: false),
                                event: .observed(observation: ScreenObservation(screen: screen, screenshot: nil))))
    check("stream: nothing is sent to a device without the permission", sentWhileOff == 0 && streamBytes == 0,
          "\(streamBytes)")

    _ = try pump(1, client, try apply(try hub.setWatchScreen(nowMs: now, deviceId: deviceId, allowed: true)))
    let listedOn = try hub.devices().first?.watchScreen == true
    check("stream: the permission is stored with the device and audited",
          keychain.contains("\"watch_screen\":true") && listedOn
              && audits.contains(.accepted(request: "watch_screen_permission", detail: "on")))
    viewerEvents = []
    _ = try pump(1, client, try apply(try hub.receive(connId: 1, bytes: try client.startWatching(), nowMs: now)))
    if case .snapshot(_, 0, _)? = viewerEvents.first {
        let both = try client.isWatching() && hub.isWatching(connId: 1)
        check("stream: watching starts with a snapshot", both)
    } else {
        check("stream: watching starts with a snapshot", false, "\(viewerEvents)")
    }
    let tap = RunnerCommand.tap(x: 196.5, y: 222)
    try update(.effect(effect: .act(command: tap), number: 2))
    try update(.effectCompleted(effect: .act(command: tap), event: .actResult(result: RunnerResult(ok: true, error: nil))))
    try update(.effect(effect: .observe(screenshot: false), number: 3))
    try update(.effectCompleted(effect: .observe(screenshot: false),
                                event: .observedRunnerText(snapshot: snapshot(wifiText(on: true)))))
    let type = RunnerCommand.typeText(text: "hunter2")
    try update(.effect(effect: .act(command: type), number: 4))
    try update(.effectCompleted(effect: .act(command: type), event: .actResult(result: RunnerResult(ok: true, error: nil))))
    try update(.finished(status: .success))
    let phoneTimeline = try phoneView.timeline()
    let remoteTimeline = try controllerView.timeline()
    check("stream: the controller's steps and activity equal the phone's",
          remoteTimeline.steps == phoneTimeline.steps && remoteTimeline.activity == phoneTimeline.activity
              && remoteTimeline.currentScreenId == phoneTimeline.currentScreenId,
          "\(remoteTimeline.steps.map(\.caption)) vs \(phoneTimeline.steps.map(\.caption))")
    let query = CoreSceneQuery(nowMs: 90_000, reduceMotion: true, viewWidth: 196.5, viewHeight: 426)
    var phoneScene = try phoneView.scene(query: query)
    var remoteScene = try controllerView.scene(query: query)
    phoneScene.generation = 0
    remoteScene.generation = 0
    check("stream: the controller draws the phone's scene", phoneScene == remoteScene, "\(phoneScene) vs \(remoteScene)")
    check("stream: no secret, no pixels in the stream",
          !streamText.contains("482913") && !streamText.contains("hunter2") && !streamText.contains("frame_key"))
    let waiting = try controllerView.needsResync()
    check("stream: no resync on a clean stream", resyncs == 0 && !waiting)

    _ = try pump(1, client, try apply(try hub.setWatchScreen(nowMs: now, deviceId: deviceId, allowed: false)))
    let hubWatching = try hub.isWatching(connId: 1)
    let clientWatching = try client.isWatching()
    check("stream: turning the permission off ends the stream at once",
          viewerEvents.last == .ended(reason: .permissionRevoked) && !hubWatching && !clientWatching,
          "\(String(describing: viewerEvents.last))")
    let afterOff = streamBytes
    try update(.ignored)
    check("stream: nothing more is sent after the permission went off", streamBytes == afterOff)
    let restored = try JarvisRemoteHub(storeJson: keychain, phoneName: "Smoke phone")
    let restoredOn = try restored.watchScreen(deviceId: deviceId)
    check("stream: the permission persists (off) in the store", !restoredOn)
    print("swift-smoke: Live view stream \(streamBytes) bytes, \(viewerEvents.count) viewer events")
} catch {
    check("Live view stream checks", false, "\(error)")
}

print("\(failures == 0 ? "PASS" : "FAIL") swift smoke: \(passes) passed, \(failures) failed")
exit(failures == 0 ? 0 : 1)
