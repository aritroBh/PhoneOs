import SwiftUI
import JarvisKit

/// Away mode (PLAN §3.1): the first-use explanation, then a pure black screen at brightness
/// 0. Leaving takes a deliberate touch-and-hold of 2 seconds; a tap does nothing.
struct AwayModeView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        switch model.awayPhase {
        case .explaining:
            AwayExplanation()
        case .on, .off:
            AwayBlackScreen()
        }
    }
}

/// What Away mode costs, stated plainly, with an explicit acknowledgement (remembered).
private struct AwayExplanation: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Spacer()
                Button {
                    model.dismissAwayExplanation()
                } label: {
                    Image.decorative(.close, size: Theme.iconStandard)
                        .foregroundStyle(Theme.textSoft)
                        .frame(width: Theme.minTarget, height: Theme.minTarget)
                        .background(Circle().fill(Theme.raised))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Close")
                .accessibilityIdentifier("away.close")
            }
            .padding(.horizontal, Theme.gutter)
            .padding(.top, 12)

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    VStack(alignment: .leading, spacing: Theme.gap) {
                        Image.decorative(.away, size: Theme.iconDisplay)
                            .foregroundStyle(Theme.textPrimary)
                            .frame(width: Theme.heroTile, height: Theme.heroTile)
                            .modifier(Bezeled(fill: Theme.raised, radius: Theme.radiusButton, stroke: Theme.controlBezel))
                        Text(AwayModeCopy.title)
                            .font(Theme.largeTitle)
                            .tracking(Theme.largeTitleTracking)
                            .foregroundStyle(Theme.textPrimary)
                            .accessibilityAddTraits(.isHeader)
                            .accessibilityIdentifier("away.explain.title")
                        Text(AwayModeCopy.summary)
                            .font(Theme.body)
                            .foregroundStyle(Theme.textSecondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        SectionLabel(text: "What it costs")
                        VStack(spacing: 0) {
                            ForEach(Array(AwayModeCopy.costs.enumerated()), id: \.offset) { index, cost in
                                if index > 0 { Divider().overlay(Theme.bezel).padding(.leading, Theme.gutter) }
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(cost.title).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                                    Text(cost.detail)
                                        .font(Theme.meta)
                                        .foregroundStyle(Theme.textSecondary)
                                        .fixedSize(horizontal: false, vertical: true)
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, Theme.gutter)
                                .padding(.vertical, 12)
                                .accessibilityElement(children: .combine)
                            }
                        }
                        .card(fill: Theme.warningFill)
                        .accessibilityIdentifier("away.explain.costs")
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        SectionLabel(text: "Option")
                        ProximityRow()
                            .card()
                    }

                    Text("To leave Away mode, touch and hold the black screen for 2 seconds. Jarvis does not listen for a wake word yet.")
                        .font(Theme.meta)
                        .foregroundStyle(Theme.textTertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 16)
            }

            Button(AwayModeCopy.acknowledgement) {
                model.acknowledgeAway()
            }
            .buttonStyle(PrimaryButtonStyle())
            .accessibilityIdentifier("away.acknowledge")
            .padding(.horizontal, Theme.gutter)
            .padding(.bottom, 12)
        }
        .screenBackground()
    }
}

/// The proximity option, shared by the explanation and Control.
struct ProximityRow: View {
    @EnvironmentObject private var model: AppModel
    var identifier = "away.proximity"

    var body: some View {
        Toggle(isOn: Binding(get: { model.settings.awayProximity }, set: { model.setAwayProximity($0) })) {
            VStack(alignment: .leading, spacing: 3) {
                Text(AwayModeCopy.proximityTitle).font(Theme.body).foregroundStyle(Theme.textPrimary)
                Text(AwayModeCopy.proximityDetail)
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .tint(Theme.connected)
        .padding(.horizontal, Theme.gutter)
        .padding(.vertical, 12)
        .accessibilityIdentifier(identifier)
    }
}

/// Pure black (OLED pixels off). A status line fades out after a few seconds; touch and
/// hold 2 seconds to leave.
private struct AwayBlackScreen: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var statusVisible = true

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            VStack(spacing: 6) {
                Spacer()
                HStack(spacing: 6) {
                    StatusDot(color: model.isRunning ? Theme.tally : Theme.textTertiary, pulsing: false)
                    Text(model.isRunning ? "Away mode · Jarvis is working" : "Away mode · Standing by")
                        .font(Theme.awayStatus)
                        .foregroundStyle(Theme.textTertiary)
                }
                Text(AwayModeCopy.exitHint)
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textTertiary.opacity(0.7))
            }
            .padding(.bottom, 40)
            .opacity(statusVisible ? 1 : 0)
            .accessibilityHidden(true)
        }
        .contentShape(Rectangle())
        .onLongPressGesture(minimumDuration: 2) {
            model.exitAway()
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Away mode")
        .accessibilityHint("Touch and hold for 2 seconds to leave.")
        .accessibilityAction(named: "Leave Away mode") { model.exitAway() }
        .accessibilityIdentifier("away.screen")
        .statusBarHidden(true)
        .persistentSystemOverlays(.hidden)
        .task {
            try? await Task.sleep(for: .seconds(4))
            if reduceMotion {
                statusVisible = false
            } else {
                withAnimation(.easeOut(duration: 1.2)) { statusVisible = false }
            }
        }
    }
}
