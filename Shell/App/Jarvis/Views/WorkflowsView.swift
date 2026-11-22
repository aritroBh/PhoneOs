import SwiftUI
import JarvisKit

/// Workflows (PLAN §2.7): a trigger plus ordered steps. The core's workflow runner is not
/// in this build's bindings yet, so this screen says so instead of showing anything fake.
struct WorkflowsView: View {
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                ScreenTitle(title: "Workflows", subtitle: "A trigger and a few steps, run for you.",
                            identifier: "workflows.title")
                ComingCard(
                    icon: .workflows,
                    title: "Not available yet",
                    message: "Workflows run your skills and tasks in order. They arrive with the next core update; nothing here runs yet.",
                    points: [
                        "Start by hand, by a phrase, on a schedule, or from a Shortcuts automation.",
                        "Steps: run a skill, run a task, ask you, notify, wait, stop unless the last step worked.",
                        "Every step is an ordinary task under your rules. A workflow never skips a confirmation.",
                    ],
                    identifier: "workflows.empty"
                )
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 24)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
    }
}
