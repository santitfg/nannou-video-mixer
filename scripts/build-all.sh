#!/usr/bin/env bash
# Builds every platform this machine can build; skips the rest with a note.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

for platform in macos linux windows; do
  echo
  echo "##### $platform"
  if ! ./build-$platform.sh; then
    echo "==> Skipped $platform (see the message above)"
  fi
done
