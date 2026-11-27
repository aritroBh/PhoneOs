import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

/// Review pass 35 (F4): the core is told where each answer was given. Jarvis's own sheet
/// (`approve`/`decline`) answers on the phone; remote control's desk
/// (`resolveConfirmation`) and any gate that does not say answer remotely, and a remote
/// answer is never kept.
@Suite("Where an answer was given", .timeLimit(.minutes(1)))
struct RemoteAnswerTests {
    private let request = ConfirmRequest(category: .firstUse, summary: "Let Jarvis act in Shop?", app: "com.example.shop", phoneOnly: false)

    @Test("the sheet answers on the phone, the desk for a paired device")
    func brokerSources() async throws {
        let broker = HumanPromptBroker()
        let onPhone = Task { [request] in try await broker.decide(request) }
        try await waitUntil { broker.pending != nil }
        #expect(broker.approve(try #require(broker.pending).id))
        #expect(try await onPhone.value == OwnerDecision(approved: true, byPhone: true))

        let remote = Task { [request] in try await broker.decide(request) }
        try await waitUntil { broker.pending != nil }
        #expect(broker.resolveConfirmation(try #require(broker.pending).id, approved: true))
        #expect(try await remote.value == OwnerDecision(approved: true, byPhone: false))

        let declined = Task { [request] in try await broker.decide(request) }
        try await waitUntil { broker.pending != nil }
        #expect(broker.decline(try #require(broker.pending).id))
        #expect(try await declined.value == OwnerDecision(approved: false, byPhone: true))
    }

    @Test("a gate that only answers confirm() is not the phone")
    func unknownIsRemote() async throws {
        struct Plain: HumanGate {
            func confirm(_ request: ConfirmRequest) async throws -> Bool { true }
            func answer(_ question: String) async throws -> String { "" }
        }
        #expect(try await Plain().decide(request) == OwnerDecision(approved: true, byPhone: false))
        #expect(Event.userDecision(approved: true) == Event.userDecision(approved: true, byPhone: false))
    }

    @Test("the driver hands the core where the answer was given")
    func driverPassesTheSource() async throws {
        for byPhone in [true, false] {
            var h = Harness()
            h.gate = FakeGate(approve: false, byPhone: byPhone)
            h.model = FakeModel { request, call in
                if request.user.contains("the owner declined") {
                    return #"{"thought":"stop","action":"fail","reason":"owner declined"}"#
                }
                return try await wifiOffScript(request, call)
            }
            let (_, updates) = try await h.run()
            let first = try #require(h.gate.confirms.first)
            #expect(updates.contains(.effectCompleted(.confirm(request: first), event: .userDecision(approved: false, byPhone: byPhone))))
        }
    }
}
