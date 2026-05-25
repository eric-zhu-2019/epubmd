import Foundation
#if canImport(FoundationXML)
import FoundationXML
#endif

enum XMLContent {
    case text(String)
    case element(XMLNode)
}

struct XMLNode {
    var name: String
    var attributes: [String: String]
    var text: String = ""
    var children: [XMLNode] = []
    var contents: [XMLContent] = []
}

final class TreeXMLParser: NSObject, XMLParserDelegate {
    private var stack: [XMLNode] = []
    private(set) var root: XMLNode?
    private(set) var parseError: Error?

    static func parse(data: Data) throws -> XMLNode {
        let delegate = TreeXMLParser()
        let parser = XMLParser(data: data)
        parser.delegate = delegate
        guard parser.parse(), let root = delegate.root else {
            throw delegate.parseError ?? parser.parserError ?? ConversionError.malformedEpub("XML could not be parsed")
        }
        return root
    }

    func parser(_ parser: XMLParser, didStartElement elementName: String, namespaceURI: String?, qualifiedName qName: String?, attributes attributeDict: [String : String] = [:]) {
        let localName = elementName.lowercased().split(separator: ":").last.map(String.init) ?? elementName.lowercased()
        stack.append(XMLNode(name: localName, attributes: attributeDict))
    }

    func parser(_ parser: XMLParser, foundCharacters string: String) {
        guard !stack.isEmpty else { return }
        stack[stack.count - 1].text += string
        stack[stack.count - 1].contents.append(.text(string))
    }

    func parser(_ parser: XMLParser, foundCDATA CDATABlock: Data) {
        guard !stack.isEmpty, let string = String(data: CDATABlock, encoding: .utf8) else { return }
        stack[stack.count - 1].text += string
        stack[stack.count - 1].contents.append(.text(string))
    }

    func parser(_ parser: XMLParser, didEndElement elementName: String, namespaceURI: String?, qualifiedName qName: String?) {
        guard let node = stack.popLast() else { return }
        if stack.isEmpty {
            root = node
        } else {
            stack[stack.count - 1].children.append(node)
            stack[stack.count - 1].contents.append(.element(node))
        }
    }

    func parser(_ parser: XMLParser, parseErrorOccurred parseError: Error) {
        self.parseError = parseError
    }
}

extension XMLNode {
    func firstDescendant(named target: String) -> XMLNode? {
        let normalized = target.lowercased()
        if name == normalized { return self }
        for child in children {
            if let match = child.firstDescendant(named: normalized) { return match }
        }
        return nil
    }

    func descendants(named target: String) -> [XMLNode] {
        let normalized = target.lowercased()
        var result: [XMLNode] = []
        if name == normalized { result.append(self) }
        for child in children { result.append(contentsOf: child.descendants(named: normalized)) }
        return result
    }

    func collapsedText() -> String {
        return rawText()
            .replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func rawText() -> String {
        var value = ""
        for content in contents {
            switch content {
            case .text(let string): value += string
            case .element(let child): value += child.rawText()
            }
        }
        return value.isEmpty ? text : value
    }
}
