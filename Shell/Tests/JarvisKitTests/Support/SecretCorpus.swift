/// Secrets as they appear on screen, with the part that must never be shown: samples from
/// the core's own redaction tests (jarvis-safety tests/redact.rs, review_fixes.rs). Used by
/// the Live view's privacy tests and the remote stream's.
enum SecretCorpus {
    static let otp = ("Your verification code is 482913.", "482913")
    static let card = ("Card: 5555 5555 5555 4444 exp 12/29", "5555 5555 5555 4444")
    static let iban = ("IBAN: GB82 WEST 1234 5698 7654 32", "GB82 WEST 1234 5698 7654 32")
    static let key = ("key sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N here", "sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N")

    static var all: [(shown: String, secret: String)] { [otp, card, iban, key] }
}
