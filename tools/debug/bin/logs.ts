// Capture serial logs from the badge via espflash monitor.
//
// Usage: deno run --allow-run --allow-write bin/logs.ts [port] [--out FILE] [--timeout SEC]
//   port         serial device              (default: auto-detect / first cu.usbmodem*)
//   --out FILE   log destination             (default: ./serial.log)
//   --timeout SEC stop after SEC seconds     (default: 0 = run until Ctrl-C / killed)
//
// Spawns `espflash monitor --non-interactive` (hard-reset by default, so the
// badge reboots and you get a clean boot log), strips ANSI colour codes, and
// writes timestamped lines to FILE while echoing them to stderr.

import { getFlag, parsePositional } from "../lib/args.ts";

const args = Deno.args;
const portArg = parsePositional(args, ["--port", "--out", "--timeout"])[0] ?? getFlag(args, "--port");
const outFile = getFlag(args, "--out") ?? "serial.log";
const timeoutSec = Number(getFlag(args, "--timeout") ?? 0);

async function detectPort(): Promise<string> {
  if (portArg) return portArg;
  for (const name of ["/dev/cu.usbmodem*", "/dev/tty.usbmodem*", "/dev/ttyACM*", "/dev/ttyUSB*"]) {
    const g = new Deno.Command("bash", { args: ["-c", `ls ${name} 2>/dev/null | head -1`] }).outputSync();
    const p = new TextDecoder().decode(g.stdout).trim();
    if (p) return p;
  }
  throw new Error("no serial port found; pass one explicitly (e.g. /dev/cu.usbmodem1101)");
}

const port = await detectPort();
console.error(`[logs] capturing ${port} -> ${outFile}`);

const cmd = new Deno.Command("espflash", {
  args: ["monitor", "--port", port, "--non-interactive"],
  stdout: "piped",
  stderr: "piped",
});
const proc = cmd.spawn();

const file = await Deno.open(outFile, { create: true, write: true, append: true });
const ansi = /\x1b\[[0-9;]*m/g;

async function pump(stream: ReadableStream<Uint8Array> | null, isErr: boolean) {
  if (!stream) return;
  const reader = stream.getReader();
  const dec = new TextDecoder();
  let buf = "";
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += dec.decode(value, { stream: true });
    let idx;
    while ((idx = buf.indexOf("\n")) >= 0) {
      let line = buf.slice(0, idx);
      buf = buf.slice(idx + 1);
      line = line.replace(ansi, "").replace(/\r$/, "").trim();
      if (!line) continue;
      const ts = new Date().toISOString().slice(11, 23);
      const tagged = `[${ts}] ${line}\n`;
      await file.write(new TextEncoder().encode(tagged));
      console.error(tagged.trimEnd());
    }
  }
}

pump(proc.stdout, false);
pump(proc.stderr, true);

if (timeoutSec > 0) {
  setTimeout(() => {
    console.error(`[logs] timeout after ${timeoutSec}s`);
    proc.kill();
    Deno.exit(0);
  }, timeoutSec * 1000);
}

const status = await proc.status;
file.close();
Deno.exit(status.success ? 0 : 1);
