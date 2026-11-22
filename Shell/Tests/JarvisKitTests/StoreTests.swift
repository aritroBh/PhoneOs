import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

private func temporaryDirectory() throws -> URL {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent("JarvisKitTests-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}

private func contents(of directory: URL) throws -> [String] {
    try FileManager.default.contentsOfDirectory(atPath: directory.path).sorted()
}

@Suite("FileMemoryStore")
struct FileMemoryStoreTests {
    @Test("missing file loads as fresh memory")
    func missing() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = FileMemoryStore(fileURL: dir.appendingPathComponent("memory.json"))
        #expect(try await store.load() == "")
    }

    @Test("save then load round-trips, creates the directory, leaves no temporary files")
    func roundTrip() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("nested/memory.json")
        let store = FileMemoryStore(fileURL: file)
        try await store.save(#"{"schema_version":1,"a":1}"#)
        try await store.save(#"{"schema_version":1,"a":2}"#)
        #expect(try await store.load() == #"{"schema_version":1,"a":2}"#)
        #expect(try contents(of: file.deletingLastPathComponent()) == ["memory.json"])
    }

    @Test("a corrupt file is quarantined and loads as fresh memory", arguments: ["{trunc", "[1,2]", "\u{FF}\u{FE}garbage", ""])
    func corrupt(_ text: String) async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("memory.json")
        try Data(text.utf8).write(to: file)
        let store = FileMemoryStore(fileURL: file, now: { Date(timeIntervalSince1970: 1_700_000_000) })
        #expect(try await store.load() == "")
        #expect(try contents(of: dir) == ["memory.json.corrupt-1700000000000"])
        let kept = try Data(contentsOf: dir.appendingPathComponent("memory.json.corrupt-1700000000000"))
        #expect(kept == Data(text.utf8), "the bad bytes are kept for inspection")
        // And the store works afterwards.
        try await store.save("{}")
        #expect(try await store.load() == "{}")
    }

    @Test("explicit quarantine (core rejected valid JSON) never overwrites an earlier one")
    func explicitQuarantine() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("memory.json")
        let store = FileMemoryStore(fileURL: file, now: { Date(timeIntervalSince1970: 1) })
        try await store.save(#"{"schema_version":999}"#)
        let first = try await store.quarantine()
        try await store.save(#"{"schema_version":998}"#)
        let second = try await store.quarantine()
        #expect(first != nil && second != nil && first != second)
        #expect(try contents(of: dir).count == 2)
        #expect(try await store.quarantine() == nil)
    }

    @Test("driver memory through the file store replays on the next run")
    func driverRoundTrip() async throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = FileMemoryStore(fileURL: dir.appendingPathComponent("memory.json"))

        func runOnce() async throws -> RunResult {
            let h = Harness()
            let config = try SessionConfigBuilder.make(capabilities: h.model.capabilities, knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")])
            var memory = try await store.load()
            let deps = DriverDependencies(runner: h.phone, model: h.model, tools: h.tools, human: h.gate, memory: store, screenshots: h.screenshots)
            let driver: SessionDriver
            do {
                driver = try SessionDriver(config: config, task: h.task, memoryJSON: memory, dependencies: deps)
            } catch {
                try await store.quarantine()
                memory = ""
                driver = try SessionDriver(config: config, task: h.task, memoryJSON: memory, dependencies: deps)
            }
            return await driver.run()
        }
        let first = try await runOnce()
        #expect(first.modelCalls == 3)
        let second = try await runOnce()
        #expect(second.outcome.status == .success)
        #expect(second.modelCalls == 0)

        // Valid JSON the core rejects: quarantined, then a fresh run.
        try await store.save(#"{"schema_version": 999}"#)
        let third = try await runOnce()
        #expect(third.modelCalls == 3)
        #expect(try contents(of: dir).count == 2)
    }
}

@Suite("ScreenshotStore")
struct ScreenshotStoreTests {
    private func shot(_ byte: UInt8) -> CapturedScreenshot {
        CapturedScreenshot(data: Data([byte]), mimeType: "image/png", pixelWidth: 3, pixelHeight: 4)
    }

    @Test("stores under fresh ids with the pixel size")
    func store() {
        let store = ScreenshotStore()
        let a = store.store(shot(1))
        let b = store.store(shot(2))
        #expect(a.id != b.id)
        #expect(a.width == 3 && a.height == 4)
        #expect(store.screenshot(for: a.id)?.data == Data([1]))
        #expect(store.screenshot(for: "nope") == nil)
    }

    @Test("evicts the least recently used beyond capacity")
    func lru() {
        let store = ScreenshotStore(capacity: 2)
        let a = store.store(shot(1))
        let b = store.store(shot(2))
        #expect(store.screenshot(for: a.id) != nil) // a is now the most recent
        let c = store.store(shot(3))
        #expect(store.screenshot(for: b.id) == nil)
        #expect(store.storedIDs == [a.id, c.id])
        store.removeAll()
        #expect(store.storedIDs.isEmpty)
        #expect(ScreenshotStore(capacity: 0).capacity == 1)
    }
}
