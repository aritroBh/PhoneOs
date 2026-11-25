import Foundation
import ObjectiveC
import XCTest
import JarvisKit

/// The app in front as the automation session knows it (docs/ARCHITECTURE.md, *Swift shell*,
/// the front-app rule, part 2). XCUITest has no public way to ask; the test framework's
/// accessibility client does, and WebDriverAgent reads it the same way: the client's active
/// applications (each with a `processIdentifier`), then `XCUIApplication` for that process
/// and its `bundleID`.
///
/// **Private API.** Inside a test-runner bundle on a personally sideloaded build only. Nothing
/// here is linked or declared from a private header: every class and selector is looked up by
/// name through the Objective-C runtime, checked (`responds(to:)`, or the method and its type
/// encoding where a C signature is cast) before it is called, and anything missing, of another
/// shape, or answering nothing makes `bundleId()` return `nil`. Any Xcode update can break it;
/// the runner then reads the front app as before (the last launched app, the known apps) and
/// the fail-closed rule (`FrontApp`, part 1) holds whatever this returns.
///
/// The one entry point is `bundleId()`; `lastPath` says which path answered.
@MainActor
enum FrontProcess {
    /// What the last `bundleId()` did: which path answered, or why none did (for the
    /// observation notes and the QA records).
    private(set) static var lastPath = "not asked"
    /// How long the last `bundleId()` took, in milliseconds.
    private(set) static var lastMilliseconds = 0.0

    /// The bundle id of the app in front (SpringBoard when no app is over it), or `nil` when
    /// the private path is unavailable or does not tell (`FrontApp.choose`).
    static func bundleId() -> String? {
        let start = Date()
        let answer: String?
        switch activeBundleIds() {
        case .failure(let why):
            lastPath = "private path unavailable (\(why.reason)); the public readings decide"
            answer = nil
        case .success(let reading):
            let choice = FrontApp.choose(active: reading.ids, runner: Bundle.main.bundleIdentifier) { id in
                XCUIApplication(bundleIdentifier: id).state == .runningForeground
            }
            lastPath = "private path (\(reading.path)): \(choice.why) -> \(choice.bundleId ?? "not decided")"
            answer = choice.bundleId
        }
        lastMilliseconds = Date().timeIntervalSince(start) * 1_000
        return answer
    }

    // MARK: - The steps (each one checked; any miss is `Unavailable`)

    private struct Unavailable: Error {
        let reason: String
    }

    private struct Reading {
        /// The bundle id of each active application, in the client's order (`nil`: not read).
        var ids: [String?]
        /// Which classes and selectors answered.
        var path: String
    }

    /// `-[NSObject getter]` returning an object, through `perform(_:)` (an Objective-C object
    /// return at +0, the shape `perform` is for), only when the object responds to it and the
    /// method takes no argument and returns an object.
    private static func objectGetter(_ object: NSObject, _ name: String) -> AnyObject? {
        let selector = NSSelectorFromString(name)
        guard object.responds(to: selector),
              let cls = object_getClass(object),
              let method = class_getInstanceMethod(cls, selector),
              method_getNumberOfArguments(method) == 2,
              returnType(method) == "@" else { return nil }
        return object.perform(selector)?.takeUnretainedValue()
    }

    /// The return type encoding of `method` (`"@"` for an object, `"i"` for `int`).
    private static func returnType(_ method: Method) -> String {
        let pointer = method_copyReturnType(method)
        defer { free(pointer) }
        return String(cString: pointer)
    }

    /// The type encoding of argument `index` of `method` (0 self, 1 the selector).
    private static func argumentType(_ method: Method, _ index: UInt32) -> String? {
        guard let pointer = method_copyArgumentType(method, index) else { return nil }
        defer { free(pointer) }
        return String(cString: pointer)
    }

    /// `+[Class selector]` returning an object, through its implementation cast to the exact
    /// C signature `(id self, SEL _cmd) -> id`, only when the method exists with no argument
    /// and an object return.
    private typealias ClassObjectGetter = @convention(c) (AnyObject, Selector) -> AnyObject?
    private static func classObjectGetter(_ cls: AnyClass, _ name: String) -> NSObject? {
        let selector = NSSelectorFromString(name)
        guard let method = class_getClassMethod(cls, selector),
              method_getNumberOfArguments(method) == 2,
              returnType(method) == "@" else { return nil }
        let function = unsafeBitCast(method_getImplementation(method), to: ClassObjectGetter.self)
        return function(cls as AnyObject, selector) as? NSObject
    }

