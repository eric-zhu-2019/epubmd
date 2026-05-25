#!/usr/bin/env bash
set -euo pipefail

APP_NAME="epubmd"
VERSION="0.1.0"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
CLI_DIR="$DIST_DIR/cli"
ZIP_PATH="$DIST_DIR/$APP_NAME-cli-$VERSION.zip"

cd "$ROOT_DIR"
rm -rf "$CLI_DIR" "$ZIP_PATH"
mkdir -p "$CLI_DIR"

xcrun swift build -c release --product "$APP_NAME"
cp "$ROOT_DIR/.build/release/$APP_NAME" "$CLI_DIR/$APP_NAME"
chmod 755 "$CLI_DIR/$APP_NAME"

(
  cd "$CLI_DIR"
  /usr/bin/ditto -c -k --norsrc "$APP_NAME" "$ZIP_PATH"
)

printf 'CLI binary:  %s\n' "$CLI_DIR/$APP_NAME"
printf 'CLI archive: %s\n' "$ZIP_PATH"
