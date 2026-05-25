import XCTest
@testable import EpubMarkdownCore

final class AssetMapperTests: XCTestCase {
    func testAssetNameCollisionsBecomeUnique() throws {
        let extracted = try TestSupport.makeTempDirectory("assets-src")
        let output = try TestSupport.makeTempDirectory("assets-out")
        try TestSupport.write("a", to: extracted.appendingPathComponent("OEBPS/images/pic.png"))
        try TestSupport.write("b", to: extracted.appendingPathComponent("OEBPS/other/pic.png"))
        let package = EpubPackage(rootFilePath: "OEBPS/content.opf", baseDirectory: "OEBPS", title: "T", manifest: [
            "a": ManifestItem(id: "a", href: "images/pic.png", mediaType: "image/png", absolutePath: "OEBPS/images/pic.png"),
            "b": ManifestItem(id: "b", href: "other/pic.png", mediaType: "image/png", absolutePath: "OEBPS/other/pic.png")
        ], spine: [])
        var mapper = AssetMapper()
        try mapper.copyAssets(for: package, extractedRoot: extracted, outputRoot: output)
        let names = mapper.copiedAssets.map { $0.lastPathComponent }
        XCTAssertEqual(Set(names).count, 2)
        XCTAssertEqual(mapper.copiedAssets.count, 2)
    }
}
