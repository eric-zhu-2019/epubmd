import Foundation
import XCTest
@testable import EpubMarkdownCore

final class TestSupport {
    static func makeTempDirectory(_ name: String = UUID().uuidString) throws -> URL {
        let url = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("EpubMarkdownTests-").appendingPathComponent(name, isDirectory: true)
        try? FileManager.default.removeItem(at: url)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true, attributes: nil)
        return url
    }

    static func write(_ string: String, to url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true, attributes: nil)
        try string.data(using: .utf8)!.write(to: url)
    }

    static func makeSyntheticEpub(includeEncryption: Bool = false, missingContainer: Bool = false, malformedContainer: Bool = false, missingOPF: Bool = false, missingSpineReference: Bool = false, missingAsset: Bool = false) throws -> URL {
        let root = try makeTempDirectory()
        let epubRoot = root.appendingPathComponent("book", isDirectory: true)
        try FileManager.default.createDirectory(at: epubRoot, withIntermediateDirectories: true, attributes: nil)
        if !missingContainer {
            if malformedContainer {
                try write("<container><rootfiles>", to: epubRoot.appendingPathComponent("META-INF/container.xml"))
            } else {
                try write("""
                <?xml version="1.0" encoding="UTF-8"?>
                <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
                  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
                </container>
                """, to: epubRoot.appendingPathComponent("META-INF/container.xml"))
            }
        }
        if includeEncryption {
            try write("<encryption></encryption>", to: epubRoot.appendingPathComponent("META-INF/encryption.xml"))
        }
        if !missingOPF {
            let secondSpineRef = missingSpineReference ? "missing" : "chap2"
            try write("""
            <?xml version="1.0" encoding="UTF-8"?>
            <package xmlns:dc="http://purl.org/dc/elements/1.1/">
              <metadata>
                <dc:title>Sample Book</dc:title>
                <dc:creator>Jane Author</dc:creator>
                <dc:language>en</dc:language>
                <dc:publisher>Example Press</dc:publisher>
                <dc:date>2026</dc:date>
                <dc:identifier>urn:isbn:0000000000</dc:identifier>
              </metadata>
              <manifest>
                <item id="chap1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
                <item id="chap2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
                <item id="img" href="images/pic.png" media-type="image/png"/>
              </manifest>
              <spine>
                <itemref idref="chap1"/>
                <itemref idref="\(secondSpineRef)"/>
              </spine>
            </package>
            """, to: epubRoot.appendingPathComponent("OEBPS/content.opf"))
        }
        try write("""
        <?xml version="1.0" encoding="UTF-8"?>
        <html xmlns="http://www.w3.org/1999/xhtml"><body>
          <h1>Opening</h1>
          <p>Hello <em>reader</em>. Visit <a href="https://example.com">Example</a> and <a href="chapter2.xhtml#next">next chapter</a>.</p>
          <ul><li>One</li><li>Two</li></ul>
          <p><img src="images/pic.png" alt="Picture"/></p>
        </body></html>
        """, to: epubRoot.appendingPathComponent("OEBPS/chapter1.xhtml"))
        try write("""
        <?xml version="1.0" encoding="UTF-8"?>
        <html xmlns="http://www.w3.org/1999/xhtml"><body>
          <h1 id="next">Second</h1>
          <p>Second paragraph.</p>
        </body></html>
        """, to: epubRoot.appendingPathComponent("OEBPS/chapter2.xhtml"))
        if !missingAsset {
            let imageData = Data([0x89, 0x50, 0x4E, 0x47])
            let imageURL = epubRoot.appendingPathComponent("OEBPS/images/pic.png")
            try FileManager.default.createDirectory(at: imageURL.deletingLastPathComponent(), withIntermediateDirectories: true, attributes: nil)
            try imageData.write(to: imageURL)
        }
        let epub = root.appendingPathComponent("sample.epub")
        let process = Process()
        process.launchPath = "/usr/bin/zip"
        process.arguments = ["-qr", epub.path, "."]
        process.currentDirectoryURL = epubRoot
        process.launch()
        process.waitUntilExit()
        XCTAssertEqual(process.terminationStatus, 0)
        return epub
    }
}
