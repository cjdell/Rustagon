// Capture one settled frame from the badge screen and save it.
//
// Usage: deno run --allow-net --allow-write bin/screenshot.ts [host] [out-prefix] [--cols N]
//   host       badge IP/hostname            (default: env BADGE_HOST or 192.168.49.144)
//   out-prefix PNG/ASCII output prefix      (default: ./screen)
//   --cols N   ASCII render width           (default: 96)
//
// Writes <prefix>.png and <prefix>.txt, prints the frame hash.

import { connectBadge } from "../lib/badge.ts";
import { renderAscii, saveFrame } from "../lib/framebuffer.ts";

import { parsePositional } from "../lib/args.ts";

const args = Deno.args;
const positional = parsePositional(args, ["--cols"]);
const host = positional[0] ?? Deno.env.get("BADGE_HOST") ?? "192.168.49.144";
const outPrefix = positional[1] ?? "screen";
const cols = Number(getFlagSafe(args, "--cols") ?? 96);

function getFlagSafe(args: string[], name: string): string | undefined {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
}

const conn = await connectBadge(host, { connectTimeoutSec: 10 });
await conn.settle(300); // let any in-flight animation finish

const hash = await saveFrame(conn.latest, outPrefix, cols);
console.log(`frame ${conn.frameCount} hash=${hash}`);
console.log(`wrote ${outPrefix}.png and ${outPrefix}.txt`);
console.log(renderAscii(conn.latest, cols));
conn.close();
