import XCTest
@testable import EpubMarkdownCore

final class HTMLToMarkdownConverterTests: XCTestCase {
    func testConvertsCommonReadingElements() throws {
        let html = """
        <html><body><h1>Title</h1><h2>Sub</h2><h3>Third</h3><h4>Fourth</h4><h5>Fifth</h5><h6>Sixth</h6><p>Hello <strong>bold</strong> <a href="https://example.com">link</a>.</p><ol><li>First <em>ordered</em> item</li><li>Second</li></ol><ul><li>Bullet <strong>bold</strong> item</li></ul><aside>Unsupported <span>tag</span></aside><p><img src="images/pic.png" alt="Pic"/></p></body></html>
        """.data(using: .utf8)!
        var mapper = AssetMapper()
        let package = EpubPackage(rootFilePath: "OEBPS/content.opf", baseDirectory: "OEBPS", title: "T", manifest: ["img": ManifestItem(id: "img", href: "images/pic.png", mediaType: "image/png", absolutePath: "OEBPS/images/pic.png")], spine: [])
        let extracted = try TestSupport.makeTempDirectory()
        let out = try TestSupport.makeTempDirectory()
        try TestSupport.write("png", to: extracted.appendingPathComponent("OEBPS/images/pic.png"))
        try mapper.copyAssets(for: package, extractedRoot: extracted, outputRoot: out)
        let markdown = try HTMLToMarkdownConverter().convert(data: html, currentEpubPath: "OEBPS/chapter.xhtml", assetMapper: mapper, chapterLinks: ChapterLinkMap(epubPathToMarkdown: [:]), packageBase: "OEBPS")
        XCTAssertTrue(markdown.contains("# Title"))
        XCTAssertTrue(markdown.contains("## Sub"))
        XCTAssertTrue(markdown.contains("### Third"))
        XCTAssertTrue(markdown.contains("#### Fourth"))
        XCTAssertTrue(markdown.contains("##### Fifth"))
        XCTAssertTrue(markdown.contains("###### Sixth"))
        XCTAssertTrue(markdown.contains("Hello **bold** [link](https://example.com)."))
        XCTAssertTrue(markdown.contains("[link](https://example.com)"))
        XCTAssertTrue(markdown.contains("1. First *ordered* item"))
        XCTAssertTrue(markdown.contains("2. Second"))
        XCTAssertTrue(markdown.contains("- Bullet **bold** item"))
        XCTAssertTrue(markdown.contains("Unsupported tag"))
        XCTAssertTrue(markdown.contains("![Pic](../assets/"))
    }
}

extension HTMLToMarkdownConverterTests {
    func testRemovesDuplicateLeadingChapterTitle() throws {
        let html = """
        <html><body><h1 id="chapter7">第零七章 • 计算</h1><p>第零七章 • 计算</p><p>正文开始。</p></body></html>
        """.data(using: .utf8)!
        let markdown = try HTMLToMarkdownConverter().convert(data: html, currentEpubPath: "OEBPS/chapter.xhtml", assetMapper: AssetMapper(), chapterLinks: ChapterLinkMap(epubPathToMarkdown: [:]), packageBase: "OEBPS")

        XCTAssertTrue(markdown.contains("# 第零七章 • 计算"))
        XCTAssertTrue(markdown.contains("正文开始。"))
        XCTAssertEqual(markdown.components(separatedBy: "第零七章 • 计算").count - 1, 1)
    }

    func testKeepsHeadingWhenPlainTitlePrecedesDuplicateHeading() throws {
        let html = """
        <html><body><p>第零七章 • 计算</p><h1 id="chapter7">第零七章 • 计算</h1><p>正文开始。</p></body></html>
        """.data(using: .utf8)!
        let markdown = try HTMLToMarkdownConverter().convert(data: html, currentEpubPath: "OEBPS/chapter.xhtml", assetMapper: AssetMapper(), chapterLinks: ChapterLinkMap(epubPathToMarkdown: [:]), packageBase: "OEBPS")

        XCTAssertTrue(markdown.contains("<a id=\"chapter7\"></a>\n# 第零七章 • 计算"))
        XCTAssertEqual(markdown.components(separatedBy: "第零七章 • 计算").count - 1, 1)
    }

    func testKeepsRepeatedLongOpeningParagraphs() throws {
        let repeated = "This intentionally repeated opening paragraph is longer than a short title and should remain in the converted chapter body for fidelity."
        let html = """
        <html><body><p>\(repeated)</p><p>\(repeated)</p><p>Next.</p></body></html>
        """.data(using: .utf8)!
        let markdown = try HTMLToMarkdownConverter().convert(data: html, currentEpubPath: "OEBPS/chapter.xhtml", assetMapper: AssetMapper(), chapterLinks: ChapterLinkMap(epubPathToMarkdown: [:]), packageBase: "OEBPS")

        XCTAssertEqual(markdown.components(separatedBy: repeated).count - 1, 2)
    }
}
