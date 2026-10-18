#!/usr/bin/env bash
# Windows binary.
#  - On Windows (Git Bash / MSYS2): a native MSVC build. Run setup-windows.ps1
#    once first. FFmpeg's configure needs MSYS2's make on PATH.
#  - On Linux: cross-compiled with mingw-w64.
#  - On macOS: not possible from here; use the GitHub Actions workflow.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$ROOT"

case "$(host_os)" in
  windows)
    TARGET="x86_64-pc-windows-msvc"
    ensure_target "$TARGET"
    echo "==> Building for Windows ($TARGET)"
    cargo build --release --target "$TARGET"
    package windows "target/$TARGET/release/$BIN.exe"
    ;;
  linux)
    TARGET="x86_64-pc-windows-gnu"
    if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
      echo "error: mingw-w64 linker not found. On Debian/Ubuntu: sudo apt install mingw-w64" >&2
      exit 1
    fi
    ensure_target "$TARGET"
    echo "==> Building for Windows ($TARGET)"
    cargo build --release --target "$TARGET"
    package windows "target/$TARGET/release/$BIN.exe"
    ;;
  *) only_on Windows "Windows or Linux" ;;
esac
