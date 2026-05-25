import Foundation

struct EpubArchive {
    let epubURL: URL

    func extract(to destination: URL) throws {
        guard FileManager.default.fileExists(atPath: epubURL.path) else {
            throw ConversionError.invalidEpub("file does not exist: \(epubURL.path)")
        }
        try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true, attributes: nil)
        let process = Process()
        process.launchPath = "/usr/bin/unzip"
        process.arguments = ["-qq", epubURL.path, "-d", destination.path]
        let pipe = Pipe()
        process.standardError = pipe
        process.standardOutput = Pipe()
        process.launch()
        process.waitUntilExit()
        if process.terminationStatus != 0 {
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let stderr = String(data: data, encoding: .utf8) ?? "unzip failed"
            throw ConversionError.invalidEpub(stderr.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty ?? "not a readable zip-based EPUB")
        }
    }

    static func isProtected(extractedRoot: URL) -> Bool {
        let encryption = extractedRoot.appendingPathComponent("META-INF/encryption.xml")
        return FileManager.default.fileExists(atPath: encryption.path)
    }
}
