# Shared helpers for the build scripts (source this, don't run it).
# Written for bash 3.2 so it also runs on the macOS default shell.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="nannou_sketch"

# macos | linux | windows (Git Bash / MSYS / Cygwin) | unknown
host_os() {
  case "$(uname -s)" in
    Darwin) echo macos ;;
    Linux) echo linux ;;
    MINGW* | MSYS* | CYGWIN*) echo windows ;;
    *) echo unknown ;;
  esac
}

ensure_target() {
  if ! rustup target list --installed | grep -q "^$1\$"; then
    echo "==> Adding rust target $1"
    rustup target add "$1"
  fi
}

# package <out dir> <built binary>: the program reads ./shaders at run time,
# so the shaders folder has to travel with the binary.
package() {
  local out="$ROOT/dist/$1" bin="$2"
  mkdir -p "$out"
  cp "$bin" "$out/"
  rm -rf "$out/shaders"
  cp -R "$ROOT/shaders" "$out/shaders"
  echo "==> Done: dist/$1/$(basename "$bin")  (with shaders/)"
}

# Cross-compiling needs toolchains that only exist on some hosts. Say what
# to do instead and stop.
only_on() {
  echo "error: $1 binaries can't be built from $(host_os) with these scripts." >&2
  echo "Build on $2, or use the GitHub Actions workflow (.github/workflows/release.yml)," >&2
  echo "which builds every platform on its own runner." >&2
  exit 1
}
