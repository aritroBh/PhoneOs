import Foundation
#if canImport(Glibc)
import Glibc
#elseif canImport(Darwin)
import Darwin
#endif
import JarvisCore
import Testing
@testable import JarvisKit

/// The work `onSmallStack` runs (kept alive by the caller until the thread has exited).
private final class SmallStackJob {
    let body: () -> Void
    init(_ body: @escaping () -> Void) { self.body = body }
}

/// Runs `body` on a thread whose stack is `stackSize` bytes (an iOS or macOS secondary
/// thread has 512 KB; Linux threads default far larger, which is why run 151's overflow was
/// green here) and joins it. Only `body` runs on that stack: the thread is a bare pthread,
/// joined before this returns, and the closure — with everything it captured — is released
/// here, on the caller's thread, after the join. (With a Foundation `Thread` the block was
/// released by the thread's own TSD destructor when it exited, on the small stack, racing
/// the test: when the test had already dropped its references, a 10,000-level input
/// captured by the block was released there, recursively, and overflowed it — the SIGSEGV
/// of the pass-31 gate, `Box.deinit` → `RawElement` value witness `destroy` → 2,950 levels
/// of `_ContiguousArrayStorage.deinit`, under `Foundation.Thread.deinit` from
/// `__nptl_deallocate_tsd`.)
func onSmallStack(_ stackSize: Int = 512 * 1024, _ body: @escaping () -> Void) {
    let job = Unmanaged.passRetained(SmallStackJob(body)).toOpaque()
    var attr = pthread_attr_t()
    pthread_attr_init(&attr)
    pthread_attr_setstacksize(&attr, stackSize)
    #if os(Linux)
    var thread = pthread_t()
    let rc = pthread_create(&thread, &attr, { raw in
        if let raw { Unmanaged<SmallStackJob>.fromOpaque(raw).takeUnretainedValue().body() }
        return nil
    }, job)
    precondition(rc == 0, "pthread_create: \(rc)")
    pthread_join(thread, nil)
    #else
    var thread: pthread_t?
    let rc = pthread_create(&thread, &attr, { raw in
        Unmanaged<SmallStackJob>.fromOpaque(raw).takeUnretainedValue().body()
        return nil
    }, job)
    precondition(rc == 0, "pthread_create: \(rc)")
    if let thread { pthread_join(thread, nil) }
    #endif
    pthread_attr_destroy(&attr)
    Unmanaged<SmallStackJob>.fromOpaque(job).release()
}

/// The stack size of the calling thread, in bytes (`nil` when it cannot be read).
func currentStackSize() -> Int? {
    #if os(Linux)
    // `pthread_getattr_np` is a GNU extension Glibc's module does not export: looked up.
    typealias GetAttr = @convention(c) (pthread_t, UnsafeMutablePointer<pthread_attr_t>) -> Int32
    guard let symbol = dlsym(dlopen(nil, RTLD_NOW), "pthread_getattr_np") else { return nil }
    let getattr = unsafeBitCast(symbol, to: GetAttr.self)
    var attr = pthread_attr_t()
    guard getattr(pthread_self(), &attr) == 0 else { return nil }
    defer { pthread_attr_destroy(&attr) }
    var size = 0
    guard pthread_attr_getstacksize(&attr, &size) == 0 else { return nil }
    return size
    #else
    return pthread_get_stacksize_np(pthread_self())
    #endif
}

/// A value handed between the test's thread and the small-stack thread.
final class Box<T>: @unchecked Sendable {
    var value: T
    init(_ value: T) { self.value = value }
}

/// Moves a deep input out of its box and releases it flat, on the caller's thread: a box
/// whose last reference goes elsewhere must never hold a deep tree.
func releaseFlat(_ box: Box<RawElement?>) {
    guard let tree = box.value else { return }
    box.value = nil
    releaseFlat(tree)
}

/// A chain of `levels` `RawElement`s under an `Application` (built iteratively).
func rawChain(_ levels: Int) -> RawElement {
    var node = RawElement(elementType: XCUIElementTypeCode.button, label: "Deep",
                          frame: Rect(x: 16, y: 100, width: 200, height: 44))
    for _ in 0..<max(levels - 2, 0) {
        node = RawElement(elementType: XCUIElementTypeCode.other, frame: Rect(x: 0, y: 0, width: 393, height: 852),
                          children: [node])
    }
    return RawElement(elementType: XCUIElementTypeCode.application, frame: Rect(x: 0, y: 0, width: 393, height: 852),
                      children: [node])
}

/// Releases a tree without recursion: every node is released after its children were moved
/// out (a nested Swift value tree is otherwise released recursively, one frame per level —
/// a 10,000-deep input would overflow the very stack the test protects).
func releaseFlat(_ root: consuming RawElement) {
    var work: [RawElement] = [consume root]
    while var e = work.popLast() {
        let kids = e.children
        e.children = []
        work.append(contentsOf: kids)
    }
}

