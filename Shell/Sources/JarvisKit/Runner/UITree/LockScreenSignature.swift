import Foundation

/// Recognises SpringBoard's lock screen in a raw snapshot, for runners without a lock-state
/// API. Evidence: the simulator lock probe (run 16, iPhone 17 Pro Max, iOS 26.5) recorded,
/// only while locked, an element labelled "Swipe up to unlock" with identifier
/// `proud-lock`, inside a system-aperture element whose identifier contains
/// `systemApertureElementIdentifierLock`. None of these appeared on the unlocked home
/// screen or in apps. Other iOS versions and devices without a Dynamic Island are
/// [unverified].
public enum LockScreenSignature {
    /// Identifier fragments seen only on the lock screen.
    public static let identifierFragments = ["systemApertureElementIdentifierLock", "proud-lock"]
    /// Labels seen only on the lock screen.
    public static let labels = ["Swipe up to unlock"]

    /// Whether `raw` (normally SpringBoard's application element) shows the lock screen.
    public static func matches(_ raw: RawElement) -> Bool {
        var stack = [raw]
        while let e = stack.popLast() {
            if labels.contains(e.label) { return true }
            if !e.identifier.isEmpty, identifierFragments.contains(where: e.identifier.contains) { return true }
            stack.append(contentsOf: e.children)
        }
        return false
    }
}
