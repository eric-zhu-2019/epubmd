# epubmd

`epubmd` is a Rust CLI that converts DRM-free EPUB files into a zip archive of Markdown chapters, assets, `README.md`, and `style.css`. The repo also includes a Tauri 2 reader app that imports EPUBs into `.zmd` Markdown ZIP books and renders them like a small e-reader.

## CLI converter

```sh
cargo run -p epubmd -- book.epub --output book-md.zip
```

The converter also accepts `.zmd` as an output extension for reader-library books:

```sh
cargo run -p epubmd -- book.epub --output book.zmd
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

Inside the app:

- **Import EPUB** converts a DRM-free `.epub` to a `.zmd` Markdown ZIP archive.
- Imported books are stored in `~/.config/epubmd/books/`.
- The sidebar lists `.zmd` books from that folder and opens the selected book for reading.
- Put Typora-compatible `.css` files in `~/.config/epubmd/themes/`, then choose them from the reader's **Theme** selector. Typora selectors such as `#write`, `body`, and `html` are scoped to the Markdown reading pane.
- While reading, the sidebar shows the book's chapters and jumps directly to the selected chapter.

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
