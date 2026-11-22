import SwiftUI
import JarvisCore
import JarvisKit

/// Workflows (PLAN §2.7): a trigger and ordered steps, each step an ordinary task under your
/// rules. The list: load issues, the schedule note, one card per workflow (trigger chip,
/// step count, on/off, next run). Used inside a `NavigationStack` (the tab's, or Control's).
struct WorkflowsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var editing: WorkflowDraft?
    /// Pushed from Control (playground builds): keep the bar for the back button.
    var pushed = false

    private var hasSchedules: Bool {
        model.workflowList.contains { if case .schedule = $0.trigger { true } else { false } }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Theme.gap) {
                HStack {
                    Text("Workflows")
                        .font(Theme.largeTitle)
                        .tracking(Theme.largeTitleTracking)
                        .foregroundStyle(Theme.textPrimary)
                        .accessibilityAddTraits(.isHeader)
                        .accessibilityIdentifier("workflows.title")
                    Spacer()
                    Button {
                        editing = WorkflowDraft()
                    } label: {
                        Image.decorative(.plus, size: Theme.iconProminent)
                            .foregroundStyle(Theme.primaryText)
                            .frame(width: Theme.minTarget, height: Theme.minTarget)
                            .background(Circle().fill(Theme.primaryFill))
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("New workflow")
                    .accessibilityIdentifier("workflows.new")
                }
                .padding(.bottom, 4)

                if !model.workflowIssues.isEmpty {
                    LoadIssuesBanner(issues: model.workflowIssues, what: "workflows")
                }
                if let error = model.setupError, !model.showingWorkflowRun {
                    InlineError(text: error, identifier: "workflows.error")
                }
                if model.workflowList.isEmpty {
                    emptyState
                }
                ForEach(model.workflowList, id: \.id) { workflow in
                    WorkflowCard(workflow: workflow)
                }
                if hasSchedules {
                    ScheduleNote().padding(.top, 4)
                }
            }
            .padding(.horizontal, Theme.gutter)
            .padding(.vertical, 24)
        }
        .screenBackground()
        .toolbar(pushed ? .visible : .hidden, for: .navigationBar)
        .onAppear { model.refreshWorkflows() }
        .sheet(item: Binding(
            get: { editing.map { EditingWorkflow(draft: $0) } },
            set: { editing = $0?.draft }
        )) { item in
            WorkflowEditorView(initial: item.draft)
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("No workflows yet")
                .font(Theme.rowTitle)
                .foregroundStyle(Theme.textPrimary)
            Text("Make one with +: a trigger (by hand, a phrase, a schedule or a Shortcuts automation) and a few steps. Every step is an ordinary task under your rules; a workflow never skips a confirmation.")
                .font(Theme.body)
                .foregroundStyle(Theme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(Theme.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("workflows.empty")
    }
}

/// What iOS allows for schedules, said plainly.
struct ScheduleNote: View {
    var body: some View {
        Label {
            Text("Schedules run while Jarvis is open. For exact times, add a Shortcuts automation that runs “Run Jarvis workflow”.")
                .font(Theme.meta)
                .fixedSize(horizontal: false, vertical: true)
        } icon: {
            Image.decorative(.info, size: Theme.iconStandard)
        }
        .foregroundStyle(Theme.textSecondary)
        .padding(Theme.gap)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(radius: Theme.radiusElement)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("workflows.scheduleNote")
    }
}

/// `WorkflowDraft` has no stable identity while new; the sheet needs one.
private struct EditingWorkflow: Identifiable {
    var draft: WorkflowDraft
    var id: String { draft.id ?? "new" }
}

/// One workflow in the list: name, trigger chip, step count, next run, on/off.
private struct WorkflowCard: View {
    @EnvironmentObject private var model: AppModel
    var workflow: Workflow

    var body: some View {
        HStack(alignment: .center, spacing: Theme.gap) {
            NavigationLink {
                WorkflowDetailView(workflowID: workflow.id)
            } label: {
                VStack(alignment: .leading, spacing: 6) {
                    Text(workflow.name)
                        .font(Theme.skillTitle)
                        .foregroundStyle(workflow.enabled ? Theme.textPrimary : Theme.textTertiary)
                        .lineLimit(2)
                    Text("\(workflow.trigger.summary) · \(workflow.steps.count) \(workflow.steps.count == 1 ? "step" : "steps")")
                        .font(Theme.meta)
                        .foregroundStyle(Theme.textSecondary)
                        .lineLimit(2)
                    if let minutes = model.minutesUntilNextRun(workflow) {
                        Text("Next run \(NextRunText.format(minutes: minutes))")
                            .font(Theme.mono)
                            .foregroundStyle(Theme.textSoft)
                            .accessibilityIdentifier("workflows.next.\(workflow.id)")
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("workflows.row.\(workflow.id)")
            Toggle(isOn: Binding(
                get: { workflow.enabled },
                set: { on in _ = model.setWorkflowEnabled(on, id: workflow.id) }
            )) {
                Text("\(workflow.name) is on")
            }
            .labelsHidden()
            .tint(Theme.connected)
            .accessibilityLabel("\(workflow.name) is on")
            .accessibilityIdentifier("workflows.toggle.\(workflow.id)")
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
    }
}

/// "today 5:30 PM", "tomorrow 7:00 AM", "Friday 8:00 AM".
enum NextRunText {
    static func format(minutes: UInt32, now: Date = Date(), calendar: Calendar = .current) -> String {
        let date = now.addingTimeInterval(Double(minutes) * 60)
        let time = date.formatted(date: .omitted, time: .shortened)
        if calendar.isDate(date, inSameDayAs: now) { return "today \(time)" }
        if calendar.isDateInTomorrow(date) { return "tomorrow \(time)" }
        return "\(date.formatted(.dateTime.weekday(.wide))) \(time)"
    }
}

extension Trigger {
    /// "Run by hand", "When you say “goodnight”", "Weekdays at 7:30 AM", "Shortcuts: arrive-home".
    var summary: String {
        switch self {
        case .manual: return "Run by hand"
        case .phrase(let text): return "When you say \u{201C}\(text)\u{201D}"
        case .schedule(let s): return "\(Self.days(s.days)) at \(Self.time(hour: Int(s.hour), minute: Int(s.minute)))"
        case .external(let key): return "Shortcuts: \(key)"
        }
    }

    var icon: JarvisIcon {
        switch self {
        case .manual: .play
        case .phrase: .phrase
        case .schedule: .clock
        case .external: .shortcut
        }
    }

    static func days(_ days: [Weekday]) -> String {
        let set = Set(days.map(WeekdayOption.init))
        if set.count == 7 { return "Every day" }
        if set == Set(WeekdayOption.weekdays) { return "Weekdays" }
        if set == [.sat, .sun] { return "Weekends" }
        return WeekdayOption.allCases.filter(set.contains).map(\.short).joined(separator: ", ")
    }

    static func time(hour: Int, minute: Int) -> String {
        var parts = DateComponents()
        parts.hour = hour
        parts.minute = minute
        guard let date = Calendar.current.date(from: parts) else { return String(format: "%02d:%02d", hour, minute) }
        return date.formatted(date: .omitted, time: .shortened)
    }
}

extension StepDraft.Kind {
    var icon: JarvisIcon {
        switch self {
        case .runSkill: .skills
        case .runTask: .ask
        case .stopUnless: .condition
        case .askOwner: .askOwner
        case .notify: .notify
        case .wait: .wait
        }
    }
}

extension Step {
    /// The node's second line.
    var detail: String {
        switch self {
        case .runSkill(let id, let args):
            return args.isEmpty ? id : "\(id) (" + args.sorted { $0.key < $1.key }.map { "\($0.key): \($0.value)" }.joined(separator: ", ") + ")"
        case .runTask(let text): return text
        case .askOwner(let question): return question
        case .notify(let text): return text
        case .wait(let ms):
            let seconds = Int(ms / 1_000)
            return seconds >= 60 ? "\(seconds / 60) min \(seconds % 60 == 0 ? "" : "\(seconds % 60) s")" : "\(max(1, seconds)) s"
        case .stopUnless(let condition):
            switch condition {
            case .lastSucceeded: return "That worked"
            case .ownerSaidYes: return "You said yes"
            case .lastSummaryContains(let text): return "The result mentions \u{201C}\(text)\u{201D}"
            }
        }
    }
}

extension WorkflowStatus {
    var title: String {
        switch self {
        case .succeeded: "Done"
        case .completedWithFailures: "Done with failures"
        case .failed: "Failed"
        case .stopped: "Stopped by a condition"
        case .cancelled: "Stopped"
        case .blocked: "Blocked"
        case .interrupted: "Phone stayed locked"
        }
    }

    var isGood: Bool { self == .succeeded }
}

// MARK: - The node chain

/// One node of the vertical chain: icon tile, kind, detail, and a line to the next node.
struct WorkflowNode: View {
    var icon: JarvisIcon
    var title: String
    var detail: String
    var state: WorkflowRunner.StepState?
    var isLast: Bool
    var identifier: String

    private var stateText: String? {
        switch state {
        case .pending?: "not run"
        case .running?: "running"
        case .done?: "done"
        case .failed?: "failed"
        case .stoppedHere?: "stopped here"
        case nil: nil
        }
    }

    var body: some View {
        HStack(alignment: .top, spacing: Theme.gap) {
            VStack(spacing: 0) {
                Image.decorative(icon, size: Theme.iconStandard)
                    .foregroundStyle(state == .running ? Theme.tally : Theme.textSoft)
                    .frame(width: Theme.rowTile, height: Theme.rowTile)
                    .modifier(Bezeled(fill: Theme.raisedHigh, radius: Theme.radiusElement,
                                      stroke: state == .running ? Theme.tally.opacity(0.6) : Theme.bezel))
                if !isLast {
                    Rectangle().fill(Theme.bezel).frame(width: 1).frame(minHeight: 18, maxHeight: .infinity)
                }
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(Theme.sectionLabel).foregroundStyle(Theme.textSecondary)
                Text(detail).font(Theme.body).foregroundStyle(Theme.textPrimary).fixedSize(horizontal: false, vertical: true)
            }
            .padding(.top, 2)
            Spacer(minLength: 8)
            if let state {
                marker(state).padding(.top, 8)
            }
        }
        .padding(.bottom, isLast ? 0 : 6)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel([title, detail, stateText].compactMap { $0 }.joined(separator: ", "))
        .accessibilityIdentifier(identifier)
    }

    @ViewBuilder private func marker(_ state: WorkflowRunner.StepState) -> some View {
        switch state {
        case .pending:
            Image.decorative(.hollowDot, size: Theme.iconCompact).foregroundStyle(Theme.textTertiary)
        case .running:
            ProgressView().controlSize(.small).tint(Theme.tally)
        case .done:
            Image.decorative(.check, size: Theme.iconStandard).foregroundStyle(Theme.textPrimary)
        case .failed:
            Image.decorative(.error, size: Theme.iconStandard).foregroundStyle(Theme.errorText)
        case .stoppedHere:
            Image.decorative(.stop, size: Theme.iconCompact).foregroundStyle(Theme.textSoft)
        }
    }
}

/// The trigger and every step as a chain.
struct WorkflowChain: View {
    var workflow: Workflow
    var states: [WorkflowRunner.StepState]?
    var prefix: String

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            WorkflowNode(icon: workflow.trigger.icon, title: "When", detail: workflow.trigger.summary, state: nil,
                         isLast: workflow.steps.isEmpty, identifier: "\(prefix).trigger")
            ForEach(Array(workflow.steps.enumerated()), id: \.offset) { index, step in
                WorkflowNode(icon: step.kind.icon, title: step.kind.title, detail: step.detail,
                             state: states.flatMap { $0.indices.contains(index) ? $0[index] : nil },
                             isLast: index == workflow.steps.count - 1, identifier: "\(prefix).step.\(index)")
            }
        }
        .padding(Theme.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(prefix).chain")
    }
}

// MARK: - Detail

struct WorkflowDetailView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    var workflowID: String
    @State private var editing: WorkflowDraft?
    @State private var confirmingDelete = false
    @State private var error: String?

    private var workflow: Workflow? { model.workflowList.first { $0.id == workflowID } }

    var body: some View {
        Group {
            if let workflow {
                content(workflow)
            } else {
                Text("This workflow is gone.")
                    .font(Theme.body)
                    .foregroundStyle(Theme.textSecondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.visible, for: .navigationBar)
        .toolbar {
            if let workflow {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Edit") { editing = WorkflowDraft(workflow: workflow) }
                        .accessibilityIdentifier("workflow.detail.edit")
                }
            }
        }
        .sheet(item: Binding(
            get: { editing.map { EditingWorkflow(draft: $0) } },
            set: { editing = $0?.draft }
        )) { item in
            WorkflowEditorView(initial: item.draft)
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
        .confirmationDialog("Delete this workflow?", isPresented: $confirmingDelete, titleVisibility: .visible) {
            Button("Delete", role: .destructive) {
                if let message = model.deleteWorkflow(id: workflowID) { error = message } else { dismiss() }
            }
            .accessibilityIdentifier("workflow.detail.confirmDelete")
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Its trigger stops working. Runs it made stay in Activity.")
        }
    }

    private func content(_ workflow: Workflow) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Theme.gutter) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(workflow.name)
                        .font(Theme.detailTitle)
                        .tracking(Theme.detailTitleTracking)
                        .foregroundStyle(Theme.textPrimary)
                        .accessibilityAddTraits(.isHeader)
                        .accessibilityIdentifier("workflow.detail.name")
                    Text(workflow.runs == 0 ? "Not run yet" : "Ran \(workflow.runs) \(workflow.runs == 1 ? "time" : "times")")
                        .font(Theme.meta)
                        .foregroundStyle(Theme.textSecondary)
                }
                .padding(.horizontal, 4)

                WorkflowChain(workflow: workflow, states: nil, prefix: "workflow.detail")

                if let minutes = model.minutesUntilNextRun(workflow) {
                    Text("Next run \(NextRunText.format(minutes: minutes))")
                        .font(Theme.mono)
                        .foregroundStyle(Theme.textSoft)
                        .padding(.horizontal, 4)
                        .accessibilityIdentifier("workflow.detail.next")
                }
                if case .schedule = workflow.trigger { ScheduleNote() }
                if case .external(let key) = workflow.trigger {
                    Text("In Shortcuts, make an automation (time of day, arriving, leaving, charger) that runs “Run Jarvis workflow” with \u{201C}\(key)\u{201D}.")
                        .font(Theme.meta)
                        .foregroundStyle(Theme.textSecondary)
                        .padding(.horizontal, 4)
                        .fixedSize(horizontal: false, vertical: true)
                }

                let warnings = model.workflowWarnings(workflow)
                if !warnings.isEmpty {
                    WarningList(warnings: warnings, identifier: "workflow.detail.warnings")
                }

                if let outcome = workflow.lastOutcome {
                    VStack(alignment: .leading, spacing: 4) {
                        SectionLabel(text: "Last run")
                        HStack(spacing: Theme.gapSmall) {
                            Image.decorative(outcome.status.isGood ? .success : .error, size: Theme.iconStandard)
                                .foregroundStyle(outcome.status.isGood ? Theme.connected : Theme.errorText)
                            Text(outcome.status.title).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                        }
                        Text(outcome.summary).font(Theme.body).foregroundStyle(Theme.textSoft)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(Theme.gutter)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .card()
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("workflow.detail.lastOutcome")
                }

                VStack(spacing: 0) {
                    Toggle(isOn: Binding(
                        get: { workflow.enabled },
                        set: { on in error = model.setWorkflowEnabled(on, id: workflow.id) }
                    )) {
                        Text("On").font(Theme.body).foregroundStyle(Theme.textPrimary)
                    }
                    .tint(Theme.connected)
                    .padding(.horizontal, Theme.gutter)
                    .frame(minHeight: 52)
                    .accessibilityIdentifier("workflow.detail.enabled")
                }
                .card()

                if let error {
                    InlineError(text: error, identifier: "workflow.detail.error")
                }

                Button {
                    error = model.startWorkflow(workflow.id)
                } label: {
                    Label {
                        Text("Run now")
                    } icon: {
                        Image.decorative(.play, size: Theme.iconStandard)
                    }
                }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(!workflow.enabled || model.isRunning)
                .accessibilityIdentifier("workflow.detail.run")

                Button {
                    confirmingDelete = true
                } label: {
                    Label {
                        Text("Delete workflow")
                    } icon: {
                        Image.decorative(.trash, size: Theme.iconStandard)
                    }
                }
                .buttonStyle(SecondaryButtonStyle(foreground: Theme.errorText))
                .accessibilityIdentifier("workflow.detail.delete")
            }
            .padding(.horizontal, Theme.gutter)
            .padding(.top, 8)
            .padding(.bottom, 24)
        }
    }
}

/// Problems that will fail a step when the workflow runs (the core's lines).
struct WarningList: View {
    var warnings: [String]
    var identifier: String

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            HStack(spacing: Theme.gapSmall) {
                Image.decorative(.warning, size: Theme.iconStandard).foregroundStyle(Theme.warning)
                Text("Will fail when it runs").font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
            }
            ForEach(Array(warnings.enumerated()), id: \.offset) { _, line in
                Text(line).font(Theme.monoSmall).foregroundStyle(Theme.textSoft).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(Theme.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(fill: Theme.warningFill)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(identifier)
    }
}

// MARK: - Editor

/// Create or edit a workflow. Every rule is the core's: Save sends the draft and the core's
/// message appears next to what it is about; warnings about skills show as you edit.
struct WorkflowEditorView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var draft: WorkflowDraft
    @State private var error: WorkflowLibraryError?

    init(initial: WorkflowDraft) {
        _draft = State(initialValue: initial)
    }

    private func message(for field: WorkflowField) -> String? {
        guard let error, error.field == field else { return nil }
        return error.description
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    if let general = message(for: .general) {
                        InlineError(text: general, identifier: "workflow.edit.error.general")
                    }
                    EditorSection(title: "Name") {
                        EditorField(placeholder: "Evening commute", text: $draft.name, identifier: "workflow.edit.name")
                        if let m = message(for: .name) { InlineError(text: m, identifier: "workflow.edit.error.name") }
                    }
                    EditorSection(title: "When") {
                        triggerEditor
                        if let m = message(for: .trigger) { InlineError(text: m, identifier: "workflow.edit.error.trigger") }
                    }
                    EditorSection(title: "Steps") {
                        ForEach(Array(draft.steps.enumerated()), id: \.element.id) { index, _ in
                            StepEditor(draft: $draft, index: index, error: message(for: .step(index)))
                        }
                        Button {
                            draft.steps.append(StepDraft(kind: .notify))
                        } label: {
                            Label {
                                Text("Add a step")
                            } icon: {
                                Image.decorative(.plus, size: Theme.iconCompact)
                            }
                            .font(Theme.meta.weight(.semibold))
                            .foregroundStyle(Theme.textSoft)
                            .frame(minHeight: Theme.minTarget)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("workflow.edit.addStep")
                        if let m = message(for: .steps) { InlineError(text: m, identifier: "workflow.edit.error.steps") }
                    }
                    EditorSection(title: "If a step fails") {
                        HStack(spacing: Theme.gapSmall) {
                            ChoiceChip(title: "Stop", selected: draft.onFailure == .stop, identifier: "workflow.edit.onFailure.stop") {
                                draft.onFailure = .stop
                            }
                            ChoiceChip(title: "Go on", selected: draft.onFailure == .continue, identifier: "workflow.edit.onFailure.continue") {
                                draft.onFailure = .continue
                            }
                        }
                        Text("A blocked, stopped or locked-out step always ends the run.")
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textTertiary)
                    }
                    let warnings = model.workflowWarnings(draft)
                    if !warnings.isEmpty {
                        WarningList(warnings: warnings, identifier: "workflow.edit.warnings")
                    }
                }
                .padding(.horizontal, Theme.gutter)
                .padding(.vertical, 20)
            }
            .screenBackground()
            .navigationTitle(draft.id == nil ? "New workflow" : "Edit workflow")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .accessibilityIdentifier("workflow.edit.cancel")
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .accessibilityIdentifier("workflow.edit.save")
                }
            }
        }
        .presentationBackground(Theme.ground)
    }

    @ViewBuilder private var triggerEditor: some View {
        FlowLayout(spacing: Theme.gapSmall) {
            ForEach(TriggerDraft.Kind.allCases) { kind in
                ChoiceChip(title: kind.title, selected: draft.trigger.kind == kind, identifier: "workflow.edit.trigger.\(kind.rawValue)") {
                    draft.trigger.kind = kind
                }
            }
        }
        switch draft.trigger.kind {
        case .manual:
            Text("Only when you tap Run now (or a paired device asks).")
                .font(Theme.meta)
                .foregroundStyle(Theme.textTertiary)
        case .phrase:
            EditorField(placeholder: "goodnight", text: $draft.trigger.phrase, identifier: "workflow.edit.phrase", mono: true)
            Text("Say it on Ask (or to Siri through “Ask Jarvis”).")
                .font(Theme.meta)
                .foregroundStyle(Theme.textTertiary)
        case .schedule:
            FlowLayout(spacing: Theme.gapSmall) {
                ForEach(WeekdayOption.allCases) { day in
                    ChoiceChip(title: day.short, selected: draft.trigger.days.contains(day),
                               identifier: "workflow.edit.day.\(day.short.lowercased())") {
                        if draft.trigger.days.contains(day) { draft.trigger.days.remove(day) } else { draft.trigger.days.insert(day) }
                    }
                }
            }
            DatePicker("Time", selection: Binding(
                get: {
                    Calendar.current.date(from: DateComponents(hour: draft.trigger.hour, minute: draft.trigger.minute)) ?? Date()
                },
                set: { date in
                    let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
                    draft.trigger.hour = parts.hour ?? 0
                    draft.trigger.minute = parts.minute ?? 0
                }
            ), displayedComponents: .hourAndMinute)
            .font(Theme.body)
            .foregroundStyle(Theme.textPrimary)
            .padding(.horizontal, Theme.gutter)
            .frame(minHeight: 52)
            .card()
            .accessibilityIdentifier("workflow.edit.time")
            ScheduleNote()
        case .external:
            EditorField(placeholder: "arrive-home", text: $draft.trigger.key, identifier: "workflow.edit.key", mono: true)
            Text("A Shortcuts automation (time, arriving, leaving, charger) runs “Run Jarvis workflow” with this key.")
                .font(Theme.meta)
                .foregroundStyle(Theme.textTertiary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func save() {
        switch model.saveWorkflow(draft) {
        case .success:
            error = nil
            dismiss()
        case .failure(let failure):
            withAnimation(Theme.fast) { error = failure }
        }
    }
}

/// A selectable capsule (trigger kinds, step kinds, days, choices): 44 pt tall target.
struct ChoiceChip: View {
    var title: String
    var icon: JarvisIcon?
    var selected: Bool
    var identifier: String
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if let icon { Image.decorative(icon, size: Theme.iconCompact) }
                Text(title).font(Theme.meta.weight(.semibold))
            }
            .foregroundStyle(selected ? Theme.primaryText : Theme.textSoft)
            .padding(.horizontal, 12)
            .frame(minHeight: Theme.minTarget)
            .background(Capsule().fill(selected ? Theme.primaryFill : Theme.surface).frame(height: Theme.filterHeight))
            .overlay(Capsule().strokeBorder(selected ? Color.clear : Theme.bezel, lineWidth: Theme.bezelWidth)
                .frame(height: Theme.filterHeight))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier(identifier)
    }
}

