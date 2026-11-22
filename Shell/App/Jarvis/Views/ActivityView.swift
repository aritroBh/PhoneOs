import SwiftUI
import JarvisKit

/// Activity: learned flows, then past tasks grouped by day.
struct ActivityView: View {
    @EnvironmentObject private var model: AppModel

    private var days: [(title: String, records: [TaskRecord])] {
        let calendar = Calendar.current
        var groups: [(String, [TaskRecord])] = []
        for record in model.history {
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

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                ScreenTitle(title: "Activity", identifier: "activity.title")
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
                                ActivityRow(record: record)
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

    private var meta: String {
        var parts: [String] = []
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
        .padding(.horizontal, Theme.gutter)
        .padding(.vertical, 12)
        .frame(minHeight: 64)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("activity.row.\(record.id)")
    }
}
