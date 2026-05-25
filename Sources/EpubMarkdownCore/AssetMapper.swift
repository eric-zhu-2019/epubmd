import Foundation

struct AssetMapper {
    private(set) var hrefToMarkdownPath: [String: String] = [:]
    private(set) var copiedAssets: [URL] = []
    private var usedNames: Set<String> = []

    mutating func copyAssets(for package: EpubPackage, extractedRoot: URL, outputRoot: URL) throws {
        let assetsDirectory = outputRoot.appendingPathComponent("assets", isDirectory: true)
        try FileManager.default.createDirectory(at: assetsDirectory, withIntermediateDirectories: true, attributes: nil)

        for item in package.manifest.values.sorted(by: { $0.absolutePath < $1.absolutePath }) {
            guard isAsset(item) else { continue }
            let source = extractedRoot.appendingPathComponent(item.absolutePath)
            guard FileManager.default.fileExists(atPath: source.path) else { continue }
            let fileName = uniqueFileName(for: item.absolutePath)
            let destination = assetsDirectory.appendingPathComponent(fileName)
            if FileManager.default.fileExists(atPath: destination.path) {
                try FileManager.default.removeItem(at: destination)
            }
            do {
                try FileManager.default.copyItem(at: source, to: destination)
            } catch {
                throw ConversionError.protectedEpub("asset could not be copied normally: \(item.absolutePath)")
            }
            hrefToMarkdownPath[item.absolutePath] = "../assets/\(fileName)"
            hrefToMarkdownPath[item.href] = "../assets/\(fileName)"
            copiedAssets.append(destination)
        }
    }

    func markdownPath(for href: String, contentBaseDirectory: String) -> String? {
        let noFragment = PathResolver.removingFragment(href)
        let absolute = PathResolver.normalize(PathResolver.join(contentBaseDirectory, noFragment))
        return hrefToMarkdownPath[absolute] ?? hrefToMarkdownPath[noFragment]
    }

    private func isAsset(_ item: ManifestItem) -> Bool {
        if item.mediaType.lowercased().hasPrefix("image/") { return true }
        let ext = (item.href as NSString).pathExtension.lowercased()
        return ["png", "jpg", "jpeg", "gif", "svg", "webp"].contains(ext)
    }

    private mutating func uniqueFileName(for path: String) -> String {
        let ns = path as NSString
        let base = ns.deletingPathExtension
        let ext = ns.pathExtension
        var candidateBase = base.replacingOccurrences(of: "[^A-Za-z0-9_-]+", with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        if candidateBase.isEmpty { candidateBase = "asset" }
        var candidate = ext.isEmpty ? candidateBase : "\(candidateBase).\(ext)"
        var index = 2
        while usedNames.contains(candidate) {
            candidate = ext.isEmpty ? "\(candidateBase)-\(index)" : "\(candidateBase)-\(index).\(ext)"
            index += 1
        }
        usedNames.insert(candidate)
        return candidate
    }
}
