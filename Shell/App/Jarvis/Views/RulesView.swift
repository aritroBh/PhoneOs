import SwiftUI
import JarvisKit

/// Rules: policy mode, what Jarvis always asks before, what it never does, hard limits.
struct RulesView: View {
    @EnvironmentObject private var model: AppModel

    /// The "Always ask before" switches; "Paying or buying" covers two categories.
    private let askRows: [(title: String, categories: [RiskCategoryOption], id: String)] = [
        ("Sending a message", [.sendMessage], "sendMessage"),
        ("Paying or buying", [.payment, .purchase], "payment"),
        ("Deleting something", [.delete], "delete"),
        ("Changing a system setting", [.systemSetting], "systemSetting"),
        ("Placing a call", [.call], "call"),
    ]

    /// Categories the owner may forbid outright.
    private let denyRows: [RiskCategoryOption] = [.payment, .purchase, .publicPost, .accountChange]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                ScreenTitle(title: "Rules", subtitle: "Limits Jarvis works inside. Only you can change them.", identifier: "rules.title")

                VStack(spacing: Theme.gap) {
                    ForEach(PolicyModeOption.allCases) { mode in
                        ModeCard(mode: mode, selected: model.settings.policyMode == mode) {
                            withAnimation(Theme.fast) { model.settings.policyMode = mode }
                        }
                    }
                }

                VStack(alignment: .leading, spacing: 10) {
                    SectionLabel(text: "Always ask before")
                    VStack(spacing: 0) {
                        ForEach(Array(askRows.enumerated()), id: \.offset) { index, row in
                            if index > 0 { Divider().overlay(Theme.bezel) }
                            SwitchRow(title: row.title, isOn: Binding(
                                get: { row.categories.allSatisfy { model.settings.asksBefore($0) } },
                                set: { on in row.categories.forEach { model.settings.setAsksBefore($0, on) } }
                            ), identifier: "rules.ask.\(row.id)")
                        }
                    }
                    .card()
                }

                VStack(alignment: .leading, spacing: 10) {
                    SectionLabel(text: "Never do")
                    VStack(spacing: 0) {
                        ForEach(Array(denyRows.enumerated()), id: \.offset) { index, category in
                            if index > 0 { Divider().overlay(Theme.bezel) }
                            SwitchRow(title: category.title, isOn: Binding(
                                get: { model.settings.denyCategories.contains(category) },
                                set: { on in
                                    model.settings.denyCategories.removeAll { $0 == category }
                                    if on { model.settings.denyCategories.append(category) }
                                }
                            ), identifier: "rules.deny.\(category.rawValue)")
                        }
                    }
                    .card()
                }

                VStack(alignment: .leading, spacing: 10) {
                    SectionLabel(text: "Hard limits")
                    VStack(spacing: 0) {
                        HStack(spacing: Theme.gap) {
                            Text("Never enter passwords").font(Theme.body).foregroundStyle(Theme.textPrimary)
                            Spacer()
                            Label {
                                Text("Always")
                            } icon: {
                                Image.decorative(.lock, size: Theme.iconCompact)
                            }
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textTertiary)
                        }
                        .padding(.horizontal, Theme.gutter)
                        .frame(minHeight: 52)
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("rules.noPasswords")
                        Divider().overlay(Theme.bezel)
                        SwitchRow(title: "Ask the first time in an app", isOn: $model.settings.firstUseApproval,
                                  identifier: "rules.firstUse")
                        Divider().overlay(Theme.bezel)
                        StepperRow(title: "Stop a task after", value: $model.settings.maxSteps, range: 5...200, step: 5,
                                   unit: "steps", identifier: "rules.maxSteps")
                        Divider().overlay(Theme.bezel)
                        StepperRow(title: "or after", value: $model.settings.maxMinutes, range: 1...60, step: 1,
                                   unit: "min", identifier: "rules.maxMinutes")
                    }
                    .card()
                }
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 24)
        }
        .screenBackground()
    }
}

/// A radio card: selected has a 1.5 pt white ring and a filled check.
private struct ModeCard: View {
    var mode: PolicyModeOption
    var selected: Bool
    var select: () -> Void

    var body: some View {
        Button(action: select) {
            HStack(alignment: .top, spacing: Theme.gap) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(mode.title).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                    Text(mode.explanation).font(Theme.meta).foregroundStyle(Theme.textSecondary)
                }
                Spacer()
                Image.decorative(selected ? .success : .hollowDot, size: Theme.iconProminent)
                    .foregroundStyle(selected ? Theme.textPrimary : Theme.textTertiary)
            }
            .padding(Theme.gutter)
            .frame(maxWidth: .infinity, minHeight: 72, alignment: .leading)
            .card()
            .overlay(
                RoundedRectangle(cornerRadius: Theme.radiusCard, style: .continuous)
                    .strokeBorder(selected ? Theme.selectedRing : Color.clear, lineWidth: Theme.selectedRingWidth)
            )
            .contentShape(RoundedRectangle(cornerRadius: Theme.radiusCard, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("rules.mode.\(mode.rawValue)")
    }
}

private struct SwitchRow: View {
    var title: String
    @Binding var isOn: Bool
    var identifier: String

    var body: some View {
        Toggle(isOn: $isOn) {
            Text(title).font(Theme.body).foregroundStyle(Theme.textPrimary)
        }
        .tint(Theme.connected)
        .padding(.horizontal, Theme.gutter)
        .frame(minHeight: 52)
        .accessibilityIdentifier(identifier)
    }
}

private struct StepperRow: View {
    var title: String
    @Binding var value: Int
    var range: ClosedRange<Int>
    var step: Int
    var unit: String
    var identifier: String

    var body: some View {
        Stepper(value: $value, in: range, step: step) {
            HStack {
                Text(title).font(Theme.body).foregroundStyle(Theme.textPrimary)
                Spacer()
                Text("\(value) \(unit)").font(Theme.mono).foregroundStyle(Theme.textSoft)
            }
        }
        .padding(.horizontal, Theme.gutter)
        .frame(minHeight: 52)
        .accessibilityIdentifier(identifier)
    }
}
