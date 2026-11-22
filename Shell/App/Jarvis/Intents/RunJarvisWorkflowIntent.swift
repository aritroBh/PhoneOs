import AppIntents
import SwiftUI

/// "Run Jarvis workflow": for Shortcuts automations (time of day, arriving, leaving,
/// connecting a charger), which iOS can start when Jarvis could not. The parameter is the
/// workflow's Shortcuts key (or its name or id). Runs without opening Jarvis when iOS lets
/// it; a step that needs you (a confirmation, a question) waits on the phone with a
/// notification. Returns the outcome in one line, or that it is still running when it takes
/// longer than the time iOS gives an intent.
struct RunJarvisWorkflowIntent: AppIntent {
    static let title: LocalizedStringResource = "Run Jarvis workflow"
    static let description: IntentDescription? = IntentDescription("Runs one of your Jarvis workflows, for example from a Shortcuts automation.")
    static let openAppWhenRun: Bool = false

    @Parameter(title: "Workflow", description: "The workflow's Shortcuts key, name or id.",
               requestValueDialog: "Which workflow?")
    var workflow: String

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> & ProvidesDialog {
        let summary = await AppModel.shared.runExternal(workflow)
        return .result(value: summary, dialog: "\(summary)")
    }
}
