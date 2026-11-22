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
        case .act(let command):
            run.acts += 1
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
    let trustedRun = drive(memoryJson: "", config: trusted, decide: { _ in false })
    check("a Trusted app toggles a setting with no confirmation",
          trustedRun.outcome?.status == .success && trustedRun.confirms.isEmpty,
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
                               knownApps: [], agentBundleId: nil)
    check("SessionConfig init without skill: defaults to nil", legacy.skill == nil && base.skill == nil)

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

print("\(failures == 0 ? "PASS" : "FAIL") swift smoke: \(passes) passed, \(failures) failed")
exit(failures == 0 ? 0 : 1)
