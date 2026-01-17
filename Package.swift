// swift-tools-version: 6.0
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "FlowWhispr",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(
            name: "FlowWhispr",
            targets: ["FlowWhispr"]
        ),
        .executable(
            name: "FlowWhisprApp",
            targets: ["FlowWhisprApp"]
        ),
    ],
    targets: [
        // C wrapper for the Rust FFI
        .target(
            name: "CFlowWhispr",
            path: "Sources/CFlowWhispr",
            publicHeadersPath: "include",
            linkerSettings: [
                // Link to the Rust static library
                .unsafeFlags([
                    "-L", "flowwhispr-core/target/debug",
                    "-lflowwhispr_core"
                ]),
                // System frameworks needed by the Rust library
                .linkedFramework("CoreAudio"),
                .linkedFramework("AudioToolbox"),
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
            ]
        ),
        // Swift wrapper
        .target(
            name: "FlowWhispr",
            dependencies: ["CFlowWhispr"],
            path: "Sources/FlowWhispr"
        ),
        // macOS App
        .executableTarget(
            name: "FlowWhisprApp",
            dependencies: [
                "FlowWhispr",
            ],
            path: "Sources/FlowWhisprApp"
        ),
    ]
)
