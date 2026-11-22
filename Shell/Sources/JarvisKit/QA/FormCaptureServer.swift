import Foundation
import Synchronization
#if canImport(Glibc)
import Glibc
#elseif canImport(Darwin)
import Darwin
#endif

/// A minimal HTTP/1.1 server on 127.0.0.1 for QA: serves one HTML form and records what is
/// submitted to it. The iOS Simulator shares the host's loopback, so a server started inside
/// a UI-test process is reachable from Safari in the simulator at `baseURL`.
///
/// - `GET /` (any path but `/submit`): the form page.
/// - `POST /submit` (`application/x-www-form-urlencoded`): records the fields, answers with
///   a page `Thanks, <name>`.
/// - Every request, including unparseable ones (a TLS hello from an HTTPS upgrade attempt),
///   is kept in `log` for diagnostics.
///
/// One thread per connection; each response closes the connection.
public final class FormCaptureServer: Sendable {
    /// One received request, as logged.
    public struct LoggedRequest: Sendable, Equatable {
        public var method: String
        public var path: String
        public var headers: [String: String]
        public var body: String
    }

    public enum ServerError: Error, Equatable { case socket(String) }

    /// The port bound on 127.0.0.1.
    public let port: UInt16
    /// The page `GET /` serves.
    public let formHTML: String
    private let listenFD: Int32
    private let submitted = Mutex<[[String: String]]>([])
    private let logged = Mutex<[String]>([])
    private let stopped = Mutex(false)

    /// `http://127.0.0.1:<port>`.
    public var baseURL: URL {
        URL(string: "http://127.0.0.1:\(port)") ?? URL(fileURLWithPath: "/")
    }

    /// Submitted forms (field name → value), in arrival order.
    public var submissions: [[String: String]] { submitted.withLock { $0 } }
    /// One line per received request (`GET / (ua=…)`, `POST /submit name=…`, `unparseable …`).
    public var log: [String] { logged.withLock { $0 } }

