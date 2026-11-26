import Foundation
import ObjectiveC
import XCTest
import JarvisKit

/// The app in front as the automation session knows it (docs/ARCHITECTURE.md, *Swift shell*,
/// the front-app rule, part 2). XCUITest has no public way to ask; the test framework's
/// accessibility client does, and WebDriverAgent reads it the same way. The chain: the client
/// (`XCUIDevice.shared.accessibilityInterface`, else `+[XCAXClient_iOS sharedClient]`), its
/// `activeApplications`, each element's `processIdentifier`, then the first of these that
/// names the process: the client's `applicationProcessTracker` and its
/// `-monitoredApplicationWithProcessIdentifier:` (what WebDriverAgent calls today), the
/// tracker's `-applicationProcessWithPID:`, the client's own
/// `-monitoredApplicationWithProcessIdentifier:`, `+[XCUIApplication applicationWithPID:]`
/// (older Xcode), each followed by the object's `bundleID`; last, the test runner's daemon
/// (`XCTRunnerDaemonSession.sharedSession.daemonProxy`,
/// `-_XCT_requestBundleIDForPID:reply:`, WebDriverAgent's way to the bundle id, answered
/// within 1 s or not asked again). Mac run 141 (Xcode 26.6, iOS 26.5 simulator): the old
/// chain (`applicationWithPID:`, then the client's `monitoredApplicationWithProcessIdentifier:`)
/// never answered, in about 390 readings; on a miss `lastPath` now says, per active element,
/// which step failed and why.
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
    /// Set once the daemon left a bundle-id request unanswered: it is not asked again, so a
    /// reply that never comes costs its wait once, not on every reading.
    private static var daemonUnanswered = false

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

    /// One way from a process identifier to a bundle id, its receiver and method checked.
    private struct Lookup {
        /// Short, for the failure note.
        let name: String
        /// In full, for the success note.
        let path: String
        let call: (Int32) -> Result<String, Unavailable>
    }

    /// The longest reason a failure note carries, in characters.
    private static let noteLimit = 600
    /// Active elements described one by one in a failure note.
    private static let elementsNoted = 4

    private static func className(_ object: AnyObject) -> String {
        NSStringFromClass(type(of: object))
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

    /// Why `method` is not of the shape a cast needs, or `nil` when it is: `arguments` counts
    /// self and the selector, `returns` is the return encoding, `argument2` and `argument3`
    /// (when given) the encodings of the first and second real arguments.
    private static func shapeMismatch(_ method: Method, arguments: UInt32, returns: String,
                                      argument2: String? = nil, argument3: String? = nil) -> String? {
        let count = method_getNumberOfArguments(method)
        let returned = returnType(method)
        let second = argumentType(method, 2)
        let third = argumentType(method, 3)
        if count == arguments, returned == returns,
           argument2 == nil || second == argument2,
           argument3 == nil || third == argument3 {
            return nil
        }
        let shape = "method shape arguments=\(count) return=\(returned) arg=\(second ?? "none")"
        return argument3 == nil ? shape : shape + " arg3=\(third ?? "none")"
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
    /// through its implementation cast to `(id self, SEL _cmd, int) -> id`: the tracker's or
    /// the client's `-monitoredApplicationWithProcessIdentifier:`, the tracker's
    /// `-applicationProcessWithPID:`, or the class method `+[XCUIApplication applicationWithPID:]`
    /// (`receiver` the class).
    private typealias ObjectForPID = @convention(c) (AnyObject, Selector, Int32) -> AnyObject?
    private static func objectForPID(_ receiver: AnyObject, method: Method?, selector: Selector, pid: Int32) -> Result<NSObject, Unavailable> {
        guard let method else { return .failure(Unavailable(reason: "no method")) }
        if let mismatch = shapeMismatch(method, arguments: 3, returns: "@", argument2: "i") {
            return .failure(Unavailable(reason: mismatch))
        }
        let function = unsafeBitCast(method_getImplementation(method), to: ObjectForPID.self)
        guard let object = function(receiver, selector, pid) else {
            return .failure(Unavailable(reason: "returned nil"))
        }
        guard let application = object as? NSObject else {
            return .failure(Unavailable(reason: "returned a \(className(object)), not an NSObject"))
        }
        return .success(application)
    }

    /// The bundle id an application object carries: its `bundleID`, a non-empty string.
    /// (Not `identifier`: on an `XCUIApplication` that is the element's accessibility
    /// identifier, read from the app's own tree, and no bundle id.)
    private static func bundleID(of application: NSObject) -> Result<String, Unavailable> {
        if let id = objectGetter(application, "bundleID") as? String, !id.isEmpty {
            return .success(id)
        }
        return .failure(Unavailable(reason: "application (\(className(application))) has no bundleID"))
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

    /// The process identifier of an active-application element, or why none: its
    /// `processIdentifier` (an `int` property) through its implementation cast to
    /// `(id self, SEL _cmd) -> int` when the method has that shape, else by key-value coding
    /// (a `pid_t` comes back boxed) when the element's class implements the method in another
    /// shape. A selector the element only forwards is not read (key-value coding would throw).
    private typealias Int32Getter = @convention(c) (AnyObject, Selector) -> Int32
    private static func processIdentifier(_ element: NSObject) -> Result<Int32, Unavailable> {
        let selector = NSSelectorFromString("processIdentifier")
        guard element.responds(to: selector) else {
            return .failure(Unavailable(reason: "no processIdentifier"))
        }
        guard let cls = object_getClass(element),
              let method = class_getInstanceMethod(cls, selector) else {
            return .failure(Unavailable(reason: "processIdentifier not implemented by the class"))
        }
        let pid: Int32
        if shapeMismatch(method, arguments: 2, returns: "i") == nil {
            let function = unsafeBitCast(method_getImplementation(method), to: Int32Getter.self)
            pid = function(element, selector)
        } else if let number = element.value(forKey: "processIdentifier") as? NSNumber {
            pid = number.int32Value
        } else {
            return .failure(Unavailable(reason: "processIdentifier unreadable, return=\(returnType(method))"))
        }
        return pid > 0 ? .success(pid) : .failure(Unavailable(reason: "pid \(pid)"))
    }

    /// `-[receiver name:pid]` then `bundleID`, once `receiver` responds to `name` and its
    /// class's method has the shape `objectForPID` casts; else why not (said once per reading).
    private static func instanceLookup(_ receiver: NSObject, _ name: String, owner: String,
                                       short: String, path: String) -> Result<Lookup, Unavailable> {
        let selector = NSSelectorFromString(name)
        guard receiver.responds(to: selector) else {
            return .failure(Unavailable(reason: "\(owner) (\(className(receiver))) does not respond to \(name)"))
        }
        guard let cls = object_getClass(receiver),
              let method = class_getInstanceMethod(cls, selector) else {
            return .failure(Unavailable(reason: "\(owner) (\(className(receiver))) only forwards \(name)"))
        }
        if let mismatch = shapeMismatch(method, arguments: 3, returns: "@", argument2: "i") {
            return .failure(Unavailable(reason: "\(short) \(mismatch)"))
        }
        return .success(Lookup(name: short, path: path) { pid in
            FrontProcess.objectForPID(receiver, method: method, selector: selector, pid: pid)
                .flatMap { FrontProcess.bundleID(of: $0) }
        })
    }

    /// The encoding `-_XCT_requestBundleIDForPID:reply:` has, offsets removed: no return,
    /// self, the selector, an `int`, a block.
    private static let bundleIDRequestTypes = "v@:i@?"
    /// The encoding of `selector` in the messaging protocol that declares it
    /// (`XCTMessagingRole_BundleRequesting`), offsets removed; `nil` when not found.
    private static func bundleRequestingTypes(_ selector: Selector) -> String? {
        guard let messaging = NSProtocolFromString("XCTMessagingRole_BundleRequesting") else { return nil }
        for required in [true, false] {
            let description = protocol_getMethodDescription(messaging, selector, required, true)
            let types: UnsafeMutablePointer<CChar>? = description.types
            if let types {
                return String(cString: types).filter { !$0.isNumber }
            }
        }
        return nil
    }

    /// `(id self, SEL _cmd, int pid, void (^reply)(NSString *, NSError *))`, the block passed
    /// as an object.
    private typealias BundleIDRequest = @convention(c) (AnyObject, Selector, Int32, AnyObject) -> Void
    private static let daemonPath = "XCTRunnerDaemonSession.daemonProxy -> _XCT_requestBundleIDForPID:reply:"
    /// The test runner's daemon (WebDriverAgent's `fb_appsInfoWithAxElements:`):
    /// `XCTRunnerDaemonSession.sharedSession.daemonProxy` and its
    /// `-_XCT_requestBundleIDForPID:reply:`, waited for 1 s as WebDriverAgent does. The proxy
    /// may forward the message rather than implement it: then the shape checked is the one
    /// the messaging protocol declares, and the implementation called is the runtime's
    /// forwarding (`class_getMethodImplementation`), as a message send would.
    private static func daemonLookup() -> Result<Lookup, Unavailable> {
        guard let cls = NSClassFromString("XCTRunnerDaemonSession") else {
            return .failure(Unavailable(reason: "no XCTRunnerDaemonSession"))
        }
        guard let session = classObjectGetter(cls, "sharedSession") else {
            return .failure(Unavailable(reason: "no XCTRunnerDaemonSession.sharedSession"))
        }
        guard let proxy = objectGetter(session, "daemonProxy"),
              let messaging = proxy as? NSObjectProtocol else {
            return .failure(Unavailable(reason: "no daemonProxy"))
        }
        let name = "_XCT_requestBundleIDForPID:reply:"
        let selector = NSSelectorFromString(name)
        guard messaging.responds(to: selector), let proxyClass = object_getClass(proxy) else {
            return .failure(Unavailable(reason: "daemon proxy (\(className(proxy))) does not respond to \(name)"))
        }
        let implementation: IMP
        if let method = class_getInstanceMethod(proxyClass, selector) {
            if let mismatch = shapeMismatch(method, arguments: 4, returns: "v", argument2: "i", argument3: "@?") {
                return .failure(Unavailable(reason: "daemon \(mismatch)"))
            }
            implementation = method_getImplementation(method)
        } else {
            let declared = bundleRequestingTypes(selector)
            let forwarding: IMP? = class_getMethodImplementation(proxyClass, selector)
            guard declared == bundleIDRequestTypes, let forwarding else {
                return .failure(Unavailable(reason: "daemon proxy (\(className(proxy))) forwards \(name), declared shape \(declared ?? "not found")"))
            }
            implementation = forwarding
        }
        let function = unsafeBitCast(implementation, to: BundleIDRequest.self)
        return .success(Lookup(name: "daemon", path: daemonPath) { pid in
            if FrontProcess.daemonUnanswered {
                return .failure(Unavailable(reason: "left a request unanswered before"))
            }
            let box = BundleIDReplyBox()
            let reply = bundleIDReply(into: box)
            withExtendedLifetime(reply) {
                function(proxy, selector, pid, unsafeBitCast(reply, to: AnyObject.self))
            }
            guard box.done.wait(timeout: DispatchTime.now() + .seconds(1)) == .success else {
                FrontProcess.daemonUnanswered = true
                return .failure(Unavailable(reason: "no reply in 1 s"))
            }
            let answer = box.reading
            guard let id = answer.id, !id.isEmpty else {
                return .failure(Unavailable(reason: "no bundle id (\(answer.problem ?? "empty reply"))"))
            }
            return .success(id)
        })
    }

    /// The ways to a bundle id this reading can take, in order, and why each other one is
    /// not taken.
    private static func lookups(client: NSObject) -> (ready: [Lookup], missing: [String]) {
        var ready: [Lookup] = []
        var missing: [String] = []
        func add(_ result: Result<Lookup, Unavailable>) {
            switch result {
            case .success(let lookup): ready.append(lookup)
            case .failure(let why): missing.append(why.reason)
            }
        }
        let monitored = "monitoredApplicationWithProcessIdentifier:"
        let trackerObject = objectGetter(client, "applicationProcessTracker")
        if let tracker = trackerObject as? NSObject {
            add(instanceLookup(tracker, monitored, owner: "tracker", short: "tracker.app",
                               path: "applicationProcessTracker -> \(monitored) -> bundleID"))
            add(instanceLookup(tracker, "applicationProcessWithPID:", owner: "tracker", short: "tracker.process",
                               path: "applicationProcessTracker -> applicationProcessWithPID: -> bundleID"))
        } else if let other = trackerObject {
            missing.append("applicationProcessTracker (\(className(other))) is not an NSObject")
        } else {
            missing.append("no applicationProcessTracker")
        }
        add(instanceLookup(client, monitored, owner: "client", short: "client.app",
                           path: "\(monitored) -> bundleID"))
        let byClass = NSSelectorFromString("applicationWithPID:")
        if let method = class_getClassMethod(XCUIApplication.self, byClass) {
            ready.append(Lookup(name: "applicationWithPID:", path: "applicationWithPID: -> bundleID") { pid in
                FrontProcess.objectForPID(XCUIApplication.self as AnyObject, method: method, selector: byClass, pid: pid)
                    .flatMap { FrontProcess.bundleID(of: $0) }
            })
        } else {
            missing.append("no +[XCUIApplication applicationWithPID:]")
        }
        if daemonUnanswered {
            missing.append("daemon: left a request unanswered before")
        } else {
            // Resolved only when every lookup before it gave nothing for an element.
            ready.append(Lookup(name: "daemon", path: daemonPath) { pid in
                FrontProcess.daemonLookup().flatMap { $0.call(pid) }
            })
        }
        return (ready: ready, missing: missing)
    }

    private static func activeBundleIds() -> Result<Reading, Unavailable> {
        guard let found = accessibilityClient() else {
            return .failure(Unavailable(reason: "no accessibility client (XCUIDevice.accessibilityInterface, XCAXClient_iOS.sharedClient)"))
        }
        let client = found.client
        let clientPath = found.path
        let clientNote = "client \(className(client)) via \(clientPath)"
        guard let list = objectGetter(client, "activeApplications") as? NSArray else {
            return .failure(Unavailable(reason: "\(clientNote) has no activeApplications"))
        }
        guard list.count > 0 else {
            return .failure(Unavailable(reason: "\(clientNote): activeApplications is empty"))
        }
        let routes = lookups(client: client)
        var ids: [String?] = []
        var how: Set<String> = []
        var elementNotes: [String] = []
        for (index, item) in list.enumerated() {
            guard let element = item as? NSObject else {
                ids.append(nil)
                elementNotes.append("[\(index)] \(className(item as AnyObject)): not an NSObject")
                continue
            }
            let pid: Int32
            switch processIdentifier(element) {
            case .failure(let why):
                ids.append(nil)
                elementNotes.append("[\(index)] \(className(element)): \(why.reason)")
                continue
            case .success(let read):
                pid = read
            }
            var named: String?
            var tried: [String] = []
            for route in routes.ready {
                let answer = route.call(pid)
                if case .success(let id) = answer {
                    named = id
                    how.insert(route.path)
                    break
                }
                if case .failure(let why) = answer {
                    tried.append("\(route.name) \(why.reason)")
                }
            }
            ids.append(named)
            if named == nil {
                let attempts = tried.isEmpty ? "no lookup available" : tried.joined(separator: "; ")
                elementNotes.append("[\(index)] \(className(element)) pid \(pid): \(attempts)")
            }
        }
        if ids.allSatisfy({ $0 == nil }) {
            var note = "no bundle id read for any of \(list.count) active application(s); \(clientNote)"
            if !routes.missing.isEmpty {
                note += "; not tried: " + routes.missing.joined(separator: "; ")
            }
            note += "; " + elementNotes.prefix(elementsNoted).joined(separator: "; ")
            if elementNotes.count > elementsNoted {
                note += "; +\(elementNotes.count - elementsNoted) more"
            }
            return .failure(Unavailable(reason: String(note.prefix(noteLimit))))
        }
        let path = "\(clientPath).activeApplications -> processIdentifier -> \(how.sorted().joined(separator: " / "))"
        return .success(Reading(ids: ids, path: path))
    }
}

/// The answer to one `-_XCT_requestBundleIDForPID:reply:`: the daemon replies on its own
/// queue, possibly after the wait gave up.
private final class BundleIDReplyBox: @unchecked Sendable {
    private let lock = NSLock()
    private var id: String?
    private var problem: String?
    let done = DispatchSemaphore(value: 0)

    func answer(_ id: String?, _ error: NSError?) {
        lock.lock()
        self.id = id
        problem = error.map { "error \($0.domain) \($0.code)" }
        lock.unlock()
        done.signal()
    }

    var reading: (id: String?, problem: String?) {
        lock.lock()
        defer { lock.unlock() }
        return (id: id, problem: problem)
    }
}

private typealias BundleIDReply = @Sendable @convention(block) (NSString?, NSError?) -> Void

/// The reply block, made outside the main actor: the daemon calls it on its own queue.
private func bundleIDReply(into box: BundleIDReplyBox) -> BundleIDReply {
    return { id, error in
        box.answer(id.map { $0 as String }, error)
    }
}
