import SwiftUI
import JarvisKit

/// Apps: runner and model status, endpoints, and the trust level per app (Policy.appRules).
struct AppsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var expanded: String?
    @State private var newBundleId = ""
    @State private var newName = ""
    @State private var showEndpoints = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                ScreenTitle(title: "Apps", subtitle: "How far Jarvis may go in each app.", identifier: "apps.title")
                HStack(spacing: Theme.gap) {
                    ConnectionTile(title: "Runner", detail: "Reads and taps the screen",
                                   status: model.runnerStatus, identifier: "apps.runnerStatus")
                    ConnectionTile(title: "Model", detail: modelDetail,
                                   status: model.modelStatus, identifier: "apps.modelStatus")
                }
                Button {
                    Task { await model.checkConnections() }
                } label: {
                    Text("Check again")
                }
                .buttonStyle(SecondaryButtonStyle())
                .accessibilityIdentifier("apps.check")

                endpoints

                VStack(spacing: 0) {
                    ForEach(Array(model.settings.appTrust.enumerated()), id: \.element.id) { index, app in
                        if index > 0 { Divider().overlay(Theme.bezel) }
                        AppTrustRow(
                            app: app,
                            expanded: expanded == app.id,
                            level: Binding(
                                get: { app.level },
                                set: { model.setTrust($0, for: app.bundleIdPrefix) }
                            )
                        ) {
                            withAnimation(Theme.medium) { expanded = expanded == app.id ? nil : app.id }
                        }
                    }
                }
                .card()

                addApp

                Text("New apps start as Guarded, and Jarvis asks the first time it acts in one. Denied apps are never opened, whatever the task says.")
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textTertiary)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 24)
        }
        .screenBackground()
    }

    private var modelDetail: String {
        model.launch.playground ? "Scripted demo model" : model.settings.modelName
    }

    private var endpoints: some View {
        VStack(alignment: .leading, spacing: Theme.gap) {
            Button {
                withAnimation(Theme.medium) { showEndpoints.toggle() }
            } label: {
                HStack {
                    Text("Endpoints").font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                    Spacer()
                    Image.decorative(showEndpoints ? .chevronDown : .chevronRight, size: Theme.iconStandard)
                        .foregroundStyle(Theme.textTertiary)
                }
                .frame(minHeight: Theme.minTarget)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("apps.endpointsToggle")
            if showEndpoints {
                LabeledField(label: "Runner URL", text: $model.settings.runnerBaseURL, identifier: "apps.runnerURL")
                LabeledField(label: "Model base URL", text: $model.settings.modelBaseURL, identifier: "apps.modelURL")
                LabeledField(label: "Model name", text: $model.settings.modelName, identifier: "apps.modelName")
                Text("Any OpenAI-compatible server. On-device models come later.")
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textTertiary)
            }
        }
        .padding(.horizontal, Theme.gutter)
        .padding(.vertical, 6)
        .card()
    }

    private var addApp: some View {
        VStack(alignment: .leading, spacing: Theme.gap) {
            Text("Add an app").font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
            LabeledField(label: "Bundle id", text: $newBundleId, identifier: "apps.newBundleId")
            LabeledField(label: "Name", text: $newName, identifier: "apps.newName")
            Button("Add as Guarded") {
                model.addApp(bundleId: newBundleId, name: newName)
                newBundleId = ""
                newName = ""
            }
            .buttonStyle(SecondaryButtonStyle())
            .disabled(newBundleId.trimmingCharacters(in: .whitespaces).isEmpty)
            .accessibilityIdentifier("apps.addApp")
        }
        .padding(Theme.gutter)
        .card()
    }
}

/// A single-line text field with a label above it.
struct LabeledField: View {
    var label: String
    @Binding var text: String
    var identifier: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(Theme.meta).foregroundStyle(Theme.textSecondary)
            TextField("", text: $text)
                .font(Theme.mono)
                .foregroundStyle(Theme.textPrimary)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled(true)
                .padding(.horizontal, 12)
                .frame(minHeight: Theme.minTarget)
                .modifier(Bezeled(fill: Theme.well, radius: Theme.radiusElement, stroke: Theme.controlBezel))
                .accessibilityLabel(label)
                .accessibilityIdentifier(identifier)
        }
    }
}

/// Runner / Model status: dot + words (never colour alone).
private struct ConnectionTile: View {
    var title: String
    var detail: String
    var status: ConnectionStatus
    var identifier: String

    private var word: String {
        switch status {
        case .unknown: "Not checked"
        case .reachable: "Connected"
        case .unreachable: "Not reachable"
        }
    }

    private var color: Color {
        switch status {
        case .unknown: Theme.textTertiary
        case .reachable: Theme.connected
        case .unreachable: Theme.error
        }
    }

    private var explanation: String {
        switch status {
        case .unknown: detail
        case .reachable(let s), .unreachable(let s): s
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(Theme.meta).foregroundStyle(Theme.textSecondary)
            HStack(spacing: 6) {
                StatusDot(color: color)
                Text(word).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
            }
            Text(explanation)
                .font(Theme.meta)
                .foregroundStyle(Theme.textTertiary)
                .lineLimit(3)
        }
        .padding(14)
        .frame(maxWidth: .infinity, minHeight: 110, alignment: .topLeading)
        .card()
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(identifier)
    }
}

/// One app: name, bundle id, trust pill; expanded: the four levels and what the chosen one
/// means.
private struct AppTrustRow: View {
    var app: AppTrust
    var expanded: Bool
    @Binding var level: TrustLevelOption
    var toggle: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.gap) {
            Button(action: toggle) {
                HStack(spacing: Theme.gap) {
                    Text(String(app.name.prefix(1)).uppercased())
                        .font(Theme.rowTitle)
                        .foregroundStyle(Theme.textSoft)
                        .frame(width: Theme.statusTile, height: Theme.statusTile)
                        .modifier(Bezeled(fill: Theme.raisedHigh, radius: Theme.radiusElement))
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(app.name).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                        Text(app.bundleIdPrefix).font(Theme.monoSmall).foregroundStyle(Theme.textTertiary)
                    }
                    Spacer(minLength: 8)
                    TrustPill(level: app.level)
                }
                .frame(minHeight: 56)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("apps.row.\(app.bundleIdPrefix)")
            if expanded {
                Picker("Trust level for \(app.name)", selection: $level) {
                    ForEach(TrustLevelOption.allCases) { level in
                        Text(level.title).tag(level)
                    }
                }
                .pickerStyle(.segmented)
                .accessibilityIdentifier("apps.trust.\(app.bundleIdPrefix)")
                Text(app.level.explanation(app: app.name))
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textSecondary)
            }
        }
        .padding(.horizontal, Theme.gutter)
        .padding(.vertical, 8)
    }
}

/// Trusted = white pill, Guarded = grey, Step by step = yellow tint, Denied = red tint + icon.
struct TrustPill: View {
    var level: TrustLevelOption

    var body: some View {
        switch level {
        case .trusted:
            Chip(text: level.title, foreground: Theme.primaryText, fill: Theme.primaryFill)
        case .guarded:
            Chip(text: level.title, foreground: Theme.textSoft, fill: Theme.raisedHigh)
        case .stepByStep:
            Chip(text: level.title, foreground: Theme.warning, fill: Theme.warningFill)
        case .denied:
            Chip(text: level.title, icon: .denied, foreground: Theme.errorText, fill: Theme.errorFill)
        }
    }
}
