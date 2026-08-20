// Watch the badge screen for changes.
//
// Usage: deno run --allow-net bin/watch.ts [host] [--timeout SEC] [--expect-hash H] [--changed] [--ascii]
//   --timeout SEC     stop after SEC seconds (default: 15)
//   --expect-hash H   exit 0 as soon as a frame with hash H arrives
//   --changed         exit 0 as soon as the frame hash differs from the first frame
//   --ascii           print an ASCII render of every changed frame
//
// Exit codes: 0 = condition met (or --expect-hash/--changed satisfied), 1 = timeout,
// 2 = connection failure.

import { connectBadge } from "../lib/badge.ts";
import { hashFrame, renderAscii } from "../lib/framebuffer.ts";

import { getFlag, parsePositional } from "../lib/args.ts";

const args = Deno.args;
const host = parsePositional(args, ["--timeout", "--expect-hash"])[0] ?? Deno.env.get("BADGE_HOST") ?? "192.168.49.144";

const timeoutSec = Number(getFlag(args, "--timeout") ?? 15);
const expectHash = getFlag(args, "--expect-hash");
const changed = args.includes("--changed");
const ascii = args.includes("--ascii");

let firstHash: string | null = null;

const conn = await connectBadge(host, { connectTimeoutSec: 10 });
console.error(`[watch] first frame hash=${hashFrame(conn.latest)}`);

const deadline = Date.now() + timeoutSec * 1000;
let result = 1;

while (Date.now() < deadline) {
  const h = hashFrame(conn.latest);
  if (firstHash === null) firstHash = h;
  if (expectHash && h === expectHash) {
    console.log(`MATCH ${h}`);
    result = 0;
    break;
  }
  if (changed && h !== firstHash) {
    console.log(`CHANGED ${firstHash} -> ${h}`);
    if (ascii) console.log(renderAscii(conn.latest));
    result = 0;
    break;
  }
  await new Promise((r) => setTimeout(r, 100));
}

if (result === 1) {
  console.log(`TIMEOUT after ${timeoutSec}s (last hash ${hashFrame(conn.latest)})`);
}
conn.close();
Deno.exit(result);
