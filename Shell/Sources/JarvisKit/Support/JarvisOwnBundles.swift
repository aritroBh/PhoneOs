import Foundation

/// Jarvis's own bundle ids, the exact list the core treats as Jarvis's own app
/// (`SessionConfig.agentBundleIds`: no first-use question, its screen hidden unless the run
/// drives it, no party to a step, a place between a question and the asked app).
///
/// Twenty-fifth review: the core matches these ids exactly (case-sensitive); it applies no
/// prefix or suffix rule to other apps' ids, so a third-party `dev.jarvis.app.evil` is just
/// another app. The list is built here, in one place, from the main id the OS reports for
/// this very process (`Bundle.main.bundleIdentifier`) plus the suffixes Jarvis's own build
/// settings give its other app bundles (`Shell/project.yml`):
/// - `.runner.xctrunner`: the runner app XCUITest builds for the `JarvisRunner` target
///   (`PRODUCT_BUNDLE_IDENTIFIER` `<main>.runner`; XCUITest names its runner app
///   `<test bundle id>.xctrunner`), installed while Jarvis's own runner is set up;
/// - `.uitests.xctrunner`: the same for the `JarvisUITests` target (`<main>.uitests`).
/// The `.xctest` bundles themselves (`<main>.runner`, `<main>.uitests`) are loaded into those
/// runner apps and are never the app in front, so they are not listed. Jarvis ships no app
/// extension.
public enum JarvisOwnBundles {
    /// The suffixes, appended to the main id, of Jarvis's other own app bundles.
    public static let suffixes = [".runner.xctrunner", ".uitests.xctrunner"]

    /// `main` (the app's own id, as the OS reports it) first, then its own runner apps. An
    /// empty `main` gives an empty list (no app is Jarvis's own).
    public static func ids(main: String) -> [String] {
        guard !main.isEmpty else { return [] }
        return [main] + suffixes.map { main + $0 }
    }
}
