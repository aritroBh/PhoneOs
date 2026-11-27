import SwiftUI
import JarvisKit

/// Control: remote control, Activity and Away mode, then the rules Jarvis works inside
/// (how careful, what it always asks before, what it never does, hard limits).
struct ControlView: View {
    @EnvironmentObject private var model: AppModel

    /// The "Always ask before" switches; "Paying or buying" covers two categories.
    private let askRows: [(title: String, icon: JarvisIcon, categories: [RiskCategoryOption], id: String)] = [
        ("Sending a message", .ask, [.sendMessage], "sendMessage"),
        ("Paying or buying", .shield, [.payment, .purchase], "payment"),
        ("Deleting something", .trash, [.delete], "delete"),
        ("Changing a system setting", .rules, [.systemSetting], "systemSetting"),
        ("Placing a call", .runner, [.call], "call"),
    ]

    private var remoteDetail: String {
        guard model.settings.remoteControlEnabled else { return "Off" }
        let n = model.remoteDevices.count
        return n == 0 ? "On, no devices" : "On · \(n) \(n == 1 ? "device" : "devices")"
    }

    /// Categories the owner may forbid outright.
    private let denyRows: [RiskCategoryOption] = [.payment, .purchase, .publicPost, .accountChange]

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    ScreenTitle(title: "Control", subtitle: "Limits Jarvis works inside. Only you can change them.",
                                identifier: "control.title")

                    if let warning = model.notificationAccess.warning {
                        // Same look as the rules problem on Ask.
                        Label {
                            Text(warning).font(Theme.meta)
                        } icon: {
                            Image.decorative(.warning, size: Theme.iconStandard)
                        }
                        .foregroundStyle(Theme.textPrimary)
                        .padding(Theme.gap)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .card(fill: Theme.warningFill, radius: Theme.radiusElement)
                        .accessibilityIdentifier("control.notificationsOff")
                    }

                    VStack(spacing: 0) {
                        NavigationLink {
                            RemoteControlView()
                        } label: {
                            NavRow(icon: .remote, title: "Remote control", detail: remoteDetail)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("control.remote")
                        RowDivider()
                        NavigationLink {
                            ActivityView()
                        } label: {
                            NavRow(icon: .history, title: "Activity", detail: "Everything Jarvis did")
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("control.activity")
                        if model.launch.playground {
                            // The Playground tab takes the Workflows slot in QA builds.
                            RowDivider()
                            NavigationLink {
                                WorkflowsView(pushed: true)
                            } label: {
                                NavRow(icon: .workflows, title: "Workflows", detail: nil)
                            }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier("control.workflows")
                        }
                        RowDivider()
                        Button {
                            model.requestAway()
                        } label: {
                            NavRow(icon: .away, title: "Away mode", detail: "Black screen, stays unlocked")
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("control.away")
                    }
                    .card()

                    VStack(alignment: .leading, spacing: 10) {
                        SectionLabel(text: "Away mode")
                        ProximityRow(identifier: "control.away.proximity").card()
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        SectionLabel(text: "Live view")
                        SwitchRow(title: "Show real screen in Live view", icon: .wayScreen, isOn: Binding(
                            get: { model.settings.liveViewScreenshots },
                            set: { model.setLiveViewScreenshots($0) }
                        ), identifier: "control.liveView.realScreen")
                        .card()
                        Text("Uses screenshots kept in memory only. Off: Jarvis draws an outline of the screen instead.")
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textSecondary)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.horizontal, 4)
                            .accessibilityIdentifier("control.liveView.footnote")
                        SwitchRow(title: "Show the real screen when Jarvis asks", icon: .wayScreen, isOn: Binding(
                            get: { model.settings.approvalScreenshots },
                            set: { model.setApprovalScreenshots($0) }
                        ), identifier: "control.approval.realScreen")
                        .card()
                        Text("When Jarvis asks for your OK it shows the screen it looked at, with what it will tap outlined. One screenshot, kept in memory only while it asks. Off: an outline of the screen instead.")
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textSecondary)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.horizontal, 4)
                            .accessibilityIdentifier("control.approval.footnote")
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        SectionLabel(text: "Voice")
                        VStack(spacing: 0) {
                            SwitchRow(title: "Listen for \u{201C}Hey Jarvis\u{201D}", icon: .microphone, isOn: Binding(
                                get: { model.settings.voiceWakeWord },
                                set: { model.setVoiceWakeWord($0) }
                            ), identifier: "voice.wakeWord")
                            Divider().overlay(Theme.bezel)
                            HStack(spacing: Theme.gap) {
                                Text("Read answers aloud").font(Theme.body).foregroundStyle(Theme.textPrimary)
                                Spacer()
                                Picker("Read answers aloud", selection: Binding(
                                    get: { model.settings.voiceSpeakAnswers },
                                    set: { model.setVoiceSpeakAnswers($0) }
                                )) {
                                    ForEach(SpeakAnswersOption.allCases) { option in
                                        Text(option.title).tag(option)
                                    }
                                }
                                .pickerStyle(.menu)
                                .accessibilityIdentifier("voice.speakAnswers")
                            }
                            .padding(.horizontal, Theme.gutter)
                            .frame(minHeight: 52)
                        }
                        .card()
                        Text("Only while Jarvis is open in front. Anything the phone or the room plays can say \u{201C}Hey Jarvis\u{201D}: a task started by voice asks before typing your words, and approvals are always a tap on the phone, never spoken. Answers are never read aloud while the phone is locked.")
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textSecondary)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.horizontal, 4)
                            .accessibilityIdentifier("voice.footnote")
                        if let error = model.voiceError {
                            Text(error)
                                .font(Theme.meta)
                                .foregroundStyle(Theme.errorText)
                                .accessibilityIdentifier("voice.error")
                        }
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        SectionLabel(text: "How careful")
                        HStack(spacing: 2) {
                            ForEach(PolicyModeOption.allCases) { mode in
                                ModeSegment(mode: mode, selected: model.settings.policyMode == mode) {
                                    withAnimation(Theme.fast) { model.settings.policyMode = mode }
                                }
                            }
                        }
                        .padding(3)
                        .card(radius: Theme.radiusButton)
                        Text(model.settings.policyMode.explanation)
                            .font(Theme.meta)
                            .foregroundStyle(Theme.textSecondary)
                            .padding(.horizontal, 4)
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        SectionLabel(text: "Always ask before")
                        VStack(spacing: 0) {
                            ForEach(Array(askRows.enumerated()), id: \.offset) { index, row in
                                if index > 0 { Divider().overlay(Theme.bezel) }
                                SwitchRow(title: row.title, icon: row.icon, isOn: Binding(
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
                                SwitchRow(title: category.title, icon: nil, isOn: Binding(
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
                            SwitchRow(title: "Ask the first time in an app", icon: nil, isOn: $model.settings.firstUseApproval,
                                      identifier: "rules.firstUse")
                            Divider().overlay(Theme.bezel)
                            StepperRow(title: "Stop a task after", value: $model.settings.maxSteps, range: 5...200, step: 5,
                                       unit: "steps", identifier: "rules.maxSteps")
                            Divider().overlay(Theme.bezel)
                            StepperRow(title: "or after", value: $model.settings.maxMinutes, range: 1...60, step: 1,
                                       unit: "min", identifier: "rules.maxMinutes")
                            Divider().overlay(Theme.bezel)
                            StepperRow(title: "Wait for an unlock up to", value: $model.settings.maxLockedWaitMinutes,
                                       range: 1...240, step: 5, unit: "min", identifier: "rules.maxLockedWait")
                        }
                        .card()
                    }
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 24)
            }
            .screenBackground()
            .toolbar(.hidden, for: .navigationBar)
        }
    }
}

