import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// The top layer (`TopLayer`, `BannerCover`): the pure decisions the Mac-only XCUITest runner
/// makes from its OS queries, on the numbers of the real iOS 26.5 trees (probe runs 134 and
/// 135).
@Suite("Top layer: system alerts, Spotlight and banners (TopLayer, BannerCover)")
struct TopLayerTests {
    let jarvis = "dev.jarvis.app"
    let settings = "com.apple.Preferences"

    @Test("the decision table: alert, then Spotlight, then the foreground reading")
    func table() {
        typealias D = TopLayer.Decision
        let sb = TopLayer.springBoard
        let sp = TopLayer.spotlight
        // (alert up, Spotlight in front, foreground) -> top layer
        let rows: [(Bool, Bool, String, D)] = [
            (false, false, jarvis, D(bundleId: jarvis)),
            (false, false, sb, D(bundleId: sb)),
            // A SpringBoard alert over the app (`alert_sign_in_consent_springboard.txt`: the
            // runner read Jarvis in front, the alert was SpringBoard's).
            (true, false, jarvis, D(bundleId: sb, alertOver: jarvis, systemAlert: true)),
            (true, false, sb, D(bundleId: sb, alertOver: nil, systemAlert: true)),
            // The alert wins over Spotlight too.
            (true, true, sb, D(bundleId: sb, alertOver: nil, systemAlert: true)),
            // Spotlight up (`spotlight_springboard_runner.txt`: the runner read SpringBoard
            // in front, its tree the status bar only).
            (false, true, sb, D(bundleId: sp)),
            (false, true, settings, D(bundleId: sp)),
        ]
        for (alert, spotlight, front, want) in rows {
            let got = TopLayer.decide(systemAlertUp: alert, spotlightInFront: spotlight, foreground: front, springBoard: .homeScreen)
            #expect(got == want, "alert \(alert), spotlight \(spotlight), foreground \(front)")
            // What SpringBoard shows matters only for a SpringBoard reading with neither an
            // alert nor Spotlight up (the front-app rule): every other row is the same.
            if alert || spotlight || !TopLayer.isSpringBoard(front) {
                for shows in [TopLayer.SpringBoardShows.lockScreen, .other] {
                    #expect(TopLayer.decide(systemAlertUp: alert, spotlightInFront: spotlight, foreground: front, springBoard: shows) == want)
                }
            }
        }
        // The front-app rule: SpringBoard is the top layer only while it shows its Home
        // Screen or the lock screen; else an app Jarvis cannot identify is in front.
        #expect(TopLayer.decide(systemAlertUp: false, spotlightInFront: false, foreground: sb, springBoard: .lockScreen) == D(bundleId: sb))
        let unknown = TopLayer.decide(systemAlertUp: false, spotlightInFront: false, foreground: sb, springBoard: .other)
        #expect(unknown == D(bundleId: FrontApp.unidentified))
        #expect(unknown.note == FrontApp.unidentifiedNote)
    }