/// One step row in the editor: kind chips, the kind's fields, move up/down, remove.
private struct StepEditor: View {
    @EnvironmentObject private var model: AppModel
    @Binding var draft: WorkflowDraft
    var index: Int
    var error: String?

    /// Guarded: a removed row can be read once more while SwiftUI updates.
    private var step: Binding<StepDraft> {
        Binding(
            get: { draft.steps.indices.contains(index) ? draft.steps[index] : StepDraft() },
            set: { if draft.steps.indices.contains(index) { draft.steps[index] = $0 } }
        )
    }

    private let id = { (index: Int, part: String) in "workflow.edit.step.\(index).\(part)" }

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            HStack {
                Text("Step \(index + 1)").font(Theme.sectionLabel).foregroundStyle(Theme.textSecondary)
                Spacer()
                iconButton(.moveUp, "Move step \(index + 1) up", id(index, "up"), enabled: index > 0) {
                    draft.moveStep(at: index, by: -1)
                }
                iconButton(.chevronDown, "Move step \(index + 1) down", id(index, "down"), enabled: index < draft.steps.count - 1) {
                    draft.moveStep(at: index, by: 1)
                }
                iconButton(.remove, "Remove step \(index + 1)", id(index, "remove"), enabled: draft.steps.count > 1) {
                    if draft.steps.indices.contains(index) { draft.steps.remove(at: index) }
                }
            }
            FlowLayout(spacing: 6) {
                ForEach(StepDraft.Kind.allCases) { kind in
                    ChoiceChip(title: kind.title, icon: kind.icon, selected: step.wrappedValue.kind == kind,
                               identifier: id(index, "kind.\(kind.rawValue)")) {
                        step.wrappedValue.kind = kind
                    }
                }
            }
            fields
            if let error { InlineError(text: error, identifier: id(index, "error")) }
        }
        .padding(Theme.gap)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(id(index, "row"))
    }

    @ViewBuilder private var fields: some View {
        switch step.wrappedValue.kind {
        case .runTask:
            EditorField(placeholder: "What Jarvis should do", text: step.text, identifier: id(index, "text"))
        case .askOwner:
            EditorField(placeholder: "A yes/no question for you", text: step.text, identifier: id(index, "text"))
        case .notify:
            EditorField(placeholder: "Notification text", text: step.text, identifier: id(index, "text"))
        case .wait:
            Stepper(value: step.waitSeconds, in: 1...1_800, step: step.wrappedValue.waitSeconds >= 60 ? 30 : 5) {
                HStack {
                    Text("Wait").font(Theme.body).foregroundStyle(Theme.textPrimary)
                    Spacer()
                    Text("\(step.wrappedValue.waitSeconds) s").font(Theme.mono).foregroundStyle(Theme.textSoft)
                }
            }
            .frame(minHeight: Theme.minTarget)
            .accessibilityIdentifier(id(index, "wait"))
        case .stopUnless:
            FlowLayout(spacing: 6) {
                ForEach(StepDraft.ConditionKind.allCases) { condition in
                    ChoiceChip(title: condition.title, selected: step.wrappedValue.condition == condition,
                               identifier: id(index, "condition.\(condition.rawValue)")) {
                        step.wrappedValue.condition = condition
                    }
                }
            }
            if step.wrappedValue.condition == .lastSummaryContains {
                EditorField(placeholder: "Words in the last result", text: step.conditionText, identifier: id(index, "conditionText"))
                Text("The result is written by the model: use this to save steps, never as a safety check.")
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textTertiary)
            }
        case .runSkill:
            if model.skillList.isEmpty {
                Text("No skills yet: make one in Skills.").font(Theme.meta).foregroundStyle(Theme.textTertiary)
            }
            FlowLayout(spacing: 6) {
                ForEach(model.skillList, id: \.id) { skill in
                    ChoiceChip(title: skill.name, selected: step.wrappedValue.skillId == skill.id,
                               identifier: id(index, "skill.\(skill.id)")) {
                        step.wrappedValue.skillId = skill.id
                    }
                }
            }
            if let skill = model.skillList.first(where: { $0.id == step.wrappedValue.skillId }) {
                ForEach(skill.params, id: \.name) { param in
                    EditorField(placeholder: param.description.isEmpty ? param.name : param.description,
                                text: Binding(
                                    get: { step.wrappedValue.args[param.name] ?? "" },
                                    set: { step.wrappedValue.args[param.name] = $0 }
                                ),
                                identifier: id(index, "arg.\(param.name)"))
                }
            }
        }
    }

    private func iconButton(_ icon: JarvisIcon, _ label: String, _ identifier: String, enabled: Bool,
                            action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image.decorative(icon, size: Theme.iconStandard)
                .foregroundStyle(enabled ? Theme.textSoft : Theme.textUnavailable)
                .frame(width: Theme.minTarget, height: Theme.minTarget)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(label)
        .accessibilityIdentifier(identifier)
    }
}

