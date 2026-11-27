import SwiftUI
import JarvisKit

/// Activity: learned flows, then past tasks grouped by day.
struct ActivityView: View {
    @EnvironmentObject private var model: AppModel
    /// The task being saved as a skill (the name prompt is up).
    @State private var promoting: TaskRecord?
    @State private var skillName = ""
    @State private var promoteMessage: (text: String, failed: Bool)?

    private var days: [(title: String, records: [TaskRecord])] {
        let calendar = Calendar.current
        var groups: [(String, [TaskRecord])] = []
        // A workflow's sessions are listed under its entry, not on their own.
        for record in model.history where !record.isWorkflowSession {
            let title: String
            if calendar.isDateInToday(record.finishedAt) {
                title = "Today"
            } else if calendar.isDateInYesterday(record.finishedAt) {
                title = "Yesterday"
            } else {
                title = record.finishedAt.formatted(date: .abbreviated, time: .omitted)
            }
            if let last = groups.indices.last, groups[last].0 == title {
                groups[last].1.append(record)
            } else {
                groups.append((title, [record]))
            }
        }
        return groups.map { (title: $0.0, records: $0.1) }
    }

    /// The sessions a workflow entry groups, in order.
    private func sessions(of record: TaskRecord) -> [TaskRecord] {
        (record.sessionIds ?? []).compactMap { id in model.history.first { $0.id == id } }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                ScreenTitle(title: "Activity", identifier: "activity.title")
                if let promoteMessage {
                    Label {
                        Text(promoteMessage.text).font(Theme.meta).fixedSize(horizontal: false, vertical: true)
                    } icon: {
                        Image.decorative(promoteMessage.failed ? .error : .success, size: Theme.iconStandard)
                    }
                    .foregroundStyle(promoteMessage.failed ? Theme.errorText : Theme.connected)
                    .padding(Theme.gap)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .card(fill: promoteMessage.failed ? Theme.errorFill : Theme.surface, radius: Theme.radiusElement)
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("activity.promoteResult")
                }
                if !model.memory.flows.isEmpty {
                    VStack(alignment: .leading, spacing: 10) {
                        SectionLabel(text: "Learned flows")
                        Text("Run without the model")
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textTertiary)
                            .padding(.horizontal, 4)
                        FlowChips(flows: model.memory.flows)
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("activity.flows")
                }
                if model.history.isEmpty {
                    Text("No tasks yet. Finished tasks show up here with their steps, model calls and time.")
                        .font(Theme.body)
                        .foregroundStyle(Theme.textSecondary)
                        .accessibilityIdentifier("activity.empty")
                }
                ForEach(days, id: \.title) { day in
                    VStack(alignment: .leading, spacing: 10) {
                        SectionLabel(text: day.title)
                        VStack(spacing: 0) {
                            ForEach(Array(day.records.enumerated()), id: \.element.id) { index, record in
                                if index > 0 { Divider().overlay(Theme.bezel).padding(.leading, 60) }
                                ActivityRow(record: record, canPromote: record.isWorkflow != true && model.promotableFlows[record.task] != nil) {
                                    skillName = record.task
                                    promoting = record
                                }
                                if record.isWorkflow == true {
                                    ForEach(sessions(of: record)) { session in
                                        ActivityRow(record: session, canPromote: false, nested: true) {}
                                    }
                                }
                            }
                        }
                        .card()
                    }
                }
            }
            .padding(.horizontal, 20)
            .padding(.top, 24)
            .padding(.bottom, 24)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
        .alert("Save as a skill", isPresented: Binding(
            get: { promoting != nil },
            set: { if !$0 { promoting = nil } }
        )) {
            TextField("Name", text: $skillName)
                .accessibilityIdentifier("activity.promote.name")
            Button("Cancel", role: .cancel) { promoting = nil }
            Button("Save") {
                guard let record = promoting else { return }
                let name = skillName
                promoting = nil
                Task {
                    switch await model.promote(record, name: name) {
                    case .success(let skill):
                        promoteMessage = ("Saved “\(skill.name)”. Say “\(skill.triggers.first ?? record.task)” on Ask to run it.", false)
                    case .failure(let error):
                        promoteMessage = (error.description, true)
                    }
                }
            }
            .accessibilityIdentifier("activity.promote.save")
        } message: {
            Text("Jarvis learned the steps of this task. As a skill it gets a name, shows in Skills, and the same words run it.")
        }
        .task { await model.refresh() }
        .refreshable { await model.refresh() }
    }
}

private struct FlowChips: View {
    var flows: [MemorySnapshot.LearnedFlow]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Theme.gapSmall) {
                ForEach(flows) { flow in
                    Chip(text: flow.intentKey, icon: .replay)
                        .accessibilityLabel("Learned flow: \(flow.intentKey), \(flow.steps) steps")
                }
            }
            .padding(.horizontal, 2)
        }
    }
}

private struct ActivityRow: View {
    var record: TaskRecord
    var canPromote: Bool
    var nested = false
    var promote: () -> Void

    private var meta: String {
        var parts: [String] = []
        if record.isWorkflow == true {
            parts.append("Workflow")
            parts.append(record.status.title)
            parts.append("\(record.steps) \(record.steps == 1 ? "step" : "steps") ran")
            if let device = record.requestedBy { parts.append("from \(device)") }
            return parts.joined(separator: " · ")
        }
        if let requester = record.requesterText { parts.append(requester) }
        if let skill = record.skillName { parts.append("skill \(skill)") }
        if let app = record.apps.first { parts.append(app) }
        parts.append(record.status.title)
        if record.status == .success && record.modelCalls == 0 && record.steps > 0 {
            parts.append("replayed, no model calls")
        } else {
            parts.append("\(record.steps) steps · \(record.modelCalls) model calls")
        }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            summary
            if canPromote {
                Button(action: promote) {
                    Label {
                        Text("Save as skill")
                    } icon: {
                        Image.decorative(.skills, size: Theme.iconCompact)
                    }
                    .font(Theme.meta.weight(.semibold))
                    .foregroundStyle(Theme.textSoft)
                    .padding(.horizontal, 12)
                    .frame(minHeight: Theme.minTarget)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .padding(.leading, 60 - 12)
                .padding(.bottom, 4)
                .accessibilityIdentifier("activity.promote.\(record.id)")
            }
        }
    }

    private var summary: some View {
        HStack(spacing: Theme.gap) {
            StatusTile(status: record.status)
            VStack(alignment: .leading, spacing: 3) {
                Text(record.task).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary).lineLimit(2)
                Text(meta).font(Theme.meta).foregroundStyle(Theme.textSecondary).lineLimit(2)
            }
            Spacer(minLength: 8)
            Text(record.durationText)
                .font(Theme.mono)
                .foregroundStyle(Theme.textTertiary)
        }
        .padding(.leading, nested ? Theme.gutter + 28 : Theme.gutter)
        .padding(.trailing, Theme.gutter)
        .padding(.vertical, 12)
        .frame(minHeight: 64)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("activity.row.\(record.id)")
    }
}
