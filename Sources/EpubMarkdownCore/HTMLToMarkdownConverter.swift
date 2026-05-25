import Foundation

struct ChapterLinkMap {
    let epubPathToMarkdown: [String: String]

    func markdownPath(for href: String, currentEpubPath: String) -> String? {
        guard !href.lowercased().hasPrefix("http://"), !href.lowercased().hasPrefix("https://"), !href.lowercased().hasPrefix("mailto:") else {
            return href
        }
        let targetNoFragment = PathResolver.removingFragment(href)
        let fragment = PathResolver.fragment(href)
        if targetNoFragment.isEmpty, let fragment = fragment {
            return "#\(fragment)"
        }
        let base = (currentEpubPath as NSString).deletingLastPathComponent
        let normalized = PathResolver.normalize(PathResolver.join(base == "." ? "" : base, targetNoFragment))
        guard let markdown = epubPathToMarkdown[normalized] else { return nil }
        if let fragment = fragment { return "\(markdown)#\(fragment)" }
        return markdown
    }
}

public struct HTMLToMarkdownConverter {
    private let blockElementNames: Set<String> = [
        "address", "article", "aside", "blockquote", "body", "caption", "dd", "div", "dl", "dt",
        "figcaption", "figure", "footer", "form", "h1", "h2", "h3", "h4", "h5", "h6", "header",
        "hr", "html", "li", "main", "nav", "ol", "p", "pre", "section", "table", "tbody", "td",
        "tfoot", "th", "thead", "tr", "ul"
    ]

    public init() {}

    func convert(data: Data, currentEpubPath: String, assetMapper: AssetMapper, chapterLinks: ChapterLinkMap, packageBase: String) throws -> String {
        let root = try TreeXMLParser.parse(data: data)
        let body = root.firstDescendant(named: "body") ?? root
        let markdown = renderMixedBlockContents(body, context: RenderContext(currentEpubPath: currentEpubPath, assetMapper: assetMapper, chapterLinks: chapterLinks))
        return cleanup(markdown)
    }

    private struct RenderContext {
        let currentEpubPath: String
        let assetMapper: AssetMapper
        let chapterLinks: ChapterLinkMap
    }

    private func renderBlockChildren(_ children: [XMLNode], context: RenderContext) -> String {
        return children.map { renderBlock($0, context: context) }
            .filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
            .joined(separator: "\n\n")
    }

    private func renderBlock(_ node: XMLNode, context: RenderContext) -> String {
        switch node.name {
        case "h1", "h2", "h3", "h4", "h5", "h6":
            let level = Int(String(node.name.dropFirst())) ?? 1
            let heading = String(repeating: "#", count: level) + " " + renderInlineChildren(node, context: context).trimmedInline()
            return anchorPrefix(for: node) + heading
        case "p":
            return anchorPrefix(for: node) + renderInlineChildren(node, context: context).trimmedInline()
        case "ul":
            return renderList(node, ordered: false, context: context)
        case "ol":
            return renderList(node, ordered: true, context: context)
        case "blockquote":
            return anchorPrefix(for: node) + renderMixedBlockContents(node, context: context).split(separator: "\n").map { "> " + $0 }.joined(separator: "\n")
        case "pre":
            return anchorPrefix(for: node) + fencedCodeBlock(node.rawText())
        case "hr":
            return "---"
        case "table":
            return anchorPrefix(for: node) + renderTable(node, context: context)
        case "figure":
            return anchorPrefix(for: node) + renderFigure(node, context: context)
        case "figcaption":
            return "_" + renderInlineChildren(node, context: context).trimmedInline() + "_"
        case "div", "section", "article", "main", "body", "html", "aside", "nav":
            return anchorPrefix(for: node) + renderMixedBlockContents(node, context: context)
        case "img":
            return anchorPrefix(for: node) + renderImage(node, context: context)
        default:
            let inline = renderInline(node, context: context).trimmedInline()
            return inline
        }
    }

