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

Build scripts live in `scripts/` and produce binaries under `dist/<platform>/`. All are run from Linux.

### Linux (native)

```
./scripts/build-linux.sh
```

Output: `dist/linux/nannou_sketch`

### Windows (cross-compile)

Requires the mingw-w64 toolchain:

```
sudo apt install mingw-w64
./scripts/build-windows.sh
```

Output: `dist/windows/nannou_sketch.exe`

### macOS

Cross-compiling for macOS from Linux requires the Apple SDK via [osxcross](https://github.com/tpoechtrager/osxcross), which can't be obtained legally without owning a Mac. If you have osxcross set up, `./scripts/build-macos.sh` will use it.

Otherwise, build a real macOS binary with no local macOS toolchain by pushing to GitHub and running the included workflow (`.github/workflows/release.yml`), which builds on a `macos-latest` runner — trigger it manually from the Actions tab or by pushing a `v*` tag.

### All at once

```
./scripts/build-all.sh
```

Builds Linux and Windows, and macOS if osxcross is available; otherwise skips it with a pointer to the GitHub Actions workflow.
# nannou-video-mixer

## Controls

| Action | How |
|---|---|
| Add a module | Click its row in the sidebar (scroll the sidebar with the wheel) |
| Select | Click a module; Shift+click adds/removes; drag on empty canvas for a rectangle (Cmd/Ctrl or Shift adds) |
| Move | Drag a module (moves the whole selection) or a group's title bar |
| Pan / zoom | Wheel or two-finger scroll, middle-drag, Shift+drag on empty canvas / Cmd/Ctrl+wheel, pinch, `+` `-` `0`; `H` recenters |
| Number box | Drag to adjust (Shift = fine), click to type, right-click for range / clamp |
| Copy / cut / paste | Cmd/Ctrl + `C` / `X` / `V` (pastes under the mouse) |
| Duplicate | `D` or Cmd/Ctrl+`D` |
| Delete | `Delete` / `Backspace` |
| Select all | Cmd/Ctrl+`A` |
| Group / ungroup | Cmd/Ctrl+`G` / Cmd/Ctrl+Shift+`G`; double-click a group's title to rename it, the star saves it to **My groups** in the sidebar (right-click there to remove it) |
| Auto layout | `L`, or right-click the canvas for row / column arrangements |
| Previews / minimize | `P` hides (or shows) all previews, `M` minimizes (or expands) all |
| Context menu | Right-click the canvas or a module |
| Fullscreen | `F`; leave with a click, `Esc`, `Space`, `Enter` or `F` |

Layout commands, `P` and `M` act on the selection when two or more modules are selected, otherwise on everything.

## Video

**Sources → Video** (or drop a video file on the window) plays a file with a
variable-speed playhead: **Speed** goes from -4x to 4x (negative plays
backwards), the transport row has play/pause and Loop / Ping-pong / Once, and
the bar under it scrubs. Both Speed and the scrub position have input ports, so
an LFO or a Lag can drive them.

How it stays fast and bounded:

* Decoding runs on its own thread per video (like cameras), with FFmpeg's
  frame/slice threading. The render loop only uploads.
* Frames are kept as YUV 4:2:0 (1.5 bytes/pixel) and converted to RGB on the
  GPU. Anything taller than 1080p is scaled down while decoding.
* Each video has a 256 MB frame cache; the frames farthest from the playhead
  are evicted first. Playing backwards seeks to the previous keyframe, decodes
  forward through that GOP and serves it in reverse, preparing the previous
  GOP before the playhead gets there.
* A Video module that isn't connected to Output (and has no preview) stops
  its decoder and frees its cache.

Long-GOP files (typical H.264/H.265 from phones and cameras) scrub and reverse
fine, but every backwards GOP costs one decode pass. Intra-only codecs
(ProRes, DNxHD, MJPEG, HAP-like) are cheapest for heavy scrubbing.

**Possible future change:** transcode on import to an intra-only proxy (kept
in memory or on disk, via `ffmpeg-next`) so every frame is a keyframe —
instant scrubbing/reverse on any file, at the cost of an import step and
proxy storage.

## WLED controllers

**Sources → WLED** reads the inputs of a [WLED](https://kno.wled.ge)
controller — potentiometers on analog pins, touch pads, buttons and switches,
as configured in WLED's *LED & Hardware* settings — and sends one of them down
its cable as a 0..1 number.

* The box lists every input the device reports, with an activity light (lit
  while it moves or is pressed), a level bar and the raw reading. Click a row
  to choose which input the box outputs. **Raw max** is the full-scale
  reading: 4095 for ESP32 (default), 1023 for ESP8266.
* The **WLED** button at the bottom of the sidebar opens the controllers
  panel: every tracked device with its connection state (online / connecting
  / no data / error) and input count, **Scan network** (mDNS, in the
  background), add by IP or host name, remove, and the poll rate. The light
  on the button sums them up. Devices are remembered between runs.
* Polling runs on background threads (one per device, 2 s timeout), so a
  slow or missing device never stalls the picture.

For range conversion, **Math → Map** rescales a number from one range to
another (optionally clamped to the output range) and **Math → Clamp** holds it
between a minimum and a maximum; both show input → output live.
