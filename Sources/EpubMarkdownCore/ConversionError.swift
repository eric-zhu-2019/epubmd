import Foundation

public enum ConversionError: Error, Equatable, CustomStringConvertible {
    case invalidEpub(String)
    case malformedEpub(String)
    case protectedEpub(String)
    case fileSystem(String)
    case conversion(String)

    public var description: String {
        switch self {
        case .invalidEpub(let message): return "Invalid EPUB: \(message)"
        case .malformedEpub(let message): return "Malformed EPUB: \(message)"
        case .protectedEpub(let message): return "Protected or unsupported EPUB: \(message)"
        case .fileSystem(let message): return "File system error: \(message)"
        case .conversion(let message): return "Conversion error: \(message)"
        }
    }
}

public struct ConversionWarning: Equatable {
    public let message: String

    public init(_ message: String) {
        self.message = message
    }
}

public struct ConversionResult: Equatable {
    public let outputDirectory: URL
    public let markdownFiles: [URL]
    public let assetFiles: [URL]
    public let warnings: [ConversionWarning]

    public init(outputDirectory: URL, markdownFiles: [URL], assetFiles: [URL], warnings: [ConversionWarning]) {
        self.outputDirectory = outputDirectory
        self.markdownFiles = markdownFiles
        self.assetFiles = assetFiles
        self.warnings = warnings
    }
}