/// A row that opens something: icon tile, title, optional detail, chevron.
struct NavRow: View {
    var icon: JarvisIcon
    var title: String
    var detail: String?

    var body: some View {
        HStack(spacing: Theme.gap) {
            Image.decorative(icon, size: Theme.iconStandard)
                .foregroundStyle(Theme.textSoft)
                .frame(width: Theme.rowTile, height: Theme.rowTile)
                .modifier(Bezeled(fill: Theme.raisedHigh, radius: Theme.radiusElement))
            Text(title).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
            Spacer(minLength: 8)
            if let detail {
                Text(detail).font(Theme.meta).foregroundStyle(Theme.textSecondary).lineLimit(1)
            }
            Image.decorative(.chevronRight, size: Theme.iconCompact)
                .foregroundStyle(Theme.textTertiary)
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 60)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

/// The hairline between rows of a card, inset past the icon tile.
struct RowDivider: View {
    var body: some View {
        Divider().overlay(Theme.bezel).padding(.leading, 62)
    }
}

/// One half of the "How careful" control: white when selected.
private struct ModeSegment: View {
    var mode: PolicyModeOption
    var selected: Bool
    var select: () -> Void

    var body: some View {
        Button(action: select) {
            Text(mode.title)
                .font(Theme.body.weight(.semibold))
                .foregroundStyle(selected ? Theme.primaryText : Theme.textSecondary)
                .frame(maxWidth: .infinity, minHeight: 46)
                .background(
                    RoundedRectangle(cornerRadius: Theme.radiusButton - 3, style: .continuous)
                        .fill(selected ? Theme.primaryFill : Color.clear)
                )
                .contentShape(RoundedRectangle(cornerRadius: Theme.radiusButton - 3, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("rules.mode.\(mode.rawValue)")
    }
}

private struct SwitchRow: View {
    var title: String
    var icon: JarvisIcon?
    @Binding var isOn: Bool
    var identifier: String

    var body: some View {
        Toggle(isOn: $isOn) {
            HStack(spacing: Theme.gap) {
                if let icon {
                    Image.decorative(icon, size: Theme.iconStandard).foregroundStyle(Theme.textSecondary)
                }
                Text(title).font(Theme.body).foregroundStyle(Theme.textPrimary)
            }
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
