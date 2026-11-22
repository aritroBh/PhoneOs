import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

// The Live view's pure parts: point → view mapping, the cursor path, the scrubber, the frame
// store and the settings field.

private func close(_ a: Double, _ b: Double, _ tolerance: Double = 1e-9) -> Bool { abs(a - b) <= tolerance }
private func close(_ a: ViewerPoint, _ b: ViewerPoint, _ tolerance: Double = 1e-9) -> Bool {
    close(a.x, b.x, tolerance) && close(a.y, b.y, tolerance)
}

@Suite("Live view: screen to view mapping", .timeLimit(.minutes(1)))
struct ViewerMappingTests {
    @Test("a portrait screen in a wide view is centred with bars left and right")
    func portraitLetterbox() {
        let m = ViewerMapping(screen: ViewerSize(width: 400, height: 800), view: ViewerSize(width: 600, height: 400))
        #expect(close(m.scale, 0.5))
        #expect(close(m.origin, ViewerPoint(x: 200, y: 0)))
        #expect(m.contentRect == ViewerRect(x: 200, y: 0, width: 200, height: 400))
        #expect(close(m.toView(ViewerPoint(x: 0, y: 0)), ViewerPoint(x: 200, y: 0)))
        #expect(close(m.toView(ViewerPoint(x: 400, y: 800)), ViewerPoint(x: 400, y: 400)))
        #expect(close(m.toView(ViewerPoint(x: 200, y: 400)), ViewerPoint(x: 300, y: 200)))
    }

    @Test("a landscape screen in a portrait view is centred with bars above and below")
    func landscapeLetterbox() {
        let screen = ViewerSize(width: 852, height: 393)
        #expect(screen.isLandscape)
        let m = ViewerMapping(screen: screen, view: ViewerSize(width: 426, height: 600))
        #expect(close(m.scale, 0.5))
        #expect(close(m.origin.x, 0))
        #expect(close(m.origin.y, (600 - 393 * 0.5) / 2))
        #expect(close(m.toView(ViewerPoint(x: 852, y: 393)), ViewerPoint(x: 426, y: m.origin.y + 196.5)))
        #expect(screen.displayAspect > 1)
    }

    @Test("points outside the screen land on its edge, never in the bars; NaN goes to the centre")
    func clamping() {
        let m = ViewerMapping(screen: ViewerSize(width: 400, height: 800), view: ViewerSize(width: 600, height: 400))
        #expect(close(m.toView(ViewerPoint(x: -100, y: -5)), ViewerPoint(x: 200, y: 0)))
        #expect(close(m.toView(ViewerPoint(x: 9_999, y: 1e300)), ViewerPoint(x: 400, y: 400)))
        #expect(close(m.toView(ViewerPoint(x: .nan, y: .infinity)), ViewerPoint(x: 300, y: 200)))
        let inside = m.contentRect
        for p in [ViewerPoint(x: -1e9, y: 3), ViewerPoint(x: .nan, y: .nan), ViewerPoint(x: 5, y: -.infinity)] {
            let v = m.toView(p)
            #expect(v.x >= inside.x && v.x <= inside.maxX && v.y >= inside.y && v.y <= inside.maxY)
        }
    }

    @Test("zero, negative and NaN screen sizes use the fallback phone; an empty view maps to nothing")
    func degenerateSizes() {
        for bad in [ViewerSize(width: 0, height: 0), ViewerSize(width: -10, height: 800), ViewerSize(width: .nan, height: 1),
                    ViewerSize(width: .infinity, height: 800)] {
            #expect(!bad.isUsable)
            let m = ViewerMapping(screen: bad, view: ViewerSize(width: 393, height: 852))
            #expect(m.screen == ViewerSize.fallbackPhone)
            #expect(close(m.scale, 1))
            #expect(m.toView(ViewerPoint(x: 10, y: 10)).isFinite)
        }
        for view in [ViewerSize(width: 0, height: 0), ViewerSize(width: -5, height: -5), ViewerSize(width: .nan, height: 100)] {
            let m = ViewerMapping(screen: ViewerSize(width: 400, height: 800), view: view)
            #expect(m.scale == 0)
            #expect(m.toView(ViewerPoint(x: 100, y: 100)).isFinite)
            #expect(m.toScreen(ViewerPoint(x: 0, y: 0)) == nil)
            #expect(m.toView(ViewerRect(x: 0, y: 0, width: 10, height: 10)).map { $0.width == 0 && $0.height == 0 } ?? true)
        }
        #expect(ViewerSize(width: 1, height: 10_000).displayAspect == 0.3)
        #expect(ViewerSize(width: 10_000, height: 1).displayAspect == 3)
    }

