import Foundation
import JarvisCore

/// The owner, as the driver reaches them: confirmation sheets, spoken questions,
/// notifications while Jarvis is in the background.
///
/// Both methods may take as long as the owner takes; `DriverConfiguration.humanTimeout`
/// bounds the wait. Throwing (no UI available, notification dismissed) is a decline for
/// `confirm` and stops the run for `answer`.
public protocol HumanGate: Sendable {
    /// Asks the owner to approve a risky action. `true` approves.
    func confirm(_ request: ConfirmRequest) async throws -> Bool

    /// Asks the owner a question and returns their answer.
    func answer(_ question: String) async throws -> String
}
