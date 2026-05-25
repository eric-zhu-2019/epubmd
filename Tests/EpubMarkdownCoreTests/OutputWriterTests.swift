import XCTest
@testable import EpubMarkdownCore

final class OutputWriterTests: XCTestCase {
    func testChapterFileNamesAreMarkdownLinkSafe() {
        let writer = OutputWriter()

        XCTAssertEqual(
            writer.chapterFileName(index: 4, title: "Multi-Paradigm Programming", fallback: "body.xhtml"),
            "004-Multi-Paradigm-Programming.md"
        )
        XCTAssertEqual(
            writer.chapterFileName(index: 7, title: "第零七章 • 计算", fallback: "chapter7.xhtml"),
            "007-第零七章-•-计算.md"
        )
        XCTAssertEqual(
            writer.chapterFileName(index: 8, title: "A title (with [bad] chars)#frag", fallback: "chapter8.xhtml"),
            "008-A-title-with-bad-chars-frag.md"
        )
    }
}
