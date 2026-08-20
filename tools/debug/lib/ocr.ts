// OCR for the badge framebuffer — converts screen pixels into text.
//
// Two fonts are supported:
//   FONT_10X20 — embedded-graphics mono_font::ascii::FONT_10X20, used by the
//                menu system (app/menu, display_renderer).
//   FONT_5X7   — the SDK's 5x7 font (sdk/src/gfx/font.rs), used by WASM apps.
//
// Strategy per text line:
//   1. Detect "inverted" lines (the menu's selection highlight): a horizontal
//      run of columns whose fill ratio is very high (solid white bar). OCR the
//      *black* pixels inside that bar as ink.
//   2. Otherwise, split the line into column runs of ink, skip dense runs
//      (icons / graphics), and OCR each remaining run.
//   3. For each run, try both fonts and several y/start alignments (glyphs are
//      fixed-width and adjacent), scoring by average mismatch per non-space
//      glyph; keep the best.

import { DISPLAY_HEIGHT, DISPLAY_WIDTH } from "./framebuffer.ts";
import { FONT_10X20, FONT_5X7, type Glyph } from "./fonts.ts";

export interface OcrLine {
  text: string;
  y: number; // top row of the line in the framebuffer
  cost: number; // average mismatch per non-space glyph (0 = perfect)
  inverted: boolean;
  font: "10x20" | "5x7";
}

const FONT_INFO: Record<string, { w: number; h: number; adv: number; glyphs: Glyph[] }> = {
  "10x20": { w: 10, h: 20, adv: 10, glyphs: FONT_10X20 },
  "5x7": { w: 5, h: 7, adv: 6, glyphs: FONT_5X7 },
};

function popcount(n: number): number {
  let c = 0;
  while (n) {
    n &= n - 1;
    c++;
  }
  return c;
}

/** Extract a w×h cell starting at (x,y) as row bitmasks (MSB = left). inkAt(x,y) → 0/1. */
function cellRows(inkAt: (x: number, y: number) => number, x: number, y: number, w: number, h: number): number[] {
  const rows: number[] = [];
  for (let r = 0; r < h; r++) {
    let m = 0;
    for (let c = 0; c < w; c++) {
      if (inkAt(x + c, y + r)) m |= 1 << (w - 1 - c);
    }
    rows.push(m);
  }
  return rows;
}

function matchCost(cell: number[], glyph: Glyph): number {
  let diff = 0;
  const mask = 0xffff;
  for (let r = 0; r < cell.length; r++) {
    diff += popcount(((cell[r] ?? 0) ^ (glyph.rows[r] ?? 0)) & mask);
  }
  return diff;
}

/**
 * Grid-walk OCR over a region [x0..x1] × [y..y+h). Cells are sampled every
 * `adv` px. Returns text + average mismatch per non-space glyph; Infinity if
 * no non-space glyphs matched.
 */
export function ocrRegion(inkAt: (x: number, y: number) => number, y: number, x0: number, x1: number, font: string): { text: string; score: number } {
  const { w, h, adv, glyphs } = FONT_INFO[font];
  const threshold = Math.max(2, Math.round(w * h * 0.25));
  let x = x0;
  let text = "";
  let totalCost = 0;
  let nonSpace = 0;
  while (x + w - 1 <= x1) {
    const cell = cellRows(inkAt, x, y, w, h);
    let best: Glyph | null = null;
    let bestCost = Infinity;
    for (const g of glyphs) {
      const c = matchCost(cell, g);
      if (c < bestCost) {
        bestCost = c;
        best = g;
      }
    }
    if (best && bestCost <= threshold) {
      const ch = String.fromCharCode(best!.code);
      text += ch;
      totalCost += bestCost;
      if (ch !== " ") nonSpace++;
    } else {
      text += "?";
      totalCost += w * h;
      nonSpace++;
    }
    x += adv;
  }
  return { text, score: nonSpace ? totalCost / nonSpace : Infinity };
}

/**
 * OCR a decoded frame. Returns lines ordered top-to-bottom, skipping lines
 * that look like icons/graphics (no low-cost glyph matches).
 *
 * Strategy:
 *  1. Detect selection bars globally (row-based): a selection highlight is a
 *     ~20-row band of mostly-white pixels in the text area. The black glyphs
 *     inside each bar are OCR'd as inverted text. Detecting bars before line
 *     grouping avoids descenders bridging two menu items into one line.
 *  2. Remaining ink rows are grouped into lines and OCR'd as normal
 *     (white-on-black) text, split into column runs.
 */
