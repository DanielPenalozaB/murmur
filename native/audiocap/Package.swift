// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "audiocap",
    platforms: [.macOS("14.4")],
    targets: [
        .executableTarget(
            name: "audiocap",
            path: "Sources/audiocap",
            swiftSettings: [.swiftLanguageMode(.v5)]
        )
    ]
)
