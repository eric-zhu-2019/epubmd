import Foundation

public final class EpubConverter {
    private let fileManager: FileManager
    private let writer: OutputWriter

    public init(fileManager: FileManager = .default) {
        self.fileManager = fileManager
        self.writer = OutputWriter(fileManager: fileManager)
    }

    public func convert(epubURL: URL, outputParentDirectory: URL) throws -> ConversionResult {
        let workRoot = outputParentDirectory.appendingPathComponent(".epub-markdown-work-\(UUID().uuidString)", isDirectory: true)
        let extracted = workRoot.appendingPathComponent("extracted", isDirectory: true)
        var staging: URL?
        do {
            try fileManager.createDirectory(at: workRoot, withIntermediateDirectories: true, attributes: nil)
            try EpubArchive(epubURL: epubURL).extract(to: extracted)
            if EpubArchive.isProtected(extractedRoot: extracted) {
                throw ConversionError.protectedEpub("META-INF/encryption.xml is present; DRM/protected content is unsupported")
            }

            let rootFile = try EpubPackageParser.parseContainer(at: extracted)
            let package = try EpubPackageParser.parsePackage(at: extracted, rootFilePath: rootFile)
            let finalDestination = writer.uniqueDestination(for: package.title, in: outputParentDirectory)
            let stagingURL = try writer.stagingDirectory(in: outputParentDirectory)
            staging = stagingURL
            let chaptersDirectory = stagingURL.appendingPathComponent("chapters", isDirectory: true)
            try fileManager.createDirectory(at: chaptersDirectory, withIntermediateDirectories: true, attributes: nil)

            var chapterData: [(offset: Int, spineItem: SpineItem, data: Data, fileName: String)] = []
            var chapterOutputByEpubPath: [String: String] = [:]
            for (offset, spineItem) in package.spine.enumerated() {
                let source = extracted.appendingPathComponent(spineItem.item.absolutePath)
                guard fileManager.fileExists(atPath: source.path) else {
                    throw ConversionError.malformedEpub("spine item missing content file: \(spineItem.item.absolutePath)")
                }
                let data: Data
                do {
                    data = try Data(contentsOf: source)
                } catch {
                    throw ConversionError.protectedEpub("required spine resource could not be read normally: \(spineItem.item.absolutePath)")
                }
                let title = chapterTitle(from: data)
                let fileName = writer.chapterFileName(index: offset + 1, title: title, fallback: spineItem.item.href)
                chapterData.append((offset, spineItem, data, fileName))
                chapterOutputByEpubPath[spineItem.item.absolutePath] = fileName
            }
            let chapterLinks = ChapterLinkMap(epubPathToMarkdown: chapterOutputByEpubPath)

            var assetMapper = AssetMapper()
            try assetMapper.copyAssets(for: package, extractedRoot: extracted, outputRoot: stagingURL)

            let converter = HTMLToMarkdownConverter()
            var markdownFiles: [URL] = []
            for chapter in chapterData {
                try validateReferencedImages(in: chapter.data, currentEpubPath: chapter.spineItem.item.absolutePath, assetMapper: assetMapper)
                let markdown = try converter.convert(data: chapter.data, currentEpubPath: chapter.spineItem.item.absolutePath, assetMapper: assetMapper, chapterLinks: chapterLinks, packageBase: package.baseDirectory)
                let destination = chaptersDirectory.appendingPathComponent(chapter.fileName)
                try writer.write(markdown, to: destination)
                markdownFiles.append(finalDestination.appendingPathComponent("chapters").appendingPathComponent(chapter.fileName))
            }

            let readmeChapterEntries = zip(chapterData, markdownFiles).map { (chapter, url) in
                ReadmeChapterEntry(title: chapterTitle(from: chapter.data) ?? (url.lastPathComponent as NSString).deletingPathExtension, fileName: url.lastPathComponent)
            }
            try writeReadme(package: package, chapters: readmeChapterEntries, stagingURL: stagingURL)
            try writeReaderStyle(stagingURL: stagingURL)
            try writer.commit(staging: stagingURL, final: finalDestination)
            staging = nil
            writer.cleanup(workRoot)

            let finalAssets = assetMapper.copiedAssets.map { finalDestination.appendingPathComponent("assets").appendingPathComponent($0.lastPathComponent) }
            return ConversionResult(outputDirectory: finalDestination, markdownFiles: markdownFiles, assetFiles: finalAssets, warnings: [])
        } catch let error as ConversionError {
            if let staging = staging { writer.cleanup(staging) }
            writer.cleanup(workRoot)
            throw error
        } catch {
            if let staging = staging { writer.cleanup(staging) }
            writer.cleanup(workRoot)
            throw ConversionError.conversion(error.localizedDescription)
        }
    }

