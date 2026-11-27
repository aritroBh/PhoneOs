import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

// The Live view pieces that stay in Swift: the frame store (pixels) and the settings field.
// The mapping, cursor path and scrubber tests moved with that logic to
// core/jarvis-viewer/tests/geometry.rs.

@Suite("Live view: frame store", .timeLimit(.minutes(1)))
struct ViewerFrameStoreTests {
    private func shot(_ bytes: Int) -> CapturedScreenshot {
        CapturedScreenshot(data: Data(repeating: 7, count: bytes), mimeType: "image/jpeg", pixelWidth: 10, pixelHeight: 20)
    }

    private func key(_ n: Int) -> ViewerFrameKey { ViewerFrameKey(taskId: "t", observation: n) }

    @Test("oldest frames go first when the count cap is reached")
    func byCount() {
        let store = ViewerFrameStore(maxFrames: 3, maxBytes: 1_000_000)
        for n in 1...5 { #expect(store.put(shot(10), for: key(n))) }
        #expect(store.keys == [key(3), key(4), key(5)])
        #expect(store.frame(for: key(1)) == nil)
        #expect(store.frame(for: key(5))?.data.count == 10)
        #expect(store.byteCount == 30)
    }

    @Test("oldest frames go first when the byte cap is reached; a frame over the cap is not kept")
    func byBytes() {
        let store = ViewerFrameStore(maxFrames: 100, maxBytes: 100)
        store.put(shot(40), for: key(1))
        store.put(shot(40), for: key(2))
        store.put(shot(40), for: key(3))
        #expect(store.keys == [key(2), key(3)])
        #expect(store.byteCount == 80)
        #expect(!store.put(shot(101), for: key(4)))
        #expect(store.keys == [key(2), key(3)])
        store.put(shot(100), for: key(5))
        #expect(store.keys == [key(5)] && store.byteCount == 100)
    }

    @Test("a frame stored again under its key replaces it; removeAll empties the store")
    func replaceAndClear() {
        let store = ViewerFrameStore(maxFrames: 4, maxBytes: 1_000)
        store.put(shot(10), for: key(1))
        store.put(shot(30), for: key(1))
        #expect(store.count == 1 && store.byteCount == 30)
        // Replacing with a frame over the cap drops the old one too.
        #expect(!store.put(shot(5_000), for: key(1)))
        #expect(store.count == 0 && store.byteCount == 0)
        store.put(shot(10), for: key(2))
        store.removeAll()
        #expect(store.count == 0 && store.byteCount == 0 && store.frame(for: key(2)) == nil)
        // Disabled (the owner turned the option off mid-run): everything goes, nothing is kept.
        store.put(shot(10), for: key(3))
        store.isEnabled = false
        #expect(store.count == 0 && !store.put(shot(10), for: key(4)) && store.count == 0)
        store.isEnabled = true
        #expect(store.put(shot(10), for: key(4)))
        let tiny = ViewerFrameStore(maxFrames: 0, maxBytes: 0)
        #expect(tiny.maxFrames == 1 && tiny.maxBytes == 1)
    }
}

@Suite("Live view: the real-screen setting", .timeLimit(.minutes(1)))
struct LiveViewSettingTests {
    private func decode(_ json: String) throws -> JarvisSettings {
        try JSONDecoder().decode(JarvisSettings.self, from: Data(json.utf8))
    }

    @Test("off by default and when missing from an older file")
    func defaults() throws {
        #expect(!JarvisSettings().liveViewScreenshots)
        #expect(try !decode("{}").liveViewScreenshots)
        #expect(try decode(#"{"live_view_screenshots": true}"#).liveViewScreenshots)
    }

    @Test("a value that cannot be read is off, and does not touch the safety fields")
    func malformed() throws {
        for bad in [#""yes""#, "null", "1", "[]", #"{"on": true}"#] {
            let settings = try decode(#"{"live_view_screenshots": \#(bad), "policy_mode": "step_by_step", "max_steps": 12, "first_use_approval": false}"#)
            #expect(!settings.liveViewScreenshots, "\(bad)")
            #expect(settings.policyMode == .stepByStep && settings.maxSteps == 12 && !settings.firstUseApproval)
        }
        // A broken safety field still fails the whole file, whatever this field says.
        #expect(throws: (any Error).self) {
            try decode(#"{"live_view_screenshots": true, "policy_mode": "yolo"}"#)
        }
    }

    @Test("written under its own key and read back")
    func roundTrip() throws {
        var settings = JarvisSettings()
        settings.liveViewScreenshots = true
        let data = try JSONEncoder().encode(settings)
        #expect(String(decoding: data, as: UTF8.self).contains("\"live_view_screenshots\":true"))
        #expect(try JSONDecoder().decode(JarvisSettings.self, from: data) == settings)
    }
}
