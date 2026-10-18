#!/usr/bin/env python3
"""Draws the app icon (three overlapping R/G/B discs that add up, like a video
mixer) into assets/icon.png, and on macOS builds assets/icon.icns from it.
Standard library only; run it again after changing the design."""
import math, os, struct, subprocess, sys, zlib

SIZE = 1024
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets")

def smooth(x):  # 1 inside, 0 outside, ~1 px soft edge
    return max(0.0, min(1.0, x))

def rounded_box(px, py, cx, cy, half, radius):
    qx, qy = abs(px - cx) - (half - radius), abs(py - cy) - (half - radius)
    outside = math.hypot(max(qx, 0.0), max(qy, 0.0))
    return outside + min(max(qx, qy), 0.0) - radius  # signed distance

def render():
    discs = []
    for angle, color in ((90, (255, 70, 95)), (210, (60, 230, 140)), (330, (70, 130, 255))):
        a = math.radians(angle)
        discs.append((512 + 118 * math.cos(a), 540 - 118 * math.sin(a), 192.0, color))
    rows = []
    for y in range(SIZE):
        row = bytearray()
        t = y / (SIZE - 1)
        bg = (int(30 - 16 * t), int(38 - 20 * t), int(72 - 38 * t))  # navy, lighter at the top
        for x in range(SIZE):
            cover = smooth(0.5 - rounded_box(x + 0.5, y + 0.5, 512, 512, 412, 185))
            if cover <= 0.0:
                row += b"\x00\x00\x00\x00"
                continue
            r, g, b = bg
            for cx, cy, rad, (dr, dg, db) in discs:
                k = smooth(0.5 - (math.hypot(x + 0.5 - cx, y + 0.5 - cy) - rad)) * 0.6
                r, g, b = r + dr * k, g + dg * k, b + db * k
            row += bytes((min(255, int(r)), min(255, int(g)), min(255, int(b)), int(cover * 255)))
        rows.append(bytes(row))
    return rows

def write_png(path, rows):
    raw = b"".join(b"\x00" + r for r in rows)
    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)

def build_icns(png):
    if sys.platform != "darwin":
        print("not macOS: skipped icon.icns")
        return
    iconset = os.path.join(OUT, "icon.iconset")
    os.makedirs(iconset, exist_ok=True)
    for size in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
            subprocess.run(["sips", "-z", str(size * scale), str(size * scale), png, "--out", os.path.join(iconset, name)],
                           check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(OUT, "icon.icns")], check=True)
    subprocess.run(["rm", "-rf", iconset], check=True)

if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    png = os.path.join(OUT, "icon.png")
    write_png(png, render())
    build_icns(png)
    print("wrote", os.path.normpath(OUT))