/// The same for a typed tree.
func releaseFlat(_ root: consuming UiNode) {
    var work: [UiNode] = [consume root]
    while var e = work.popLast() {
        let kids = e.children
        e.children = []
        work.append(contentsOf: kids)
    }
}

/// Levels of a typed tree (iterative).
func treeDepth(_ n: UiNode) -> Int {
    var best = 0
    var stack: [(UiNode, Int)] = [(n, 1)]
    while let (e, d) = stack.popLast() {
        best = max(best, d)
        stack.append(contentsOf: e.children.map { ($0, d + 1) })
    }
    return best
}

/// Levels of a raw tree (iterative), and whether an element says its children were cut.
func rawDepth(_ n: RawElement) -> (Int, Bool) {
    var best = 0
    var cut = false
    var stack: [(RawElement, Int)] = [(n, 1)]
    while let (e, d) = stack.popLast() {
        best = max(best, d)
        cut = cut || e.childrenCut
        stack.append(contentsOf: e.children.map { ($0, d + 1) })
    }
    return (best, cut)
}

/// Review pass 31 (L1, and run 151's crash): one depth bound, the core's
/// (`jarvis_proto::MAX_TREE_DEPTH`, 160 levels, the root being level 1). The first copy
/// (`RawElement.copyTree`, which `SnapshotCopy` uses) and `UiTreeBuilder` cut there and flag
/// the cut (`childrenCut`, `Result.treeCut`, `RunnerObservation.treeCut`, the FFI
/// `Screen.treeCut`); every walk of the shell is iterative or sees at most that many levels,
/// on a 512 KB stack.
@Suite("Depth bound: the shell's copies cut at the core's bound and flag it, on a 512 KB stack")
struct DepthBoundTests {
    @Test("the bound is the core's")
    func bound() {
        #expect(UiTreeBuilder.maxDepth == 160)
    }