    /// A method taking one `pid_t` (`int`, encoding `"i"`) and returning an object, called
    /// through its implementation cast to `(id self, SEL _cmd, int) -> id`: the class method
    /// `+[XCUIApplication applicationWithPID:]` (`receiver` the class) or the client's
    /// `-monitoredApplicationWithProcessIdentifier:` (`receiver` the client).
    private typealias ObjectForPID = @convention(c) (AnyObject, Selector, Int32) -> AnyObject?
    private static func objectForPID(_ receiver: AnyObject, method: Method?, selector: Selector, pid: Int32) -> NSObject? {
        guard let method,
              method_getNumberOfArguments(method) == 3,
              returnType(method) == "@",
              argumentType(method, 2) == "i" else { return nil }
        let function = unsafeBitCast(method_getImplementation(method), to: ObjectForPID.self)
        return function(receiver, selector, pid) as? NSObject
    }

    /// The accessibility client: `XCUIDevice.shared.accessibilityInterface` (current
    /// WebDriverAgent), else `+[XCAXClient_iOS sharedClient]` (older Xcode).
    private static func accessibilityClient() -> (client: NSObject, path: String)? {
        let device: NSObject = XCUIDevice.shared
        if let interface = objectGetter(device, "accessibilityInterface") as? NSObject {
            return (client: interface, path: "XCUIDevice.accessibilityInterface")
        }
        if let cls = NSClassFromString("XCAXClient_iOS"),
           let shared = classObjectGetter(cls, "sharedClient") {
            return (client: shared, path: "XCAXClient_iOS.sharedClient")
        }
        return nil
    }

    /// The process identifier of an active-application element (`processIdentifier`, read
    /// by key-value coding once the element responds to it: a `pid_t` comes back boxed).
    private static func processIdentifier(_ element: NSObject) -> Int32? {
        let selector = NSSelectorFromString("processIdentifier")
        guard element.responds(to: selector),
              let number = element.value(forKey: "processIdentifier") as? NSNumber else { return nil }
        let pid = number.int32Value
        return pid > 0 ? pid : nil
    }

    /// The bundle id of process `pid`: `+[XCUIApplication applicationWithPID:]`, else the
    /// client's `-monitoredApplicationWithProcessIdentifier:`, then that application's
    /// `bundleID`.
    private static func bundleId(pid: Int32, client: NSObject) -> (id: String, path: String)? {
        let byClass = NSSelectorFromString("applicationWithPID:")
        if let app = objectForPID(XCUIApplication.self as AnyObject,
                                  method: class_getClassMethod(XCUIApplication.self, byClass),
                                  selector: byClass, pid: pid),
           let id = objectGetter(app, "bundleID") as? String, !id.isEmpty {
            return (id: id, path: "applicationWithPID:")
        }
        let byClient = NSSelectorFromString("monitoredApplicationWithProcessIdentifier:")
        if client.responds(to: byClient), let cls = object_getClass(client),
           let app = objectForPID(client, method: class_getInstanceMethod(cls, byClient),
                                  selector: byClient, pid: pid),
           let id = objectGetter(app, "bundleID") as? String, !id.isEmpty {
            return (id: id, path: "monitoredApplicationWithProcessIdentifier:")
        }
        return nil
    }

    private static func activeBundleIds() -> Result<Reading, Unavailable> {
        guard let found = accessibilityClient() else {
            return .failure(Unavailable(reason: "no accessibility client (XCUIDevice.accessibilityInterface, XCAXClient_iOS.sharedClient)"))
        }
        let client = found.client
        let clientPath = found.path
        guard let list = objectGetter(client, "activeApplications") as? NSArray else {
            return .failure(Unavailable(reason: "\(clientPath) has no activeApplications"))
        }
        guard list.count > 0 else {
            return .failure(Unavailable(reason: "\(clientPath).activeApplications is empty"))
        }
        var ids: [String?] = []
        var how: Set<String> = []
        for case let element as NSObject in list {
            guard let pid = processIdentifier(element),
                  let named = bundleId(pid: pid, client: client) else {
                ids.append(nil)
                continue
            }
            ids.append(named.id)
            how.insert(named.path)
        }
        if ids.count != list.count { ids.append(nil) } // an element that is not an object
        if ids.allSatisfy({ $0 == nil }) {
            return .failure(Unavailable(reason: "no bundle id read for any of \(list.count) active application(s)"))
        }
        let path = "\(clientPath).activeApplications -> processIdentifier -> \(how.sorted().joined(separator: " / ")) -> bundleID"
        return .success(Reading(ids: ids, path: path))
    }
}