    @Test("rectangles are clipped to the screen; fully off-screen ones map to nil")
    func rects() {
        let m = ViewerMapping(screen: ViewerSize(width: 400, height: 800), view: ViewerSize(width: 400, height: 800))
        #expect(m.toView(ViewerRect(x: -50, y: 790, width: 100, height: 100)) == ViewerRect(x: 0, y: 790, width: 50, height: 10))
        #expect(m.toView(ViewerRect(x: 500, y: 0, width: 10, height: 10)) == nil)
        #expect(m.toView(ViewerRect(x: 0, y: 0, width: 0, height: 10)) == nil)
        #expect(m.toView(ViewerRect(x: .nan, y: 0, width: 10, height: 10)) == nil)
    }

    @Test("view points map back to the screen point under them")
    func inverse() throws {
        let m = ViewerMapping(screen: ViewerSize(width: 393, height: 852), view: ViewerSize(width: 300, height: 500))
        for p in [ViewerPoint(x: 0, y: 0), ViewerPoint(x: 196.5, y: 426), ViewerPoint(x: 393, y: 852), ViewerPoint(x: 12.25, y: 700)] {
            let back = try #require(m.toScreen(m.toView(p)))
            #expect(close(back, p, 1e-6))
        }
        #expect(m.toScreen(ViewerPoint(x: 0, y: 0)) == nil, "the letterbox is not on the screen")
    }

    @Test("half-open containment, like the core's Rect")
    func containment() {
        let r = ViewerRect(x: 10, y: 10, width: 10, height: 10)
        #expect(r.contains(ViewerPoint(x: 10, y: 10)))
        #expect(!r.contains(ViewerPoint(x: 20, y: 15)))
        #expect(!r.contains(ViewerPoint(x: .nan, y: 15)))
        #expect(!ViewerRect(x: 0, y: 0, width: -5, height: 5).contains(ViewerPoint(x: -1, y: 1)))
    }
}

@Suite("Live view: cursor path", .timeLimit(.minutes(1)))
struct CursorPathTests {
    let screen = ViewerSize(width: 393, height: 852)

    @Test("ease-in-out starts at 0, ends exactly at 1 and never goes back")
    func ease() {
        #expect(CursorPath.ease(0) == 0)
        #expect(CursorPath.ease(1) == 1)
        #expect(CursorPath.ease(-3) == 0)
        #expect(CursorPath.ease(7) == 1)
        #expect(CursorPath.ease(.nan) == 0)
        var last = -1.0
        for i in 0...1_000 {
            let v = CursorPath.ease(Double(i) / 1_000)
            #expect(v >= last)
            last = v
        }
    }

    @Test("a tap glides from rest to the target, presses there and ends exactly on it")
    func tap() {
        let rest = ViewerPoint(x: 50, y: 700)
        let target = ViewerPoint(x: 300, y: 120)
        let gesture = ViewerGesture.tap(target)
        let timing = CursorPath.timing(gesture, from: rest, screen: screen)
        #expect(close(timing.glideMs, CursorPath.glideMs(distance: rest.distance(to: target))))
        #expect(timing.actionMs == CursorPath.tapMs)
        #expect(CursorPath.pose(gesture, from: rest, screen: screen, atMs: 0).position == rest)
        // Monotone progress: the distance to the target never grows.
        var lastDistance = Double.infinity
        for i in 0...200 {
            let t = timing.glideMs * Double(i) / 200
            let pose = CursorPath.pose(gesture, from: rest, screen: screen, atMs: t)
            let d = pose.position.distance(to: target)
            #expect(d <= lastDistance + 1e-9)
            lastDistance = d
        }
        let pressing = CursorPath.pose(gesture, from: rest, screen: screen, atMs: timing.glideMs + 1)
        #expect(pressing.phase == .pressing && pressing.isPressed && pressing.position == target)
        let end = CursorPath.pose(gesture, from: rest, screen: screen, atMs: timing.totalMs)
        #expect(end.phase == .resting && end.position == target && end.progress == 1)
        #expect(CursorPath.pose(gesture, from: rest, screen: screen, atMs: 1e12).position == target)
        #expect(CursorPath.restingPoint(after: gesture, from: rest, screen: screen) == target)
    }

