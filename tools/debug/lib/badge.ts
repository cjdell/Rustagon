// Badge WebSocket client — screen stream + remote input injection.
//
// The device exposes `ws://<host>/api/ws` (subprotocol "messages"):
//   device -> client: binary frame every 250 ms, 1 bit per pixel (see framebuffer.ts)
//   client -> device: JSON text, {"HexButton":"Up"} | {"HexButton":"UpReleased"} |
//                     {"SystemMessage":"BootButton"}
// Injected buttons reach the platform input queues via
// firmware's websocket_input_forwarder_task — indistinguishable from physical
// presses.

import { FRAME_BYTES, decodeFrame } from "./framebuffer.ts";

export type HexButtonName =
  | "Up" | "Down" | "Left" | "Right" | "Fire"
  | "HexA" | "HexB" | "HexC" | "HexD" | "HexE" | "HexF"
  | "Touch01" | "Touch02" | "Touch03" | "Touch04" | "Touch05" | "Touch06"
  | "Touch07" | "Touch08" | "Touch09" | "Touch10" | "Touch11" | "Touch12";

export const RELEASE_SUFFIX = "Released";

export function pressName(name: HexButtonName): HexButtonName {
  return name;
}
export function releaseName(name: HexButtonName): string {
  return `${name}Released`;
}

export interface BadgeConnectionOptions {
  /** Seconds to wait for the first frame. */
  connectTimeoutSec?: number;
  /** Called for every decoded frame (row-major 0/1 pixels). */
  onFrame?: (px: Uint8Array) => void;
}

export interface BadgeConnection {
  /** Latest decoded frame, updated on every binary message. */
  latest: Uint8Array;
  /** Number of frames received so far. */
  frameCount: number;
  sendJson(payload: unknown): void;
  /** Send a HexButton press (or explicit press/release name). */
  press(button: string, holdMs?: number): Promise<void>;
  /** Send a SystemMessage. */
  system(message: string): Promise<void>;
  /** Wait until `latest` is a fresh frame (at least `ms` after last change). */
  settle(ms?: number): Promise<void>;
  close(): void;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Connect to the badge screen/input WebSocket. Resolves once the first frame arrives. */
export function connectBadge(host: string, opts: BadgeConnectionOptions = {}): Promise<BadgeConnection> {
  const url = `ws://${host}/api/ws`;
  const timeoutMs = (opts.connectTimeoutSec ?? 10) * 1000;

  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url, "messages");
    const conn: BadgeConnection = {
      latest: new Uint8Array(FRAME_BYTES),
      frameCount: 0,
      sendJson(payload: unknown) {
        ws.send(JSON.stringify(payload));
      },
      async press(button: string, holdMs = 120) {
        this.sendJson({ HexButton: button });
        await sleep(holdMs);
        this.sendJson({ HexButton: button.endsWith(RELEASE_SUFFIX) ? button : `${button}Released` });
      },
      async system(message: string) {
        this.sendJson({ SystemMessage: message });
      },
      async settle(ms = 150) {
        const before = this.latest.slice();
        const beforeCount = this.frameCount;
        const deadline = Date.now() + ms;
        while (Date.now() < deadline) {
          await sleep(25);
          if (this.frameCount > beforeCount) {
            const after = this.latest;
            if (!after.every((v, i) => v === before[i])) return;
          }
        }
      },
      close() {
        try {
          ws.close();
        } catch {
          /* already closed */
        }
      },
    };

    const timer = setTimeout(() => {
      reject(new Error(`No frame received from ${url} within ${timeoutMs / 1000}s (device reachable? is WiFi up?)`));
      try {
        ws.close();
      } catch {
        /* ignore */
      }
    }, timeoutMs);

    ws.onopen = () => console.error(`[badge] connected ${url}`);
    ws.onerror = (e) => console.error("[badge] ws error", e);

    ws.onmessage = async (e) => {
      if (!(e.data instanceof Blob)) return;
      const bits = new Uint8Array(await e.data.arrayBuffer());
      if (bits.length !== FRAME_BYTES) {
        console.error(`[badge] unexpected frame size ${bits.length} (want ${FRAME_BYTES})`);
        return;
      }
      conn.latest = decodeFrame(bits);
      conn.frameCount++;
      if (conn.frameCount === 1) {
        clearTimeout(timer);
        resolve(conn);
      }
      opts.onFrame?.(conn.latest);
    };
  });
}
