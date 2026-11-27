import SwiftUI
import JarvisCore
import JarvisKit

/// Skills: search, a kind filter, the load-issue banner, one card per skill.
struct SkillsView: View {
    enum Filter: String, CaseIterable, Identifiable {
        case all, learned, written, builtIn

        var id: String { rawValue }

        var title: String {
            switch self {
            case .all: "All"
            case .learned: "Learned"
            case .written: "Written"
            case .builtIn: "Built in"
            }
        }

        func includes(_ kind: SkillKind) -> Bool {
            switch self {
            case .all: true
            case .learned: kind == .learned
            case .written: kind == .written
            case .builtIn: kind == .builtIn
            }
        }
    }

    @EnvironmentObject private var model: AppModel
    @State private var search = ""
    @State private var filter: Filter = .all
    @State private var editing: SkillDraft?

    private var shown: [Skill] {
        let query = search.trimmingCharacters(in: .whitespaces).lowercased()
        return model.skillList.filter { skill in
            filter.includes(skill.kind) && (query.isEmpty
                || skill.name.lowercased().contains(query)
                || skill.summary.lowercased().contains(query)
                || skill.triggers.contains { $0.lowercased().contains(query) })
        }
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: Theme.gap) {
                    HStack {
                        Text("Skills")
                            .font(Theme.largeTitle)
                            .tracking(Theme.largeTitleTracking)
                            .foregroundStyle(Theme.textPrimary)
                            .accessibilityAddTraits(.isHeader)
                            .accessibilityIdentifier("skills.title")
                        Spacer()
                        Button {
                            editing = SkillDraft()
                        } label: {
                            Image.decorative(.plus, size: Theme.iconProminent)
                                .foregroundStyle(Theme.primaryText)
                                .frame(width: Theme.minTarget, height: Theme.minTarget)
                                .background(Circle().fill(Theme.primaryFill))
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("New skill")
                        .accessibilityIdentifier("skills.new")
                    }
                    .padding(.bottom, 4)

                    if !model.skillIssues.isEmpty {
                        LoadIssuesBanner(issues: model.skillIssues)
                    }

                    searchField
                    filterBar

                    if shown.isEmpty {
                        emptyState
                    }
                    ForEach(shown, id: \.id) { skill in
                        NavigationLink {
                            SkillDetailView(skillID: skill.id)
                        } label: {
                            SkillCard(skill: skill)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("skills.row.\(skill.id)")
                    }
                }
                .padding(.horizontal, Theme.gutter)
                .padding(.top, 24)
                .padding(.bottom, 24)
            }
            .screenBackground()
            .toolbar(.hidden, for: .navigationBar)
            .onAppear { model.refreshSkills() }
        }
        .sheet(item: Binding(
            get: { editing.map { EditingDraft(draft: $0) } },
            set: { editing = $0?.draft }
        )) { item in
            SkillEditorView(initial: item.draft)
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
    }

    private var searchField: some View {
        HStack(spacing: 10) {
            Image.decorative(.search, size: Theme.iconStandard)
                .foregroundStyle(Theme.textTertiary)
            TextField("", text: $search, prompt: Text("Search skills").foregroundColor(Theme.textTertiary))
                .font(.system(size: 16))
                .foregroundStyle(Theme.textPrimary)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled(true)
                .accessibilityLabel("Search skills")
                .accessibilityIdentifier("skills.search")
        }
        .padding(.horizontal, 14)
        .frame(minHeight: Theme.minTarget)
        .card(radius: Theme.radiusField)
    }

