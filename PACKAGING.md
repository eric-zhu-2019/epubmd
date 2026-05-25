# Packaging

Create a local macOS `.app` bundle and zip archive:

```sh
./script/package_app.sh
```

Outputs:

- `dist/EpubMarkdownApp.app`
- `dist/EpubMarkdownApp-0.1.0.zip`

Launch the packaged app locally:

```sh
open -n dist/EpubMarkdownApp.app
```

The script ad-hoc signs the app for local use. For public distribution, replace ad-hoc signing with a Developer ID Application certificate, enable hardened runtime, then notarize and staple the archive with Apple notary credentials.
