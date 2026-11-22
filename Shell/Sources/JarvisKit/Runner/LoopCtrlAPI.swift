import Foundation
import JarvisCore

/// Every LoopCtrl-specific name and value `LoopCtrlRunner` relies on, in one place, so the
/// adapter can be corrected quickly after the first device test.
///
/// Each item carries its evidence tag from docs/RUNNER.md:
/// - [source]     seen in LoopCtrl's code (path in RUNNER.md)
/// - [readme]     claimed by a README / SKILL.md, not confirmed in code
/// - [unverified] not seen anywhere; an assumption to confirm on the device
///
/// Nothing here has been run against a real runner.
public enum LoopCtrlAPI {
    // MARK: Transport

    /// Default port 8300 [readme]; settable 1024 to 65533 in the app [readme].
    public static let defaultPort = 8300
    /// Bound to 127.0.0.1 when local-only [source: LoopCtrlUITests.swift].
    public static let defaultHost = "127.0.0.1"
    /// `POST /rpc` [readme].
    public static let rpcPath = "/rpc"
    /// `http://127.0.0.1:8300`.
    public static var defaultBaseURL: URL {
        // A literal that always parses; the fallback keeps this free of force unwraps.
        URL(string: "http://\(defaultHost):\(defaultPort)") ?? URL(fileURLWithPath: "/")
    }

    // MARK: JSON-RPC envelope

    /// JSON-RPC 2.0 [readme]; types `JSONRPCRequest` / `JSONRPCResponse` exist
    /// [source: LoopCtrlKit/RPCTypes.swift]. Their exact field names were not read: the
    /// JSON-RPC 2.0 standard names below are [unverified] for LoopCtrl.
    enum Envelope {
        static let version = "2.0"           // [unverified] value of "jsonrpc"
        static let jsonrpc = "jsonrpc"       // [unverified]
        static let id = "id"                 // [unverified]
        static let method = "method"         // [unverified]
        static let params = "params"         // [unverified]
        /// Result is an open dictionary `[String: JSONValue]?` [source: RPCTypes.swift].
        static let result = "result"         // [source] (type), key name [unverified]
        /// Error object with code, message, data [source: RPCTypes.swift].
        static let error = "error"           // [source]
        static let errorCode = "code"        // [source]
        static let errorMessage = "message"  // [source]
        static let errorData = "data"        // [source]
    }

    /// JSON-RPC error codes [source: RPCTypes.swift]: -32700 parse, -32600 invalid request,
    /// -32601 method not found, -32602 invalid params, -32603 internal, -32099 auth,
    /// -32000 business error (returned with HTTP 200).
    enum ErrorCode {
        static let authentication = -32099   // [source] means: re-pair
        static let business = -32000         // [source]
        static let methodNotFound = -32601   // [source]
        static let invalidParams = -32602    // [source]
    }

    // MARK: Methods [source: LoopCtrlKit/RPCMethod.swift, params as summarised in RUNNER.md]

    enum Method {
        static let pageSource = "page_source"     // [source]
        static let deviceInfo = "device_info"     // [source]
        static let screenshot = "screenshot"      // [source]
        static let tap = "tap"                    // [source]
        static let longPress = "long_press"       // [source]
        /// Used for `Swipe{from,to}`: `swipe` has no from/to, `drag` has
        /// `from_x, from_y, to_x, to_y, duration` [source].
        static let drag = "drag"                  // [source]
        static let type = "type"                  // [source]
        static let home = "home"                  // [source]
        static let launchApp = "launch_app"       // [source] name; body unseen
        static let activateApp = "activate_app"   // [source] name; body unseen
        static let openURL = "open_url"           // [source]
        static let wait = "wait"                  // [source]
    }

    // MARK: Parameters

    enum Param {
        static let x = "x"                        // [source] tap/long_press take x,y
        static let y = "y"                        // [source]
        static let duration = "duration"          // [source] name; unit [unverified]
        static let fromX = "from_x"               // [source] drag
        static let fromY = "from_y"               // [source]
        static let toX = "to_x"                   // [source]
        static let toY = "to_y"                   // [source]
        static let text = "text"                  // [source] type{text*, clear, submit}
        static let submit = "submit"              // [source] name; "presses Return" [unverified]
        static let bundleID = "bundle_id"         // [source] launch_app / activate_app
        static let url = "url"                    // [source] open_url{url*}
        static let visibleOnly = "visible_only"   // [source] page_source param
        static let maxChildren = "max_children"   // [source] page_source param
    }

