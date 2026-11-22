import Foundation
import JarvisCore
import Synchronization

/// `PhoneRunner` for the LoopCtrl XCUITest runner's JSON-RPC surface (docs/RUNNER.md 2.1).
///
/// Not verified on a device: every LoopCtrl name it uses is in `LoopCtrlAPI` with its
/// evidence tag. Where the runner's shape is unknown it decodes leniently, records what it
/// assumed in `RunnerObservation.notes`, and throws `LoopCtrlError` when a field it cannot
/// do without is missing.
///
/// One observation is `page_source` (text tree, keyboard) plus `device_info` (foreground
/// app, orientation), plus `screenshot` when asked for, or once to learn the screen size in
/// points (unless `Configuration.screenSize` is set).
public final class LoopCtrlRunner: PhoneRunner {
    /// How `RunnerCommand.launchApp` is sent.
    public enum LaunchStrategy: Sendable, Equatable {
        /// `launch_app{bundle_id}`: resumes the app, does not relaunch [readme].
        case launchApp
        /// `activate_app{bundle_id}` [source: method name only].
        case activateApp
    }

    /// Connection and adapter settings.
    public struct Configuration: Sendable {
        /// Runner base URL; default `http://127.0.0.1:8300`.
        public var baseURL: URL
        /// Per-request HTTP timeout in seconds (a `wait` command adds its own duration).
        public var requestTimeout: TimeInterval
        /// Screen size in points; when `nil` it is learned from the first screenshot.
        public var screenSize: Size?
        /// Display names by bundle id, for `RunnerObservation.appName`.
        public var appNames: [String: String]
        /// Method used for `LaunchApp`.
        public var launchStrategy: LaunchStrategy

        /// The defaults.
        public init(
            baseURL: URL = LoopCtrlAPI.defaultBaseURL,
            requestTimeout: TimeInterval = 15,
            screenSize: Size? = nil,
            appNames: [String: String] = [:],
            launchStrategy: LaunchStrategy = .launchApp
        ) {
            self.baseURL = baseURL
            self.requestTimeout = requestTimeout
            self.screenSize = screenSize
            self.appNames = appNames
            self.launchStrategy = launchStrategy
        }
    }

    private let configuration: Configuration
    private let client: LoopCtrlRPCClient
    private let learnedSize = Mutex<Size?>(nil)

    /// A runner client. `transport` defaults to `URLSession`, `sealer` to no sealing.
    public init(
        configuration: Configuration = Configuration(),
        transport: (any HTTPTransport)? = nil,
        sealer: any RequestSealer = PlaintextSealer()
    ) {
        self.configuration = configuration
        self.client = LoopCtrlRPCClient(
            baseURL: configuration.baseURL,
            timeout: configuration.requestTimeout,
            transport: transport ?? URLSessionTransport(timeout: configuration.requestTimeout),
            sealer: sealer
        )
    }

    // MARK: - Observation

    public func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        var notes: [String] = []
        let (source, keyboardVisible) = try await pageSource(notes: &notes)
        let (bundleID, reportedName, orientation) = try await deviceInfo(notes: &notes)