    @Test("glide time grows with distance and is clamped")
    func glideDuration() {
        #expect(CursorPath.glideMs(distance: 0) == CursorPath.glideRangeMs.lowerBound)
        #expect(CursorPath.glideMs(distance: 1e9) == CursorPath.glideRangeMs.upperBound)
        #expect(CursorPath.glideMs(distance: .nan) == CursorPath.glideRangeMs.lowerBound)
        #expect(CursorPath.glideMs(distance: -40) == CursorPath.glideRangeMs.lowerBound)
        #expect(CursorPath.glideMs(distance: 300) > CursorPath.glideMs(distance: 200))
    }

    @Test("a swipe glides to its start, drags to its end over its duration and rests there")
    func swipe() {
        let rest = ViewerPoint(x: 196, y: 426)
        let from = ViewerPoint(x: 196, y: 700), to = ViewerPoint(x: 196, y: 200)
        let gesture = ViewerGesture.swipe(from: from, to: to, durationMs: 400)
        let timing = CursorPath.timing(gesture, from: rest, screen: screen)
        #expect(timing.actionMs == 400)
        #expect(CursorPath.pose(gesture, from: rest, screen: screen, atMs: timing.glideMs).position == from)
        var lastY = Double.infinity
        for i in 0...100 {
            let pose = CursorPath.pose(gesture, from: rest, screen: screen, atMs: timing.glideMs + 400 * Double(i) / 100)
            #expect(pose.position.y <= lastY + 1e-9, "moves up only")
            lastY = pose.position.y
        }
        #expect(CursorPath.pose(gesture, from: rest, screen: screen, atMs: timing.totalMs).position == to)
        #expect(CursorPath.timing(.swipe(from: from, to: to, durationMs: 1), from: rest, screen: screen).actionMs
            == CursorPath.swipeRangeMs.lowerBound)
        #expect(CursorPath.timing(.swipe(from: from, to: to, durationMs: 90_000), from: rest, screen: screen).actionMs
            == CursorPath.swipeRangeMs.upperBound)
    }

    @Test("a long press holds for its (clamped) duration with the ring filling")
    func longPress() {
        let rest = ViewerPoint(x: 10, y: 10)
        let p = ViewerPoint(x: 100, y: 100)
        let gesture = ViewerGesture.longPress(p, durationMs: 1_000)
        let timing = CursorPath.timing(gesture, from: rest, screen: screen)
        #expect(timing.actionMs == 1_000)
        let half = CursorPath.pose(gesture, from: rest, screen: screen, atMs: timing.glideMs + 500)
        #expect(half.phase == .pressing && close(half.progress, 0.5) && half.position == p)
        #expect(CursorPath.timing(.longPress(p, durationMs: 0), from: rest, screen: screen).actionMs == CursorPath.longPressRangeMs.lowerBound)
        #expect(CursorPath.timing(.longPress(p, durationMs: .max), from: rest, screen: screen).actionMs
            == CursorPath.longPressRangeMs.upperBound)
    }

    @Test("typing, keys, launching, URLs and waits leave the cursor parked")
    func parked() {
        let rest = ViewerPoint(x: 120, y: 330)
        for gesture: ViewerGesture in [.type(text: "hi"), .type(text: nil), .key("Return"), .launch("Settings"),
                                       .openURL(host: "example.com"), .wait(ms: 800)] {
            for t in [0.0, 100, 5_000] {
                let pose = CursorPath.pose(gesture, from: rest, screen: screen, atMs: t)
                #expect(pose.position == rest && pose.phase == .parked && !pose.isPressed)
            }
            #expect(CursorPath.restingPoint(after: gesture, from: rest, screen: screen) == rest)
            #expect(CursorPath.timing(gesture, from: rest, screen: screen).glideMs == 0)
        }
    }

    @Test("targets off the screen and NaN rest points are clamped onto it; NaN and negative times are 0")
    func clamping() {
        let gesture = ViewerGesture.tap(ViewerPoint(x: -500, y: 5_000))
        let rest = ViewerPoint(x: .nan, y: .nan)
        let start = CursorPath.pose(gesture, from: rest, screen: screen, atMs: -10)
        #expect(start.position == screen.center)
        #expect(CursorPath.pose(gesture, from: rest, screen: screen, atMs: .nan) == start)
        let end = CursorPath.pose(gesture, from: rest, screen: screen, atMs: 1e9)
        #expect(end.position == ViewerPoint(x: 0, y: 852))
        // A zero-size screen draws on the fallback phone.
        let zero = CursorPath.pose(.tap(ViewerPoint(x: 10, y: 10)), from: .zero, screen: ViewerSize(width: 0, height: 0), atMs: 1e9)
        #expect(zero.position == ViewerPoint(x: 10, y: 10))
    }

    @Test("Reduce Motion jumps to the target instead of gliding")
    func reduceMotion() {
        let rest = ViewerPoint(x: 0, y: 0), p = ViewerPoint(x: 300, y: 600)
        let pose = CursorPath.pose(.tap(p), from: rest, screen: screen, atMs: 0, reduceMotion: true)
        #expect(pose.position == p && pose.phase == .pressing)
        let swipe = ViewerGesture.swipe(from: p, to: ViewerPoint(x: 300, y: 100), durationMs: 300)
        #expect(CursorPath.pose(swipe, from: rest, screen: screen, atMs: 10, reduceMotion: true).position == ViewerPoint(x: 300, y: 100))
    }

    @Test("consecutive steps join: each starts exactly where the previous one rested")
    func continuity() {
        let size = ViewerSize(width: 393, height: 852)
        let screenModel = ViewerScreen(id: 1, size: size, appName: "A", bundleId: "a", source: .tree, elements: [],
                                       elementsTruncated: false, frameKey: nil, focusedInputSecure: nil)
        let gestures: [ViewerGesture] = [
            .tap(ViewerPoint(x: 30, y: 40)), .type(text: nil), .swipe(from: ViewerPoint(x: 200, y: 700), to: ViewerPoint(x: 200, y: 100), durationMs: 300),
            .key("Return"), .longPress(ViewerPoint(x: 999, y: -4), durationMs: 600), .wait(ms: 100), .tap(ViewerPoint(x: 196, y: 426)),
        ]
        let steps = gestures.enumerated().map { i, g in
            ViewerStep(number: i + 1, coreStep: nil, screenID: 1, gesture: g, target: nil, caption: "", state: .done)
        }
        let timeline = ViewerTimeline(taskId: "t", steps: steps, screens: [screenModel], currentScreenID: 1)
        #expect(CursorPath.restingPoint(before: 1, in: timeline) == size.center, "the first step glides in from the middle")
        for i in 0..<(steps.count - 1) {
            let a = steps[i], b = steps[i + 1]
            let endA = CursorPath.pose(step: a.number, in: timeline, atMs: 1e9)!
            let startB = CursorPath.pose(step: b.number, in: timeline, atMs: 0)!
            #expect(close(endA.position, startB.position), "jump between step \(a.number) and \(b.number)")
        }
        #expect(CursorPath.restingPoint(afterLatestOf: timeline) == ViewerPoint(x: 196, y: 426))
        #expect(CursorPath.pose(step: 99, in: timeline, atMs: 0) == nil)
    }
}

