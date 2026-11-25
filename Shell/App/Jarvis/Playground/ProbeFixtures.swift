import AuthenticationServices
import SafariServices
import SwiftUI
import UIKit
import UserNotifications

/// Screens for the tree fact probes (`TreeFactProbeTests`): each hosts one system control
/// whose accessibility tree the probes record. Reached from the Playground's last row
/// ("Fact probes"), so the other fixtures keep their rows and positions. Playground only.
struct ProbesIndex: View {
    var body: some View {
        List {
            row("Notification", .probeNotify, "probe.notify")
            row("Browser sheets", .probeSheets, "probe.sheets")
            row("Menus", .probeMenus, "probe.menus")
            row("Share sheet", .probeShare, "probe.share")
            row("Controls", .probeControls, "probe.controls")
            row("Snapshot counter", .probeCounter, "probe.counter")
        }
        .navigationTitle("Fact probes")
    }

    private func row(_ title: String, _ route: PlaygroundView.Route, _ id: String) -> some View {
        NavigationLink(value: route) {
            Text(title)
        }
        .accessibilityIdentifier(id)
    }
}

/// Presents a UIKit controller over whatever is in front, the way an app presents
/// `SFSafariViewController` or `UIActivityViewController` (modally, not as a SwiftUI sheet).
@MainActor
enum ProbePresenter {
    static func present(_ controller: UIViewController) {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let windows = scenes.flatMap { $0.windows }
        let window = windows.first(where: { $0.isKeyWindow }) ?? windows.first
        guard let root = window?.rootViewController else { return }
        var top = root
        while let presented = top.presentedViewController { top = presented }
        top.present(controller, animated: true)
    }

    /// The URL the probe passed with `-jarvis-probe-url <url>` (the test's local server).
    static var probeURL: URL? {
        let arguments = ProcessInfo.processInfo.arguments
        guard let index = arguments.firstIndex(of: "-jarvis-probe-url"), index + 1 < arguments.count else { return nil }
        return URL(string: arguments[index + 1])
    }
}

/// Fact (a): schedules a local notification a few seconds ahead, so its banner shows over
/// another app. Asks for permission first (the system alert; playground mode never asks
/// anywhere else).
struct ProbeNotifyFixture: View {
    static let bannerBody = "Probe banner 4417"
    @State private var status = "Not scheduled"

    var body: some View {
        Form {
            Button("Notify in 6 s") { schedule() }
                .accessibilityIdentifier("probe.notify.schedule")
            Text(status)
                .accessibilityIdentifier("probe.notify.status")
        }
        .navigationTitle("Notification")
    }

    private func schedule() {
        status = "Asking"
        Task { @MainActor in
            let center = UNUserNotificationCenter.current()
            let granted: Bool
            do {
                granted = try await center.requestAuthorization(options: [.alert, .sound])
            } catch {
                status = "Authorization failed: \(error.localizedDescription)"
                return
            }
            guard granted else {
                status = "Not allowed"
                return
            }
            let content = UNMutableNotificationContent()
            content.title = "Jarvis probe"
            content.body = Self.bannerBody
            let trigger = UNTimeIntervalNotificationTrigger(timeInterval: 6, repeats: false)
            do {
                try await center.add(UNNotificationRequest(identifier: "jarvis.probe.banner", content: content, trigger: trigger))
                status = "Scheduled"
            } catch {
                status = "Scheduling failed: \(error.localizedDescription)"
            }
        }
    }
}

/// Fact (b): an in-app browser (`SFSafariViewController`) and a web sign-in
/// (`ASWebAuthenticationSession`, through SwiftUI's `webAuthenticationSession`) on the
/// probe's local page.
struct ProbeSheetsFixture: View {
    @Environment(\.webAuthenticationSession) private var webAuthenticationSession
    @State private var status = "Idle"

    var body: some View {
        Form {
            Button("Open in-app browser") {
                guard let url = ProbePresenter.probeURL else {
                    status = "No probe URL"
                    return
                }
                ProbePresenter.present(SFSafariViewController(url: url))
                status = "Browser presented"
            }
            .accessibilityIdentifier("probe.sheets.safari")
            Button("Start web sign-in") { signIn() }
                .accessibilityIdentifier("probe.sheets.auth")
            Text(status)
                .accessibilityIdentifier("probe.sheets.status")
            Text(ProbePresenter.probeURL?.absoluteString ?? "no probe URL")
                .accessibilityIdentifier("probe.sheets.url")
        }
        .navigationTitle("Browser sheets")
    }

