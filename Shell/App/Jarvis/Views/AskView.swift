import SwiftUI
import JarvisKit

/// Ask (idle): status, the prompt, suggested tasks with a Learned/New badge, the composer.
struct AskView: View {
    @EnvironmentObject private var model: AppModel
    @State private var text = ""
    @FocusState private var composerFocused: Bool
    @State private var showingActivity = false

    /// Tasks to offer: the demo tasks in playground mode, else recent distinct tasks.
    private var suggestions: [String] {
        if model.launch.playground { return ["Turn off Wi-Fi", "Delete my account"] }
        var seen: [String] = []
        for record in model.history where !seen.contains(record.task) {
            seen.append(record.task)
            if seen.count == 3 { break }
        }
        return seen
    }

    /// Whether a learned flow exists for `task`. The core's intent key also folds spelling
    /// variants; this compares lowercase words only, which is enough for a badge.
    private func isLearned(_ task: String) -> Bool {
        let words = task.lowercased().split { !$0.isLetter && !$0.isNumber }.joined(separator: " ")
        return model.memory.flows.contains { flow in
            let key = flow.intentKey.lowercased().split { !$0.isLetter && !$0.isNumber }.joined(separator: " ")
            return !key.isEmpty && (key == words || words.contains(key))
        }
    }

    var body: some View {
        NavigationStack {
            content
                .toolbar(.hidden, for: .navigationBar)
                .navigationDestination(isPresented: $showingActivity) { ActivityView() }
        }
    }

    private var content: some View {
        VStack(spacing: 0) {
            header
            ScrollView {
                VStack(alignment: .leading, spacing: 32) {
                    VStack(alignment: .leading, spacing: Theme.gap) {
                        Text("What should I take off your hands?")
                            .font(Theme.largeTitle)
                            .tracking(Theme.largeTitleTracking)
                            .foregroundStyle(Theme.textPrimary)
                            .accessibilityIdentifier("ask.prompt")
                        Text("Jarvis reads the screen as text, taps and types for you, and stops to ask before anything risky.")
                            .font(Theme.body)
                            .foregroundStyle(Theme.textSecondary)
                    }
                    if let error = model.setupError {
                        Label {
                            Text(error).font(Theme.meta)
                        } icon: {
                            Image.decorative(.error, size: Theme.iconStandard)
                        }
                        .foregroundStyle(Theme.errorText)
                        .padding(Theme.gap)
                        .card(fill: Theme.errorFill, radius: Theme.radiusElement)
                        .accessibilityIdentifier("ask.setupError")
                    }
                    if !suggestions.isEmpty {
                        VStack(spacing: 10) {
                            SectionLabel(text: "Try one")
                            ForEach(Array(suggestions.enumerated()), id: \.offset) { index, task in
                                suggestionRow(task, index: index)
                            }
                        }
                    }
                }
                .padding(.horizontal, 20)
                .padding(.top, 32)
                .padding(.bottom, 24)
            }
            composer
        }
        .screenBackground()
    }

    private var header: some View {
        HStack {
            Text("Jarvis")
                .font(.system(size: 17, weight: .bold))
                .foregroundStyle(Theme.textPrimary)
            Spacer()
            StatusPill(status: model.live.status)
            HeaderButton(icon: .away, label: "Away mode", identifier: "ask.away") { model.requestAway() }
            HeaderButton(icon: .history, label: "Activity", identifier: "ask.history") { showingActivity = true }
        }
        .padding(.leading, 20)
        .padding(.trailing, 10)
        .padding(.top, 4)
    }

    private func suggestionRow(_ task: String, index: Int) -> some View {
        let learned = isLearned(task)
        return Button {
            model.start(task)
        } label: {
            HStack(spacing: Theme.gap) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(task).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                    Text(learned ? "Runs without the model" : "Jarvis works it out on screen")
                        .font(Theme.meta)
                        .foregroundStyle(Theme.textSecondary)
                }
                Spacer(minLength: 8)
                Chip(text: learned ? "Learned" : "New",
                     foreground: learned ? Theme.primaryText : Theme.textSoft,
                     fill: learned ? Theme.primaryFill : Theme.raisedHigh)
            }
            .padding(.horizontal, Theme.gutter)
            .padding(.vertical, 14)
            .frame(maxWidth: .infinity, minHeight: 72, alignment: .leading)
            .card()
        }
        .buttonStyle(.plain)
        .disabled(model.isRunning)
        .accessibilityIdentifier("ask.suggestion.\(index)")
    }

    private var composer: some View {
        HStack(spacing: Theme.gapSmall) {
            TextField("", text: $text, prompt: Text("Ask Jarvis to do something").foregroundColor(Theme.textTertiary))
                .font(.system(size: 16))
                .foregroundStyle(Theme.textPrimary)
                .focused($composerFocused)
                .submitLabel(.go)
                .onSubmit(send)
                .padding(.leading, 14)
                .frame(minHeight: Theme.minTarget)
                .accessibilityLabel("Task for Jarvis")
                .accessibilityIdentifier("ask.composer")
            Button(action: send) {
                Image.decorative(.send, size: Theme.iconStandard)
                    .foregroundStyle(Theme.primaryText)
                    .frame(width: Theme.minTarget, height: Theme.minTarget)
                    .background(Circle().fill(Theme.primaryFill))
            }
            .buttonStyle(.plain)
            .disabled(text.trimmingCharacters(in: .whitespaces).isEmpty || model.isRunning)
            .accessibilityLabel("Start task")
            .accessibilityIdentifier("ask.send")
        }
        .padding(.horizontal, 6)
        .frame(height: Theme.composerHeight)
        .modifier(Bezeled(fill: Theme.surface, radius: Theme.radiusComposer, stroke: Theme.controlBezel))
        .padding(.horizontal, Theme.gutter)
        .padding(.bottom, 14)
    }

    private func send() {
        let task = text
        guard !task.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        composerFocused = false
        text = ""
        model.ask(task)
    }
}

