import SwiftUI
import JarvisKit

/// Ask (running / done): what Jarvis sees, the step timeline, Stop; then the result.
struct RunView: View {
    @EnvironmentObject private var model: AppModel

    private var live: LiveRun { model.live }

    var body: some View {
        VStack(spacing: Theme.gutter) {
            HStack {
                StatusPill(status: live.status)
                Spacer()
            }
            VStack(alignment: .leading, spacing: 6) {
                Text(live.caption)
                    .font(Theme.monoSmall.weight(.medium))
                    .tracking(0.6)
                    .foregroundStyle(Theme.textTertiary)
                    .accessibilityIdentifier("run.caption")
                Text(live.task)
                    .font(Theme.runTitle)
                    .tracking(Theme.runTitleTracking)
                    .foregroundStyle(Theme.textPrimary)
                    .accessibilityIdentifier("run.title")
                if let skill = live.skillName {
                    Label {
                        Text("Using skill: \(skill)")
                    } icon: {
                        Image.decorative(.skills, size: Theme.iconCompact)
                    }
                    .font(Theme.meta.weight(.semibold))
                    .foregroundStyle(Theme.textSoft)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel("Using skill: \(skill)")
                    .accessibilityIdentifier("run.skill")
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 4)

            ScrollView {
                VStack(spacing: Theme.gutter) {
                    if live.status == .locked {
                        UnlockCard(reason: live.lockReason, waitedMs: live.lockedWaitMs)
                    }
                    if let result = live.result {
                        ResultCard(result: result)
                    }
                    SightCard(live: live)
                    StepTimeline(steps: live.steps)
                }
            }

            controls
        }
        .padding(.horizontal, Theme.gutter)
        .padding(.top, 20)
        .padding(.bottom, 12)
        .screenBackground()
        .ownerPrompts(active: model.showingRun)
    }

    @ViewBuilder private var controls: some View {
        if live.isRunning || live.result == nil {
            Button {
                model.stop()
            } label: {
                Label {
                    Text("Stop")
                } icon: {
                    Image.decorative(.stop, size: Theme.iconStandard)
                }
            }
            .buttonStyle(SecondaryButtonStyle())
            .accessibilityIdentifier("run.stop")
        } else {
            HStack(spacing: Theme.gap) {
                Button("New task") { model.dismissRun() }
                    .buttonStyle(SecondaryButtonStyle())
                    .accessibilityIdentifier("run.newTask")
                Button("Run again") {
                    let task = live.task
                    let skill = model.lastSkill
                    model.dismissRun()
                    model.runAgain(task, skill: skill)
                }
                .buttonStyle(PrimaryButtonStyle())
                .accessibilityIdentifier("run.runAgain")
            }
        }
    }
}

/// "Unlock to continue": the needs-you treatment, the lock symbol, the time waited.
private struct UnlockCard: View {
    var reason: String?
    var waitedMs: UInt64

    private var waited: String {
        let seconds = Int(waitedMs / 1_000)
        return "Waiting \(seconds / 60):" + String(format: "%02d", seconds % 60)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            HStack(spacing: Theme.gapSmall) {
                Image.decorative(.lock, size: Theme.iconProminent)
                    .foregroundStyle(Theme.warning)
                Text("Unlock to continue")
                    .font(Theme.rowTitle)
                    .foregroundStyle(Theme.textPrimary)
                    .accessibilityIdentifier("run.lock.title")
                Spacer()
                StatusDot(color: Theme.warning, pulsing: true)
            }
            Text(reason ?? "The phone is locked. Jarvis never enters a passcode; unlock it and the task goes on.")
                .font(Theme.body)
                .foregroundStyle(Theme.textSoft)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("run.lock.reason")
            Text(waited)
                .font(Theme.mono)
                .foregroundStyle(Theme.textTertiary)
                .accessibilityIdentifier("run.lock.waited")
        }
        .padding(Theme.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(fill: Theme.warningFill)
        .accessibilityElement(children: .contain)
    }
}

/// The finished run: status in icon, colour and words.
private struct ResultCard: View {
    var result: RunResult

    private var success: Bool { result.outcome.status == .success }
    private var interrupted: Bool { result.outcome.status == .interrupted }

    private var icon: JarvisIcon { success ? .success : interrupted ? .lock : .error }
    private var iconColor: Color { success ? Theme.connected : interrupted ? Theme.warning : Theme.errorText }

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            HStack(spacing: Theme.gapSmall) {
                Image.decorative(icon, size: Theme.iconProminent)
                    .foregroundStyle(iconColor)
                Text(statusWord)
                    .font(Theme.rowTitle)
                    .foregroundStyle(Theme.textPrimary)
                    .accessibilityIdentifier("run.status")
            }
            Text(result.outcome.summary)
                .font(Theme.body)
                .foregroundStyle(Theme.textSoft)
                .accessibilityIdentifier("run.result")
            Text(meta)
                .font(Theme.mono)
                .foregroundStyle(Theme.textTertiary)
            if success && result.modelCalls > 0 {
                Text("Saved as a learned flow. Next run skips the model and still asks you at the same step.")
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textSecondary)
            }
        }
        .padding(Theme.gutter)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("run.resultCard")
    }

    private var statusWord: String {
        switch result.outcome.status {
        case .success: "Done"
        case .failed: "Failed"
        case .cancelled: "Stopped"
        case .blocked: "Blocked"
        case .interrupted: "Phone stayed locked"
        }
    }

    private var meta: String {
        let seconds = Double(result.outcome.durationMs) / 1_000
        return "\(result.outcome.steps) steps · \(result.modelCalls) model calls · \(String(format: "%.1f", seconds)) s"
    }
}

