// Send remote button presses to the badge over the WebSocket.
//
// Usage: deno run --allow-net bin/input.ts [host] -- <button> [<button> ...]
//   Each button is sent as press + release (120 ms hold) unless it already ends
//   in "Released". Special words: boot (SystemMessage BootButton).
//
// Examples:
//   deno task dbg:input 192.168.49.144 -- Down Fire
//   deno task dbg:input -- HexA HexAReleased
//   deno task dbg:input -- boot
//   deno task dbg:input -- '{"HexButton":"Fire","HexButton2":null}'   # raw JSON passthrough

import { connectBadge } from "../lib/badge.ts";

const args = Deno.args;
const BUTTON_RE = /^(Up|Down|Left|Right|Fire|Hex[A-F]|Touch\d\d)(Released)?$/;
const positional = args.filter((a) => a !== "" && a !== "--");
const firstIsHost = positional.length > 0 && !BUTTON_RE.test(positional[0]) && positional[0] !== "boot" && !positional[0].startsWith("{");
const host = firstIsHost ? positional[0] : (Deno.env.get("BADGE_HOST") ?? "192.168.49.144");
const buttons = firstIsHost ? positional.slice(1) : positional;
if (buttons.length === 0) {
  console.error("usage: deno run --allow-net bin/input.ts [host] -- <button> [...]");
  Deno.exit(2);
}

const conn = await connectBadge(host, { connectTimeoutSec: 10 });

for (const b of buttons) {
  if (b === "boot") {
    console.log(`> SystemMessage BootButton`);
    await conn.system("BootButton");
  } else if (b.startsWith("{") && b.endsWith("}")) {
    console.log(`> raw ${b}`);
    conn.sendJson(JSON.parse(b));
  } else if (b.endsWith("Released")) {
    console.log(`> HexButton ${b}`);
    conn.sendJson({ HexButton: b });
  } else {
    console.log(`> HexButton ${b} + ${b}Released`);
    await conn.press(b);
  }
  await new Promise((r) => setTimeout(r, 80));
}

await conn.settle(250);
console.log(`done (${conn.frameCount} frames seen)`);
conn.close();
