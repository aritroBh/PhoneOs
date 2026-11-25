import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// No event while a notification banner is up (`BannerGate`; Mac run 138: XCUITest held a tap
/// well below the banner for 5.6 s, waiting up to 15 s for the banner to go, then sent it).
@Suite("Banner gate: no synthesized event while a banner is up (BannerGate)")
struct BannerGateTests {
    private let p = Point(x: 10, y: 20)

    @Test("which commands synthesize an element event: touches, typing and keys typed; not Home, launching, a URL or a wait")
    func synthesizes() {
        let yes: [RunnerCommand] = [
            .tap(x: 220, y: 406.3), .longPress(x: 1, y: 2, durationMs: 800),
            .swipe(from: p, to: p, durationMs: 300), .typeText(text: "a"),
            .pressKey(key: .return), .pressKey(key: .delete), .pressKey(key: .tab),
            .pressKey(key: .space), .pressKey(key: .escape),
        ]
        let no: [RunnerCommand] = [
            .pressKey(key: .home), .launchApp(bundleId: "com.apple.Preferences"),
            .openUrl(url: "https://example.com"), .wait(ms: 500),
        ]
        for c in yes { #expect(BannerGate.synthesizesEvent(c), "\(c)") }
        for c in no { #expect(!BannerGate.synthesizesEvent(c), "\(c)") }
    }

    @Test("a banner up refuses every synthesizing command wherever its point (run 138's tap well below the banner included); none up, all proceed")
    func decision() {
        // Run 138: the banner at {8, 58.7, 424x66}; the tap on Settings' General row at
        // (220, 406.3), far below it, was held and then sent.
        let below = RunnerCommand.tap(x: 220, y: 406.3)
        let banner = Rect(x: 8, y: 58.7, width: 424, height: 66)
        #expect(!BannerCover.refuses(x: 220, y: 406.3, banners: [banner], planned: []), "the point is not under the banner")
        #expect(!BannerGate.mayProceed(below, bannersUp: true), "but the banner is up: no event")
        #expect(BannerGate.mayProceed(below, bannersUp: false))
        #expect(BannerGate.mayProceed(.pressKey(key: .home), bannersUp: true), "Home is the way out")
        #expect(BannerGate.mayProceed(.wait(ms: 1), bannersUp: true))
        #expect(!BannerGate.mayProceed(.typeText(text: "x"), bannersUp: true))
    }

    @Test("the refusal: stale, nothing carried out, the banner-up reason, how long it waited")
    func refusal() {
        let r = BannerGate.refusal(waited: .milliseconds(5_600), bannerLeft: true)
        #expect(!r.ok)
        #expect(r.failure == .stale)
        #expect(r.error?.contains("a notification banner was up; the gesture was not carried out") == true)
        #expect(r.error?.contains("5.6 s") == true)
        #expect(BannerGate.refusal(waited: .zero, bannerLeft: false).error?.contains("still up") == true)
    }

    @Test("the wait fits the server's command deadline, with 6 s left for the rest of the command")
    func budget() {
        #expect(BannerGate.maxWait + .seconds(6) <= JarvisRunnerTiming.serverCommand)
        #expect(BannerGate.pollInterval < BannerGate.maxWait)
    }

    @Test("a held gesture is noted when the call took longer than the event and its own duration explain (run 138: 5.6 s for a tap)")
    func held() {
        let tap = RunnerCommand.tap(x: 220, y: 406.3)
        #expect(BannerGate.heldNote(tap, elapsed: .milliseconds(5_640))?.contains("held 5.6 s") == true)
        #expect(BannerGate.heldNote(tap, elapsed: .milliseconds(900)) == nil, "an ordinary tap")
        #expect(BannerGate.heldNote(tap, elapsed: .seconds(4)) == nil, "at the allowance")
        // A long press's and a typing's own durations are allowed for.
        #expect(BannerGate.heldNote(.longPress(x: 0, y: 0, durationMs: 3_000), elapsed: .seconds(6)) == nil)
        #expect(BannerGate.heldNote(.longPress(x: 0, y: 0, durationMs: 3_000), elapsed: .seconds(8)) != nil)
        #expect(BannerGate.heldNote(.typeText(text: String(repeating: "a", count: 20)), elapsed: .seconds(7)) == nil)
        #expect(BannerGate.heldNote(.swipe(from: p, to: p, durationMs: 300), elapsed: .seconds(5)) == nil)
        #expect(BannerGate.heldNote(.swipe(from: p, to: p, durationMs: 300), elapsed: .seconds(6)) != nil)
        // Commands that synthesize no element event are never noted.
        #expect(BannerGate.heldNote(.pressKey(key: .home), elapsed: .seconds(30)) == nil)
        #expect(BannerGate.heldNote(.launchApp(bundleId: "x"), elapsed: .seconds(30)) == nil)
    }
}