    private func renderList(_ node: XMLNode, ordered: Bool, context: RenderContext) -> String {
        var lines: [String] = []
        var index = 1
        for child in node.children where child.name == "li" {
            let marker = ordered ? "\(index). " : "- "
            let hasBlockChildren = child.children.contains { blockElementNames.contains($0.name) && $0.name != "br" }
            let content = hasBlockChildren ? (renderMixedBlockContents(child, context: context).nilIfEmpty ?? renderInlineChildren(child, context: context)) : renderInlineChildren(child, context: context)
            let normalized = content.split(separator: "\n", omittingEmptySubsequences: false).enumerated().map { offset, line -> String in
                if offset == 0 { return marker + line }
                return String(repeating: " ", count: marker.count) + line
            }.joined(separator: "\n")
            lines.append(normalized)
            index += 1
        }
        return lines.joined(separator: "\n")
    }

    private func renderMixedBlockContents(_ node: XMLNode, context: RenderContext) -> String {
        var blocks: [String] = []
        var inlineBuffer = ""

        func flushInlineBuffer() {
            let inline = inlineBuffer.trimmedInline()
            if !inline.isEmpty { blocks.append(inline) }
            inlineBuffer = ""
        }

        for content in node.contents {
            switch content {
            case .text(let string):
                inlineBuffer += string
            case .element(let child):
                if blockElementNames.contains(child.name) {
                    flushInlineBuffer()
                    let rendered = renderBlock(child, context: context).trimmingCharacters(in: .whitespacesAndNewlines)
                    if !rendered.isEmpty { blocks.append(rendered) }
                } else {
                    inlineBuffer += renderInline(child, context: context)
                }
            }
        }

        flushInlineBuffer()

        if blocks.isEmpty {
            return renderBlockChildren(node.children, context: context)
        }
        return blocks.joined(separator: "\n\n")
    }

    private func renderInlineChildren(_ node: XMLNode, context: RenderContext) -> String {
        if node.contents.isEmpty { return node.text }
        var result = ""
        for content in node.contents {
            switch content {
            case .text(let string):
                result += string
            case .element(let child):
                result += renderInline(child, context: context)
                if ["p", "div", "br"].contains(child.name) { result += " " }
            }
        }
        return result
    }

    private func renderInline(_ node: XMLNode, context: RenderContext) -> String {
        switch node.name {
        case "strong", "b": return "**\(renderInlineChildren(node, context: context).trimmedInline())**"
        case "em", "i": return "*\(renderInlineChildren(node, context: context).trimmedInline())*"
        case "code": return "`\(escapeInlineCode(renderInlineChildren(node, context: context).trimmedInline()))`"
        case "sup": return "<sup>\(renderInlineChildren(node, context: context).trimmedInline())</sup>"
        case "sub": return "<sub>\(renderInlineChildren(node, context: context).trimmedInline())</sub>"
        case "a":
            let text = renderInlineChildren(node, context: context).trimmedInline()
            guard let href = node.attributes["href"], !href.isEmpty else { return text }
            let resolved = context.chapterLinks.markdownPath(for: href, currentEpubPath: context.currentEpubPath) ?? href
            return "[\(text.nilIfEmpty ?? resolved)](\(resolved))"
        case "img": return renderImage(node, context: context)
        case "br": return "\n"
        case "li", "ul", "ol": return renderBlock(node, context: context)
        default: return renderInlineChildren(node, context: context)
        }
    }

    private func renderTable(_ node: XMLNode, context: RenderContext) -> String {
        let rows = node.descendants(named: "tr")
        let renderedRows: [[String]] = rows.map { row in
            let cells = row.children.filter { $0.name == "th" || $0.name == "td" }
            return cells.map { renderInlineChildren($0, context: context).trimmedInline().replacingOccurrences(of: "|", with: "\\|") }
        }.filter { !$0.isEmpty }
        guard let first = renderedRows.first else { return renderInlineChildren(node, context: context).trimmedInline() }
        let columnCount = renderedRows.map { $0.count }.max() ?? first.count
        func padded(_ row: [String]) -> [String] {
            if row.count >= columnCount { return Array(row.prefix(columnCount)) }
            return row + Array(repeating: "", count: columnCount - row.count)
        }
        let header = padded(first)
        var lines = ["| " + header.joined(separator: " | ") + " |"]
        lines.append("| " + Array(repeating: "---", count: columnCount).joined(separator: " | ") + " |")
        for row in renderedRows.dropFirst() {
            lines.append("| " + padded(row).joined(separator: " | ") + " |")
        }
        return lines.joined(separator: "\n")
    }

