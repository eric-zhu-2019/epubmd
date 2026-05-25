import XCTest
@testable import EpubMarkdownCore

final class SyntheticEpubConversionTests: XCTestCase {
    func testFullConversionPreservesOrderLinksAndAssets() throws {
        let epub = try TestSupport.makeSyntheticEpub()
        let outputParent = try TestSupport.makeTempDirectory("output")
        let result = try EpubConverter().convert(epubURL: epub, outputParentDirectory: outputParent)

        XCTAssertEqual(result.outputDirectory.lastPathComponent, "Sample Book")
        let chapters = result.outputDirectory.appendingPathComponent("chapters")
        let first = chapters.appendingPathComponent("001-Opening.md")
        let second = chapters.appendingPathComponent("002-Second.md")
        XCTAssertTrue(FileManager.default.fileExists(atPath: first.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: second.path))
        let firstText = try String(contentsOf: first)
        XCTAssertTrue(firstText.contains("# Opening"))
        XCTAssertTrue(firstText.contains("- One"))
        XCTAssertTrue(firstText.contains("[Example](https://example.com)"))
        XCTAssertTrue(firstText.contains("[next chapter](002-chapter2.md#next)") || firstText.contains("[next chapter](002-Second.md#next)"))
        XCTAssertTrue(firstText.contains("![Picture](../assets/"))
        XCTAssertFalse(result.assetFiles.isEmpty)
        XCTAssertTrue(FileManager.default.fileExists(atPath: result.assetFiles[0].path))
        let readme = try String(contentsOf: result.outputDirectory.appendingPathComponent("README.md"))
        XCTAssertTrue(readme.contains("## Metadata"))
        XCTAssertTrue(readme.contains("- Author: Jane Author"))
        XCTAssertTrue(readme.contains("- Publisher: Example Press"))
        XCTAssertTrue(readme.contains("- [Opening](chapters/001-Opening.md)"))
        XCTAssertTrue(readme.contains("use `style.css`"))
        let style = try String(contentsOf: result.outputDirectory.appendingPathComponent("style.css"))
        XCTAssertTrue(style.contains("color: #1f2937;"))
        XCTAssertTrue(style.contains("max-width: 78ch;"))
    }

    func testConversionCopiesExistingReferencedAssetWhenManifestOmitsIt() throws {
        let epub = try TestSupport.makeSyntheticEpub(declareAsset: false)
        let outputParent = try TestSupport.makeTempDirectory("undeclared-asset")
        let result = try EpubConverter().convert(epubURL: epub, outputParentDirectory: outputParent)

        let first = result.outputDirectory.appendingPathComponent("chapters/001-Opening.md")
        let firstText = try String(contentsOf: first)
        XCTAssertTrue(firstText.contains("![Picture](../assets/"))
        XCTAssertEqual(result.assetFiles.count, 1)
        XCTAssertTrue(FileManager.default.fileExists(atPath: result.assetFiles[0].path))
    }

    func testExistingDestinationCreatesUniqueFolder() throws {
        let epub = try TestSupport.makeSyntheticEpub()
        let outputParent = try TestSupport.makeTempDirectory("collision")
        try FileManager.default.createDirectory(at: outputParent.appendingPathComponent("Sample Book"), withIntermediateDirectories: true, attributes: nil)
        let result = try EpubConverter().convert(epubURL: epub, outputParentDirectory: outputParent)
        XCTAssertEqual(result.outputDirectory.lastPathComponent, "Sample Book 2")
    }

    func testProtectedEpubFailsWithoutOutputSuccess() throws {
        let epub = try TestSupport.makeSyntheticEpub(includeEncryption: true)
        let outputParent = try TestSupport.makeTempDirectory("protected")
        XCTAssertThrowsError(try EpubConverter().convert(epubURL: epub, outputParentDirectory: outputParent)) { error in
            guard case ConversionError.protectedEpub(let message) = error else { return XCTFail("Unexpected error: \(error)") }
            XCTAssertTrue(message.contains("encryption.xml"))
        }
        let visible = (try? FileManager.default.contentsOfDirectory(atPath: outputParent.path).filter { !$0.hasPrefix(".") }) ?? []
        XCTAssertTrue(visible.isEmpty)
    }
}
