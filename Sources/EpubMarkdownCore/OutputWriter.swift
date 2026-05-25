import Foundation

struct OutputWriter {
    let fileManager: FileManager

    init(fileManager: FileManager = .default) {
        self.fileManager = fileManager
    }

    func uniqueDestination(for title: String, in parent: URL) -> URL {
        let sanitized = sanitizeFileName(title.nilIfEmpty ?? "Book")
        var candidate = parent.appendingPathComponent(sanitized, isDirectory: true)
        var index = 2
        while fileManager.fileExists(atPath: candidate.path) {
            candidate = parent.appendingPathComponent("\(sanitized) \(index)", isDirectory: true)
            index += 1
        }
        return candidate
    }

    func stagingDirectory(in parent: URL) throws -> URL {
        let staging = parent.appendingPathComponent(".epub-markdown-staging-\(UUID().uuidString)", isDirectory: true)
        try fileManager.createDirectory(at: staging, withIntermediateDirectories: true, attributes: nil)
        return staging
    }

    func chapterFileName(index: Int, title: String?, fallback: String) -> String {
        let base = sanitizeFileName(title?.nilIfEmpty ?? ((fallback as NSString).deletingPathExtension.nilIfEmpty ?? "chapter"))
        return String(format: "%03d-%@.md", index, base)
    }

    func write(_ string: String, to url: URL) throws {
        try fileManager.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true, attributes: nil)
        guard let data = string.data(using: .utf8) else {
            throw ConversionError.fileSystem("could not encode UTF-8 output for \(url.path)")
        }
        try data.write(to: url, options: .atomic)
    }

    func commit(staging: URL, final: URL) throws {
        if fileManager.fileExists(atPath: final.path) {
            throw ConversionError.fileSystem("destination already exists: \(final.path)")
        }
        try fileManager.moveItem(at: staging, to: final)
    }

    func cleanup(_ url: URL) {
        try? fileManager.removeItem(at: url)
    }

    private func sanitizeFileName(_ input: String) -> String {
        var value = input.replacingOccurrences(of: "[^A-Za-z0-9 _-]+", with: "-", options: .regularExpression)
            .replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: " .-_"))
        if value.isEmpty { value = "Book" }
        return value
    }
}