        var screenshot: CapturedScreenshot?
        if includeScreenshot {
            screenshot = try await takeScreenshot(notes: &notes)
        }
        let size: Size
        if let configured = configuration.screenSize {
            size = configured
        } else if let learned = learnedSize.withLock({ $0 }) {
            size = learned
        } else {
            _ = try await takeScreenshot(notes: &notes)
            guard let learned = learnedSize.withLock({ $0 }) else {
                throw LoopCtrlError.missingField(method: LoopCtrlAPI.Method.screenshot, field: LoopCtrlAPI.Screenshot.pointWidth)
            }
            notes.append("screen size \(learned.width)x\(learned.height) pt learned from a screenshot")
            size = learned
        }
        return RunnerObservation(
            source: source,
            appBundleId: bundleID,
            appName: reportedName ?? configuration.appNames[bundleID],
            screenSize: size,
            keyboardVisible: keyboardVisible,
            orientation: orientation,
            screenshot: screenshot,
            notes: notes
        )
    }

    private func pageSource(notes: inout [String]) async throws -> (String, Bool) {
        typealias K = LoopCtrlAPI.PageSource
        let method = LoopCtrlAPI.Method.pageSource
        let page = try await client.result(method, LoopCtrlAPI.pageSourceParams)
        guard let sourceValue = page[K.source] else {
            throw LoopCtrlError.missingField(method: method, field: K.source)
        }
        guard let source = sourceValue.stringValue else {
            throw LoopCtrlError.unexpectedValue(method: method, field: K.source, found: sourceValue.typeName)
        }
        switch page[K.format] {
        case .string(K.textFormat)?:
            break
        case nil:
            notes.append("page_source: no '\(K.format)' key; treating 'source' as the text tree")
        case let other?:
            throw LoopCtrlError.unexpectedValue(method: method, field: K.format, found: Self.short(other))
        }
        if let visible = page[K.keyboard]?[K.keyboardVisible]?.boolValue {
            return (source, visible)
        }
        let sniffed = source.split(separator: "\n").contains {
            $0.drop(while: { $0 == " " }).hasPrefix(LoopCtrlAPI.keyboardLinePrefix)
        }
        let found = page[K.keyboard].map(Self.short) ?? "absent"
        notes.append("page_source: '\(K.keyboard).\(K.keyboardVisible)' unavailable (\(K.keyboard): \(found)); keyboard \(sniffed ? "visible" : "hidden") judged from the tree")
        return (source, sniffed)
    }

    private func deviceInfo(notes: inout [String]) async throws -> (String, String?, Orientation) {
        typealias K = LoopCtrlAPI.DeviceInfo
        let method = LoopCtrlAPI.Method.deviceInfo
        let info = try await client.result(method)
        let field = "\(K.system).\(K.foregroundApp)"
        guard let foreground = info[K.system]?[K.foregroundApp] else {
            throw LoopCtrlError.missingField(method: method, field: field)
        }
        let bundleID: String
        var name: String?
        switch foreground {
        case .string(let id) where !id.isEmpty:
            bundleID = id
        case .object(let app):
            guard let id = app[K.foregroundAppBundleID]?.stringValue, !id.isEmpty else {
                throw LoopCtrlError.unexpectedValue(method: method, field: field, found: Self.short(foreground))
            }
            bundleID = id
            name = app[K.foregroundAppName]?.stringValue
        default:
            throw LoopCtrlError.unexpectedValue(method: method, field: field, found: Self.short(foreground))
        }
        let orientation: Orientation
        switch info[K.orientation] {
        case .string(let raw)?:
            if let known = LoopCtrlAPI.orientations[raw.lowercased()] {
                orientation = known
            } else {
                notes.append("device_info: unknown orientation '\(raw)'; assuming portrait")
                orientation = .portrait
            }
        case nil:
            notes.append("device_info: no '\(K.orientation)'; assuming portrait")
            orientation = .portrait
        case let other?:
            notes.append("device_info: orientation is \(other.typeName); assuming portrait")
            orientation = .portrait
        }
        return (bundleID, name, orientation)
    }

    private func takeScreenshot(notes: inout [String]) async throws -> CapturedScreenshot {
        typealias K = LoopCtrlAPI.Screenshot
        let method = LoopCtrlAPI.Method.screenshot
        let shot = try await client.result(method)
        guard let encoded = shot[K.data] else {
            throw LoopCtrlError.missingField(method: method, field: K.data)
        }
        guard let base64 = encoded.stringValue, let data = Data(base64Encoded: base64, options: .ignoreUnknownCharacters), !data.isEmpty else {
            throw LoopCtrlError.unexpectedValue(method: method, field: K.data, found: "not base64 (\(encoded.typeName))")
        }
        guard let pixelWidth = Self.dimension(shot, K.pixelWidth, fallback: K.width, notes: &notes),
              let pixelHeight = Self.dimension(shot, K.pixelHeight, fallback: K.height, notes: &notes)
        else {
            throw LoopCtrlError.missingField(method: method, field: K.pixelWidth)
        }
        let scale = shot[K.scale]?.numberValue
        if let w = shot[K.pointWidth]?.numberValue, let h = shot[K.pointHeight]?.numberValue, w > 0, h > 0 {
            learnedSize.withLock { $0 = Size(width: w, height: h) }
        } else if let scale, scale > 0 {
            // Point equals pixel divided by scale [readme: SKILL.md].
            learnedSize.withLock { $0 = Size(width: Double(pixelWidth) / scale, height: Double(pixelHeight) / scale) }
            notes.append("screenshot: no point size; derived from pixels / scale")
        }
        let mimeType: String
        switch shot[K.format]?.stringValue?.lowercased() {
        case "jpeg"?, "jpg"?:
            mimeType = "image/jpeg"
        case "png"?:
            mimeType = "image/png"
        default:
            guard let sniffed = Self.sniffImageType(data) else {
                throw LoopCtrlError.unexpectedValue(method: method, field: K.format, found: shot[K.format].map(Self.short) ?? "absent, and the bytes are neither JPEG nor PNG")
            }
            mimeType = sniffed
        }
        return CapturedScreenshot(data: data, mimeType: mimeType, pixelWidth: pixelWidth, pixelHeight: pixelHeight, scale: scale)
    }

    // MARK: - Commands

    public func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        let (method, params, extra) = Self.request(for: command, launch: configuration.launchStrategy)
        switch try await client.call(method, params, extraTimeout: extra) {
        case .result:
            // Success is a result dictionary (usually with `message`) [source].
            return RunnerResult(ok: true, error: nil)
        case .failure(let code, let message):
            return RunnerResult(ok: false, error: "\(message) (\(method) error \(code))")
        }
    }

    /// The JSON-RPC method and params for a command (RUNNER.md 3.1), and extra HTTP timeout.
    static func request(for command: RunnerCommand, launch: LaunchStrategy) -> (String, [String: JSONValue], TimeInterval) {
        typealias M = LoopCtrlAPI.Method
        typealias P = LoopCtrlAPI.Param
        switch command {
        case .tap(let x, let y):
            return (M.tap, [P.x: .number(x), P.y: .number(y)], 0)
        case .longPress(let x, let y, let durationMs):
            return (M.longPress, [P.x: .number(x), P.y: .number(y), P.duration: LoopCtrlAPI.durationValue(milliseconds: durationMs)], Double(durationMs) / 1_000)
        case .swipe(let from, let to, let durationMs):
            return (M.drag, [
                P.fromX: .number(from.x), P.fromY: .number(from.y),
                P.toX: .number(to.x), P.toY: .number(to.y),
                P.duration: LoopCtrlAPI.durationValue(milliseconds: durationMs),
            ], Double(durationMs) / 1_000)
        case .typeText(let text):
            return (M.type, [P.text: .string(text)], 0)
        case .pressKey(let key):
            switch LoopCtrlAPI.command(for: key) {
            case .method(let method, let params):
                return (method, params, 0)
            }
        case .launchApp(let bundleID):
            return (launch == .launchApp ? M.launchApp : M.activateApp, [P.bundleID: .string(bundleID)], 0)
        case .openUrl(let url):
            return (M.openURL, [P.url: .string(url)], 0)
        case .wait(let ms):
            return (M.wait, [P.duration: LoopCtrlAPI.durationValue(milliseconds: ms)], Double(ms) / 1_000)
        }
    }

    // MARK: - Decoding helpers

    private static func dimension(_ object: [String: JSONValue], _ key: String, fallback: String, notes: inout [String]) -> UInt32? {
        if let n = object[key]?.numberValue, n > 0, n < Double(UInt32.max) { return UInt32(n) }
        if let n = object[fallback]?.numberValue, n > 0, n < Double(UInt32.max) {
            notes.append("screenshot: no '\(key)'; used '\(fallback)' as pixels")
            return UInt32(n)
        }
        return nil
    }

    private static func sniffImageType(_ data: Data) -> String? {
        let head = [UInt8](data.prefix(4))
        if head.starts(with: [0xFF, 0xD8]) { return "image/jpeg" }
        if head.starts(with: [0x89, 0x50, 0x4E, 0x47]) { return "image/png" }
        return nil
    }

    /// A short rendering of an unexpected value for error messages.
    private static func short(_ value: JSONValue) -> String {
        let text = (try? value.encoded()).map { String(decoding: $0, as: UTF8.self) } ?? value.typeName
        return text.count > 80 ? String(text.prefix(80)) + "…" : text
    }
}
