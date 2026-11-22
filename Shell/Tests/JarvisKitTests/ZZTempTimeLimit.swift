import Testing
@Suite("ZZ temp", .timeLimit(.minutes(1)))
struct ZZTemp {
    @Test("zz sleeps too long") func zz() async throws { try await Task.sleep(for: .seconds(600)) }
}
