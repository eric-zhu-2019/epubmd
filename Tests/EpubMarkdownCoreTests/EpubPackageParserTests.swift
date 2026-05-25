import XCTest
@testable import EpubMarkdownCore

final class EpubPackageParserTests: XCTestCase {
    func testParsesContainerPackageManifestAndSpine() throws {
        let epub = try TestSupport.makeSyntheticEpub()
        let extract = try TestSupport.makeTempDirectory()
        try EpubArchive(epubURL: epub).extract(to: extract)

        let rootFile = try EpubPackageParser.parseContainer(at: extract)
        XCTAssertEqual(rootFile, "OEBPS/content.opf")
        let package = try EpubPackageParser.parsePackage(at: extract, rootFilePath: rootFile)
        XCTAssertEqual(package.title, "Sample Book")
        XCTAssertEqual(package.metadata.creators, ["Jane Author"])
        XCTAssertEqual(package.metadata.language, "en")
        XCTAssertEqual(package.metadata.publisher, "Example Press")
        XCTAssertEqual(package.metadata.date, "2026")
        XCTAssertEqual(package.metadata.identifier, "urn:isbn:0000000000")
        XCTAssertEqual(package.spine.map { $0.item.absolutePath }, ["OEBPS/chapter1.xhtml", "OEBPS/chapter2.xhtml"])
        XCTAssertEqual(package.manifest["img"]?.absolutePath, "OEBPS/images/pic.png")
    }

    func testMissingContainerFailsClearly() throws {
        let epub = try TestSupport.makeSyntheticEpub(missingContainer: true)
        let extract = try TestSupport.makeTempDirectory()
        try EpubArchive(epubURL: epub).extract(to: extract)
        XCTAssertThrowsError(try EpubPackageParser.parseContainer(at: extract)) { error in
            guard case ConversionError.invalidEpub(let message) = error else { return XCTFail("Unexpected error: \(error)") }
            XCTAssertTrue(message.contains("container.xml"))
        }
    }
}
