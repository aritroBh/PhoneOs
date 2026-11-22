import Foundation
import Synchronization
@testable import JarvisKit
#if canImport(Glibc)
import Glibc
#elseif canImport(Darwin)
import Darwin
#endif

/// A tiny HTTP/1.1 server on 127.0.0.1 (ephemeral port) for client tests. One thread per
/// connection; it reads the complete request (headers, then exactly Content-Length body
/// bytes) before calling the handler, answers with `Connection: close` and closes.
final class StubHTTPServer: Sendable {
    struct Request: Sendable {
        var method: String
        var path: String
        /// Header names lowercased.
        var headers: [String: String]
        var body: Data

        /// The body as JSON.
        func json() throws -> JSONValue { try JSONValue(parsing: body) }
    }

    struct Response: Sendable {
        var status: Int
        var body: Data
        var contentType: String = "application/json"
        /// Wait this long before answering (for client timeout tests).
        var delay: TimeInterval = 0

        static func json(_ value: JSONValue, status: Int = 200, delay: TimeInterval = 0) -> Response {
            Response(status: status, body: (try? value.encoded()) ?? Data(), delay: delay)
        }

        static func text(_ text: String, status: Int = 200) -> Response {
            Response(status: status, body: Data(text.utf8), contentType: "text/plain")
        }
    }

    typealias Handler = @Sendable (Request) -> Response

    enum ServerError: Error { case socket(String) }

    let port: UInt16
    private let listenFD: Int32
    private let handler: Handler
    private let recorded = Mutex<[Request]>([])
    private let stopped = Mutex(false)

    var baseURL: URL {
        URL(string: "http://127.0.0.1:\(port)") ?? URL(fileURLWithPath: "/")
    }

    /// Every complete request received so far, in arrival order.
    var requests: [Request] { recorded.withLock { $0 } }

    init(handler: @escaping Handler) throws {
        self.handler = handler
        #if canImport(Glibc)
        let fd = socket(AF_INET, Int32(SOCK_STREAM.rawValue), 0)
        #else
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        #endif
        guard fd >= 0 else { throw ServerError.socket("socket() failed") }
        var yes: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size))
        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = in_port_t(0).bigEndian
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        let bound = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0, listen(fd, 64) == 0 else {
            close(fd)
            throw ServerError.socket("bind/listen failed")
        }
        var actual = sockaddr_in()
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        _ = withUnsafeMutablePointer(to: &actual) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { getsockname(fd, $0, &length) }
        }
        self.port = UInt16(bigEndian: actual.sin_port)
        self.listenFD = fd
        Thread.detachNewThread { [self] in acceptLoop() }
    }

    /// Stops accepting connections. Only the accept loop closes the listening socket (within
    /// one poll interval): closing it here could hand its descriptor number to another
    /// server while this loop still calls accept() on it.
    func stop() {
        let wasStopped = stopped.withLock { s -> Bool in
            defer { s = true }
            return s
        }
        guard !wasStopped else { return }
        shutdown(listenFD, Int32(SHUT_RDWR))
    }

    deinit { stop() }

    private func acceptLoop() {
        while true {
            if stopped.withLock({ $0 }) {
                close(listenFD)
                return
            }
            var pfd = pollfd(fd: listenFD, events: Int16(POLLIN), revents: 0)
            guard poll(&pfd, 1, 20) > 0 else { continue }
            if stopped.withLock({ $0 }) { continue }
            let client = accept(listenFD, nil, nil)
            if client < 0 { continue }
            #if canImport(Darwin)
            var one: Int32 = 1
            setsockopt(client, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout<Int32>.size))
            #endif
            Thread.detachNewThread { [self] in serve(client) }
        }
    }

    private func serve(_ fd: Int32) {
        defer {
            shutdown(fd, Int32(SHUT_RDWR))
            close(fd)
        }
        guard let request = readRequest(fd) else { return }
        recorded.withLock { $0.append(request) }
        let response = handler(request)
        if response.delay > 0 { Thread.sleep(forTimeInterval: response.delay) }
        var head = "HTTP/1.1 \(response.status) \(response.status < 400 ? "OK" : "Error")\r\n"
        head += "Content-Type: \(response.contentType)\r\n"
        head += "Content-Length: \(response.body.count)\r\n"
        head += "Connection: close\r\n\r\n"
        writeAll(fd, Data(head.utf8) + response.body)
    }

    /// Reads headers up to the blank line, then exactly Content-Length body bytes.
    private func readRequest(_ fd: Int32) -> Request? {
        var buffer = Data()
        let separator = Data("\r\n\r\n".utf8)
        var chunk = [UInt8](repeating: 0, count: 64 * 1024)
        var headerEnd: Range<Data.Index>?
        while headerEnd == nil {
            let n = recv(fd, &chunk, chunk.count, 0)
            guard n > 0 else { return nil }
            buffer.append(contentsOf: chunk[0..<n])
            headerEnd = buffer.range(of: separator)
        }
        guard let headerEnd else { return nil }
        let headText = String(decoding: buffer[buffer.startIndex..<headerEnd.lowerBound], as: UTF8.self)
        var lines = headText.components(separatedBy: "\r\n")
        guard !lines.isEmpty else { return nil }
        let requestLine = lines.removeFirst().split(separator: " ")
        guard requestLine.count >= 2 else { return nil }
        var headers: [String: String] = [:]
        for line in lines {
            guard let colon = line.firstIndex(of: ":") else { continue }
            let name = line[..<colon].trimmingCharacters(in: .whitespaces).lowercased()
            let value = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
            headers[name] = value
        }
        let length = Int(headers["content-length"] ?? "0") ?? 0
        var body = Data(buffer[headerEnd.upperBound...])
        while body.count < length {
            let n = recv(fd, &chunk, min(chunk.count, length - body.count), 0)
            guard n > 0 else { return nil }
            body.append(contentsOf: chunk[0..<n])
        }
        return Request(method: String(requestLine[0]), path: String(requestLine[1]), headers: headers, body: body)
    }

    private func writeAll(_ fd: Int32, _ data: Data) {
        #if canImport(Glibc)
        let flags = Int32(MSG_NOSIGNAL)
        #else
        let flags: Int32 = 0
        #endif
        data.withUnsafeBytes { raw in
            guard let base = raw.baseAddress else { return }
            var sent = 0
            while sent < raw.count {
                let n = send(fd, base + sent, raw.count - sent, flags)
                if n <= 0 { return }
                sent += n
            }
        }
    }
}