// MARK: - Run view

/// A workflow running (or its result): the chain with each step's state, what happens now,
/// the session inside (the run view's step timeline and what Jarvis sees), Stop; then the
/// outcome.
struct WorkflowRunView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: Theme.gutter) {
            if let progress = model.workflowProgress {
                content(progress)
            } else {
                Text("Starting…").font(Theme.body).foregroundStyle(Theme.textSecondary)
                Spacer()
            }
        }
        .padding(.horizontal, Theme.gutter)
        .padding(.top, 20)
        .padding(.bottom, 12)
        .screenBackground()
        .ownerPrompts(active: model.showingWorkflowRun && !model.showingRun)
    }

    @ViewBuilder private func content(_ progress: WorkflowRunner.Progress) -> some View {
        HStack {
            Chip(text: progress.isRunning ? "Running workflow" : "Workflow", icon: .workflows,
                 foreground: progress.isRunning ? Theme.tally : Theme.textSoft, fill: Theme.raised)
            Spacer()
            if case .remote(_, _, let name) = progress.origin, let name {
                Text("From \(name)").font(Theme.meta).foregroundStyle(Theme.textSecondary)
            }
        }
        Text(progress.workflow.name)
            .font(Theme.runTitle)
            .tracking(Theme.runTitleTracking)
            .foregroundStyle(Theme.textPrimary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityIdentifier("workflow.run.title")
        ScrollView {
            VStack(spacing: Theme.gutter) {
                if let outcome = progress.outcome {
                    outcomeCard(outcome)
                } else {
                    activityLine(progress.activity)
                }
                WorkflowChain(workflow: progress.workflow, states: progress.states, prefix: "workflow.run")
                if case .session = progress.activity, progress.isRunning {
                    VStack(alignment: .leading, spacing: Theme.gapSmall) {
                        HStack {
                            StatusPill(status: model.live.status)
                            Spacer()
                            Text(model.live.caption).font(Theme.monoSmall).foregroundStyle(Theme.textTertiary)
                        }
                        SightCard(live: model.live)
                        StepTimeline(steps: model.live.steps)
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("workflow.run.session")
                }
                if !progress.notes.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        SectionLabel(text: "Notifications")
                        ForEach(Array(progress.notes.enumerated()), id: \.offset) { index, note in
                            Label {
                                Text(note).font(Theme.body)
                            } icon: {
                                Image.decorative(.notify, size: Theme.iconCompact)
                            }
                            .foregroundStyle(Theme.textSoft)
                            .accessibilityIdentifier("workflow.run.note.\(index)")
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        if progress.isRunning {
            Button {
                model.stop()
            } label: {
                Label {
                    Text("Stop workflow")
                } icon: {
                    Image.decorative(.stop, size: Theme.iconStandard)
                }
            }
            .buttonStyle(SecondaryButtonStyle())
            .accessibilityIdentifier("workflow.run.stop")
        } else {
            Button("Done") { model.dismissWorkflowRun() }
                .buttonStyle(PrimaryButtonStyle())
                .accessibilityIdentifier("workflow.run.done")
        }
    }

    private func activityLine(_ activity: WorkflowRunner.Activity) -> some View {
        let text: String = switch activity {
        case .starting: "Starting…"
        case .session(let task): "Doing: \(task)"
        case .asking(let question): "Asking you: \(question)"
        case .notifying(let text): "Notifying: \(text)"
        case .waiting: "Waiting…"
        case .finished: "Finished"
        }
        return HStack(spacing: Theme.gapSmall) {
            StatusDot(color: Theme.tally, pulsing: true)
            Text(text).font(Theme.body).foregroundStyle(Theme.textPrimary).lineLimit(3)
            Spacer()
        }
        .padding(Theme.gap)
        .card(radius: Theme.radiusElement)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("workflow.run.activity")
    }

    private func outcomeCard(_ outcome: WorkflowOutcome) -> some View {
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            HStack(spacing: Theme.gapSmall) {
                Image.decorative(outcome.status.isGood ? .success : .error, size: Theme.iconProminent)
                    .foregroundStyle(outcome.status.isGood ? Theme.connected : Theme.errorText)
                Text(outcome.status.title)
                    .font(Theme.rowTitle)
                    .foregroundStyle(Theme.textPrimary)
                    .accessibilityIdentifier("workflow.run.status")
            }
            Text(outcome.summary)
                .font(Theme.body)
                .foregroundStyle(Theme.textSoft)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("workflow.run.summary")
            Text("\(outcome.stepsRun) \(outcome.stepsRun == 1 ? "step" : "steps") ran · in Activity")
                .font(Theme.mono)
                .foregroundStyle(Theme.textTertiary)
        }
        .padding(Theme.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("workflow.run.outcome")
    }
}