    @Test("the small stack is what it says (and the test's own thread is recorded)")
    func stackSizes() async {
        let small = Box<Int?>(nil)
        onSmallStack { small.value = currentStackSize() }
        print("DepthBoundTests: stack of a test task's thread \(currentStackSize().map(String.init) ?? "?") bytes, of onSmallStack's \(small.value.map(String.init) ?? "?")")
        if let size = small.value { #expect(size <= 512 * 1024 + 64 * 1024) }
    }

    @Test("UiTreeBuilder at the bound keeps every level and flags nothing; past it cuts and flags")
    func builderAtAndPastTheBound() {
        let at = Box<RawElement?>(rawChain(160))
        let over = Box<RawElement?>(rawChain(161))
        let out = Box<[(Int, Bool)]>([])
        onSmallStack {
            for input in [at.value, over.value].compactMap({ $0 }) {
                let built = UiTreeBuilder.build(input, screenSize: nil)
                out.value.append((treeDepth(built.root), built.treeCut))
                releaseFlat(built.root)
            }
        }
        releaseFlat(at)
        releaseFlat(over)
        #expect(out.value.count == 2)
        #expect(out.value.first?.0 == 160 && out.value.first?.1 == false)
        #expect(out.value.last?.0 == 160 && out.value.last?.1 == true)
    }

    @Test("10,000 levels: the builder, the copy, the dumps and the counts run on a 512 KB stack")
    func tenThousandLevelsOnASmallStack() {
        let deep = Box<RawElement?>(rawChain(10_000))
        let result = Box<(depth: Int, cut: Bool, copyDepth: Int, copyCut: Bool, lines: Int, count: Int,
                          rebuiltCut: Bool)?>(nil)
        onSmallStack {
            guard let input = deep.value else { return }
            let built = UiTreeBuilder.build(input, screenSize: Size(width: 393, height: 852))
            // The first copy, from a source 10,000 levels deep (an index chain: no input tree).
            let copied = RawElement.copyTree(
                0, children: { $0 < 9_999 ? [$0 + 1] : [] },
                element: { RawElement(elementType: XCUIElementTypeCode.other, label: "L\($0)",
                                      frame: Rect(x: 0, y: 0, width: 393, height: 852)) }
            )
            let (copyDepth, copyCut) = rawDepth(copied)
            let lines = UiTreeBuilder.dump(built.root).split(separator: "\n").count
                + UiTreeBuilder.dump(copied).split(separator: "\n").count
            let count = copied.count + Transcript.countNodes(built.root)
            // A walk of the copy as the runner does next: the copy builds at the bound.
            let rebuilt = UiTreeBuilder.build(copied, screenSize: nil)
            result.value = (treeDepth(built.root), built.treeCut, copyDepth, copyCut, lines, count,
                            rebuilt.treeCut)
            releaseFlat(built.root)
            releaseFlat(rebuilt.root)
            releaseFlat(copied)
        }
        // The input is released here, flat, on the test's own thread.
        releaseFlat(deep)
        let r = result.value
        #expect(r?.depth == 160)
        #expect(r?.cut == true)
        #expect(r?.copyDepth == 160)
        #expect(r?.copyCut == true)
        #expect(r?.lines == 320)
        #expect(r?.count == 320)
        #expect(r?.rebuiltCut == true, "the copy's cut reaches the builder's result")
    }

    @Test("a cut tree crosses the FFI flagged and the core reads an incomplete screen, on a 512 KB stack")
    func throughTheCoreOnASmallStack() throws {
        // Twice the bound proves the cut (the 10,000-level case is the one above).
        let raw = Box<RawElement?>(rawChain(2 * UiTreeBuilder.maxDepth))
        let prompt = Box<String?>(nil)
        let digest = Box<String?>(nil)
        let flagged = Box(false)
        onSmallStack {
            guard let input = raw.value else { return }
            let built = UiTreeBuilder.build(input, screenSize: Size(width: 393, height: 852))
            let obs = RunnerObservation(root: built.root, appBundleId: "com.example.shop", appName: "Shop",
                                        screenSize: Size(width: 393, height: 852), treeCut: built.treeCut)
            // The FFI lowering and lifting of a 160-deep tree (generated, recursive) and the
            // core's conversion and compaction, all on this thread.
            if case .observed(let observation) = obs.event(screenshot: nil) {
                flagged.value = observation.screen.treeCut
                digest.value = try? screenDigestHex(screen: observation.screen)
            }
            guard let config = try? SessionConfigBuilder.make(
                capabilities: ModelCapabilities(contextTokens: 8_000, supportsVision: false), agentBundleIds: []
            ), let session = try? JarvisSession(config: config, task: TaskSpec(id: "t", text: "x"), memoryJson: "")
            else { return }
            _ = session.step(nowMs: 1, event: .start)
            let effect = session.step(nowMs: 2, event: obs.event(screenshot: nil))
            if case .infer(let request) = effect { prompt.value = request.user }
        }
        releaseFlat(raw)
        #expect(flagged.value)
        #expect(digest.value != nil)
        let user = try #require(prompt.value)
        #expect(user.contains("deeper than Jarvis can read"), "\(user.suffix(400))")
    }

    @Test("runner wire records carry the flag both ways")
    func wireCarriesTheFlag() {
        let built = UiTreeBuilder.build(rawChain(200), screenSize: nil)
        let obs = RunnerObservation(root: built.root, appBundleId: "a", screenSize: Size(width: 1, height: 1),
                                    treeCut: built.treeCut)
        let wire = RunnerWireSnapshot(obs)
        #expect(wire?.treeCut == true)
        #expect(wire?.observation.treeCut == true)
        let plain = RunnerWireSnapshot(RunnerObservation(root: UiTreeBuilder.build(rawChain(3), screenSize: nil).root,
                                                         appBundleId: "a", screenSize: Size(width: 1, height: 1)))
        #expect(plain?.treeCut == false)
    }

    @Test("the runner's first copy, in SnapshotCopy's shape (main actor, an existential snapshot), cuts and flags")
    @MainActor
    func snapshotCopyShape() {
        var node: any FakeSnapshot = FakeNode(label: "Deep", children: [])
        for i in 0..<200 { node = FakeNode(label: "L\(i)", children: [node]) }
        let copied = FakeSnapshotCopy.copy(node)
        let (depth, cut) = rawDepth(copied)
        #expect(depth == UiTreeBuilder.maxDepth)
        #expect(cut)
        #expect(UiTreeBuilder.build(copied, screenSize: nil).treeCut)
    }
}

/// A stand-in for `XCUIElementSnapshot` (an Objective-C protocol of XCTest, Mac only), so the
/// shape of `SnapshotCopy.copy` (Shell/App/RunnerXCUITest) compiles and runs here.
protocol FakeSnapshot {
    var label: String { get }
    var children: [any FakeSnapshot] { get }
}

final class FakeNode: FakeSnapshot {
    let label: String
    let children: [any FakeSnapshot]
    init(label: String, children: [any FakeSnapshot]) {
        self.label = label
        self.children = children
    }
}

/// `SnapshotCopy`, line for line, over the stand-in.
enum FakeSnapshotCopy {
    private struct Node {
        let snapshot: any FakeSnapshot
    }

    @MainActor
    static func copy(_ snapshot: any FakeSnapshot) -> RawElement {
        RawElement.copyTree(
            Node(snapshot: snapshot),
            maxDepth: UiTreeBuilder.maxDepth,
            children: { (n: Node) -> [Node] in n.snapshot.children.map { Node(snapshot: $0) } },
            element: { (n: Node) -> RawElement in one(n.snapshot) }
        )
    }

    @MainActor
    private static func one(_ snapshot: any FakeSnapshot) -> RawElement {
        RawElement(elementType: XCUIElementTypeCode.other, label: snapshot.label,
                   frame: Rect(x: 0, y: 0, width: 393, height: 852))
    }
}
