// swift-tools-version:5.5
import PackageDescription

let package = Package(
    name: "epubmd-swift-core",
    platforms: [.macOS(.v12)],
    products: [
        .library(name: "EpubMarkdownCore", targets: ["EpubMarkdownCore"])
    ],
    targets: [
        .target(name: "EpubMarkdownCore", dependencies: []),
        .testTarget(name: "EpubMarkdownCoreTests", dependencies: ["EpubMarkdownCore"])
    ]
)
