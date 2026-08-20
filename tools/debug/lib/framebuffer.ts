// Framebuffer helpers for the badge's WebSocket screen stream.
//
// The device broadcasts a 1-bit-per-pixel bitmask (LSB first within each byte,
// bit set = lit pixel) at 240x240, so each binary frame is exactly 7200 bytes.
// See app/src/http/web_socket.rs (u16_bitmask_to_u8_slice) and
// web/src/lib/device/badge.ts for the same decoding done in the WebUI.

export const DISPLAY_WIDTH = 240;
export const DISPLAY_HEIGHT = 240;
export const FRAME_BYTES = (DISPLAY_WIDTH * DISPLAY_HEIGHT) / 8;

/** Decode a 7200-byte bitmask frame into a row-major 0/1 pixel array (W*H). */
export function decodeFrame(bits: Uint8Array): Uint8Array {
  const px = new Uint8Array(DISPLAY_WIDTH * DISPLAY_HEIGHT);
  for (let p = 0; p < px.length; p++) {
    px[p] = (bits[(p / 8) | 0] & (1 << (p % 8))) ? 1 : 0;
  }
  return px;
}

/** FNV-1a 32-bit hash of the decoded frame — cheap change detector. */
export function hashFrame(px: Uint8Array): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < px.length; i++) {
    h ^= px[i];
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

/** Number of differing pixels between two decoded frames. */
export function diffCount(a: Uint8Array, b: Uint8Array): number {
  let n = 0;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) n++;
  return n;
}

/**
 * Render a decoded frame as ASCII. `cols` is the target width; rows are scaled
 * to preserve aspect ratio (1:1 sampling of the 240x240 framebuffer).
 */
export function renderAscii(px: Uint8Array, cols = 96): string {
  const rows = Math.round((cols * DISPLAY_HEIGHT) / DISPLAY_WIDTH);
  const cw = DISPLAY_WIDTH / cols;
  const ch = DISPLAY_HEIGHT / rows;
  const out: string[] = [];
  for (let r = 0; r < rows; r++) {
    let line = "";
    for (let c = 0; c < cols; c++) {
      const x = Math.min(DISPLAY_WIDTH - 1, Math.floor((c + 0.5) * cw));
      const y = Math.min(DISPLAY_HEIGHT - 1, Math.floor((r + 0.5) * ch));
      line += px[y * DISPLAY_WIDTH + x] ? "#" : " ";
    }
    out.push(line);
  }
  return out.join("\n");
}

// ---------------------------------------------------------------------------
// PNG encoding (1-bit grayscale) using Deno's built-in CompressionStream.
// ---------------------------------------------------------------------------

function crc32(buf: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    c ^= buf[i];
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const dv = new DataView(out.buffer);
  dv.setUint32(0, data.length);
  out.set(new TextEncoder().encode(type), 4);
  out.set(data, 8);
  dv.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

/** Encode a decoded 0/1 frame as a 240x240 1-bit grayscale PNG. */
export async function encodePng(px: Uint8Array, w = DISPLAY_WIDTH, h = DISPLAY_HEIGHT): Promise<Uint8Array> {
  const sig = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = new Uint8Array(13);
  const dv = new DataView(ihdr.buffer);
  dv.setUint32(0, w);
  dv.setUint32(4, h);
  ihdr[8] = 1; // bit depth
  ihdr[9] = 0; // color type: grayscale
  const rowBytes = Math.ceil(w / 8);
  const raw = new Uint8Array((rowBytes + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (rowBytes + 1)] = 0; // filter: none
    for (let x = 0; x < w; x++) {
      if (px[y * w + x]) raw[y * (rowBytes + 1) + 1 + (x >> 3)] |= 0x80 >> (x & 7);
    }
  }
  const cs = new CompressionStream("deflate");
  const writer = cs.writable.getWriter();
  writer.write(raw);
  writer.close();
  const compressed = new Uint8Array(await new Response(cs.readable).arrayBuffer());
  const parts: Uint8Array[] = [sig, pngChunk("IHDR", ihdr), pngChunk("IDAT", compressed), pngChunk("IEND", new Uint8Array(0))];
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

/** Write a decoded frame to `<path>.png`, `<path>.txt` (ASCII) and `<path>.frame` (raw 0/1 pixels). Returns the hash. */
export async function saveFrame(px: Uint8Array, path: string, asciiCols = 96): Promise<string> {
  const hash = hashFrame(px);
  const png = await encodePng(px);
  await Deno.writeFile(`${path}.png`, png);
  await Deno.writeTextFile(`${path}.txt`, renderAscii(px, asciiCols));
  await Deno.writeFile(`${path}.frame`, px);
  return hash;
}
