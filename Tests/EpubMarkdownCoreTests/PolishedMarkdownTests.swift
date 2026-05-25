import XCTest
@testable import EpubMarkdownCore

final class PolishedMarkdownTests: XCTestCase {
    func testConvertsEpubLikeStructuresToReadableMarkdown() throws {
        let html = """
        <html><body>
          <h2 id="chapter">Chapter Title</h2>
          <blockquote><p>A quoted <em>passage</em>.</p></blockquote>
          <figure><img src="images/pic.png" alt="Diagram"/><figcaption>Figure 1. Diagram caption.</figcaption></figure>
          <table><tr><th>Name</th><th>Value</th></tr><tr><td>A</td><td>1</td></tr></table>
          <p>H<sub>2</sub>O and note<sup>1</sup> with <code>inline()</code>.</p>
          <pre><code>let x = 1&#10;print(x)</code></pre>
          <hr/>
        </body></html>
        """.data(using: .utf8)!
        var mapper = AssetMapper()
        let package = EpubPackage(rootFilePath: "OEBPS/content.opf", baseDirectory: "OEBPS", title: "T", manifest: [
            "img": ManifestItem(id: "img", href: "images/pic.png", mediaType: "image/png", absolutePath: "OEBPS/images/pic.png")
        ], spine: [])
        let extracted = try TestSupport.makeTempDirectory("polished-src")
        let output = try TestSupport.makeTempDirectory("polished-out")
        try TestSupport.write("png", to: extracted.appendingPathComponent("OEBPS/images/pic.png"))
        try mapper.copyAssets(for: package, extractedRoot: extracted, outputRoot: output)

        let markdown = try HTMLToMarkdownConverter().convert(data: html, currentEpubPath: "OEBPS/chapter.xhtml", assetMapper: mapper, chapterLinks: ChapterLinkMap(epubPathToMarkdown: [:]), packageBase: "OEBPS")

        XCTAssertTrue(markdown.contains("<a id=\"chapter\"></a>\n## Chapter Title"))
        XCTAssertTrue(markdown.contains("> A quoted *passage*."))
        XCTAssertTrue(markdown.contains("![Diagram](../assets/"))
        XCTAssertTrue(markdown.contains("_Figure 1. Diagram caption._"))
        XCTAssertTrue(markdown.contains("| Name | Value |"))
        XCTAssertTrue(markdown.contains("| --- | --- |"))
        XCTAssertTrue(markdown.contains("| A | 1 |"))
        XCTAssertTrue(markdown.contains("H<sub>2</sub>O and note<sup>1</sup> with `inline()`."))
        XCTAssertTrue(markdown.contains("```\nlet x = 1\nprint(x)\n```"))
        XCTAssertTrue(markdown.contains("\n---\n"))
    }
}