    @Test("the note says what the top layer is, and which app an alert is over")
    func notes() {
        let over = TopLayer.decide(systemAlertUp: true, spotlightInFront: false, foreground: jarvis, springBoard: .homeScreen)
        #expect(over.note?.contains("over \(jarvis)") == true)
        #expect(over.note?.contains("SpringBoard") == true)
        let home = TopLayer.decide(systemAlertUp: true, spotlightInFront: false, foreground: " COM.APPLE.springboard ", springBoard: .homeScreen)
        #expect(home.alertOver == nil)
        #expect(TopLayer.decide(systemAlertUp: false, spotlightInFront: true, foreground: TopLayer.springBoard, springBoard: .homeScreen).note?
            .contains(TopLayer.spotlight) == true)
        #expect(TopLayer.decide(systemAlertUp: false, spotlightInFront: false, foreground: jarvis, springBoard: .homeScreen).note == nil)
    }

    // MARK: - Banner frames: pixels to points

    /// SpringBoard's application node and windows as in `banner_springboard.txt` (run 134):
    /// the banner's window is in points; two other windows report pixels.
    private func springBoard(bannerInPixelWindow: Bool) -> RawElement {
        let screen = Rect(x: 0, y: 0, width: 440, height: 956)
        let pixels = Rect(x: 0, y: 0, width: 1320, height: 2868)
        let bannerPoints = Rect(x: 8, y: 58.7, width: 424, height: 66)
        let bannerPixels = Rect(x: 24, y: 176.1, width: 1272, height: 198)
        let banner = RawElement(elementType: 83, label: "JARVIS, now, Jarvis probe, Probe banner 4417",
                                identifier: BannerCover.identifier,
                                frame: bannerInPixelWindow ? bannerPixels : bannerPoints)
        let notification = RawElement(elementType: XCUIElementTypeCode.other, label: "Notification",
                                      frame: bannerInPixelWindow ? bannerPixels : bannerPoints, children: [banner])
        let pixelWindow = RawElement(elementType: XCUIElementTypeCode.window, frame: pixels, children: [
            RawElement(elementType: XCUIElementTypeCode.other, frame: pixels),
        ] + (bannerInPixelWindow ? [notification] : []))
        let pointWindow = RawElement(elementType: XCUIElementTypeCode.window, frame: screen, children: [
            RawElement(elementType: XCUIElementTypeCode.other, frame: screen, children: bannerInPixelWindow ? [] : [notification]),
        ])
        return RawElement(elementType: XCUIElementTypeCode.application, label: " ", frame: screen,
                          children: [pixelWindow, RawElement(elementType: XCUIElementTypeCode.window, frame: pixels), pointWindow])
    }

    @Test("a banner in a window reported in points keeps its frame (the real capture)")
    func bannerInPoints() throws {
        let frames = BannerCover.frames(inSpringBoard: springBoard(bannerInPixelWindow: false))
        let frame = try #require(frames.first)
        #expect(frames.count == 1)
        #expect(frame == Rect(x: 8, y: 58.7, width: 424, height: 66))
    }

    @Test("a frame in a pixel window ({1320, 2868} on a 440 x 956 pt screen) is divided by its factor")
    func bannerInPixels() throws {
        let frame = try #require(BannerCover.frames(inSpringBoard: springBoard(bannerInPixelWindow: true)).first)
        #expect(abs(frame.x - 8) < 0.01)
        #expect(abs(frame.y - 58.7) < 0.01)
        #expect(abs(frame.width - 424) < 0.01)
        #expect(abs(frame.height - 66) < 0.01)
    }

    @Test("the conversion is decided by the window and the screen, never by a constant")
    func conversion() {
        let screen = Rect(x: 0, y: 0, width: 440, height: 956)
        let f = Rect(x: 24, y: 176, width: 1272, height: 198)
        // Factor 3 (the real pixel windows).
        let three = BannerCover.points(f, window: Rect(x: 0, y: 0, width: 1320, height: 2868), screen: screen)
        #expect(abs(three.width - 424) < 0.01 && abs(three.y - 58.67) < 0.01)
        // Factor 2 (another device): divided by 2, not by 3.
        let two = BannerCover.points(f, window: Rect(x: 0, y: 0, width: 880, height: 1912), screen: screen)
        #expect(abs(two.width - 636) < 0.01)
        // A window in points, or one whose sides do not scale alike: unchanged.
        #expect(BannerCover.points(f, window: screen, screen: screen) == f)
        #expect(BannerCover.points(f, window: Rect(x: 0, y: 0, width: 1320, height: 956), screen: screen) == f)
        // No screen frame: unchanged.
        #expect(BannerCover.points(f, window: screen, screen: Rect(x: 0, y: 0, width: 0, height: 0)) == f)
    }

    // MARK: - The synthetic node

    @Test("the covering node: the banner's frame, a control role, no text, disabled, not hittable")
    func node() {
        let frame = Rect(x: 8, y: 58.7, width: 424, height: 66)
        let n = BannerCover.node(frame: frame)
        #expect(n.role == .button)
        #expect(n.label == nil && n.value == nil && n.identifier == nil && n.placeholder == nil)
        #expect(n.frame == frame)
        #expect(!n.enabled && !n.hittable && n.visible)
        #expect(!n.focused && !n.selected && !n.secure && n.children.isEmpty)
    }

    @Test("covering appends one node per banner after the app's own nodes")
    func covering() {
        let app = UiNode(role: .application, label: "Settings", value: nil, identifier: nil, placeholder: nil,
                         frame: Rect(x: 0, y: 0, width: 440, height: 956), enabled: true, visible: true, hittable: true,
                         focused: false, selected: false, secure: false, children: [
                             UiNode(role: .window, label: nil, value: nil, identifier: nil, placeholder: nil,
                                    frame: Rect(x: 0, y: 0, width: 440, height: 956), enabled: true, visible: true,
                                    hittable: true, focused: false, selected: false, secure: false, children: []),
                         ])
        let banner = Rect(x: 8, y: 58.7, width: 424, height: 66)
        let out = BannerCover.covering(app, banners: [banner])
        #expect(out.children.count == 2)
        #expect(out.children.last == BannerCover.node(frame: banner))
        #expect(BannerCover.covering(app, banners: []) == app)
    }

    @Test("a touch in a banner the look did not hold is refused; one the look held is not")
    func touchTime() {
        let banner = Rect(x: 8, y: 58.7, width: 424, height: 66)
        // The Back button of the Playground's probe screens is at {20, 62, 44, 44}: its centre.
        #expect(BannerCover.refuses(x: 42, y: 84, banners: [banner], planned: []))
        #expect(!BannerCover.refuses(x: 42, y: 84, banners: [banner], planned: [banner]))
        // Planned with the banner still sliding in (a few points off): not refused.
        #expect(!BannerCover.refuses(x: 42, y: 84, banners: [banner],
                                     planned: [Rect(x: 8, y: 56, width: 424, height: 66)]))
        // Outside the banner, or no banner now: not refused.
        #expect(!BannerCover.refuses(x: 220, y: 400, banners: [banner], planned: []))
        #expect(!BannerCover.refuses(x: 42, y: 84, banners: [], planned: []))
    }

    @Test("the touch point of each command")
    func touchPoints() throws {
        let tap = try #require(BannerCover.touchPoint(.tap(x: 1, y: 2)))
        #expect(tap.x == 1 && tap.y == 2)
        let press = try #require(BannerCover.touchPoint(.longPress(x: 3, y: 4, durationMs: 500)))
        #expect(press.x == 3 && press.y == 4)
        let swipe = try #require(BannerCover.touchPoint(.swipe(from: Point(x: 5, y: 6), to: Point(x: 7, y: 8), durationMs: 300)))
        #expect(swipe.x == 5 && swipe.y == 6)
        #expect(BannerCover.touchPoint(.typeText(text: "x")) == nil)
        #expect(BannerCover.touchPoint(.pressKey(key: .home)) == nil)
    }

    @Test("the banner refusal is a stale refusal")
    func staleBanner() {
        let r = RunnerResult.staleBanner(expected: settings)
        #expect(!r.ok && r.failure == .stale)
        #expect(r.error?.contains("banner") == true)
    }
}