export function ocrFrame(px: Uint8Array, minCost = 8): OcrLine[] {
  const inkAt = (x: number, y: number) => (x >= 0 && x < DISPLAY_WIDTH && y >= 0 && y < DISPLAY_HEIGHT ? px[y * DISPLAY_WIDTH + x] : 0);
  const out: OcrLine[] = [];
  const consumed = new Array<boolean>(DISPLAY_HEIGHT).fill(false);

  // 1. Selection bars.
  //    For each row, find the longest run of consecutive white pixels and its
  //    start position. Bar text (black glyphs on a white bar) fragments the
  //    run, so candidate rows only need a modest white run in the text area
  //    (x >= 50, past the menu icon column); a band is a bar when it spans
  //    >= 15 rows and contains at least 2 solid rows (white run >= 15), which
  //    are the bar's edges around the text.
  const whiteRun = new Array<number>(DISPLAY_HEIGHT).fill(0);
  const whiteRunX = new Array<number>(DISPLAY_HEIGHT).fill(-1);
  for (let y = 0; y < DISPLAY_HEIGHT; y++) {
    let run = 0;
    let runX = 0;
    for (let x = 0; x < DISPLAY_WIDTH; x++) {
      if (px[y * DISPLAY_WIDTH + x]) {
        if (run === 0) runX = x;
        run++;
        if (run > whiteRun[y]) {
          whiteRun[y] = run;
          whiteRunX[y] = runX;
        }
      } else {
        run = 0;
      }
    }
  }
  const isCandidate = (y: number) => whiteRun[y] >= 4 && whiteRunX[y] >= 50;
  const rawBands: Array<{ y0: number; y1: number }> = [];
  let by0 = -1;
  for (let y = 0; y <= DISPLAY_HEIGHT; y++) {
    const cand = y < DISPLAY_HEIGHT && isCandidate(y);
    if (cand && by0 < 0) by0 = y;
    if (!cand && by0 >= 0) {
      rawBands.push({ y0: by0, y1: y - 1 });
      by0 = -1;
    }
  }
  const bands: Array<{ y0: number; y1: number; x0: number; x1: number }> = [];
  for (const band of rawBands) {
    const last = bands[bands.length - 1];
    if (last && band.y0 - last.y1 <= 3) {
      last.y1 = band.y1;
    } else {
      bands.push({ y0: band.y0, y1: band.y1, x0: 0, x1: 0 });
    }
  }
  for (const band of bands) {
    if (band.y1 - band.y0 + 1 < 15) continue;
    let solid = 0;
    for (let yy = band.y0; yy <= band.y1; yy++) if (whiteRun[yy] >= 15) solid++;
    if (solid < 2) continue;
    // bar bbox (white extent)
    let bx0 = DISPLAY_WIDTH, bx1 = 0;
    for (let yy = band.y0; yy <= band.y1; yy++) {
      for (let x = 0; x < DISPLAY_WIDTH; x++) {
        if (px[yy * DISPLAY_WIDTH + x]) {
          if (x < bx0) bx0 = x;
          if (x > bx1) bx1 = x;
        }
      }
    }
    band.x0 = bx0;
    band.x1 = bx1;
    for (let yy = band.y0; yy <= band.y1; yy++) consumed[yy] = true;
  }
  for (const band of bands) {
    const blackAt = (x: number, y: number) =>
      x >= band.x0 && x <= band.x1 && y >= band.y0 && y <= band.y1 && px[y * DISPLAY_WIDTH + x] === 0 ? 1 : 0;
    const line = bestOcr(blackAt, band.y0, band.y1, band.x0, band.x1, minCost);
    if (line) out.push({ ...line, inverted: true });
  }

  // 2. Normal text lines: group remaining ink rows, OCR each line.
  const ranges: Array<{ y0: number; y1: number }> = [];
  let y0 = -1;
  for (let y = 0; y <= DISPLAY_HEIGHT; y++) {
    let has = false;
    if (y < DISPLAY_HEIGHT && !consumed[y]) {
      for (let x = 0; x < DISPLAY_WIDTH; x++) {
        if (px[y * DISPLAY_WIDTH + x]) { has = true; break; }
      }
    }
    if (has && y0 < 0) y0 = y;
    if (!has && y0 >= 0) {
      ranges.push({ y0, y1: y - 1 });
      y0 = -1;
    }
  }

  for (const { y0, y1 } of ranges) {
    const h = y1 - y0 + 1;
    if (h < 4) continue;

    const colFill = new Array<number>(DISPLAY_WIDTH).fill(0);
    for (let x = 0; x < DISPLAY_WIDTH; x++) {
      let n = 0;
      for (let y = y0; y <= y1; y++) n += px[y * DISPLAY_WIDTH + x];
      colFill[x] = n / h;
    }

    // Split into ink runs. Merge with a 6-col gap: 10x20 glyphs have zero
    // character spacing, so adjacent glyphs are separated only by each glyph's
    // empty edge columns (2-4 cols); the menu icon column also joins the text
    // this way. Dense runs (icons/graphics) are dropped.
    const inkCols: number[] = [];
    for (let x = 0; x < DISPLAY_WIDTH; x++) if (colFill[x] > 0) inkCols.push(x);
    const runs = mergeRuns(inkCols, 6);
    for (const [rx0, rx1] of runs) {
      if (rx1 - rx0 + 1 < 5) continue;
      let filled = 0;
      for (let x = rx0; x <= rx1; x++) filled += colFill[x];
      const fill = filled / (rx1 - rx0 + 1);
      if (fill > 0.6) continue; // dense blob — icon or graphic
      // Pad both edges: the run's ink may be narrower than a glyph cell
      // (e.g. "A", "I", "l"), so give the walk room to fit the full cell.
      const line = bestOcr(inkAt, y0, y1, Math.max(0, rx0 - 6), Math.min(rx1 + 10, DISPLAY_WIDTH - 1), minCost);
      if (line) out.push({ ...line, inverted: false });
    }
  }
  out.sort((a, b) => a.y - b.y);
  return out;
}

