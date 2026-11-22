import Foundation
import UIKit
import UserNotifications
import JarvisKit

/// `OwnerAttention` for the app: when a run starts waiting for an unlock while Jarvis is
/// not in front, post a local notification ("Unlock to continue"). Permission is never asked
/// here (the system cannot show the alert in the background or on a locked phone):
/// `AppModel` asks in the foreground, and without it nothing is posted. The notification is
/// removed when the wait ends.
struct UnlockNotifier: OwnerAttention {
    static let identifier = "jarvis.unlock-to-continue"

    func awaitUnlock(reason: String, waitedMs: UInt64) async {
        await Self.post(reason: reason)
    }

    func unlockWaitEnded() async {
        await Self.clear()
    }

    @MainActor
    private static func post(reason: String) async {
        guard UIApplication.shared.applicationState != .active else { return }
        guard await NotificationPermission.current() == .allowed else { return }
        let center = UNUserNotificationCenter.current()
        let content = UNMutableNotificationContent()
        content.title = "Unlock to continue"
        content.body = reason
        content.sound = .default
        let request = UNNotificationRequest(identifier: identifier, content: content, trigger: nil)
        try? await center.add(request)
    }

    @MainActor
    private static func clear() async {
        let center = UNUserNotificationCenter.current()
        center.removePendingNotificationRequests(withIdentifiers: [identifier])
        center.removeDeliveredNotifications(withIdentifiers: [identifier])
    }
}
