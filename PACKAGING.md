# Packaging

`epubmd` now has two deliverables:

1. Rust CLI converter: converts a DRM-free EPUB into a Markdown/assets zip.
2. Tauri reader app: opens an epubmd-generated zip and renders the Markdown with the zip's `style.css`.

## Build the CLI converter

```sh
cargo build --release -p epubmd
```

Run it directly:

```sh
target/release/epubmd book.epub --output book-md.zip
```

Create a current-platform CLI archive on macOS, Linux, or Windows:

```sh
python3 script/package_cli.py
```

On macOS/Linux, `./script/package_cli.sh` is a convenience wrapper around the same portable Python packager.

Outputs use the current OS/CPU in the filename, for example:

- `dist/cli/epubmd` or `dist/cli/epubmd.exe`
- `dist/epubmd-cli-0.1.0-darwin-arm64.zip`
- `dist/epubmd-cli-0.1.0-linux-x86_64.zip`
- `dist/epubmd-cli-0.1.0-windows-amd64.zip`

For release distribution, run the same Cargo build/package command on each target platform or from CI cross-target jobs.

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
