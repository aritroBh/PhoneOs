import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// Second break-it review of the confirm-step fixes (5e7cebe), phone side. Each test asserts
// what should hold; it fails today for a finding.

@Suite("Remote control: second break-it", .timeLimit(.minutes(2)))
@MainActor
struct RemoteBreakIt2Tests {
    /// Finding: the owner removes a device while the store write fails. The device is gone
    /// from the screen at once, the store then works again and the service keeps ticking,
    /// but nothing writes the removal again: the saved state still holds the device, so a
    /// relaunch brings it back, confirmed, with its key and permissions.
    @Test("a device removed while the store write failed is not back after a relaunch")
    func removedDeviceStaysRemovedAfterAFailedWrite() async throws {
        let r = RemoteHarness("bi2-revoke")
        defer { r.cleanUp() }
        let device = try await r.pairDevice("Stolen Mac")
        let id = try #require(device.deviceId)

        r.secrets.failWrites.withLock { $0 = true }
        try r.service.revoke(deviceId: id)
        #expect(!r.service.devices.contains { $0.id == id }, "the owner sees the device removed")
        #expect(r.service.lastError != nil)

        // The store works again; the service runs as usual for a while.
        r.secrets.failWrites.withLock { $0 = false }
        for _ in 0..<5 {
            r.service.tick()
            try await Task.sleep(for: .milliseconds(20))
        }

        // A relaunch loads what was saved.
        let json = try #require(try r.secrets.read(RemoteControlService.secretKey))
        let relaunched = try JarvisRemoteHub(storeJson: json, phoneName: "Test phone")
        let back = try relaunched.devices().filter { $0.id == id }
        #expect(back.isEmpty, "the removed device is back after a relaunch: \(back)")
    }

    /// Finding: after a failed write the service closes every connection itself and
    /// forgets them (`closeConnection` removes them from `open`, so the transport's
    /// `.closed` is then ignored), but never tells the hub. The hub keeps them as open,
    /// authenticated connections (never evicted) until its 2-minute idle timeout: with a
    /// few devices connected, a failed write leaves the table full and the phone refuses
    /// every device, the owner's included, for up to 2 minutes.
    @Test("after a failed store write closed the connections, the hub knows they are closed")
    func hubIsToldOfTheConnectionsAFailedWriteClosed() async throws {
        let r = RemoteHarness("bi2-ghost")
        defer { r.cleanUp() }
        let device = try await r.pairDevice("Test Mac")
        let id = try #require(device.deviceId)
        let probe = try #require(r.probe)
        try await eventually("one connection in the hub") { (try? probe.connectionCount()) == 1 }

        r.secrets.failWrites.withLock { $0 = true }
        var permissions = try newDeviceRemotePermissions()
        permissions.approve = true
        try r.service.setPermissions(deviceId: id, permissions)
        try await eventually("the connection closed") { device.isClosed }
        r.secrets.failWrites.withLock { $0 = false }
        r.service.tick()
        try await Task.sleep(for: .milliseconds(200))

        let count = try probe.connectionCount()
        let conn = try #require(lastConnId(probe))
        let authenticated = try probe.isAuthenticated(connId: conn)
        #expect(count == 0, "the hub still holds \(count) connection(s) the service closed (connection \(conn) authenticated: \(authenticated))")
    }
}

/// The connection id the hub last saw opened (from its returned effects).
private func lastConnId(_ probe: HubProbe) -> UInt64? {
    probe.returned.withLock { lists in
        lists.flatMap { $0 }.compactMap { effect -> UInt64? in
            if case .send(let id, _) = effect { return id }
            return nil
        }.last
    }
}