@Suite("Live view: replay controls", .timeLimit(.minutes(1)))
struct ReplayStateTests {
    @Test("live follows the newest step and restarts its animation")
    func follows() {
        var r = ReplayState()
        r.sync(numbers: [], now: 0)
        #expect(r.current == nil)
        r.sync(numbers: [1], now: 1)
        #expect(r.current == 1 && r.isFollowing && r.startedAt == 1)
        r.sync(numbers: [1], now: 2)
        #expect(r.startedAt == 1, "the same step does not restart")
        r.sync(numbers: [1, 2], now: 3)
        #expect(r.current == 2 && r.startedAt == 3)
        #expect(r.fraction(numbers: [1, 2]) == 1)
    }

    @Test("stepping back leaves live; new steps do not move the shown step; Live returns")
    func scrubbedBackStays() {
        var r = ReplayState()
        r.sync(numbers: [1, 2, 3], now: 0)
        r.stepBack(numbers: [1, 2, 3], now: 1)
        #expect(r.current == 2 && !r.isFollowing && !r.isPlaying)
        r.stepBack(numbers: [1, 2, 3], now: 1)
        r.stepBack(numbers: [1, 2, 3], now: 1)
        #expect(r.current == 1, "stops at the oldest")
        r.sync(numbers: [1, 2, 3, 4, 5], now: 2)
        #expect(r.current == 1, "not yanked forward")
        r.stepForward(numbers: [1, 2, 3, 4, 5], now: 3)
        #expect(r.current == 2 && !r.isFollowing)
        r.goLive(numbers: [1, 2, 3, 4, 5], now: 4)
        #expect(r.current == 5 && r.isFollowing && r.startedAt == 4)
        r.sync(numbers: [1, 2, 3, 4, 5, 6], now: 5)
        #expect(r.current == 6)
    }

