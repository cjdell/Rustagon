#!/usr/bin/env python3
"""Regenerate tools/debug/lib/fonts.ts from the badge's two fonts.

The OCR tool needs the exact glyph bitmaps the badge renders with:
  * FONT_10X20 — embedded-graphics `mono_font::ascii::FONT_10X20`, used by the
    menu system (app/menu, libs/display_renderer). Glyphs come from the
    embedded-graphics crate's PNG at:
      <cargo registry>/embedded-graphics-0.8.1/fonts/png/ascii/font_10x20.png
  * FONT_5X7   — the SDK's 5x7 font (sdk/src/gfx/font.rs, Adafruit GFX 5x7
    table, column-major bytes with bit 0 = top row).

Pure Python stdlib — no PIL, no third-party deps. Any python3 works.

Usage: python3 tools/debug/scripts/gen_fonts.py
Reads the PNG path from FONT_10X20_PNG (defaults to the cargo registry
location) and sdk/src/gfx/font.rs relative to the repo root. Writes
lib/fonts.ts.

Only regenerate after either font changes upstream; the checked-in table is
self-contained so the OCR tool has no runtime dependency on cargo's registry.
"""

from __future__ import annotations

import os
import re
import struct
import sys
import zlib
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
OUT = REPO / "tools" / "debug" / "lib" / "fonts.ts"
SDK_FONT = REPO / "sdk" / "src" / "gfx" / "font.rs"


def read_png_grayscale(path: Path) -> tuple[int, int, list[list[int]]]:
    """Decode an 8-bit grayscale PNG (bit depth 8, colour type 0) using stdlib
    zlib. Returns (width, height, rows) where each row is a list of 0/1 values
    (1 = pixel > 127). Only filter types 0-4 are handled."""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos = 8
    width = height = bit_depth = color_type = None
    idat = b""
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        typ = data[pos + 4 : pos + 8]
        payload = data[pos + 8 : pos + 8 + length]
        if typ == b"IHDR":
            width, height, bit_depth, color_type = struct.unpack(">IIBB", payload[:10])
            if bit_depth != 8 or color_type != 0:
                sys.exit(f"unsupported PNG {bit_depth}-bit colour type {color_type}")
        elif typ == b"IDAT":
            idat += payload
        pos += 12 + length

    raw = zlib.decompress(idat)
    stride = width + 1
    rows: list[list[int]] = []
    prev = [0] * width
    for y in range(height):
        line = bytearray(raw[y * stride + 1 : (y + 1) * stride])
        filt = raw[y * stride]
        if filt == 1:  # Sub
            for x in range(1, width):
                line[x] = (line[x] + line[x - 1]) & 0xFF
        elif filt == 2:  # Up
            for x in range(width):
                line[x] = (line[x] + prev[x]) & 0xFF
        elif filt == 3:  # Average
            for x in range(width):
                left = line[x - 1] if x > 0 else 0
                line[x] = (line[x] + ((left + prev[x]) >> 1)) & 0xFF
        elif filt == 4:  # Paeth
            for x in range(width):
                a = line[x - 1] if x > 0 else 0
                b = prev[x]
                c = prev[x - 1] if x > 0 else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[x] = (line[x] + pr) & 0xFF
        elif filt != 0:
            sys.exit(f"unsupported PNG filter {filt}")
        rows.append([1 if v > 127 else 0 for v in line])
        prev = line
    return width, height, rows


def load_10x20() -> list[list[int]]:
    default = next(
        (p for p in Path.home().joinpath(".cargo/registry/src").glob("*/embedded-graphics-0.8.1/fonts/png/ascii/font_10x20.png")),
        None,
    )
    png_path = Path(os.environ.get("FONT_10X20_PNG", default or ""))
    if not png_path.exists():
        sys.exit(f"font_10x20.png not found (looked at {png_path}); set FONT_10X20_PNG")
    w, h, px = read_png_grayscale(png_path)
    if (w, h) != (160, 120):
        sys.exit(f"unexpected font_10x20.png size {w}x{h}")
    glyphs: list[list[int]] = []
    for i in range(95):
        x = (i % 16) * 10
        y = (i // 16) * 20
        rows: list[int] = []
        for r in range(20):
            m = 0
            for c in range(10):
                if px[y + r][x + c]:
                    m |= 1 << (9 - c)
            rows.append(m)
        glyphs.append(rows)
    return glyphs


def load_5x7() -> list[list[int]]:
    src = SDK_FONT.read_text()
    nums = [int(n, 16) for n in re.findall(r"0x([0-9A-Fa-f]{2})", src)]
    if len(nums) != 95 * 5:
        sys.exit(f"sdk font.rs: expected 95*5 bytes, found {len(nums)}")
    glyphs: list[list[int]] = []
    for i in range(95):
        cols = nums[i * 5 : (i + 1) * 5]
        rows: list[int] = []
        for r in range(7):
            m = 0
            for c in range(5):
                if cols[c] & (1 << r):
                    m |= 1 << (4 - c)
            rows.append(m)
        glyphs.append(rows)
    return glyphs


def main() -> None:
    g10 = load_10x20()
    g7 = load_5x7()
    lines = [
        "// Generated OCR font tables — do not edit by hand.",
        "// Regenerate with: python3 tools/debug/scripts/gen_fonts.py",
        "// 10x20: embedded-graphics mono_font::ascii::FONT_10X20 (fonts/png/ascii/font_10x20.png)",
        "// 5x7:   sdk/src/gfx/font.rs (Adafruit GFX 5x7 table)",
        "",
        "export interface Glyph { code: number; rows: number[]; }",
        "",
        "// ASCII 32..126, 10x20 (rows are 10-bit bitmasks, MSB = left)",
        "export const FONT_10X20: Glyph[] = [",
    ]
    for i, rows in enumerate(g10):
        lines.append(f"  {{ code: {32 + i}, rows: [{', '.join(str(r) for r in rows)}] }},")
    lines += ["];", "", "// ASCII 32..126, 5x7 (rows are 5-bit bitmasks, MSB = left)", "export const FONT_5X7: Glyph[] = ["]
    for i, rows in enumerate(g7):
        lines.append(f"  {{ code: {32 + i}, rows: [{', '.join(str(r) for r in rows)}] }},")
    lines += ["];", ""]
    OUT.write_text("\n".join(lines))
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