    private var filterBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Theme.gapSmall) {
                ForEach(Filter.allCases) { option in
                    let selected = filter == option
                    Button {
                        withAnimation(Theme.fast) { filter = option }
                    } label: {
                        Text(option.title)
                            .font(Theme.filter)
                            .foregroundStyle(selected ? Theme.primaryText : Theme.textSoft)
                            .padding(.horizontal, 14)
                            .frame(minHeight: Theme.minTarget)
                            .background(Capsule().fill(selected ? Theme.primaryFill : Theme.surface).frame(height: Theme.filterHeight))
                            .overlay(Capsule().strokeBorder(selected ? Color.clear : Theme.bezel, lineWidth: Theme.bezelWidth)
                                .frame(height: Theme.filterHeight))
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(selected ? .isSelected : [])
                    .accessibilityIdentifier("skills.filter.\(option.rawValue)")
                }
            }
        }
    }

    @ViewBuilder private var emptyState: some View {
        VStack(alignment: .leading, spacing: 6) {
            if model.skillList.isEmpty {
                Text("No skills yet")
                    .font(Theme.rowTitle)
                    .foregroundStyle(Theme.textPrimary)
                Text("Write one with +, or save a finished task as a skill from Activity. A skill names a job, the phrases that start it and the apps it stays in.")
                    .font(Theme.body)
                    .foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                Text("No skill matches").font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
            }
        }
        .padding(Theme.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("skills.empty")
    }
}

/// `SkillDraft` is not `Identifiable` (a new one has no id); the sheet needs an identity.
private struct EditingDraft: Identifiable {
    var draft: SkillDraft
    var id: String { draft.id ?? "new" }
}

/// What the core reported when it loaded the skills file. Damaged entries are kept.
struct LoadIssuesBanner: View {
    var issues: [String]
    /// What was loaded ("skills", "workflows"); also the identifier prefix.
    var what = "skills"

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            HStack(spacing: Theme.gapSmall) {
                Image.decorative(.warning, size: Theme.iconStandard)
                    .foregroundStyle(Theme.warning)
                Text(issues.count == 1 ? "1 problem loading \(what)" : "\(issues.count) problems loading \(what)")
                    .font(Theme.rowTitle)
                    .foregroundStyle(Theme.textPrimary)
                    .accessibilityIdentifier("\(what).issues.title")
            }
            ForEach(Array(issues.enumerated()), id: \.offset) { index, issue in
                Text(issue)
                    .font(Theme.monoSmall)
                    .foregroundStyle(Theme.textSoft)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("\(what).issue.\(index)")
            }
            Text("Entries that could not be loaded are kept unchanged in the file; nothing was deleted.")
                .font(Theme.meta)
                .foregroundStyle(Theme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(Theme.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(fill: Theme.warningFill)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("\(what).issues")
    }
}

/// A red message next to a field: the core's words.
struct InlineError: View {
    var text: String
    var identifier: String

    var body: some View {
        Label {
            Text(text).font(Theme.meta).fixedSize(horizontal: false, vertical: true)
        } icon: {
            Image.decorative(.error, size: Theme.iconCompact)
        }
        .foregroundStyle(Theme.errorText)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(identifier)
    }
}

extension SkillKind {
    var title: String {
        switch self {
        case .learned: "Learned"
        case .written: "Written"
        case .builtIn: "Built in"
        }
    }

    var origin: String {
        switch self {
        case .learned: "Learned from a task"
        case .written: "Written by you"
        case .builtIn: "Comes with Jarvis"
        }
    }

    var icon: JarvisIcon {
        switch self {
        case .learned: .replay
        case .written: .edit
        case .builtIn: .builtIn
        }
    }
}

/// The kind's icon on a raised tile.
struct SkillKindTile: View {
    var kind: SkillKind
    var size: CGFloat = Theme.skillTile