    /// `page_source{visible_only: true, max_children: 50}`, the documented on-screen read
    /// [readme: RUNNER.md section 4, item 4].
    static let pageSourceParams: [String: JSONValue] = [
        Param.visibleOnly: true,
        Param.maxChildren: 50,
    ]

    // MARK: Result keys

    enum PageSource {
        /// The indented text tree [source: ActionHandler.swift].
        static let source = "source"
        /// `"text"` for the tree format we parse [source].
        static let format = "format"
        static let textFormat = "text"
        /// Key exists [source]; inner shape `{visible, covers_from_y}` [readme].
        static let keyboard = "keyboard"
        static let keyboardVisible = "visible"    // [readme]
    }

    enum DeviceInfo {
        /// `foreground_app` is in the `device_info` system block [source: RPCMethod.swift
        /// result lists]. Its value type (string or object) is [unverified].
        static let system = "system"
        static let foregroundApp = "foreground_app"
        /// If `foreground_app` is an object, the key holding the bundle id [unverified].
        static let foregroundAppBundleID = "bundle_id"
        /// If `foreground_app` is an object, the key holding the display name [unverified].
        static let foregroundAppName = "name"
        /// `device_info.orientation` [readme]; its value strings are [unverified].
        static let orientation = "orientation"
        /// `device_info.system.screen_locked`, a boolean [unverified: named by the planner
        /// from the LoopCtrl lineage; not seen in the code read for RUNNER.md, where only an
        /// `is_locked` method is listed]. Absent → the lock state is unknown.
        static let screenLocked = "screen_locked"
    }

    /// Words in a JSON-RPC business error that mean "refused: the device is locked"
    /// [unverified: the refusal text was never seen]. Matched case-insensitively.
    static let lockedRefusalWords = ["locked", "lock screen", "passcode"]

    /// Orientation strings we accept [unverified]: snake_case and camelCase of UIKit names.
    static let orientations: [String: Orientation] = [
        "portrait": .portrait,
        "portrait_upside_down": .portraitUpsideDown,
        "portraitupsidedown": .portraitUpsideDown,
        "upside_down": .portraitUpsideDown,
        "landscape_left": .landscapeLeft,
        "landscapeleft": .landscapeLeft,
        "landscape_right": .landscapeRight,
        "landscaperight": .landscapeRight,
    ]

    enum Screenshot {
        /// Base64 image [source: ActionHandler.swift].
        static let data = "data"
        /// e.g. `"jpeg"` (default JPEG, quality 0.6, scale 2.0) [source].
        static let format = "format"
        static let pixelWidth = "pixel_width"     // [source, iCTRL file; same lineage]
        static let pixelHeight = "pixel_height"   // [source, iCTRL file]
        static let pointWidth = "point_width"     // [source, iCTRL file]
        static let pointHeight = "point_height"   // [source, iCTRL file]
        static let scale = "scale"                // [source, iCTRL file]
        /// Fallback size keys, also listed [source, iCTRL file]; units [unverified].
        static let width = "width"
        static let height = "height"
    }

    // MARK: Units and key mapping

    /// `duration` of `long_press`, `drag` and `wait` is taken to be seconds
    /// [unverified: "likely seconds", RUNNER.md 3.1].
    static func durationValue(milliseconds: UInt64) -> JSONValue {
        .number(Double(milliseconds) / 1_000)
    }

    /// How each `Key` is sent. `home` is a method [source]; Return as `type{text:"",
    /// submit:true}` is a guess [unverified]; the others type XCUIKeyboardKey characters
    /// through `type` (`"\u{8}"` delete, `"\t"`, `" "`, `"\u{1B}"` escape) [unverified].
    enum KeyCommand: Equatable {
        case method(String, [String: JSONValue])
    }

    static func command(for key: Key) -> KeyCommand {
        switch key {
        case .home:
            return .method(Method.home, [:])
        case .return:
            return .method(Method.type, [Param.text: "", Param.submit: true])
        case .delete:
            return .method(Method.type, [Param.text: "\u{8}"])
        case .tab:
            return .method(Method.type, [Param.text: "\t"])
        case .space:
            return .method(Method.type, [Param.text: " "])
        case .escape:
            return .method(Method.type, [Param.text: "\u{1B}"])
        }
    }

    /// Fallback keyboard detection when `page_source.keyboard` is missing: a tree line
    /// whose type is `Keyboard` (RUNNER.md 3.2 suggests sniffing the tree) [unverified].
    static let keyboardLinePrefix = "Keyboard"
}
