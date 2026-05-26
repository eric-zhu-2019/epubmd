#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
XCODE_DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"

strip_swiftly_path() {
  local input_path="${1:-}"
  local output=""
  local entry

  IFS=':' read -r -a entries <<< "$input_path"
  for entry in "${entries[@]}"; do
    case "$entry" in
      "$HOME/.swiftly/bin"|"$HOME"/Library/Developer/Toolchains/*/usr/bin)
        continue
        ;;
    esac

    if [[ -z "$output" ]]; then
      output="$entry"
    else
      output="$output:$entry"
    fi
  done

  printf '%s' "$output"
}

export DEVELOPER_DIR="$XCODE_DEVELOPER_DIR"
export PATH="$(strip_swiftly_path "$PATH")"
unset SDKROOT TOOLCHAINS SWIFT_EXEC

echo "Using DEVELOPER_DIR=$DEVELOPER_DIR"
echo "Using swift: $(command -v swift)"
swift --version | sed 's/^/  /'

cd "$ROOT_DIR"
exec npm run tauri -- ios "$@"
