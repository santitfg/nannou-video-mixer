#!/usr/bin/env bash
# One-time setup of the build dependencies on macOS (nasm is not needed on ARM).
set -euo pipefail

xcode-select -p >/dev/null 2>&1 || xcode-select --install
echo "==> Ready: cargo run"
