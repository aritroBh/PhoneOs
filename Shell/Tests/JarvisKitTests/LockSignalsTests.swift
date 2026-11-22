import Foundation
import Testing
@testable import JarvisKit

@Suite("LockSignals decision rule")
struct LockSignalsTests {
    private let app = LockSignals.Front.app("dev.jarvis.app")

    @Test("commands are refused when either reading says locked")
    func commands() {
        #expect(LockSignals(notify: true, lockScreen: nil, front: app).forCommands == .locked)
        #expect(LockSignals(notify: false, lockScreen: true, front: .springBoard).forCommands == .locked)
        #expect(LockSignals(notify: true, lockScreen: false, front: .springBoard).forCommands == .locked)
        #expect(LockSignals(notify: false, lockScreen: false, front: .springBoard).forCommands == .unlocked)
        #expect(LockSignals(notify: nil, lockScreen: nil, front: app).forCommands == .unlocked,
                "an app in front counts as no lock screen")
        #expect(LockSignals(notify: nil, lockScreen: nil, front: .springBoard).forCommands == .unknown)
    }

    @Test("a stale notify state never holds a resume: app in front or home screen is unlocked")
    func resume() {
        #expect(LockSignals(notify: true, lockScreen: nil, front: app).forResume == .unlocked)
        #expect(LockSignals(notify: true, lockScreen: false, front: .springBoard).forResume == .unlocked)
        #expect(LockSignals(notify: false, lockScreen: true, front: .springBoard).forResume == .locked)
        #expect(LockSignals(notify: true, lockScreen: nil, front: .springBoard).forResume == .locked,
                "without a lock-screen reading the notify state decides")
        #expect(LockSignals(notify: false, lockScreen: nil, front: .springBoard).forResume == .unlocked)
        #expect(LockSignals(notify: nil, lockScreen: nil, front: .springBoard).forResume == .unknown)
    }

    @Test("the description carries both raw readings and both decisions")
    func description() {
        let text = LockSignals(notify: true, lockScreen: false, front: .springBoard).description
        #expect(text == "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=false, front=SpringBoard -> commands locked, resume unlocked")
        #expect(LockSignals(notify: nil, lockScreen: nil, front: app).description.contains("notify(com.apple.springboard.lockstate)=unavailable, lock screen=not checked, front=dev.jarvis.app"))
    }
}
