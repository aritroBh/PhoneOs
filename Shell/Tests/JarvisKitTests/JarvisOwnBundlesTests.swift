import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// Twenty-fifth review: Jarvis's own bundles are an exact list built from the main id, and
/// the core matches it exactly.
@Suite("Jarvis's own bundles", .timeLimit(.minutes(1)))
struct JarvisOwnBundlesTests {
    @Test("the list is the main id and its own runner apps; empty main gives none")
    func ids() {
        #expect(JarvisOwnBundles.ids(main: "dev.jarvis.app") == [
            "dev.jarvis.app", "dev.jarvis.app.runner.xctrunner", "dev.jarvis.app.uitests.xctrunner",
        ])
        #expect(JarvisOwnBundles.ids(main: "").isEmpty)
    }

    /// Every UI-test target in `project.yml` has its XCUITest runner app
    /// (`<test bundle id>.xctrunner`) in the list, and the list holds nothing else.
    @Test("the list matches the bundle ids in project.yml")
    func matchesProject() throws {
        let project = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("project.yml")
        let text = try String(contentsOf: project, encoding: .utf8)
        let ids = text.split(separator: "\n").compactMap { line -> String? in
            let parts = line.split(separator: ":", maxSplits: 1)
            guard parts.count == 2, parts[0].trimmingCharacters(in: .whitespaces) == "PRODUCT_BUNDLE_IDENTIFIER"
            else { return nil }
            return parts[1].trimmingCharacters(in: .whitespaces)
        }
        let main = "dev.jarvis.app"
        #expect(ids.contains(main), "\(ids)")
        let runners = ids.filter { $0 != main }.map { $0 + ".xctrunner" }
        #expect(!runners.isEmpty)
        #expect(Set(JarvisOwnBundles.ids(main: main)) == Set([main] + runners), "\(ids)")
    }

    private func hidden(_ app: String, ids: [String]) throws -> Bool {
        let config = try SessionConfigBuilder.make(
            capabilities: ModelCapabilities(contextTokens: 8_000, supportsVision: false), agentBundleIds: ids
        )
        #expect(config.agentBundleIds == ids)
        let session = try JarvisSession(config: config, task: TaskSpec(id: "t", text: "x"), memoryJson: "")
        _ = session.step(nowMs: 1, event: .start)
        let root = UiNode(role: .application, label: "App", value: nil, identifier: nil, placeholder: nil,
                          frame: Rect(x: 0, y: 0, width: 393, height: 852), enabled: true, visible: true,
                          hittable: true, focused: false, selected: false, secure: false, children: [])
        let screen = Screen(appBundleId: app, appName: "App", size: Size(width: 393, height: 852), root: root,
                            keyboardVisible: false, orientation: .portrait)
        _ = session.step(nowMs: 2, event: .observed(observation: ScreenObservation(screen: screen, screenshot: nil,
                                                                                   deviceLocked: false)))
        return try session.progress().screenHidden
    }

    @Test("the builder passes the list through the FFI; the core matches it exactly")
    func throughFFI() throws {
        let own = JarvisOwnBundles.ids(main: "dev.jarvis.app")
        for id in own { #expect(try hidden(id, ids: own), "\(id)") }
        for id in ["dev.jarvis.app.evil", "dev.jarvis", "DEV.jarvis.app", "dev.jarvis.app.runner"] {
            #expect(try !hidden(id, ids: own), "\(id)")
        }
        #expect(try !hidden("dev.jarvis.app", ids: []))
    }
}