    @Test("step forward stops at the newest step without going live")
    func forwardEnd() {
        var r = ReplayState()
        r.sync(numbers: [1, 2], now: 0)
        r.stepForward(numbers: [1, 2], now: 1)
        #expect(r.current == 2 && !r.isFollowing)
        r.sync(numbers: [1, 2, 3], now: 2)
        #expect(r.current == 2)
    }

    @Test("play runs through the steps by time, at 1× or 2×, and stops at the newest")
    func play() {
        var r = ReplayState()
        let numbers = [4, 5, 6]
        r.sync(numbers: numbers, now: 0)
        r.togglePlay(numbers: numbers, now: 10)
        #expect(r.isPlaying && !r.isFollowing && r.current == 4, "at the newest step, play starts from the oldest")
        let duration: (Int) -> Double = { _ in 1_000 }
        let moved1 = r.advance(numbers: numbers, now: 10.5, playMs: duration)
        #expect(!moved1)
        let moved2 = r.advance(numbers: numbers, now: 11, playMs: duration)
        #expect(moved2)
        #expect(r.current == 5 && r.startedAt == 11)
        r.setSpeed(.double, now: 11)
        #expect(close(r.elapsedMs(now: 11.25), 500))
        let moved3 = r.advance(numbers: numbers, now: 11.5, playMs: duration)
        #expect(moved3)
        #expect(r.current == 6)
        let moved4 = r.advance(numbers: numbers, now: 20, playMs: duration)
        #expect(!moved4)
        #expect(!r.isPlaying && r.current == 6, "stops at the newest")
        r.togglePlay(numbers: numbers, now: 21)
        r.togglePlay(numbers: numbers, now: 21.1)
        #expect(!r.isPlaying && r.current == 4)
    }

    @Test("changing speed keeps the animation where it is")
    func speedKeepsPosition() {
        var r = ReplayState()
        r.sync(numbers: [1, 2], now: 0)
        r.scrub(to: 1, numbers: [1, 2], now: 10)
        #expect(close(r.elapsedMs(now: 10.4), 400))
        r.setSpeed(.double, now: 10.4)
        #expect(close(r.elapsedMs(now: 10.4), 400))
        #expect(close(r.elapsedMs(now: 10.5), 600))
        #expect(ReplayState.Speed.normal.next == .double && ReplayState.Speed.double.title == "2×")
    }

    @Test("the shown step dropped by the cap moves to the oldest kept step")
    func evicted() {
        var r = ReplayState()
        r.sync(numbers: [1, 2, 3], now: 0)
        r.scrub(to: 1, numbers: [1, 2, 3], now: 1)
        r.sync(numbers: [3, 4, 5], now: 2)
        #expect(r.current == 3 && !r.isFollowing)
        r.sync(numbers: [], now: 3)
        #expect(r.current == nil && !r.isPlaying)
    }

    @Test("the scrubber's fraction picks a step; unknown numbers and empty timelines are ignored")
    func scrubber() {
        var r = ReplayState()
        let numbers = [10, 11, 12, 13, 14]
        r.sync(numbers: numbers, now: 0)
        r.scrub(fraction: 0.5, numbers: numbers, now: 1)
        #expect(r.current == 12 && r.fraction(numbers: numbers) == 0.5)
        r.scrub(fraction: .nan, numbers: numbers, now: 1)
        #expect(r.current == 10)
        r.scrub(fraction: 7, numbers: numbers, now: 1)
        #expect(r.current == 14)
        r.scrub(to: 99, numbers: numbers, now: 2)
        #expect(r.current == 14)
        var empty = ReplayState()
        empty.stepBack(numbers: [], now: 0)
        empty.togglePlay(numbers: [], now: 0)
        empty.scrub(fraction: 0.3, numbers: [], now: 0)
        #expect(empty.current == nil && !empty.isPlaying && empty.isFollowing)
        #expect(empty.elapsedMs(now: .nan) == 0)
    }

    private func close(_ a: Double, _ b: Double) -> Bool { abs(a - b) < 1e-6 }
}

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