    private func validateReferencedImages(in data: Data, currentEpubPath: String, assetMapper: AssetMapper) throws {
        guard let root = try? TreeXMLParser.parse(data: data) else { return }
        let contentBaseDirectory = (currentEpubPath as NSString).deletingLastPathComponent
        for image in root.descendants(named: "img") {
            guard let src = image.attributes["src"], !src.isEmpty else { continue }
            let lower = src.lowercased()
            if lower.hasPrefix("http://") || lower.hasPrefix("https://") || lower.hasPrefix("data:") { continue }
            if assetMapper.markdownPath(for: src, contentBaseDirectory: contentBaseDirectory) == nil {
                throw ConversionError.malformedEpub("referenced image asset is missing or not declared: \(src)")
            }
        }
    }

    private func chapterTitle(from data: Data) -> String? {
        guard let root = try? TreeXMLParser.parse(data: data) else { return nil }
        for heading in ["h1", "h2", "h3"] {
            if let title = root.firstDescendant(named: heading)?.collapsedText().nilIfEmpty {
                return title
            }
        }
        return root.firstDescendant(named: "title")?.collapsedText().nilIfEmpty
    }

    private struct ReadmeChapterEntry {
        let title: String
        let fileName: String
    }

    private func writeReadme(package: EpubPackage, chapters: [ReadmeChapterEntry], stagingURL: URL) throws {
        var readme = "# \(package.title)\n\n"
        var metadataLines: [String] = []
        if !package.metadata.creators.isEmpty { metadataLines.append("- Author: \(package.metadata.creators.joined(separator: ", "))") }
        if let publisher = package.metadata.publisher { metadataLines.append("- Publisher: \(publisher)") }
        if let date = package.metadata.date { metadataLines.append("- Date: \(date)") }
        if let language = package.metadata.language { metadataLines.append("- Language: \(language)") }
        if let identifier = package.metadata.identifier { metadataLines.append("- Identifier: \(identifier)") }
        if !metadataLines.isEmpty {
            readme += "## Metadata\n\n"
            readme += metadataLines.joined(separator: "\n") + "\n\n"
        }
        readme += "## Contents\n\n"
        for chapter in chapters {
            readme += "- [\(chapter.title)](chapters/\(chapter.fileName))\n"
        }
        readme += "\n## Reading style\n\n"
        readme += "If your Markdown viewer supports custom stylesheets, use `style.css` for a higher-contrast book-like reading view.\n"
        try writer.write(readme, to: stagingURL.appendingPathComponent("README.md"))
    }

    private func writeReaderStyle(stagingURL: URL) throws {
        let css = """
        :root {
          color-scheme: light dark;
        }

        body {
          max-width: 78ch;
          margin: 3rem auto;
          padding: 0 1.5rem;
          color: #1f2937;
          background: #ffffff;
          font: 17px/1.65 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        }

        h1, h2, h3, h4, h5, h6 {
          color: #111827;
          line-height: 1.25;
          margin-top: 2rem;
        }

        a {
          color: #0969da;
        }

        blockquote {
          color: #374151;
          border-left: 4px solid #d1d5db;
          margin-left: 0;
          padding-left: 1rem;
        }

        pre, code {
          color: #111827;
          background: #f6f8fa;
        }

        img {
          max-width: 100%;
          height: auto;
        }

        @media (prefers-color-scheme: dark) {
          body {
            color: #e5e7eb;
            background: #0f172a;
          }

          h1, h2, h3, h4, h5, h6,
          pre, code {
            color: #f9fafb;
          }

          a {
            color: #8ab4f8;
          }

          blockquote {
            color: #d1d5db;
            border-left-color: #4b5563;
          }

          pre, code {
            background: #111827;
          }
        }
        """
        try writer.write(css + "\n", to: stagingURL.appendingPathComponent("style.css"))
    }
}
