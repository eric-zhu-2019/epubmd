import Foundation
import EpubMarkdownCore

private let version = "0.1.0"

private struct CLIOptions {
    var input: String?
    var output: String?
    var overwrite = false
    var showHelp = false
    var showVersion = false
}

private enum CLIError: Error, CustomStringConvertible {
    case missingInput
    case unexpectedArgument(String)
    case missingValue(String)
    case tooManyInputs(String)

    var description: String {
        switch self {
        case .missingInput:
            return "missing input EPUB path"
        case .unexpectedArgument(let argument):
            return "unexpected argument: \(argument)"
        case .missingValue(let option):
            return "missing value for \(option)"
        case .tooManyInputs(let input):
            return "only one input EPUB is supported; extra input: \(input)"
        }
    }
}

private func usage() -> String {
    """
    epubmd \(version)

    Convert a DRM-free EPUB into a zip archive containing Markdown chapters, assets, README.md, and style.css.

    Usage:
      epubmd <book.epub> --output <book.zip> [--force]
      epubmd <book.epub> -o <book.zip> [--force]
      epubmd <book.epub> [--force]

    Options:
      -o, --output <path>   Destination .zip path. Defaults to <book>.zip next to the EPUB.
      -f, --force           Replace an existing destination zip.
      -h, --help            Show this help.
      --version             Show the version.
    """
}

private func parse(_ arguments: [String]) throws -> CLIOptions {
    var options = CLIOptions()
    var index = 1
    while index < arguments.count {
        let argument = arguments[index]
        switch argument {
        case "-h", "--help":
            options.showHelp = true
        case "--":
            break
        case "--version":
            options.showVersion = true
        case "-f", "--force":
            options.overwrite = true
        case "-o", "--output":
            let valueIndex = index + 1
            guard valueIndex < arguments.count else { throw CLIError.missingValue(argument) }
            options.output = arguments[valueIndex]
            index += 1
        default:
            if argument.hasPrefix("-") { throw CLIError.unexpectedArgument(argument) }
            if options.input != nil { throw CLIError.tooManyInputs(argument) }
            options.input = argument
        }
        index += 1
    }
    return options
}

private func defaultOutputZip(for input: URL) -> URL {
    input.deletingPathExtension().appendingPathExtension("zip")
}

private func run() -> Int32 {
    do {
        let options = try parse(CommandLine.arguments)
        if options.showHelp {
            print(usage())
            return 0
        }
        if options.showVersion {
            print(version)
            return 0
        }
        guard let input = options.input else { throw CLIError.missingInput }
        let inputURL = URL(fileURLWithPath: input).standardizedFileURL
        let outputURL = options.output.map { URL(fileURLWithPath: $0).standardizedFileURL } ?? defaultOutputZip(for: inputURL)
        let zipURL = try EpubConverter().convertToZip(epubURL: inputURL, outputZipURL: outputURL, overwrite: options.overwrite)
        print("Wrote \(zipURL.path)")
        return 0
    } catch {
        FileHandle.standardError.write(Data("epubmd: \(error)\n\n\(usage())\n".utf8))
        return 1
    }
}

exit(run())
