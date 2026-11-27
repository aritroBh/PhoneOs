import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// One HTTP exchange. The seam between the runner/model clients and the network, so tests
/// and alternative stacks can replace `URLSession`.
public protocol HTTPTransport: Sendable {
    /// Sends `request` and returns the complete body and the HTTP response. Throws for
    /// transport failures (no connection, timeout, cancelled); any HTTP status is returned.
    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse)
}

/// Errors of `URLSessionTransport`.
public enum HTTPTransportError: Error, Equatable, Sendable {
    /// The response was not an HTTP response.
    case notHTTP
    /// The request did not complete; the message is the underlying error's description.
    case failed(String)
}

/// `HTTPTransport` over a `URLSession` (FoundationNetworking on Linux).
public struct URLSessionTransport: HTTPTransport {
    private let session: URLSession

    /// Uses `session`, or by default an ephemeral session (no cookies, no cache) whose
    /// request timeout is `timeout` seconds.
    public init(session: URLSession? = nil, timeout: TimeInterval = 30) {
        if let session {
            self.session = session
        } else {
            let configuration = URLSessionConfiguration.ephemeral
            configuration.timeoutIntervalForRequest = timeout
            configuration.timeoutIntervalForResource = timeout
            configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
            configuration.urlCache = nil
            configuration.httpCookieStorage = nil
            self.session = URLSession(configuration: configuration)
        }
    }

    public func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await session.data(for: request)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            if Task.isCancelled { throw CancellationError() }
            throw HTTPTransportError.failed(String(describing: error))
        }
        guard let http = response as? HTTPURLResponse else { throw HTTPTransportError.notHTTP }
        return (data, http)
    }
}

extension Data {
    /// The first `limit` bytes as (lossy) UTF-8, for error messages.
    func snippet(_ limit: Int = 300) -> String {
        String(decoding: prefix(limit), as: UTF8.self)
    }
}
