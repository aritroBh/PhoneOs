import Foundation
import JarvisCore

/// Whether Jarvis may post notifications ("Unlock to continue", a workflow's Notify step).
/// The app asks while it is in front (the system shows the alert only then) and reads the
/// state again when it becomes active; the posting code never asks.
public enum NotificationAccess: Sendable, Equatable {
    /// Not read yet (or the system reported a state this build does not know).
    case unknown
    /// The owner has not been asked yet.
    case notDetermined
    case allowed
    case denied

    public static let deniedWarning = "Notifications are off for Jarvis, so it cannot tell you when it needs an unlock."

    /// The line Control shows, `nil` unless notifications are denied.
    public var warning: String? { self == .denied ? Self.deniedWarning : nil }
}

extension Workflow {
    /// Whether a step posts a notification (saving such a workflow asks for permission).
    public var hasNotifyStep: Bool {
        steps.contains { step in
            if case .notify = step { return true }
            return false
        }
    }
}
