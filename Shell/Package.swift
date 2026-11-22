// swift-tools-version:6.0
//
// JarvisKit: the non-UI Swift shell around the Rust core (see README.md).
//
// Targets:
//   JarvisCoreFFI   the C module of the UniFFI scaffolding (header + module map)
//   JarvisCore      the generated UniFFI Swift bindings (Sources/JarvisCore/JarvisCore.swift)
//   JarvisKit       the shell: SessionDriver, runner/model clients, stores, tools
//   JarvisKitTests  tests (drive the real Rust core)
//
// Generated files are not committed: run scripts/bootstrap.sh first. How JarvisCoreFFI is
// provided depends on the host that evaluates this manifest:
//   - Linux, or macOS with JARVIS_LINK=host: a system-library module over
//     Generated/JarvisCoreFFI and the host static library Generated/lib/libjarvis_core_ffi.a,
//     linked with -L (an unsafe flag; acceptable for this dev/test path only).
//   - macOS (default): the binary target Frameworks/JarvisCoreFFI.xcframework (iOS device +
//     simulator) built by core/jarvis-ffi/scripts/build-xcframework.sh via bootstrap.sh.
//     That xcframework has no macOS slice, so run `JARVIS_LINK=host swift test` on a Mac.

import Foundation
import PackageDescription

let root = Context.packageDirectory
let fileManager = FileManager.default

#if os(Linux)
let linkXCFramework = false
#else
let linkXCFramework = Context.environment["JARVIS_LINK"] != "host"
#endif

let bindingsFile = "\(root)/Sources/JarvisCore/JarvisCore.swift"
let hostModuleMap = "\(root)/Generated/JarvisCoreFFI/module.modulemap"
let hostLibrary = "\(root)/Generated/lib/libjarvis_core_ffi.a"
let xcframeworkPath = "Frameworks/JarvisCoreFFI.xcframework"

let generatedPresent: Bool = {
    guard fileManager.fileExists(atPath: bindingsFile) else { return false }
    if linkXCFramework {
        return fileManager.fileExists(atPath: "\(root)/\(xcframeworkPath)")
    }
    return fileManager.fileExists(atPath: hostModuleMap) && fileManager.fileExists(atPath: hostLibrary)
}()

let jarvisKitSwiftSettings: [SwiftSetting] = [
    .enableUpcomingFeature("ExistentialAny"),
]

var targets: [Target] = []

if generatedPresent {
    if linkXCFramework {
        targets.append(.binaryTarget(name: "JarvisCoreFFI", path: xcframeworkPath))
        targets.append(.target(name: "JarvisCore", dependencies: ["JarvisCoreFFI"], path: "Sources/JarvisCore"))
    } else {
        targets.append(.systemLibrary(name: "JarvisCoreFFI", path: "Generated/JarvisCoreFFI"))
        targets.append(.target(
            name: "JarvisCore",
            dependencies: ["JarvisCoreFFI"],
            path: "Sources/JarvisCore",
            linkerSettings: [
                .unsafeFlags(["-L\(root)/Generated/lib"]),
                .linkedLibrary("jarvis_core_ffi"),
            ]
        ))
    }
} else {
    // The bindings are missing: compile a stand-in whose only content is an #error that
    // tells the developer to run scripts/bootstrap.sh.
    targets.append(.target(name: "JarvisCore", path: "Sources/JarvisCoreMissing"))
}

targets.append(.target(
    name: "JarvisKit",
    dependencies: ["JarvisCore"],
    path: "Sources/JarvisKit",
    swiftSettings: jarvisKitSwiftSettings
))
targets.append(.testTarget(
    name: "JarvisKitTests",
    dependencies: ["JarvisKit", "JarvisCore"],
    path: "Tests/JarvisKitTests",
    swiftSettings: [.enableUpcomingFeature("ExistentialAny")]
))

let package = Package(
    name: "JarvisKit",
    platforms: [.iOS("26.0"), .macOS(.v15)],
    products: [
        .library(name: "JarvisKit", targets: ["JarvisKit"]),
        .library(name: "JarvisCore", targets: ["JarvisCore"]),
    ],
    targets: targets,
    swiftLanguageModes: [.v6]
)
