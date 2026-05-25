#!/usr/bin/env bash
set -euo pipefail

APP_NAME="epubmd"
VERSION="0.1.0"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_APP="$ROOT_DIR/src-tauri/target/release/bundle/macos/$APP_NAME.app"
DIST_DIR="$ROOT_DIR/dist"
DIST_APP="$DIST_DIR/$APP_NAME.app"
ZIP_PATH="$DIST_DIR/$APP_NAME-reader-$VERSION.zip"

cd "$ROOT_DIR"
npm run tauri:build
rm -rf "$DIST_APP" "$ZIP_PATH"
mkdir -p "$DIST_DIR"
cp -R "$SOURCE_APP" "$DIST_APP"
/usr/bin/codesign --force --deep --sign - "$DIST_APP" >/dev/null
/usr/bin/codesign --verify --deep --strict --verbose=2 "$DIST_APP"
(
  cd "$DIST_DIR"
  /usr/bin/ditto -c -k --norsrc --keepParent "$APP_NAME.app" "$ZIP_PATH"
)

printf 'Reader app:     %s\n' "$DIST_APP"
printf 'Reader archive: %s\n' "$ZIP_PATH"