    var body: some View {
        Image.decorative(kind.icon, size: Theme.iconProminent)
            .foregroundStyle(Theme.textSoft)
            .frame(width: size, height: size)
            .modifier(Bezeled(fill: Theme.raisedHigh, radius: Theme.radiusElement + 2))
    }
}

/// A skill in the list: kind tile, name, kind chip, its first phrase, the apps it works in.
private struct SkillCard: View {
    var skill: Skill

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gap) {
            HStack(spacing: Theme.gap) {
                SkillKindTile(kind: skill.kind)
                Text(skill.name)
                    .font(Theme.skillTitle)
                    .foregroundStyle(skill.enabled ? Theme.textPrimary : Theme.textTertiary)
                    .lineLimit(2)
                Spacer(minLength: 8)
                if !skill.enabled {
                    Chip(text: "Off", foreground: Theme.textTertiary, fill: Theme.raised)
                }
                Chip(text: skill.kind.title, icon: skill.kind.icon, foreground: Theme.textSoft, fill: Theme.raisedHigh)
            }
            HStack(spacing: 10) {
                Text(skill.triggers.first.map { "“\($0)”" } ?? "No phrase: Run now only")
                    .font(Theme.mono)
                    .foregroundStyle(Theme.textSecondary)
                    .lineLimit(1)
                Spacer(minLength: 8)
                AppTiles(bundleIds: skill.apps)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

/// Up to three small tiles with an app's first letter (no app icons are available to the
/// app; letters keep the row honest).
struct AppTiles: View {
    @EnvironmentObject private var model: AppModel
    var bundleIds: [String]

    var body: some View {
        if !bundleIds.isEmpty {
            HStack(spacing: 4) {
                ForEach(Array(bundleIds.prefix(3)), id: \.self) { id in
                    Text(String(model.name(ofApp: id).prefix(1)).uppercased())
                        .font(Theme.monoSmall.weight(.semibold))
                        .foregroundStyle(Theme.textSecondary)
                        .frame(width: Theme.appMiniTile, height: Theme.appMiniTile)
                        .modifier(Bezeled(fill: Theme.well, radius: Theme.radiusSmallTile))
                }
                if bundleIds.count > 3 {
                    Text("+\(bundleIds.count - 3)").font(Theme.monoSmall).foregroundStyle(Theme.textTertiary)
                }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Works in " + bundleIds.map(model.name(ofApp:)).joined(separator: ", "))
        }
    }
}

// MARK: - Detail

/// One skill: the phrases, what Jarvis does, the guardrails; on/off, Run now, Edit, Delete.
struct SkillDetailView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    var skillID: String
    @State private var confirmingDelete = false
    @State private var error: String?
    @State private var editing: SkillDraft?

    private var skill: Skill? { model.skillList.first { $0.id == skillID } }

    var body: some View {
        Group {
            if let skill {
                content(skill)
            } else {
                Text("This skill is gone.")
                    .font(Theme.body)
                    .foregroundStyle(Theme.textSecondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if let skill, skill.kind == .written {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Edit") { editing = SkillDraft(skill: skill) }
                        .accessibilityIdentifier("skill.detail.edit")
                }
            }
        }
        .sheet(item: Binding(
            get: { editing.map { EditingDraft(draft: $0) } },
            set: { editing = $0?.draft }
        )) { item in
            SkillEditorView(initial: item.draft)
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
        .confirmationDialog("Delete this skill?", isPresented: $confirmingDelete, titleVisibility: .visible) {
            Button("Delete", role: .destructive) {
                if let message = model.deleteSkill(id: skillID) {
                    error = message
                } else {
                    dismiss()
                }
            }
            .accessibilityIdentifier("skill.detail.confirmDelete")
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Jarvis forgets its phrases and guidance. Tasks you ran with it stay in Activity.")
        }
    }

    private func content(_ skill: Skill) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Theme.gutter) {
                HStack(spacing: 14) {
                    Image.decorative(skill.kind.icon, size: Theme.iconDisplay)
                        .foregroundStyle(Theme.textPrimary)
                        .frame(width: Theme.heroTile, height: Theme.heroTile)
                        .modifier(Bezeled(fill: Theme.raised, radius: Theme.radiusButton, stroke: Theme.controlBezel))
                    VStack(alignment: .leading, spacing: 4) {
                        Text(skill.name)
                            .font(Theme.detailTitle)
                            .tracking(Theme.detailTitleTracking)
                            .foregroundStyle(Theme.textPrimary)
                            .accessibilityAddTraits(.isHeader)
                            .accessibilityIdentifier("skill.detail.name")
                        Text(originLine(skill)).font(Theme.meta).foregroundStyle(Theme.textSecondary)
                    }
                }
                .padding(.horizontal, 4)

                VStack(alignment: .leading, spacing: Theme.gapSmall) {
                    SectionLabel(text: "Say")
                    if skill.triggers.isEmpty {
                        Text("No phrase: start it with Run now.").font(Theme.meta).foregroundStyle(Theme.textTertiary)
                            .padding(.horizontal, 4)
                    }
                    FlowLayout(spacing: Theme.gapSmall) {
                        ForEach(Array(skill.triggers.enumerated()), id: \.offset) { index, trigger in
                            HStack(spacing: Theme.gapSmall) {
                                Image.decorative(.phrase, size: Theme.iconCompact).foregroundStyle(Theme.textTertiary)
                                Text(trigger).font(Theme.mono).foregroundStyle(Theme.textPrimary)
                            }
                            .padding(.leading, 12)
                            .padding(.trailing, 14)
                            .frame(minHeight: Theme.filterHeight)
                            .background(Capsule().fill(Theme.surface))
                            .overlay(Capsule().strokeBorder(Theme.bezel, lineWidth: Theme.bezelWidth))
                            .accessibilityElement(children: .combine)
                            .accessibilityIdentifier("skill.detail.trigger.\(index)")
                        }
                    }
                }

                VStack(alignment: .leading, spacing: Theme.gapSmall) {
                    SectionLabel(text: "What Jarvis does")
                    VStack(alignment: .leading, spacing: 6) {
                        if !skill.summary.isEmpty {
                            Text(skill.summary).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                        }
                        Text(skill.instructions.isEmpty
                             ? "No guidance written: Jarvis works it out on screen from your words."
                             : skill.instructions)
                            .font(Theme.body)
                            .foregroundStyle(skill.instructions.isEmpty ? Theme.textTertiary : Theme.textSoft)
                            .fixedSize(horizontal: false, vertical: true)
                            .accessibilityIdentifier("skill.detail.instructions")
                        if skill.kind == .learned && !skill.flowKeys.isEmpty {
                            Label {
                                Text("Replays a learned flow without the model when the screen matches")
                            } icon: {
                                Image.decorative(.replay, size: Theme.iconCompact)
                            }
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textSecondary)
                        }
                    }
                    .padding(Theme.gutter)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .card()
                }

                VStack(alignment: .leading, spacing: Theme.gapSmall) {
                    SectionLabel(text: "Guardrails")
                    VStack(spacing: 0) {
                        GuardrailRow(title: skill.confineToApps ? "Stays inside" : "Works in",
                                     value: appsLine(skill), identifier: "skill.detail.apps")
                        Divider().overlay(Theme.bezel)
                        GuardrailRow(title: "Allowed tools", value: toolsLine(skill), identifier: "skill.detail.tools")
                        Divider().overlay(Theme.bezel)
                        GuardrailRow(title: "Your rules still apply", value: "Always", identifier: "skill.detail.rules")
                    }
                    .card()
                }

                VStack(spacing: 0) {
                    Toggle(isOn: Binding(
                        get: { skill.enabled },
                        set: { on in error = model.setSkillEnabled(on, id: skill.id) }
                    )) {
                        Text("On").font(Theme.body).foregroundStyle(Theme.textPrimary)
                    }
                    .tint(Theme.connected)
                    .padding(.horizontal, Theme.gutter)
                    .frame(minHeight: 52)
                    .accessibilityIdentifier("skill.detail.enabled")
                }
                .card()

                if let error {
                    InlineError(text: error, identifier: "skill.detail.error")
                }

                Button {
                    error = model.runNow(skill)
                } label: {
                    Label {
                        Text("Run now")
                    } icon: {
                        Image.decorative(.play, size: Theme.iconStandard)
                    }
                }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(!skill.enabled || model.isRunning)
                .accessibilityIdentifier("skill.detail.run")

                Button {
                    confirmingDelete = true
                } label: {
                    Label {
                        Text("Delete skill")
                    } icon: {
                        Image.decorative(.trash, size: Theme.iconStandard)
                    }
                }
                .buttonStyle(SecondaryButtonStyle(foreground: Theme.errorText))
                .accessibilityIdentifier("skill.detail.delete")
            }
            .padding(.horizontal, Theme.gutter)
            .padding(.top, 8)
            .padding(.bottom, 24)
        }
    }

    private func originLine(_ skill: Skill) -> String {
        skill.runs == 0 ? skill.kind.origin : "\(skill.kind.origin) · ran \(skill.runs) \(skill.runs == 1 ? "time" : "times")"
    }

    private func appsLine(_ skill: Skill) -> String {
        skill.apps.isEmpty ? "Any app your rules allow" : skill.apps.map(model.name(ofApp:)).joined(separator: ", ")
    }

    private func toolsLine(_ skill: Skill) -> String {
        if !skill.tools.isEmpty { return skill.tools.joined(separator: ", ") }
        return skill.confineToApps ? "Only tools for these apps" : "Any tool your rules allow"
    }
}

private struct GuardrailRow: View {
    var title: String
    var value: String
    var identifier: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Theme.gap) {
            Image.decorative(.guardrail, size: Theme.iconStandard).foregroundStyle(Theme.textSecondary)
            Text(title).font(Theme.body).foregroundStyle(Theme.textPrimary)
            Spacer(minLength: 8)
            Text(value)
                .font(Theme.meta)
                .foregroundStyle(Theme.textSoft)
                .multilineTextAlignment(.trailing)
        }
        .padding(.horizontal, Theme.gutter)
        .padding(.vertical, 14)
        .frame(minHeight: 52)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(identifier)
    }
}

