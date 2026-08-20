// Continuous screen recording with change detection.
//
// Usage: deno run --allow-net --allow-write bin/record.ts [host] [--dir DIR] [--timeout SEC] [--ascii]
//   --dir DIR     output directory (default: ./record)
//   --timeout SEC stop after SEC seconds (default: 30)
//   --ascii       print ASCII for each changed frame
//
// Writes <dir>/frame-<NNN>-<hash>.png (+ .txt) only when the frame hash changes,
// plus <dir>/changes.log with one line per change. Prints the final hash.

import { connectBadge } from "../lib/badge.ts";
import { hashFrame, renderAscii, saveFrame } from "../lib/framebuffer.ts";

import { getFlag, parsePositional } from "../lib/args.ts";

const args = Deno.args;
const host = parsePositional(args, ["--dir", "--timeout"])[0] ?? Deno.env.get("BADGE_HOST") ?? "192.168.49.144";
const dir = getFlag(args, "--dir") ?? "record";
const timeoutSec = Number(getFlag(args, "--timeout") ?? 30);
const ascii = args.includes("--ascii");

await Deno.mkdir(dir, { recursive: true });
const changesLog = await Deno.open(`${dir}/changes.log`, { create: true, write: true, append: true });

let lastHash = "";
let count = 0;

const conn = await connectBadge(host, { connectTimeoutSec: 10, onFrame: async (px) => {
  const h = hashFrame(px);
  if (h === lastHash) return;
  lastHash = h;
  count++;
  const base = `${dir}/frame-${String(count).padStart(3, "0")}-${h}`;
  await saveFrame(px, base);
  const line = `${new Date().toISOString()} ${h} ${base}.png\n`;
  await changesLog.write(new TextEncoder().encode(line));
  console.error(`[record] #${count} ${h}`);
  if (ascii) console.log(renderAscii(px));
}});

const deadline = Date.now() + timeoutSec * 1000;
while (Date.now() < deadline) {
  await new Promise((r) => setTimeout(r, 200));
}

changesLog.close();
console.log(`recorded ${count} changed frames -> ${dir}/`);
console.log(`final hash=${hashFrame(conn.latest)}`);
conn.close();
