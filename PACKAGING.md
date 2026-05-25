# Packaging

`epubmd` now has two deliverables:

1. CLI converter: converts a DRM-free EPUB into a Markdown/assets zip.
2. Tauri reader app: opens an epubmd-generated zip and renders the Markdown with the zip's `style.css`.

## Build the CLI converter

```sh
xcrun swift build -c release --product epubmd
```

Run it directly:

```sh
.build/release/epubmd book.epub --output book-md.zip
```

Create a distributable CLI archive:

```sh
./script/package_cli.sh
```

Outputs:

- `dist/cli/epubmd`
- `dist/epubmd-cli-0.1.0.zip`

## Build the Tauri reader app

Install frontend/Rust dependencies once:

```sh
npm install
```

Build the reader app:

```sh
npm run tauri:build
```

Create a local app archive:

```sh
./script/package_app.sh
```

Outputs:

- `dist/epubmd.app`
- `dist/epubmd-reader-0.1.0.zip`

Launch locally:

```sh
open -n dist/epubmd.app
```

The app bundle is suitable for local testing. For public distribution, sign with a Developer ID Application certificate, enable hardened runtime, then notarize and staple with Apple notary credentials.
