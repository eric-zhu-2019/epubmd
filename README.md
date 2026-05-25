# epubmd

`epubmd` is a Rust CLI that converts DRM-free EPUB files into a zip archive of Markdown chapters, assets, `README.md`, and `style.css`. The repo also includes a Tauri 2 reader app that opens those zip archives and renders the Markdown like an e-reader using the archive's `style.css`.

## CLI converter

```sh
cargo run -p epubmd -- book.epub --output book-md.zip
```

Options:

```sh
cargo run -p epubmd -- --help
```

The generated zip contains:

```text
README.md
style.css
chapters/*.md
assets/*
```

## Tauri reader

Install dependencies once:

```sh
npm install
```

Run/build the reader:

```sh
npm run tauri:dev
npm run tauri:build
```

Inside the app, choose **Open zip** and select a zip produced by the CLI.

## Packaging

The CLI is Rust-based and can be built on macOS, Linux, or Windows with Cargo:

```sh
cargo build --release -p epubmd
```

For a zip archive of the current platform binary, run:

```sh
python3 script/package_cli.py
```

See [PACKAGING.md](PACKAGING.md).
