import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Authenticates runner requests.
///
/// LoopCtrl wraps each request in an encrypted envelope (fields `v, tok, ts, nonce, ct,
/// tag`, HMAC-SHA256 based, after X25519 pairing [source: LoopCtrlKit/SecureEnvelope.swift]).
/// Its details are not verified and we will fork the runner, so no crypto is implemented
/// here: a sealer for the fork plugs in behind this protocol (reuse the fork's
/// `SecureEnvelope.swift` / `AuthManager.swift`, keys in the Keychain).
public protocol RequestSealer: Sendable {
    /// Returns `request` with the sealed form of `body` as its HTTP body (and any headers
    /// the scheme needs).
    func seal(body: Data, into request: URLRequest) async throws -> URLRequest

    /// Returns the plain JSON of a response body.
    func unseal(body: Data, response: HTTPURLResponse) async throws -> Data
}

/// No authentication: the JSON goes out and comes back as is. For a runner fork without
/// auth, a local test server, or LoopCtrl's unauthenticated development mode.
public struct PlaintextSealer: RequestSealer {
    /// The pass-through sealer.
    public init() {}

    public func seal(body: Data, into request: URLRequest) async throws -> URLRequest {
        var request = request
        request.httpBody = body
        return request
    }

    public func unseal(body: Data, response: HTTPURLResponse) async throws -> Data {
        body
    }
}