/// "What Jarvis sees": the element list of the latest prompt, the chosen one highlighted.
struct SightCard: View {
    var live: LiveRun

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            HStack(alignment: .firstTextBaseline) {
                Text("What Jarvis sees").font(Theme.sectionLabel).foregroundStyle(Theme.textSoft)
                Spacer()
                Text(live.sightApp ?? "")
                    .font(Theme.monoSmall)
                    .foregroundStyle(Theme.textTertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .padding(.horizontal, 6)
            if live.sight.isEmpty {
                Text(live.steps.isEmpty ? "Reading the screen…" : "Replaying a learned flow: no model call, so no list.")
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textTertiary)
                    .padding(.horizontal, 6)
                    .frame(minHeight: 40)
            }
            ForEach(Array(live.sight.prefix(12).enumerated()), id: \.offset) { _, element in
                row(element)
            }
            if live.sight.count > 12 {
                Text("+ \(live.sight.count - 12) more")
                    .font(Theme.monoSmall)
                    .foregroundStyle(Theme.textTertiary)
                    .padding(.horizontal, 6)
            }
        }
        .padding(EdgeInsets(top: 14, leading: 10, bottom: 10, trailing: 10))
        .card(fill: Theme.well)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("run.sight")
    }

    private func row(_ e: PromptScreen.Element) -> some View {
        let chosen = live.chosen == e.index && live.isRunning
        return HStack(spacing: 10) {
            Text("\(e.index)")
                .font(Theme.monoSmall.weight(.medium))
                .foregroundStyle(chosen ? Theme.primaryText : Theme.textSecondary)
                .frame(width: 22, height: 22)
                .background(RoundedRectangle(cornerRadius: Theme.radiusInner, style: .continuous)
                    .fill(chosen ? Theme.tally : Theme.raisedHigh))
            Text(e.role)
                .font(Theme.monoSmall)
                .foregroundStyle(Theme.textTertiary)
                .frame(width: 92, alignment: .leading)
                .lineLimit(1)
            Text(e.label.isEmpty ? (e.placeholder ?? "") : e.label)
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(Theme.textPrimary)
                .lineLimit(1)
            Spacer(minLength: 4)
            if chosen, let verb = live.chosenVerb {
                Text(verb)
                    .font(Theme.monoSmall.weight(.medium))
                    .foregroundStyle(Theme.tally)
            } else if let value = e.displayValue {
                Text(value).font(Theme.monoSmall).foregroundStyle(Theme.textTertiary).lineLimit(1)
            }
        }
        .padding(.horizontal, 8)
        .frame(minHeight: 40)
        .background(
            RoundedRectangle(cornerRadius: Theme.radiusElement, style: .continuous)
                .fill(chosen ? Theme.tally.opacity(0.12) : Color.clear)
        )
        .overlay(
            RoundedRectangle(cornerRadius: Theme.radiusElement, style: .continuous)
                .strokeBorder(chosen ? Theme.tally.opacity(0.6) : Color.clear, lineWidth: Theme.bezelWidth)
        )
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("run.sight.\(e.index)")
    }
}

/// Executed steps: check / spinner / failed mark, with the source in mono.
struct StepTimeline: View {
    var steps: [LiveRun.Step]

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(steps) { step in
                HStack(spacing: Theme.gap) {
                    marker(step.state)
                        .frame(width: 20, height: 20)
                    Text(step.label)
                        .font(Theme.body)
                        .foregroundStyle(step.state == .running ? Theme.textPrimary : Theme.textSoft)
                        .lineLimit(2)
                    Spacer(minLength: 4)
                    Text(step.source.rawValue)
                        .font(Theme.monoSmall)
                        .foregroundStyle(Theme.textTertiary)
                }
                .frame(minHeight: 36)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("run.step.\(step.id)")
            }
        }
        .padding(.horizontal, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("run.steps")
    }

    @ViewBuilder private func marker(_ state: LiveRun.StepState) -> some View {
        switch state {
        case .done:
            Image.decorative(.check, size: Theme.iconStandard).foregroundStyle(Theme.textPrimary)
        case .running:
            ProgressView().controlSize(.small).tint(Theme.tally)
        case .failed:
            Image.decorative(.error, size: Theme.iconStandard).foregroundStyle(Theme.errorText)
        }
    }
}
