import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// The fixes for the second break-it review (RemoteBreakIt2Tests), beyond what its tests pin.

@Suite("Remote control: second review fixes", .timeLimit(.minutes(2)))
@MainActor
struct RemoteBreakIt2FixTests {
    @Test("a failed write pauses remote control until a write succeeds, and says so")
    func unsavedChangesPauseRemoteControl() async throws {
        let r = RemoteHarness("bi2fix-unsaved")
        defer { r.cleanUp() }
        let device = try await r.pairDevice("Test Mac")
        let id = try #require(device.deviceId)
        let credentials = try #require(device.credentials)
        let probe = try #require(r.probe)
        #expect(!r.service.changesUnsaved)

        r.secrets.failWrites.withLock { $0 = true }
        var permissions = try newDeviceRemotePermissions()
        permissions.seeProgress = false
        try r.service.setPermissions(deviceId: id, permissions)
        #expect(r.service.changesUnsaved)
        #expect(r.service.lastError == RemoteControlService.unsavedMessage)
        try await eventually("the connection closed") { device.isClosed }
        try await eventually("the hub holds no connection") { (try? probe.connectionCount()) == 0 }

        // Still failing: a tick tries again (and fails again); a device cannot get in.
        r.service.tick()
        #expect(r.service.changesUnsaved)
        await #expect(throws: (any Error).self) {
            let late = try await InProcessRemote.connect(credentials: credentials, over: r.transport, timeout: .seconds(2))
            late.close()
        }
        try await eventually("the refused connection is gone from the hub") { (try? probe.connectionCount()) == 0 }

        // The Keychain works again: the next tick writes, remote control resumes.
        r.secrets.failWrites.withLock { $0 = false }
        r.service.tick()
        try await eventually("the change saved") { !r.service.changesUnsaved }
        #expect(r.service.lastError == nil)
        let json = try #require(try r.secrets.read(RemoteControlService.secretKey))
        let saved = try JarvisRemoteHub(storeJson: json, phoneName: "Test phone").devices().first { $0.id == id }
        #expect(saved?.permissions.seeProgress == false)
        let again = try await InProcessRemote.connect(credentials: credentials, over: r.transport)
        try again.send(.ping)
        try await again.waitForMessage { $0 == .pong }
        again.close()
    }

    @Test("one peer address holds at most a few connections; others are not affected")
    func peerConnectionLimiter() {
        var limiter = PeerConnectionLimiter(maxPerPeer: 4)
        let a: [UInt8] = [192, 168, 1, 20], b: [UInt8] = [192, 168, 1, 21]
        for id in UInt64(1)...4 {
            let admitted = limiter.admit(id, peer: a)
            #expect(admitted)
        }
        let fifth = limiter.admit(5, peer: a)
        #expect(!fifth, "a fifth connection from one address was admitted")
        #expect(limiter.count(peer: a) == 4)
        let other = limiter.admit(6, peer: b)
        #expect(other)
        let twice = limiter.admit(6, peer: b)
        #expect(!twice, "an id was recorded twice")
        limiter.remove(2)
        limiter.remove(2)
        limiter.remove(99)
        #expect(limiter.count(peer: a) == 3)
        let again = limiter.admit(7, peer: a)
        #expect(again)
        let over = limiter.admit(8, peer: a)
        #expect(!over)
        limiter.removeAll()
        #expect(limiter.count(peer: a) == 0 && limiter.count(peer: b) == 0)
        #expect(PeerConnectionLimiter().maxPerPeer == RemoteTransportLimits.maxConnectionsPerPeer)
        #expect(RemoteTransportLimits.maxConnections >= 8 + 32, "the transport has fewer places than the hub")
    }
}
