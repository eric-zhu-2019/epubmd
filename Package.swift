// swift-tools-version:5.5
import PackageDescription

let package = Package(
    name: "EpubMarkdown",
    platforms: [.macOS(.v12)],
    products: [
        .library(name: "EpubMarkdownCore", targets: ["EpubMarkdownCore"]),
        .executable(name: "EpubMarkdownApp", targets: ["EpubMarkdownApp"])
    ],
    targets: [
        .target(name: "EpubMarkdownCore", dependencies: []),
        .executableTarget(name: "EpubMarkdownApp", dependencies: ["EpubMarkdownCore"]),
        .testTarget(name: "EpubMarkdownCoreTests", dependencies: ["EpubMarkdownCore"])
    ]
)
