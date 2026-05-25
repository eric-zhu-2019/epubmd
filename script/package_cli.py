#!/usr/bin/env python3
"""Build and zip the Rust epubmd CLI on macOS, Linux, or Windows."""
from __future__ import annotations

import os
import platform
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

APP_NAME = "epubmd"
VERSION = "0.1.0"


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    dist_dir = root / "dist"
    cli_dir = dist_dir / "cli"
    archive_path = dist_dir / f"{APP_NAME}-cli-{VERSION}-{platform.system().lower()}-{platform.machine().lower()}.zip"
    binary_name = f"{APP_NAME}.exe" if os.name == "nt" else APP_NAME
    built_binary = root / "target" / "release" / binary_name

    subprocess.run(["cargo", "build", "--release", "-p", APP_NAME], cwd=root, check=True)

    if cli_dir.exists():
        shutil.rmtree(cli_dir)
    if archive_path.exists():
        archive_path.unlink()
    cli_dir.mkdir(parents=True, exist_ok=True)

    packaged_binary = cli_dir / binary_name
    shutil.copy2(built_binary, packaged_binary)
    if os.name != "nt":
        packaged_binary.chmod(0o755)

    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.write(packaged_binary, arcname=binary_name)

    print(f"CLI binary:  {packaged_binary}")
    print(f"CLI archive: {archive_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
