// swift-tools-version: 6.0
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "FlowWispr",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(
            name: "FlowWispr",
            targets: ["FlowWispr"]
        ),
        .executable(
            name: "FlowWisprApp",
            targets: ["FlowWisprApp"]
        ),
    ],
    dependencies: [
        .package(url: "https://github.com/amplitude/Amplitude-iOS", from: "8.0.0")
    ],
    targets: [
        // C wrapper for the Rust FFI
        .target(
            name: "CFlowWispr",
            path: "Sources/CFlowWispr",
            publicHeadersPath: "include",
            linkerSettings: [
                // Link to the Rust static library
                .unsafeFlags([
                    "-L", "flowwispr-core/target/debug",
                    "-lflowwispr_core"
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
            name: "FlowWispr",
            dependencies: ["CFlowWispr"],
            path: "Sources/FlowWispr"
        ),
        // macOS App
        .executableTarget(
            name: "FlowWisprApp",
            dependencies: [
                "FlowWispr",
                .product(name: "Amplitude", package: "Amplitude-iOS"),
            ],
            path: "Sources/FlowWisprApp",
            resources: [
                .copy("../../menubar.svg"),
            ]
        ),
    ]
)