/// `FrontAppGuard` with a top-layer reading: the runner's `frontBundleId()` is the top layer.
@Suite("Stale-screen guard against the top layer")
struct TopLayerGuardTests {
    @Test("a tap planned on the app while a system alert is on top: refused, nothing sent")
    func alertOverApp() async throws {
        let top = TopLayer.decide(systemAlertUp: true, spotlightInFront: false, foreground: settingsBundle, springBoard: .homeScreen).bundleId
        let phone = FrontScriptedPhone([top])
        let out = try await FrontAppGuard().perform(.tap(x: 10, y: 20), expect: CommandExpect(bundleId: settingsBundle), on: phone)
        #expect(out.result.failure == .stale)
        #expect(phone.inner.commands.isEmpty)
        // Next to the gesture, the same reading refuses too.
        #expect(FrontAppGuard.refusalNextToGesture(CommandExpect(bundleId: settingsBundle), front: top)?.failure == .stale)
    }

    @Test("a tap planned on the alert (SpringBoard) goes through")
    func tapOnAlert() async throws {
        let top = TopLayer.decide(systemAlertUp: true, spotlightInFront: false, foreground: settingsBundle, springBoard: .homeScreen).bundleId
        let phone = FrontScriptedPhone([top])
        let out = try await FrontAppGuard().perform(.tap(x: 10, y: 20), expect: CommandExpect(bundleId: TopLayer.springBoard), on: phone)
        #expect(out.result.ok)
        #expect(phone.inner.commands.count == 1)
        #expect(FrontAppGuard.refusalNextToGesture(CommandExpect(bundleId: TopLayer.springBoard), front: top) == nil)
    }

    @Test("Spotlight on top: a tap planned on the Home Screen is refused, one planned on Spotlight is not")
    func spotlight() {
        let top = TopLayer.decide(systemAlertUp: false, spotlightInFront: true, foreground: TopLayer.springBoard, springBoard: .homeScreen).bundleId
        #expect(FrontAppGuard.refusalNextToGesture(CommandExpect(bundleId: TopLayer.springBoard), front: top)?.failure == .stale)
        #expect(FrontAppGuard.refusalNextToGesture(CommandExpect(bundleId: "com.apple.spotlight"), front: top) == nil)
    }
}