    private func renderFigure(_ node: XMLNode, context: RenderContext) -> String {
        let parts = node.children.map { renderBlock($0, context: context) }
            .filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        return parts.joined(separator: "\n\n")
    }

    private func fencedCodeBlock(_ code: String) -> String {
        let fence = code.contains("```") ? "~~~~" : "```"
        return "\(fence)\n\(code.trimmingCharacters(in: .whitespacesAndNewlines))\n\(fence)"
    }

    private func escapeInlineCode(_ code: String) -> String {
        return code.replacingOccurrences(of: "`", with: "\\`")
    }

    private func anchorPrefix(for node: XMLNode) -> String {
        guard let id = node.attributes["id"]?.nilIfEmpty ?? node.attributes["name"]?.nilIfEmpty else { return "" }
        return "<a id=\"\(id)\"></a>\n"
    }

    private func renderImage(_ node: XMLNode, context: RenderContext) -> String {
        guard let src = node.attributes["src"], !src.isEmpty else { return "" }
        let alt = node.attributes["alt"] ?? ""
        let resolved = context.assetMapper.markdownPath(for: src, contentBaseDirectory: (context.currentEpubPath as NSString).deletingLastPathComponent) ?? src
        return "![\(alt)](\(resolved))"
    }

    private func cleanup(_ markdown: String) -> String {
        var value = markdown.replacingOccurrences(of: "[ \\t]+", with: " ", options: .regularExpression)
        value = value.replacingOccurrences(of: "\\n{3,}", with: "\n\n", options: .regularExpression)
        value = removeDuplicateLeadingTitleBlocks(from: value)
        return value.trimmingCharacters(in: .whitespacesAndNewlines) + "\n"
    }

    private func removeDuplicateLeadingTitleBlocks(from markdown: String) -> String {
        let blocks = markdown.components(separatedBy: "\n\n")
        guard blocks.count >= 2 else { return markdown }

        var result = blocks
        var index = 1
        while index < min(result.count, 4) {
            let previous = titleComparableText(result[index - 1])
            let current = titleComparableText(result[index])
            guard !previous.isEmpty, previous == current else {
                index += 1
                continue
            }
            let previousIsHeading = isHeadingBlock(result[index - 1])
            let currentIsHeading = isHeadingBlock(result[index])
            let looksLikeLeadingPlainTitle = index == 1 && previous.count <= 100 && current.count <= 100
            guard previousIsHeading || currentIsHeading || looksLikeLeadingPlainTitle else {
                index += 1
                continue
            }

            if currentIsHeading || !previousIsHeading {
                result.remove(at: index - 1)
            } else {
                result.remove(at: index)
            }
        }
        return result.joined(separator: "\n\n")
    }

    private func isHeadingBlock(_ block: String) -> Bool {
        return block
            .split(separator: "\n")
            .contains { line in
                line.trimmingCharacters(in: .whitespaces).range(of: "^#{1,6}\\s+", options: .regularExpression) != nil
            }
    }

    private func titleComparableText(_ block: String) -> String {
        var lines = block.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        lines.removeAll { line in
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed.range(of: "^<a\\s+id=\"[^\"]+\"></a>$", options: .regularExpression) != nil
        }
        var value = lines.joined(separator: " ")
        value = value.replacingOccurrences(of: "^#{1,6}\\s+", with: "", options: .regularExpression)
        value = value.replacingOccurrences(of: "<[^>]+>", with: "", options: .regularExpression)
        value = value.replacingOccurrences(of: "[*_`]+", with: "", options: .regularExpression)
        value = value.replacingOccurrences(of: "\\[[^\\]]+\\]\\(([^)]+)\\)", with: "$1", options: .regularExpression)
        value = value.replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
        return value.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

extension String {
    func trimmedInline() -> String {
        return replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
