import Foundation
import UIKit
import UserNotifications
import JarvisKit

/// Notification permission. `AppModel` asks while Jarvis is in front (the system shows the
/// alert only then: asked from the background or on a locked phone it never appears and the
/// notification is lost) and reads the state again when Jarvis becomes active. The posting
/// code only reads it and never asks.
enum NotificationPermission {
    static func current() async -> NotificationAccess {
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        return access(settings.authorizationStatus)
    }

    /// Shows the system alert if the owner was never asked, then reads the state.
    static func request() async -> NotificationAccess {
        _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
        return await current()
    }

    static func access(_ status: UNAuthorizationStatus) -> NotificationAccess {
        switch status {
        case .authorized, .provisional, .ephemeral: .allowed
        case .denied: .denied
        case .notDetermined: .notDetermined
        @unknown default: .unknown
        }
    }
}

/// A workflow's Notify step as a local notification. While Jarvis is in front the workflow
/// run view already shows the text, so nothing is posted. Without permission nothing is
/// posted either (it was asked in the foreground when the workflow was saved).
struct UserNotificationWorkflowNotifier: WorkflowNotifier {
    func post(_ text: String, workflow: String) async throws {
        try await Self.post(text, workflow: workflow)
    }

    @MainActor
    private static func post(_ text: String, workflow: String) async throws {
        guard UIApplication.shared.applicationState != .active else { return }
        guard await NotificationPermission.current() == .allowed else { return }
        let center = UNUserNotificationCenter.current()
        let content = UNMutableNotificationContent()
        content.title = workflow
        content.body = text
        content.sound = .default
        try await center.add(UNNotificationRequest(identifier: "jarvis.workflow.\(UUID().uuidString)", content: content, trigger: nil))
    }
}
