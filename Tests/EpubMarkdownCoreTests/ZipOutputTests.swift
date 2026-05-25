import XCTest
@testable import EpubMarkdownCore

final class ZipOutputTests: XCTestCase {
    func testConvertToZipWritesReadableArchiveAtRequestedPath() throws {
        let epub = try TestSupport.makeSyntheticEpub()
        let outputParent = try TestSupport.makeTempDirectory("zip-output")
        let zipURL = outputParent.appendingPathComponent("sample-md.zip")

        let written = try EpubConverter().convertToZip(epubURL: epub, outputZipURL: zipURL)

        XCTAssertEqual(written, zipURL)
        XCTAssertTrue(FileManager.default.fileExists(atPath: zipURL.path))
        try assertZipIsValid(zipURL)

        let expanded = outputParent.appendingPathComponent("expanded", isDirectory: true)
        try unzip(zipURL, to: expanded)
        let readme = try String(contentsOf: expanded.appendingPathComponent("README.md"))
        XCTAssertTrue(readme.contains("# Sample Book"))
        XCTAssertTrue(FileManager.default.fileExists(atPath: expanded.appendingPathComponent("style.css").path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: expanded.appendingPathComponent("chapters/001-Opening.md").path))
        XCTAssertFalse((try FileManager.default.contentsOfDirectory(atPath: outputParent.path)).contains { $0.hasPrefix(".epubmd-zip-") })
    }

    func testConvertToZipRefusesExistingDestinationUnlessOverwriteIsEnabled() throws {
        let epub = try TestSupport.makeSyntheticEpub()
        let outputParent = try TestSupport.makeTempDirectory("zip-overwrite")
        let zipURL = outputParent.appendingPathComponent("sample-md.zip")
        try "old".write(to: zipURL, atomically: true, encoding: .utf8)

        XCTAssertThrowsError(try EpubConverter().convertToZip(epubURL: epub, outputZipURL: zipURL))
        XCTAssertNoThrow(try EpubConverter().convertToZip(epubURL: epub, outputZipURL: zipURL, overwrite: true))
        try assertZipIsValid(zipURL)
    }

    private func assertZipIsValid(_ url: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/unzip")
        process.arguments = ["-t", url.path]
        try process.run()
        process.waitUntilExit()
        XCTAssertEqual(process.terminationStatus, 0)
    }

    private func unzip(_ zipURL: URL, to destination: URL) throws {
        try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true, attributes: nil)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/unzip")
        process.arguments = ["-q", zipURL.path, "-d", destination.path]
        try process.run()
        process.waitUntilExit()
        XCTAssertEqual(process.terminationStatus, 0)
    }
}
