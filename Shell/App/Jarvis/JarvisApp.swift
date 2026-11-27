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

/// The tabs: Ask · Skills · Workflows · Apps · Control (PLAN §2.7). In playground mode the
/// Playground tab (the QA fixture screens) comes first and Workflows moves to a row in
/// Control, so the bar keeps five tabs (a sixth would hide tabs behind "More").
enum AppTab: Hashable {
    case playground, ask, skills, workflows, apps, control
}

struct RootView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.scenePhase) private var scenePhase
    @State private var tab: AppTab = LaunchOptions.current.playground ? .playground : .ask

    var body: some View {
        // `tabItem` + `tag` (stable since iOS 13) rather than the iOS 18 `Tab` builder, so
        // the optional tabs are plain `if`s.
        TabView(selection: $tab) {
            if model.launch.playground {
                PlaygroundView()
                    .tabItem { Label("Playground", systemImage: JarvisIcon.playground.rawValue) }
                    .tag(AppTab.playground)
            }
            AskView()
                .tabItem { Label("Ask", systemImage: JarvisIcon.ask.rawValue) }
                .tag(AppTab.ask)
            SkillsView()
                .tabItem { Label("Skills", systemImage: JarvisIcon.skills.rawValue) }
                .tag(AppTab.skills)
            if !model.launch.playground {
                NavigationStack { WorkflowsView() }
                    .tabItem { Label("Workflows", systemImage: JarvisIcon.workflows.rawValue) }
                    .tag(AppTab.workflows)
            }
            AppsView()
                .tabItem { Label("Apps", systemImage: JarvisIcon.apps.rawValue) }
                .tag(AppTab.apps)
            ControlView()
                .tabItem { Label("Control", systemImage: JarvisIcon.rules.rawValue) }
                .tag(AppTab.control)
        }
        // Which skill, or the values a skill needs (from Ask or a skill's Run now). The run
        // starts once the sheet is gone: a full-screen cover cannot start while a sheet is
        // still being dismissed.
        .sheet(item: $model.skillChoice, onDismiss: { model.skillSheetDismissed() }) { choice in
            SkillChoiceSheet(choice: choice)
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
        // The run view covers the tabs (the tab bar is hidden while Jarvis acts).
        .fullScreenCover(isPresented: $model.showingRun) {
            RunView()
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
        // A workflow run: the step chain, with the session inside.
        .fullScreenCover(isPresented: Binding(
            get: { model.showingWorkflowRun && !model.showingRun },
            set: { presented in if !presented { model.dismissWorkflowRun() } }
        )) {
            WorkflowRunView()
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
        .ownerPrompts(active: !model.showingRun && !model.showingWorkflowRun && model.awayPhase == .off)
        // Away mode: the explanation the first time, then the black screen.
        .fullScreenCover(isPresented: Binding(
            get: { model.awayPhase != .off && !model.showingRun },
            set: { presented in if !presented { model.exitAway() } }
        )) {
            AwayModeView()
                .environmentObject(model)
                .preferredColorScheme(.dark)
        }
        .onChange(of: scenePhase) { _, phase in
            model.sceneBecameActive(phase == .active)
        }
        // Voice's "in front" (R4): the background only, from the first phase on.
        .onChange(of: scenePhase, initial: true) { _, phase in
            model.voiceSceneChanged(inBackground: phase == .background)
        }
        .task {
            await model.refresh()
            model.startSchedulerLoop()
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
                case .yesNo(_, let text):
                    YesNoSheet(prompt: prompt, question: text)
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
