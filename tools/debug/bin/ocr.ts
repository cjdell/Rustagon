// OCR the badge screen — live or from a saved .frame file.
//
// Usage: deno run --allow-net --allow-read bin/ocr.ts [host | file-prefix] [--cols N]
//   If the argument names an existing `<prefix>.frame` file, OCR that capture;
//   otherwise treat it as a badge host and grab a live frame first.
//   Lines with high mismatch (icons/graphics) are skipped; print with --raw
//   to see them too.
//
// Examples:
//   deno task dbg:ocr 192.168.49.144
//   deno task dbg:ocr ./screen1

import { connectBadge } from "../lib/badge.ts";
import { ocrFrame } from "../lib/ocr.ts";

import { parsePositional } from "../lib/args.ts";

const args = Deno.args;
const target = parsePositional(args, [])[0] ?? Deno.env.get("BADGE_HOST") ?? "192.168.49.144";
const raw = args.includes("--raw");

let px: Uint8Array;
let source: string;

const framePath = target.endsWith(".frame") ? target : `${target}.frame`;
try {
  px = await Deno.readFile(framePath);
  source = framePath;
} catch {
  const conn = await connectBadge(target, { connectTimeoutSec: 10 });
  await conn.settle(300);
  px = conn.latest;
  source = target;
  conn.close();
}

const lines = ocrFrame(px, raw ? 1_000_000 : 40);
if (lines.length === 0) {
  console.log(`(no text found in ${source})`);
} else {
  for (const l of lines) {
    const tag = `${l.inverted ? "INV" : "   "} ${l.font} y=${l.y.toString().padStart(3)} cost=${l.cost}`;
    console.log(`${tag}  ${l.text}`);
  }
}
