import Foundation
import UIKit
import UserNotifications
import JarvisKit

/// A workflow's Notify step as a local notification. While Jarvis is in front the workflow
/// run view already shows the text, so nothing is posted (and no permission is asked);
/// permission is asked the first time a notification is actually needed.
struct UserNotificationWorkflowNotifier: WorkflowNotifier {
    func post(_ text: String, workflow: String) async throws {
        try await Self.post(text, workflow: workflow)
    }

    @MainActor
    private static func post(_ text: String, workflow: String) async throws {
        guard UIApplication.shared.applicationState != .active else { return }
        let center = UNUserNotificationCenter.current()
        let granted = (try? await center.requestAuthorization(options: [.alert, .sound])) ?? false
        guard granted else { return }
        let content = UNMutableNotificationContent()
        content.title = workflow
        content.body = text
        content.sound = .default
        try await center.add(UNNotificationRequest(identifier: "jarvis.workflow.\(UUID().uuidString)", content: content, trigger: nil))
    }
}
