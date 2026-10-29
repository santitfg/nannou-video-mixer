# nannou_sketch

A simple [Nannou](https://nannou.cc) sketch — a ball bouncing around the window.

## Development (Linux)

Run it directly with cargo while iterating:

```
cargo run
```

Debug builds are slow to start under nannou/wgpu but fine for tweaking the sketch. For smoother frame rates while testing animation/timing, use a release build instead:

```
cargo run --release
```

Optional: auto-rebuild and restart on file changes with [`cargo-watch`](https://github.com/watchexec/cargo-watch):

```
cargo install cargo-watch
cargo watch -x run
```

## Building

Video files are decoded with FFmpeg, which is **compiled from source and
linked statically** (the `build` feature of `ffmpeg-next`), so the binary
needs nothing installed to run. Building it needs, on every platform:

* `git` (FFmpeg's source is cloned during the first build), `make`, a C compiler
* `libclang` (bindgen): Xcode Command Line Tools on macOS, `libclang-dev` on Debian/Ubuntu
* `nasm` on x86/x86_64 hosts (Linux/Windows) for FFmpeg's assembly
* Windows (MSVC): an MSYS2 shell with `make` on `PATH` (FFmpeg's configure is a shell script)

The first build takes a couple of minutes longer; later builds reuse it.

Install these once with the setup script for your platform, then `cargo run`:

| Platform | Script |
|---|---|
| Linux (Debian/Ubuntu) | `./scripts/setup-linux.sh` |
| macOS | `./scripts/setup-macos.sh` |
| Windows 11 | `powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1` (then open a new terminal) |

Build scripts live in `scripts/` and produce a folder under `dist/<platform>/` with the binary **and the `shaders/` folder** (the program reads `./shaders` at run time, so keep them together; the examples are inside the binary). Each script builds for the system it runs on, and tells you what to do instead when it can't:

| Script | Runs on | Output |
|---|---|---|
| `./scripts/build-macos.sh` | macOS (native), or Linux with osxcross | `dist/macos/` |
| `./scripts/build-linux.sh` | Linux | `dist/linux/` |
| `./scripts/build-windows.sh` | Windows (Git Bash, native MSVC) or Linux (mingw-w64 cross) | `dist/windows/` |
| `./scripts/build-all.sh` | anywhere | everything the machine can build, skipping the rest |

### macOS

On a Mac, after `./scripts/setup-macos.sh`:

```
./scripts/build-macos.sh              # your architecture (Apple Silicon or Intel)
./scripts/build-macos.sh --arm64      # Apple Silicon
./scripts/build-macos.sh --x86_64     # Intel
./scripts/build-macos.sh --universal  # both in one binary (lipo)
```

Output: `dist/macos/nannou_sketch` plus `dist/macos/shaders/`. Run it from that folder (`cd dist/macos && ./nannou_sketch`).

### macOS app and .dmg

```
./scripts/build-dmg.sh              # your architecture
./scripts/build-dmg.sh --universal  # Apple Silicon + Intel
```

Builds `dist/macos/Video Mixer.app` (with the icon from `assets/icon.icns`) and `dist/Video-Mixer-<version>-<arch>.dmg`, a disk image with an Applications shortcut. A double-clicked app has `/` as its working directory, so the app starts through a small launcher that works inside `~/Library/Application Support/Video Mixer/`: shaders (seeded from the bundle, your edits are kept), settings, presets and the last session live there. The bundle is signed ad hoc, which is enough to run it on the Mac that built it and to ask for camera access; for other people's Macs it needs a Developer ID signature and notarization (otherwise right-click → Open the first time).

The icon is drawn by `scripts/make-icon.py` (standard-library Python); edit it and run it to regenerate `assets/icon.png` and `assets/icon.icns`.

### Linux and Windows

Linux needs a Linux machine (`./scripts/build-linux.sh`). Windows builds natively from Git Bash after `setup-windows.ps1`, or cross-compiles from Linux with `sudo apt install mingw-w64`.

The Windows `.exe` links the C runtime statically (`.cargo/config.toml`), so it runs on any Windows 10/11 x64 without the Visual C++ Redistributable. Release builds open no console window.

### Windows installer

On Windows, after `setup-windows.ps1`:

```
powershell -ExecutionPolicy Bypass -File scripts\build-windows-installer.ps1
```

Builds the release `.exe` and packs `dist\Video-Mixer-<version>-x64-setup.exe` with [Inno Setup](https://jrsoftware.org/isinfo.php) (installed with winget on first use; `-SkipBuild` reuses the last build). The installer needs no admin rights: it installs to `%LOCALAPPDATA%\Programs\Video Mixer`, and the Start menu shortcut starts the program in `%APPDATA%\Video Mixer`, where shaders (seeded from the installer, your edits are kept on updates), settings, presets and the last session live. Uninstalling keeps that folder. The installer script is `scripts/windows-installer.iss`.

### Other platforms: GitHub Actions

Cross-compiling a GUI app for another OS needs that OS's SDK, so the reliable way to get every platform from one machine is the included workflow (`.github/workflows/release.yml`): it builds Linux, Windows and macOS (Apple Silicon and Intel) on their own runners. Trigger it from the Actions tab or by pushing a `v*` tag, then download the artifacts.
