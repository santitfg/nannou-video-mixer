#!/usr/bin/env bash
# Linux binary: a native build, so it has to run on Linux (ALSA, udev and v4l
# development files are linked in; see setup-linux.sh).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$ROOT"

[ "$(host_os)" = "linux" ] || only_on Linux "a Linux machine"

TARGET="x86_64-unknown-linux-gnu"
ensure_target "$TARGET"
echo "==> Building for Linux ($TARGET)"
cargo build --release --target "$TARGET"
package linux "target/$TARGET/release/$BIN"
