#!/usr/bin/env bash
# macOS binary. On a Mac this is a plain native build (the Apple toolchain is
# already there); from Linux it needs osxcross.
#
#   ./scripts/build-macos.sh              host architecture (Apple Silicon -> arm64, Intel -> x86_64)
#   ./scripts/build-macos.sh --arm64      Apple Silicon
#   ./scripts/build-macos.sh --x86_64     Intel
#   ./scripts/build-macos.sh --universal  both, joined with lipo into one binary
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$ROOT"

ARM="aarch64-apple-darwin"
INTEL="x86_64-apple-darwin"

case "$(host_os)" in
  macos)
    command -v xcode-select >/dev/null && xcode-select -p >/dev/null 2>&1 || {
      echo "error: Xcode Command Line Tools missing. Run ./scripts/setup-macos.sh" >&2
      exit 1
    }
    case "${1:-}" in
      --arm64) MODE=$ARM ;;
      --x86_64) MODE=$INTEL ;;
      --universal) MODE=universal ;;
      "") [ "$(uname -m)" = "arm64" ] && MODE=$ARM || MODE=$INTEL ;;
      *) echo "usage: $0 [--arm64 | --x86_64 | --universal]" >&2; exit 2 ;;
    esac

    if [ "$MODE" = "universal" ]; then
      for t in $ARM $INTEL; do
        ensure_target "$t"
        echo "==> Building for macOS ($t)"
        cargo build --release --target "$t"
      done
      mkdir -p target/universal
      lipo -create -output "target/universal/$BIN" "target/$ARM/release/$BIN" "target/$INTEL/release/$BIN"
      package macos "target/universal/$BIN"
    else
      ensure_target "$MODE"
      echo "==> Building for macOS ($MODE)"
      cargo build --release --target "$MODE"
      package macos "target/$MODE/release/$BIN"
    fi
    ;;

  linux)
    # Cross-compiling a GUI app (winit + wgpu link Cocoa/Metal) needs the real
    # Apple SDK through osxcross; it can only be extracted from Xcode on a Mac.
    if [ -z "${OSXCROSS_ROOT:-}" ] || ! command -v x86_64-apple-darwin-clang >/dev/null 2>&1; then
      only_on macOS "a Mac (or set up osxcross and export OSXCROSS_ROOT)"
    fi
    export CC_x86_64_apple_darwin="x86_64-apple-darwin-clang"
    export CXX_x86_64_apple_darwin="x86_64-apple-darwin-clang++"
    export CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER="x86_64-apple-darwin-clang"
    ensure_target "$INTEL"
    echo "==> Building for macOS ($INTEL, osxcross)"
    cargo build --release --target "$INTEL"
    package macos "target/$INTEL/release/$BIN"
    ;;

  *) only_on macOS "a Mac" ;;
esac
