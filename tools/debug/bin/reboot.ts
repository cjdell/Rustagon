// Reboot the badge via the HTTP API.
//
// Usage: deno run --allow-net bin/reboot.ts [host]
// POSTs to /api/reboot — equivalent to pressing the physical boot button.
// The WebSocket reconnects automatically after reboot (the badge serves the
// web UI itself, so it comes back on the same IP).

import { parsePositional } from "../lib/args.ts";

const host = parsePositional(Deno.args, [])[0] ?? Deno.env.get("BADGE_HOST") ?? "192.168.49.144";

const res = await fetch(`http://${host}/api/reboot`, { method: "POST" });
console.log(`reboot: HTTP ${res.status}`);
if (res.status !== 200) {
  console.log(await res.text());
  Deno.exit(1);
}