/// A 44 pt round icon button for a screen header.
struct HeaderButton: View {
    var icon: JarvisIcon
    var label: String
    var identifier: String
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Image.decorative(icon, size: Theme.iconProminent)
                .foregroundStyle(Theme.textSoft)
                .frame(width: Theme.minTarget, height: Theme.minTarget)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityIdentifier(identifier)
    }
}

/// Ask's skill sheet: which skill (the matcher could not pick), or the values a skill needs.
struct SkillChoiceSheet: View {
    @EnvironmentObject private var model: AppModel
    var choice: SkillChoice
    @State private var values: [String: String] = [:]
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gutter) {
            switch choice {
            case .choose(let task, let candidates):
                chooser(task: task, candidates: candidates)
            case .arguments(let task, _, let skill, let params):
                arguments(task: task, skill: skill, params: params)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 28)
        .padding(.bottom, 12)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .presentationBackground(Theme.sheet)
        .presentationCornerRadius(Theme.radiusSheet)
    }

    @ViewBuilder
    private func chooser(task: String, candidates: [SkillCandidate]) -> some View {
        Chip(text: "Which skill?", icon: .skills, foreground: Theme.textSoft, fill: Theme.raisedHigh)
        Text("“\(task)” fits more than one skill.")
            .font(Theme.question)
            .foregroundStyle(Theme.textPrimary)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityIdentifier("skill.choice.title")
        ScrollView {
            VStack(spacing: Theme.gapSmall) {
                ForEach(candidates) { candidate in
                    Button {
                        model.choose(candidate, for: task)
                    } label: {
                        HStack(spacing: Theme.gap) {
                            SkillKindTile(kind: candidate.skill.kind, size: Theme.rowTile)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(candidate.skill.name).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                                Text("“\(candidate.match.matchedTrigger)”")
                                    .font(Theme.mono)
                                    .foregroundStyle(Theme.textSecondary)
                                    .lineLimit(1)
                            }
                            Spacer(minLength: 8)
                            Image.decorative(.chevronRight, size: Theme.iconCompact).foregroundStyle(Theme.textTertiary)
                        }
                        .padding(.horizontal, 14)
                        .frame(minHeight: 60)
                        .card()
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("skill.choice.\(candidate.id)")
                }
            }
        }
        Button("Just do what I said") { model.runWithoutSkill(task) }
            .buttonStyle(SecondaryButtonStyle())
            .accessibilityIdentifier("skill.choice.plain")
    }

    @ViewBuilder
    private func arguments(task: String?, skill: Skill, params: [SkillParam]) -> some View {
        Chip(text: skill.name, icon: .skills, foreground: Theme.textSoft, fill: Theme.raisedHigh)
        Text(params.count == 1 ? "One more thing" : "A few more things")
            .font(Theme.question)
            .foregroundStyle(Theme.textPrimary)
            .accessibilityIdentifier("skill.args.title")
        ScrollView {
            VStack(alignment: .leading, spacing: Theme.gap) {
                ForEach(params, id: \.name) { param in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(param.description.isEmpty ? param.name : param.description)
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textSecondary)
                        TextField("", text: Binding(get: { values[param.name] ?? "" }, set: { values[param.name] = $0 }),
                                  prompt: Text(param.name).foregroundColor(Theme.textTertiary))
                            .font(.system(size: 16))
                            .foregroundStyle(Theme.textPrimary)
                            .padding(.horizontal, 14)
                            .frame(minHeight: Theme.composerHeight)
                            .modifier(Bezeled(fill: Theme.surface, radius: Theme.radiusButton, stroke: Theme.controlBezel))
                            .accessibilityLabel(param.description.isEmpty ? param.name : param.description)
                            .accessibilityIdentifier("skill.arg.\(param.name)")
                    }
                }
                if let error {
                    InlineError(text: error, identifier: "skill.args.error")
                }
            }
        }
        HStack(spacing: Theme.gap) {
            Button("Cancel") { model.skillChoice = nil }
                .buttonStyle(SecondaryButtonStyle())
                .accessibilityIdentifier("skill.args.cancel")
            Button("Run") { error = model.submitArguments(choice, values: values) }
                .buttonStyle(PrimaryButtonStyle())
                .accessibilityIdentifier("skill.args.run")
        }
    }
}
