import AppIntents
import SwiftUI

/// "Ask Jarvis": runs a task and returns its summary. Opens the app, because the run view
/// and the confirmation sheets live there (a risky step must be approved on screen).
struct AskJarvisIntent: AppIntent {
    static var title: LocalizedStringResource = "Ask Jarvis"
    static var description = IntentDescription("Give Jarvis a task to do on this iPhone.")
    static var openAppWhenRun: Bool = true

    @Parameter(title: "Task", requestValueDialog: "What should Jarvis do?")
    var task: String

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> & ProvidesDialog {
        let summary = await AppModel.shared.runAndWait(task)
        return .result(value: summary, dialog: "\(summary)")
    }
}

/// Siri, Shortcuts and the Action Button. Phrases cannot carry a free-text parameter, so
/// Siri asks for the task.
struct JarvisShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: AskJarvisIntent(),
            phrases: [
                "Ask \(.applicationName)",
                "Ask \(.applicationName) to do something",
                "Give \(.applicationName) a task",
            ],
            shortTitle: "Ask Jarvis",
            // A literal: App Shortcut metadata is read at build time (same symbol as JarvisIcon.ask).
            systemImageName: "message"
        )
    }
}
