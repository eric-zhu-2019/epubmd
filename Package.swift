// swift-tools-version:5.5
import PackageDescription

let package = Package(
    name: "epubmd",
    platforms: [.macOS(.v12)],
    products: [
        .library(name: "EpubMarkdownCore", targets: ["EpubMarkdownCore"]),
        .executable(name: "epubmd", targets: ["epubmd"])
    ],
    targets: [
        .target(name: "EpubMarkdownCore", dependencies: []),
        .executableTarget(name: "epubmd", dependencies: ["EpubMarkdownCore"]),
        .testTarget(name: "EpubMarkdownCoreTests", dependencies: ["EpubMarkdownCore"])
    ]
)
