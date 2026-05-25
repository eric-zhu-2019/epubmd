import Foundation

struct ManifestItem: Equatable {
    let id: String
    let href: String
    let mediaType: String
    let absolutePath: String
    let properties: String

    init(id: String, href: String, mediaType: String, absolutePath: String, properties: String = "") {
        self.id = id
        self.href = href
        self.mediaType = mediaType
        self.absolutePath = absolutePath
        self.properties = properties
    }
}

struct EpubMetadata: Equatable {
    let title: String
    let creators: [String]
    let language: String?
    let publisher: String?
    let date: String?
    let identifier: String?

    static let empty = EpubMetadata(title: "", creators: [], language: nil, publisher: nil, date: nil, identifier: nil)
}

struct SpineItem: Equatable {
    let idref: String
    let item: ManifestItem
}

struct EpubPackage: Equatable {
    let rootFilePath: String
    let baseDirectory: String
    let title: String
    let manifest: [String: ManifestItem]
    let spine: [SpineItem]
    let metadata: EpubMetadata

    init(rootFilePath: String, baseDirectory: String, title: String, manifest: [String: ManifestItem], spine: [SpineItem], metadata: EpubMetadata = .empty) {
        self.rootFilePath = rootFilePath
        self.baseDirectory = baseDirectory
        self.title = title
        self.manifest = manifest
        self.spine = spine
        self.metadata = metadata.title.isEmpty ? EpubMetadata(title: title, creators: metadata.creators, language: metadata.language, publisher: metadata.publisher, date: metadata.date, identifier: metadata.identifier) : metadata
    }
}

struct EpubPackageParser {
    static func parseContainer(at extractedRoot: URL) throws -> String {
        let containerURL = extractedRoot.appendingPathComponent("META-INF/container.xml")
        guard FileManager.default.fileExists(atPath: containerURL.path) else {
            throw ConversionError.invalidEpub("missing META-INF/container.xml")
        }
        let data: Data
        do {
            data = try Data(contentsOf: containerURL)
        } catch {
            throw ConversionError.protectedEpub("container.xml could not be read normally")
        }
        let root: XMLNode
        do {
            root = try TreeXMLParser.parse(data: data)
        } catch {
            throw ConversionError.malformedEpub("container.xml could not be parsed")
        }
        guard let rootFile = root.descendants(named: "rootfile").first,
              let path = rootFile.attributes["full-path"], !path.isEmpty else {
            throw ConversionError.malformedEpub("container.xml does not declare an OPF rootfile")
        }
        return path
    }

    static func parsePackage(at extractedRoot: URL, rootFilePath: String) throws -> EpubPackage {
        let opfURL = extractedRoot.appendingPathComponent(rootFilePath)
        guard FileManager.default.fileExists(atPath: opfURL.path) else {
            throw ConversionError.malformedEpub("OPF file not found at \(rootFilePath)")
        }
        let data: Data
        do {
            data = try Data(contentsOf: opfURL)
        } catch {
            throw ConversionError.protectedEpub("OPF file could not be read normally at \(rootFilePath)")
        }
        let root: XMLNode
        do {
            root = try TreeXMLParser.parse(data: data)
        } catch {
            throw ConversionError.malformedEpub("OPF file could not be parsed at \(rootFilePath)")
        }
        let baseDirectory = (rootFilePath as NSString).deletingLastPathComponent
        let normalizedBase = baseDirectory == "." ? "" : baseDirectory
        let fallbackTitle = ((rootFilePath as NSString).lastPathComponent as NSString).deletingPathExtension
        let metadata = parseMetadata(root: root, fallbackTitle: fallbackTitle)
        let title = metadata.title

        var manifest: [String: ManifestItem] = [:]
        for item in root.descendants(named: "item") {
            guard let id = item.attributes["id"], let href = item.attributes["href"] else { continue }
            let mediaType = item.attributes["media-type"] ?? ""
            let properties = item.attributes["properties"] ?? ""
            let absolutePath = PathResolver.normalize(PathResolver.join(normalizedBase, href))
            manifest[id] = ManifestItem(id: id, href: href, mediaType: mediaType, absolutePath: absolutePath, properties: properties)
        }
        guard !manifest.isEmpty else {
            throw ConversionError.malformedEpub("OPF manifest is empty")
        }

        var spine: [SpineItem] = []
        for itemref in root.descendants(named: "itemref") {
            guard let idref = itemref.attributes["idref"] else { continue }
            guard let item = manifest[idref] else {
                throw ConversionError.malformedEpub("spine references missing manifest item \(idref)")
            }
            spine.append(SpineItem(idref: idref, item: item))
        }
        guard !spine.isEmpty else {
            throw ConversionError.malformedEpub("OPF spine is empty")
        }

        return EpubPackage(rootFilePath: rootFilePath, baseDirectory: normalizedBase, title: title, manifest: manifest, spine: spine, metadata: metadata)
    }

    private static func parseMetadata(root: XMLNode, fallbackTitle: String) -> EpubMetadata {
        let metadataNode = root.firstDescendant(named: "metadata") ?? root
        let title = metadataNode.firstDescendant(named: "title")?.collapsedText().nilIfEmpty ?? fallbackTitle
        let creators = metadataNode.descendants(named: "creator").compactMap { $0.collapsedText().nilIfEmpty }
        let language = metadataNode.firstDescendant(named: "language")?.collapsedText().nilIfEmpty
        let publisher = metadataNode.firstDescendant(named: "publisher")?.collapsedText().nilIfEmpty
        let date = metadataNode.firstDescendant(named: "date")?.collapsedText().nilIfEmpty
        let identifier = metadataNode.firstDescendant(named: "identifier")?.collapsedText().nilIfEmpty
        return EpubMetadata(title: title, creators: creators, language: language, publisher: publisher, date: date, identifier: identifier)
    }
}

struct PathResolver {
    static func join(_ base: String, _ path: String) -> String {
        if base.isEmpty { return path }
        return (base as NSString).appendingPathComponent(path)
    }

    static func normalize(_ path: String) -> String {
        let ns = path as NSString
        let components = ns.standardizingPath.split(separator: "/").map(String.init)
        return components.joined(separator: "/")
    }

    static func removingFragment(_ href: String) -> String {
        return href.components(separatedBy: "#").first ?? href
    }

    static func fragment(_ href: String) -> String? {
        let parts = href.components(separatedBy: "#")
        return parts.count > 1 ? parts.dropFirst().joined(separator: "#") : nil
    }
}

extension String {
    var nilIfEmpty: String? {
        let trimmed = trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}
