// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import Foundation
import PackageDescription

let packageRoot = URL(fileURLWithPath: #file).deletingLastPathComponent().path
let debugLibPath = "\(packageRoot)/flow-core/target/debug/libflow_core.a"
let releaseLibPath = "\(packageRoot)/flow-core/target/release/libflow_core.a"
let buildConfiguration = (ProcessInfo.processInfo.environment["SWIFT_BUILD_CONFIGURATION"]
    ?? ProcessInfo.processInfo.environment["CONFIGURATION"])?.lowercased()
let preferRelease = buildConfiguration == "release"
let rustLibPath: String = {
    let fileManager = FileManager.default
    if preferRelease {
        if fileManager.fileExists(atPath: releaseLibPath) {
            return releaseLibPath
        }
        return debugLibPath
    }
    if fileManager.fileExists(atPath: debugLibPath) {
        return debugLibPath
    }
    return releaseLibPath
}()

let package = Package(
    name: "Flow",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(
            name: "Flow",
            targets: ["Flow"]
        ),
        .executable(
            name: "Flow",
            targets: ["FlowApp"]
        ),
    ],
    dependencies: [
        .package(url: "https://github.com/amplitude/Amplitude-iOS", from: "8.0.0")
    ],
    targets: [
        // C wrapper for the Rust FFI
        .target(
            name: "CFlow",
            path: "Sources/CFlowWispr",
            publicHeadersPath: "include",
            linkerSettings: [
                // Link to the Rust static library
                .unsafeFlags([rustLibPath]),
                // System frameworks needed by the Rust library
                .linkedFramework("CoreAudio"),
                .linkedFramework("AudioToolbox"),
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                // Metal frameworks for Candle GPU acceleration
                .linkedFramework("Metal"),
                .linkedFramework("MetalKit"),
                .linkedFramework("MetalPerformanceShaders"),
                .linkedFramework("Accelerate"),
                .linkedFramework("Foundation"),
            ]
        ),
        // Swift wrapper
        .target(
            name: "Flow",
            dependencies: ["CFlow"],
            path: "Sources/FlowWispr"
        ),
        // macOS App
        .executableTarget(
            name: "FlowApp",
            dependencies: [
                "Flow",
                .product(name: "Amplitude", package: "Amplitude-iOS"),
            ],
            path: "Sources/FlowWisprApp",
            resources: [
                .process("Resources"),
                .copy("../../menubar.svg"),
            ]
        ),
    ]
)
