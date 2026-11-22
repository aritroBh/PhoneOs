import SwiftUI
import JarvisKit

@main
struct JarvisApp: App {
    @StateObject private var model = AppModel.shared

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(model)
                .preferredColorScheme(.dark)
                .tint(Theme.textPrimary)
        }
    }
}

/// The tabs. In playground mode a Playground tab (the QA fixture screens) comes first.
enum AppTab: Hashable {
    case playground, ask, activity, apps, rules
}

struct RootView: View {
    @EnvironmentObject private var model: AppModel
    @State private var tab: AppTab = LaunchOptions.current.playground ? .playground : .ask

    var body: some View {
        // `tabItem` + `tag` (stable since iOS 13) rather than the iOS 18 `Tab` builder, so
        // the optional Playground tab is a plain `if`.
        TabView(selection: $tab) {
            if model.launch.playground {
                PlaygroundView()
                    .tabItem { Label("Playground", systemImage: JarvisIcon.playground.rawValue) }
                    .tag(AppTab.playground)
            }
            AskView()
                .tabItem { Label("Ask", systemImage: JarvisIcon.ask.rawValue) }
                .tag(AppTab.ask)
            ActivityView()
                .tabItem { Label("Activity", systemImage: JarvisIcon.clock.rawValue) }
                .tag(AppTab.activity)
            AppsView()
                .tabItem { Label("Apps", systemImage: JarvisIcon.apps.rawValue) }
                .tag(AppTab.apps)
            RulesView()
                .tabItem { Label("Rules", systemImage: JarvisIcon.rules.rawValue) }
                .tag(AppTab.rules)
        }
        // The run view covers the tabs (the tab bar is hidden while Jarvis acts).
        .fullScreenCover(isPresented: $model.showingRun) {
            RunView()
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
        .ownerPrompts(active: !model.showingRun)
        .task {
            await model.refresh()
            await model.checkConnections()
        }
    }
}

/// Presents the pending confirmation or question as a bottom sheet. Applied to the root
/// and to the run view; only one of them is active at a time (a presentation cannot start
/// from under a full-screen cover).
struct OwnerPrompts: ViewModifier {
    @EnvironmentObject private var model: AppModel
    var active: Bool

    func body(content: Content) -> some View {
        content.sheet(item: Binding(
            get: { active ? model.prompt : nil },
            set: { newValue in
                // Swiping the sheet away declines (or leaves a question unanswered).
                if newValue == nil, active, let pending = model.prompt { model.dismissPrompt(pending) }
            }
        )) { prompt in
            Group {
                switch prompt {
                case .confirm(_, let request):
                    ConfirmationSheet(prompt: prompt, request: request)
                case .question(_, let text):
                    QuestionSheet(prompt: prompt, question: text)
                }
            }
            .environmentObject(model)
            .preferredColorScheme(.dark)
        }
    }
}

extension View {
    func ownerPrompts(active: Bool) -> some View {
        modifier(OwnerPrompts(active: active))
    }
}
