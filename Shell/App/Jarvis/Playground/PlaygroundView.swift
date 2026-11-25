import SwiftUI

/// The QA fixture screens (`-jarvis-playground`): deterministic targets for the agent in
/// the simulator tests. State lives in memory only, so every launch starts the same.
/// Labels are explicit (`accessibilityLabel`) so the rendered element lines are stable.
struct PlaygroundView: View {
    enum Route: Hashable {
        case login, toggles, list, danger, alerts
        case item(Int)
        /// The tree fact probes' screens (`ProbeFixtures.swift`), behind one row at the end.
        case probes, probeNotify, probeSheets, probeMenus, probeShare, probeControls, probeCounter
    }

    @State private var path: [Route] = []

    var body: some View {
        NavigationStack(path: $path) {
            List {
                fixture("Login form", .login, "playground.login")
                fixture("Toggles", .toggles, "playground.toggles")
                fixture("Long list", .list, "playground.list")
                fixture("Danger zone", .danger, "playground.danger")
                fixture("Alerts", .alerts, "playground.alerts")
                fixture("Fact probes", .probes, "playground.probes")
            }
            .navigationTitle("Playground")
            .navigationDestination(for: Route.self) { route in
                switch route {
                case .login: LoginFixture()
                case .toggles: TogglesFixture()
                case .list: LongListFixture()
                case .danger: DangerFixture()
                case .alerts: AlertFixture()
                case .item(let n): ItemDetail(number: n)
                case .probes: ProbesIndex()
                case .probeNotify: ProbeNotifyFixture()
                case .probeSheets: ProbeSheetsFixture()
                case .probeMenus: ProbeMenusFixture()
                case .probeShare: ProbeShareFixture()
                case .probeControls: ProbeControlsFixture()
                case .probeCounter: ProbeCounterFixture()
                }
            }
        }
    }

    private func fixture(_ title: String, _ route: Route, _ id: String) -> some View {
        NavigationLink(value: route) {
            Text(title)
        }
        .accessibilityIdentifier(id)
    }
}

/// Username, password, Sign In → "Welcome, <name>".
private struct LoginFixture: View {
    @State private var username = ""
    @State private var password = ""
    @State private var message = "Signed out"

    var body: some View {
        Form {
            Section {
                TextField("Username", text: $username)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled(true)
                    .accessibilityLabel("Username")
                    .accessibilityIdentifier("login.username")
                SecureField("Password", text: $password)
                    .accessibilityLabel("Password")
                    .accessibilityIdentifier("login.password")
            }
            Section {
                Button("Sign In") {
                    if username.isEmpty {
                        message = "Enter a username"
                    } else if password.isEmpty {
                        message = "Enter a password"
                    } else {
                        message = "Welcome, \(username)"
                    }
                }
                .accessibilityIdentifier("login.submit")
                Text(message)
                    .accessibilityIdentifier("login.message")
            }
        }
        .navigationTitle("Login form")
    }
}

/// One switch with a visible state line. The label is hidden visually so the switch's
/// frame is the control itself (a tap on its centre toggles it).
private struct TogglesFixture: View {
    @State private var tips = false

    var body: some View {
        Form {
            HStack {
                Text("Show tips")
                    .accessibilityHidden(true)
                Spacer()
                Toggle("Show tips", isOn: $tips)
                    .labelsHidden()
                    .accessibilityLabel("Show tips")
                    .accessibilityIdentifier("toggles.tips")
            }
            Text(tips ? "Tips are on" : "Tips are off")
                .accessibilityIdentifier("toggles.status")
        }
        .navigationTitle("Toggles")
    }
}

/// 80 rows; a row opens its detail.
private struct LongListFixture: View {
    var body: some View {
        List(1...80, id: \.self) { n in
            NavigationLink(value: PlaygroundView.Route.item(n)) {
                Text("Item \(n)")
            }
            .accessibilityIdentifier("list.item.\(n)")
        }
        .navigationTitle("Long list")
    }
}

private struct ItemDetail: View {
    var number: Int

    var body: some View {
        Text("Selected Item \(number)")
            .font(.title2)
            .accessibilityIdentifier("list.selected")
            .navigationTitle("Item \(number)")
    }
}

/// A destructive button the confirmation gate must stop.
private struct DangerFixture: View {
    @State private var deleted = false

    var body: some View {
        Form {
            Text(deleted ? "Account deleted" : "Account active")
                .accessibilityIdentifier("danger.status")
            Button("Delete account", role: .destructive) {
                deleted = true
            }
            .accessibilityIdentifier("danger.delete")
        }
        .navigationTitle("Danger zone")
    }
}

/// A system-style alert with one button.
private struct AlertFixture: View {
    @State private var showing = false
    @State private var status = "No alert yet"

    var body: some View {
        Form {
            Button("Show alert") { showing = true }
                .accessibilityIdentifier("alerts.show")
            Text(status)
                .accessibilityIdentifier("alerts.status")
        }
        .navigationTitle("Alerts")
        .alert("Storage almost full", isPresented: $showing) {
            Button("Dismiss", role: .cancel) { status = "Alert dismissed" }
        } message: {
            Text("Free up space to keep syncing.")
        }
    }
}