    /// Starts listening on an ephemeral port (or `port` when non-zero).
    public init(formHTML: String = FormCaptureServer.defaultForm, port: UInt16 = 0) throws {
        self.formHTML = formHTML
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
        addr.sin_port = in_port_t(port).bigEndian
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        let bound = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0, listen(fd, 64) == 0 else {
            close(fd)
            throw ServerError.socket("bind/listen on 127.0.0.1:\(port) failed (errno \(errno))")
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

    /// Stops accepting connections (the accept loop closes the socket).
    public func stop() {
        let wasStopped = stopped.withLock { s -> Bool in
            defer { s = true }
            return s
        }
        guard !wasStopped else { return }
        shutdown(listenFD, Int32(SHUT_RDWR))
    }

    deinit { stop() }

    /// Waits until at least one form was submitted or `timeout` passes.
    public func waitForSubmission(timeout: Duration) async -> [String: String]? {
        let deadline = ContinuousClock.now + timeout
        while ContinuousClock.now < deadline {
            if let first = submissions.first { return first }
            try? await Task.sleep(for: .milliseconds(100))
        }
        return submissions.first
    }

    /// The form: a full name, an email address and a submit button, sized for touch, with
    /// autocorrection off so typed text arrives unchanged.
    public static let defaultForm = """
    <!doctype html>
    <html><head><meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Jarvis QA form</title>
    <style>
    body { font: 20px -apple-system, sans-serif; margin: 24px; }
    label { display: block; margin-top: 16px; }
    input { font-size: 20px; width: 100%; height: 44px; box-sizing: border-box; }
    button { font-size: 20px; margin-top: 24px; width: 100%; height: 50px; }
    </style></head>
    <body>
    <h1>Contact form</h1>
    <form method="post" action="/submit">
    <label for="name">Full name</label>
    <input id="name" name="name" type="text" aria-label="Full name" placeholder="Full name" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false">
    <label for="email">Email address</label>
    <input id="email" name="email" type="text" aria-label="Email address" placeholder="Email address" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false">
    <button type="submit">Send form</button>
    </form>
    </body></html>
    """

    /// Decodes `application/x-www-form-urlencoded` (`+` is a space, `%XX` escapes).
    public static func parseForm(_ body: String) -> [String: String] {
        var out: [String: String] = [:]
        for pair in body.split(separator: "&", omittingEmptySubsequences: true) {
            let parts = pair.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
            let decode = { (s: Substring) -> String in
                let spaced = s.replacingOccurrences(of: "+", with: " ")
                return spaced.removingPercentEncoding ?? spaced
            }
            guard let key = parts.first else { continue }
            out[decode(key)] = parts.count > 1 ? decode(parts[1]) : ""
        }
        return out
    }

    static func escapeHTML(_ s: String) -> String {
        s.replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
            .replacingOccurrences(of: "\"", with: "&quot;")
    }

    // MARK: - Serving

    private func acceptLoop() {
        while true {
            if stopped.withLock({ $0 }) {
                close(listenFD)
                return
            }
            var pfd = pollfd(fd: listenFD, events: Int16(POLLIN), revents: 0)
            guard poll(&pfd, 1, 50) > 0 else { continue }
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
        let (request, raw) = readRequest(fd)
        guard let request else {
            let preview = raw.prefix(16).map { String(format: "%02x", $0) }.joined(separator: " ")
            logged.withLock { $0.append("unparseable request (\(raw.count) bytes: \(preview)) - an HTTPS attempt starts with 16 03") }
            writeAll(fd, Data("HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".utf8))
            return
        }
        let page: String
        let status: String
        if request.method == "POST" && request.path.hasPrefix("/submit") {
            let fields = Self.parseForm(request.body)
            submitted.withLock { $0.append(fields) }
            logged.withLock { $0.append("POST \(request.path) \(fields.keys.sorted().map { "\($0)=\(fields[$0] ?? "")" }.joined(separator: " "))") }
            let name = Self.escapeHTML(fields["name"] ?? "")
            page = """
            <!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Received</title></head>
            <body style="font: 20px -apple-system, sans-serif; margin: 24px;"><h1>Thanks, \(name)</h1><p>Form received.</p></body></html>
            """
            status = "200 OK"
        } else if request.method == "GET" {
            logged.withLock { $0.append("GET \(request.path) (user-agent: \(request.headers["user-agent"] ?? "-"))") }
            page = request.path.hasPrefix("/favicon") ? "" : formHTML
            status = request.path.hasPrefix("/favicon") ? "404 Not Found" : "200 OK"
        } else {
            logged.withLock { $0.append("\(request.method) \(request.path) (not handled)") }
            page = ""
            status = "405 Method Not Allowed"
        }
        let body = Data(page.utf8)
        var head = "HTTP/1.1 \(status)\r\n"
        head += "Content-Type: text/html; charset=utf-8\r\n"
        head += "Cache-Control: no-store\r\n"
        head += "Content-Length: \(body.count)\r\n"
        head += "Connection: close\r\n\r\n"
        writeAll(fd, Data(head.utf8) + body)
    }

    private func readRequest(_ fd: Int32) -> (LoggedRequest?, Data) {
        var buffer = Data()
        let separator = Data("\r\n\r\n".utf8)
        var chunk = [UInt8](repeating: 0, count: 16 * 1024)
        var headerEnd: Range<Data.Index>?
        while headerEnd == nil {
            let n = recv(fd, &chunk, chunk.count, 0)
            guard n > 0 else { return (nil, buffer) }
            buffer.append(contentsOf: chunk[0..<n])
            headerEnd = buffer.range(of: separator)
            if buffer.count > 256 * 1024 { return (nil, buffer) }
        }
        guard let headerEnd else { return (nil, buffer) }
        let headText = String(decoding: buffer[buffer.startIndex..<headerEnd.lowerBound], as: UTF8.self)
        var lines = headText.components(separatedBy: "\r\n")
        guard !lines.isEmpty else { return (nil, buffer) }
        let requestLine = lines.removeFirst().split(separator: " ")
        guard requestLine.count >= 2 else { return (nil, buffer) }
        var headers: [String: String] = [:]
        for line in lines {
            guard let colon = line.firstIndex(of: ":") else { continue }
            let name = line[..<colon].trimmingCharacters(in: .whitespaces).lowercased()
            headers[name] = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
        }
        let length = min(Int(headers["content-length"] ?? "0") ?? 0, 1_000_000)
        var body = Data(buffer[headerEnd.upperBound...])
        while body.count < length {
            let n = recv(fd, &chunk, min(chunk.count, length - body.count), 0)
            guard n > 0 else { return (nil, buffer) }
            body.append(contentsOf: chunk[0..<n])
        }
        let request = LoggedRequest(
            method: String(requestLine[0]), path: String(requestLine[1]), headers: headers,
            body: String(decoding: body, as: UTF8.self)
        )
        return (request, buffer)
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