    private func signIn() {
        guard let url = ProbePresenter.probeURL else {
            status = "No probe URL"
            return
        }
        status = "Signing in"
        Task { @MainActor in
            do {
                let callback = try await webAuthenticationSession.authenticate(using: url, callbackURLScheme: "jarvisprobe")
                status = "Callback \(callback.absoluteString)"
            } catch {
                status = "Sign-in ended: \(error.localizedDescription)"
            }
        }
    }
}

/// Fact (c): a text field with known text (edit menu) and a view with a context menu.
struct ProbeMenusFixture: View {
    @State private var text = "Probe edit menu text"
    @State private var chosen = "No action chosen"

    var body: some View {
        Form {
            TextField("Probe text", text: $text)
                .autocorrectionDisabled(true)
                .textInputAutocapitalization(.never)
                .accessibilityIdentifier("probe.menus.field")
            Text("Hold for the context menu")
                .frame(maxWidth: .infinity, minHeight: 80)
                .contentShape(Rectangle())
                .contextMenu {
                    Button("Probe first action") { chosen = "First action" }
                    Button("Probe second action") { chosen = "Second action" }
                }
                .accessibilityIdentifier("probe.menus.context")
            Text(chosen)
                .accessibilityIdentifier("probe.menus.status")
        }
        .navigationTitle("Menus")
    }
}

/// Fact (g): the system share sheet (`UIActivityViewController`) on a plain text.
struct ProbeShareFixture: View {
    var body: some View {
        Form {
            Button("Share text") {
                ProbePresenter.present(UIActivityViewController(activityItems: ["Jarvis probe share text"], applicationActivities: nil))
            }
            .accessibilityIdentifier("probe.share.open")
        }
        .navigationTitle("Share sheet")
    }
}

/// Fact (h): wheel picker, wheel date picker, slider, switch and stepper.
struct ProbeControlsFixture: View {
    @State private var choice = "Two"
    @State private var date = Date(timeIntervalSince1970: 1_800_000_000)
    @State private var level = 0.4
    @State private var on = false
    @State private var count = 3

    var body: some View {
        Form {
            Picker("Probe choice", selection: $choice) {
                Text("One").tag("One")
                Text("Two").tag("Two")
                Text("Three").tag("Three")
            }
            .pickerStyle(.wheel)
            .accessibilityIdentifier("probe.controls.picker")
            DatePicker("Probe date", selection: $date, displayedComponents: [.date])
                .datePickerStyle(.wheel)
                .accessibilityIdentifier("probe.controls.date")
            Slider(value: $level, in: 0...1)
                .accessibilityLabel("Probe level")
                .accessibilityIdentifier("probe.controls.slider")
            Toggle("Probe switch", isOn: $on)
                .accessibilityIdentifier("probe.controls.toggle")
            Stepper("Probe count \(count)", value: $count, in: 0...10)
                .accessibilityIdentifier("probe.controls.stepper")
        }
        .navigationTitle("Controls")
    }
}

/// Fact R23: does a snapshot run the app's own accessibility code? `CountingView`'s
/// `accessibilityLabel` getter counts its calls; the count is shown in an ordinary text
/// (`snapshot.counter`, "calls: N") that is not itself the counted element. The count is
/// lock-protected (the getter's thread is not assumed) and shown from the main queue.
final class ProbeCallCounter: ObservableObject, @unchecked Sendable {
    /// Main queue only.
    @Published var shown = 0
    private let lock = NSLock()
    private var calls = 0

    func count() {
        lock.lock()
        calls += 1
        let n = calls
        lock.unlock()
        // Not inside the getter's own call: the accessibility request finishes first.
        DispatchQueue.main.async { self.shown = n }
    }
}

struct ProbeCounterFixture: View {
    @StateObject private var counter = ProbeCallCounter()

    var body: some View {
        VStack(spacing: 32) {
            Text("calls: \(counter.shown)")
                .font(.title2)
                .accessibilityIdentifier("snapshot.counter")
            CountingElement(counter: counter)
                .frame(width: 220, height: 64)
            Spacer()
        }
        .padding(.top, 40)
        .navigationTitle("Snapshot counter")
    }
}

private struct CountingElement: UIViewRepresentable {
    let counter: ProbeCallCounter

    func makeUIView(context: Context) -> CountingView {
        CountingView(counter: counter)
    }

    func updateUIView(_ uiView: CountingView, context: Context) {}
}

final class CountingView: UIView {
    let counter: ProbeCallCounter

    init(counter: ProbeCallCounter) {
        self.counter = counter
        super.init(frame: .zero)
        isAccessibilityElement = true
        accessibilityIdentifier = "snapshot.counted"
        backgroundColor = .systemGray4
    }

    required init?(coder: NSCoder) {
        fatalError("CountingView is made in code only")
    }

    override var accessibilityLabel: String? {
        get {
            counter.count()
            return "Counted element"
        }
        set { super.accessibilityLabel = newValue }
    }
}
