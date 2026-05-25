import XCTest
@testable import EpubMarkdownCore

final class ErrorHandlingTests: XCTestCase {
    func testInvalidZipFailsClearly() throws {
        let dir = try TestSupport.makeTempDirectory("invalidzip")
        let invalid = dir.appendingPathComponent("bad.epub")
        try "not a zip".data(using: .utf8)!.write(to: invalid)
        XCTAssertThrowsError(try EpubConverter().convert(epubURL: invalid, outputParentDirectory: dir)) { error in
            guard case ConversionError.invalidEpub = error else { return XCTFail("Unexpected error: \(error)") }
        }
    }

    func testMalformedContainerFailsClearly() throws {
        let epub = try TestSupport.makeSyntheticEpub(malformedContainer: true)
        let out = try TestSupport.makeTempDirectory("malformedcontainer")
        XCTAssertThrowsError(try EpubConverter().convert(epubURL: epub, outputParentDirectory: out)) { error in
            guard case ConversionError.malformedEpub = error else { return XCTFail("Unexpected error: \(error)") }
        }
    }

    func testMissingOPFFailsClearly() throws {
        let epub = try TestSupport.makeSyntheticEpub(missingOPF: true)
        let out = try TestSupport.makeTempDirectory("missingopf")
        XCTAssertThrowsError(try EpubConverter().convert(epubURL: epub, outputParentDirectory: out)) { error in
            guard case ConversionError.malformedEpub(let message) = error else { return XCTFail("Unexpected error: \(error)") }
            XCTAssertTrue(message.contains("OPF"))
        }
    }

    func testMissingSpineReferenceFailsClearly() throws {
        let epub = try TestSupport.makeSyntheticEpub(missingSpineReference: true)
        let out = try TestSupport.makeTempDirectory("missingspine")
        XCTAssertThrowsError(try EpubConverter().convert(epubURL: epub, outputParentDirectory: out)) { error in
            guard case ConversionError.malformedEpub(let message) = error else { return XCTFail("Unexpected error: \(error)") }
            XCTAssertTrue(message.contains("spine references"))
        }
    }

    func testMissingReferencedAssetFailsAndLeavesNoVisibleOutput() throws {
        let epub = try TestSupport.makeSyntheticEpub(missingAsset: true)
        let out = try TestSupport.makeTempDirectory("missingasset")
        XCTAssertThrowsError(try EpubConverter().convert(epubURL: epub, outputParentDirectory: out)) { error in
            guard case ConversionError.malformedEpub(let message) = error else { return XCTFail("Unexpected error: \(error)") }
            XCTAssertTrue(message.contains("image asset"))
        }
        let visible = try FileManager.default.contentsOfDirectory(atPath: out.path).filter { !$0.hasPrefix(".") }
        XCTAssertTrue(visible.isEmpty)
    }
}