/// Wraps chips onto lines (the phrases on a skill's page).
struct FlowLayout: Layout {
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var x: CGFloat = 0, y: CGFloat = 0, line: CGFloat = 0, widest: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(ProposedViewSize(width: width, height: nil))
            if x > 0 && x + size.width > width {
                y += line + spacing
                x = 0
                line = 0
            }
            x += size.width + spacing
            line = max(line, size.height)
            widest = max(widest, x - spacing)
        }
        return CGSize(width: proposal.width ?? widest, height: y + line)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, line: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(ProposedViewSize(width: bounds.width, height: nil))
            if x > bounds.minX && x + size.width > bounds.maxX {
                y += line + spacing
                x = bounds.minX
                line = 0
            }
            view.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            line = max(line, size.height)
        }
    }
}

// MARK: - Editor

/// Create or edit a Written skill. Every rule is the core's: Save sends the draft and the
/// core's message appears next to the field it is about.
struct SkillEditorView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var draft: SkillDraft
    @State private var error: SkillLibraryError?

    init(initial: SkillDraft) {
        _draft = State(initialValue: initial)
    }

    private func message(for field: SkillField) -> String? {
        guard let error, error.field == field else { return nil }
        return error.description
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    if let general = message(for: .general) {
                        InlineError(text: general, identifier: "skill.edit.error.general")
                    }

                    EditorSection(title: "Name") {
                        EditorField(placeholder: "Leaving work", text: $draft.name, identifier: "skill.edit.name")
                        if let m = message(for: .name) { InlineError(text: m, identifier: "skill.edit.error.name") }
                    }

                    EditorSection(title: "Say") {
                        ForEach(draft.triggers.indices, id: \.self) { index in
                            HStack(spacing: Theme.gapSmall) {
                                EditorField(placeholder: index == 0 ? "I'm leaving work" : "Another phrase",
                                            text: Binding(
                                                // Guarded: a removed row can be read once more.
                                                get: { draft.triggers.indices.contains(index) ? draft.triggers[index] : "" },
                                                set: { if draft.triggers.indices.contains(index) { draft.triggers[index] = $0 } }
                                            ),
                                            identifier: "skill.edit.trigger.\(index)", mono: true)
                                if draft.triggers.count > 1 {
                                    Button {
                                        draft.triggers.remove(at: index)
                                    } label: {
                                        Image.decorative(.close, size: Theme.iconCompact)
                                            .foregroundStyle(Theme.textTertiary)
                                            .frame(width: Theme.minTarget, height: Theme.minTarget)
                                    }
                                    .buttonStyle(.plain)
                                    .accessibilityLabel("Remove phrase \(index + 1)")
                                    .accessibilityIdentifier("skill.edit.removeTrigger.\(index)")
                                }
                            }
                        }
                        Button {
                            draft.triggers.append("")
                        } label: {
                            Label {
                                Text("Add a phrase")
                            } icon: {
                                Image.decorative(.plus, size: Theme.iconCompact)
                            }
                            .font(Theme.meta.weight(.semibold))
                            .foregroundStyle(Theme.textSoft)
                            .frame(minHeight: Theme.minTarget)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("skill.edit.addTrigger")
                        Text("Put a value in braces to fill it from your words: “text {contact} {message}”.")
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textTertiary)
                        if let m = message(for: .triggers) { InlineError(text: m, identifier: "skill.edit.error.triggers") }
                    }

                    EditorSection(title: "What it is for") {
                        EditorField(placeholder: "One line, optional", text: $draft.summary, identifier: "skill.edit.summary")
                        if let m = message(for: .summary) { InlineError(text: m, identifier: "skill.edit.error.summary") }
                    }

                    EditorSection(title: "Guidance for Jarvis") {
                        TextEditor(text: $draft.instructions)
                            .font(Theme.body)
                            .foregroundStyle(Theme.textPrimary)
                            .scrollContentBackground(.hidden)
                            .frame(minHeight: 110)
                            .padding(8)
                            .modifier(Bezeled(fill: Theme.well, radius: Theme.radiusElement, stroke: Theme.controlBezel))
                            .accessibilityLabel("Guidance for Jarvis")
                            .accessibilityIdentifier("skill.edit.instructions")
                        Text("How you want it done, in your words. It goes into Jarvis's prompt; it never overrides your rules.")
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textTertiary)
                        if let m = message(for: .instructions) { InlineError(text: m, identifier: "skill.edit.error.instructions") }
                    }

                    EditorSection(title: "Apps") {
                        VStack(spacing: 0) {
                            ForEach(Array(model.skillAppChoices.enumerated()), id: \.element.id) { index, app in
                                if index > 0 { Divider().overlay(Theme.bezel) }
                                Toggle(isOn: Binding(
                                    get: { draft.apps.contains(app.bundleId) },
                                    set: { on in
                                        draft.apps.removeAll { $0 == app.bundleId }
                                        if on { draft.apps.append(app.bundleId) }
                                    }
                                )) {
                                    Text(app.name).font(Theme.body).foregroundStyle(Theme.textPrimary)
                                }
                                .tint(Theme.connected)
                                .padding(.horizontal, Theme.gutter)
                                .frame(minHeight: 48)
                                .accessibilityIdentifier("skill.edit.app.\(app.bundleId)")
                            }
                        }
                        .card()
                        Toggle(isOn: $draft.confineToApps) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Stay inside these apps").font(Theme.body).foregroundStyle(Theme.textPrimary)
                                Text("Jarvis refuses to act anywhere else while running this skill.")
                                    .font(Theme.meta)
                                    .foregroundStyle(Theme.textSecondary)
                            }
                        }
                        .tint(Theme.connected)
                        .padding(.horizontal, Theme.gutter)
                        .padding(.vertical, 10)
                        .card()
                        .accessibilityIdentifier("skill.edit.confine")
                        if let m = message(for: .apps) { InlineError(text: m, identifier: "skill.edit.error.apps") }
                        if let m = message(for: .tools) { InlineError(text: m, identifier: "skill.edit.error.tools") }
                    }
                }
                .padding(.horizontal, Theme.gutter)
                .padding(.vertical, 20)
            }
            .screenBackground()
            .navigationTitle(draft.id == nil ? "New skill" : "Edit skill")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .accessibilityIdentifier("skill.edit.cancel")
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                        .accessibilityIdentifier("skill.edit.save")
                }
            }
        }
        .presentationBackground(Theme.ground)
    }

    private func save() {
        switch model.saveSkill(draft) {
        case .success:
            error = nil
            dismiss()
        case .failure(let failure):
            withAnimation(Theme.fast) { error = failure }
        }
    }
}

struct EditorSection<Content: View>: View {
    var title: String
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            SectionLabel(text: title)
            content
        }
    }
}

struct EditorField: View {
    var placeholder: String
    @Binding var text: String
    var identifier: String
    var mono = false

    var body: some View {
        TextField("", text: $text, prompt: Text(placeholder).foregroundColor(Theme.textTertiary))
            .font(mono ? Theme.mono : Theme.body)
            .foregroundStyle(Theme.textPrimary)
            .autocorrectionDisabled(true)
            .padding(.horizontal, 12)
            .frame(minHeight: Theme.minTarget)
            .modifier(Bezeled(fill: Theme.well, radius: Theme.radiusElement, stroke: Theme.controlBezel))
            .accessibilityLabel(placeholder)
            .accessibilityIdentifier(identifier)
    }
}
