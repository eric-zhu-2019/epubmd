import SwiftUI
import EpubMarkdownCore
#if canImport(UniformTypeIdentifiers)
import UniformTypeIdentifiers
#endif

final class ConversionViewModel: ObservableObject {
    @Published var epubURL: URL?
    @Published var outputDirectory: URL?
    @Published var status: String = "Choose a DRM-free EPUB and output folder."

    private let converter = EpubConverter()

    func chooseEpub() {
        let panel = NSOpenPanel()
        if let epubType = UTType(filenameExtension: "epub") {
            panel.allowedContentTypes = [epubType]
        }
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        if panel.runModal() == .OK, let url = panel.url {
            epubURL = url
            status = "Selected EPUB: \(url.lastPathComponent)"
        }
    }

    func chooseOutputFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        if panel.runModal() == .OK, let url = panel.url {
            outputDirectory = url
            status = "Selected output folder: \(url.path)"
        }
    }

    func acceptDroppedFile(_ url: URL) {
        guard url.pathExtension.lowercased() == "epub" else {
            status = "Drop a .epub file."
            return
        }
        epubURL = url
        status = "Selected EPUB: \(url.lastPathComponent)"
    }

    func convert() {
        guard let epubURL = epubURL else {
            status = "Choose an EPUB file first."
            return
        }
        guard let outputDirectory = outputDirectory else {
            status = "Choose an output folder first."
            return
        }
        status = "Converting…"
        do {
            let result = try converter.convert(epubURL: epubURL, outputParentDirectory: outputDirectory)
            status = "Converted to \(result.outputDirectory.path)"
        } catch {
            status = String(describing: error)
        }
    }
}

struct ContentView: View {
    @StateObject private var viewModel = ConversionViewModel()
    @State private var isDropTargeted = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("EPUB to Markdown")
                .font(.title)
                .bold()

            dropArea

            HStack {
                Button("Choose EPUB…") { viewModel.chooseEpub() }
                Text(viewModel.epubURL?.path ?? "No EPUB selected")
                    .lineLimit(1)
                    .truncationMode(.middle)
            }

            HStack {
                Button("Choose Output…") { viewModel.chooseOutputFolder() }
                Text(viewModel.outputDirectory?.path ?? "No output folder selected")
                    .lineLimit(1)
                    .truncationMode(.middle)
            }

            Button("Convert") { viewModel.convert() }
                .keyboardShortcut(.defaultAction)

            Text(viewModel.status)
                .font(.callout)
                .foregroundColor(.secondary)
                .lineLimit(3)
                .textSelection(.enabled)
        }
        .padding(24)
        .frame(minWidth: 560, minHeight: 320)
    }

    private var dropArea: some View {
        RoundedRectangle(cornerRadius: 10)
            .stroke(isDropTargeted ? Color.accentColor : Color.secondary, style: StrokeStyle(lineWidth: 1, dash: [6]))
            .overlay(Text("Drop one .epub file here").foregroundColor(.secondary))
            .frame(height: 90)
            .onDrop(of: ["public.file-url"], isTargeted: $isDropTargeted) { providers in
                guard let provider = providers.first else { return false }
                provider.loadItem(forTypeIdentifier: "public.file-url", options: nil) { item, _ in
                    let url: URL?
                    if let data = item as? Data, let string = String(data: data, encoding: .utf8) {
                        url = URL(string: string)
                    } else if let itemURL = item as? URL {
                        url = itemURL
                    } else {
                        url = nil
                    }
                    if let url = url {
                        DispatchQueue.main.async { viewModel.acceptDroppedFile(url) }
                    }
                }
                return true
            }
    }
}

@main
struct EpubMarkdownApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
