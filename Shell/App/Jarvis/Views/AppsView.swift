import SwiftUI
import JarvisKit

/// Apps: for each app the ways Jarvis can get in (screen, app actions, direct; only what
/// this build really has) and its trust level; then runner and model status and endpoints.
struct AppsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var expanded: String?
    @State private var newBundleId = ""
    @State private var newName = ""
    @State private var showEndpoints = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                ScreenTitle(title: "Apps", subtitle: "Jarvis can work in any app through its screen. Some apps also offer faster ways in.",
                            identifier: "apps.title")

                VStack(alignment: .leading, spacing: Theme.gap) {
                    WaysLegend()
                    VStack(spacing: 0) {
                        ForEach(Array(model.appAccess.enumerated()), id: \.element.id) { index, app in
                            if index > 0 { Divider().overlay(Theme.bezel).padding(.leading, 62) }
                            AppAccessRow(
                                app: app,
                                expanded: expanded == app.id,
                                level: Binding(
                                    get: { app.trust ?? .guarded },
                                    set: { model.setTrust($0, for: app.bundleId, name: app.name) }
                                )
                            ) {
                                withAnimation(Theme.medium) { expanded = expanded == app.id ? nil : app.id }
                            }
                        }
                    }
                    .card()
                    Text("New apps start as Guarded, and Jarvis asks the first time it opens one. Denied apps are never opened, whatever the task says. App actions and Direct appear when this build has a tool for that app.")
                        .font(Theme.meta)
                        .foregroundStyle(Theme.textTertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                addApp

                VStack(alignment: .leading, spacing: Theme.gap) {
                    SectionLabel(text: "Connections")
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
                }
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

extension AppAccess.Way {
    var icon: JarvisIcon {
        switch self {
        case .screen: .wayScreen
        case .appActions: .wayAppActions
        case .direct: .wayDirect
        }
    }
}

/// The key above the list: Screen · App actions · Direct.
private struct WaysLegend: View {
    var body: some View {
        HStack(spacing: Theme.gutter) {
            ForEach(AppAccess.Way.allCases, id: \.self) { way in
                HStack(spacing: 6) {
                    Image.decorative(way.icon, size: Theme.iconCompact)
                    Text(way.title)
                }
                .font(Theme.meta.weight(.semibold))
                .foregroundStyle(Theme.textSoft)
            }
        }
        .padding(.horizontal, 6)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Ways in: screen, app actions, direct")
        .accessibilityIdentifier("apps.legend")
    }
}

/// The three way icons of one app, lit when the way exists.
private struct WaysIcons: View {
    var app: AppAccess

    var body: some View {
        HStack(spacing: 6) {
            ForEach(AppAccess.Way.allCases, id: \.self) { way in
                Image.decorative(way.icon, size: Theme.iconCompact)
                    .foregroundStyle(app.has(way) ? Theme.textPrimary : Theme.textUnavailable)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(app.waysDescription)
        .accessibilityIdentifier("apps.ways.\(app.bundleId)")
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

/// One app: letter tile, name, ways in, trust pill; expanded: the four levels, what the
/// chosen one means, and what each way in is for this app.
private struct AppAccessRow: View {
    var app: AppAccess
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
                        .frame(width: Theme.rowTile, height: Theme.rowTile)
                        .modifier(Bezeled(fill: Theme.raisedHigh, radius: Theme.radiusElement))
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(app.name).font(Theme.rowTitle).foregroundStyle(Theme.textPrimary)
                        Text(app.bundleId).font(Theme.monoSmall).foregroundStyle(Theme.textTertiary).lineLimit(1)
                    }
                    Spacer(minLength: 8)
                    WaysIcons(app: app)
                    TrustPill(level: app.deniedByCore ? .denied : (app.trust ?? .guarded))
                }
                .frame(minHeight: 58)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("apps.row.\(app.bundleId)")
            if expanded {
                if app.deniedByCore {
                    Text("Jarvis never opens \(app.name): it is on the core's own deny list, whatever the level here says.")
                        .font(Theme.meta)
                        .foregroundStyle(Theme.errorText)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Picker("Trust level for \(app.name)", selection: $level) {
                    ForEach(TrustLevelOption.allCases) { level in
                        Text(level.title).tag(level)
                    }
                }
                .pickerStyle(.segmented)
                .accessibilityIdentifier("apps.trust.\(app.bundleId)")
                Text(app.trust == nil
                     ? "No level set yet: Guarded, and Jarvis asks the first time it acts in \(app.name)."
                     : level.explanation(app: app.name))
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
                VStack(alignment: .leading, spacing: 6) {
                    wayLine(.screen, app.screen ? "Reads and taps the screen through the runner." : "Closed: the app is denied.")
                    wayLine(.appActions, app.appActions.isEmpty ? "None in this build." : app.appActions.joined(separator: ", "))
                    wayLine(.direct, app.directTools.isEmpty ? "None in this build." : app.directTools.joined(separator: ", "))
                }
                .padding(Theme.gap)
                .frame(maxWidth: .infinity, alignment: .leading)
                .card(fill: Theme.well, radius: Theme.radiusElement)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("apps.waysDetail.\(app.bundleId)")
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 4)
        .padding(.bottom, expanded ? 10 : 0)
    }

    private func wayLine(_ way: AppAccess.Way, _ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: Theme.gapSmall) {
            Image.decorative(way.icon, size: Theme.iconCompact)
                .foregroundStyle(app.has(way) ? Theme.textPrimary : Theme.textUnavailable)
            Text(way.title).font(Theme.meta.weight(.semibold)).foregroundStyle(Theme.textSoft)
            Text(text).font(Theme.meta).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .combine)
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