/** Merge a sorted list of column indices into [start,end] runs, joining gaps ≤ gap. */
function mergeRuns(cols: number[], gap = 0): Array<[number, number]> {
  const runs: Array<[number, number]> = [];
  for (const c of cols) {
    const last = runs[runs.length - 1];
    if (last && c <= last[1] + 1 + gap) {
      last[1] = c;
    } else {
      runs.push([c, c]);
    }
  }
  return runs;
}

/** Try both fonts + y alignments over a region; return the best OcrLine or null. */
function bestOcr(
  inkAt: (x: number, y: number) => number,
  y0: number,
  y1: number,
  x0: number,
  x1: number,
  minCost: number,
): OcrLine | null {
  const h = y1 - y0 + 1;
  let best: { text: string; score: number; font: "10x20" | "5x7" } | null = null;
  for (const [font, info] of Object.entries(FONT_INFO) as Array<[string, (typeof FONT_INFO)["10x20"]]>) {
    if (h < info.h - 6 || x1 - x0 + 1 < info.w) continue;
    // yOff may be negative: the first ink row can sit a few pixels below the
    // glyph cell top (icons / descenders start the line early).
    for (const yOff of [-3, -2, -1, 0, 1, 2, h - info.h]) {
      const y = y0 + yOff;
      if (y < 0 || y + info.h - 1 > DISPLAY_HEIGHT - 1) continue;
      // xStart window: up to 24px, so a menu icon (x0..x0+16) can be skipped
      // to reach the text that follows it.
      for (let xs = x0; xs <= Math.min(x0 + 24, x1 - info.w + 1); xs++) {
        const r = ocrRegion(inkAt, y, xs, x1, font);
        if (r.score < Infinity && !r.text.includes("?")) {
          const t = r.text.replace(/^\?+/, "").replace(/\?+$/, "").trim();
          if (t.length === 0) continue;
          if (!best || r.score < best.score) {
            best = { text: t, score: r.score, font: font as "10x20" | "5x7" };
          }
        }
      }
    }
  }
  if (!best || best.score > minCost) return null;
  return { text: best.text, y: y0, cost: Math.round(best.score), inverted: false, font: best.font };
}

/** Convenience: OCR and return just the text lines. */
export function ocrText(px: Uint8Array, minCost = 30): string[] {
  return ocrFrame(px, minCost).map((l) => l.text);
}
