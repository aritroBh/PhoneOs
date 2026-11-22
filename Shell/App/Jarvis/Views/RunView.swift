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
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 4)

            ScrollView {
                VStack(spacing: Theme.gutter) {
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
                    model.dismissRun()
                    model.start(task)
                }
                .buttonStyle(PrimaryButtonStyle())
                .accessibilityIdentifier("run.runAgain")
            }
        }
    }
}

/// The finished run: status in icon, colour and words.
private struct ResultCard: View {
    var result: RunResult

    private var success: Bool { result.outcome.status == .success }

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gapSmall) {
            HStack(spacing: Theme.gapSmall) {
                Image.decorative(success ? .success : .error, size: Theme.iconProminent)
                    .foregroundStyle(success ? Theme.connected : Theme.errorText)
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
private struct SightCard: View {
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
            } else if let value = e.value {
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
private struct StepTimeline: View {
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
