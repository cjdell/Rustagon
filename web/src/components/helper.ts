import type { HexButton } from "@lib";

export class HexagonCanvasManager {
  private ctx: CanvasRenderingContext2D;

  // Layout constants
  private hexCenterX: number;
  private hexCenterY: number;
  private hexRadius: number;

  // 6 hex buttons (A-F) at the outer vertices, A at top
  private hexButtonRadius = 18;
  private hexPositions: { x: number; y: number; label: string }[] = [];

  // 12 touch buttons in a ring within the hexagon
  private touchButtonRadius = 16;
  private touchRingRadius = 140;
  private touchPositions: { x: number; y: number }[] = [];

  // Direction control stick below the hexagon
  private stickButtonRadius = 18;
  private stickCenterX: number;
  private stickCenterY: number;
  private stickPositions: { x: number; y: number; direction: HexButton }[] = [];

  // Screen overlay
  private screen: HTMLCanvasElement | null = null;

  // Active button highlight
  private activeHex: number | null = null;
  private activeTouch: number | null = null;
  private activeStick: HexButton | null = null;

  // Handlers
  private hexHandler = (_i: number) => {};
  private touchHandler = (_i: number) => {};
  private stickHandler = (_dir: HexButton) => {};

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = this.canvas.getContext("2d") as CanvasRenderingContext2D;

    // Fixed layout dimensions
    this.hexCenterX = this.canvas.width / 2;
    this.hexCenterY = 200;
    this.hexRadius = 180;
    this.stickCenterX = this.canvas.width / 4;
    this.stickCenterY = 400;

    this.computeHexPositions();
    this.computeTouchPositions();
    this.computeStickPositions();

    this.canvas.addEventListener("click", (e) => this.handleClick(e));
    this.canvas.addEventListener("mousemove", (e) => this.handleMove(e));
    this.canvas.addEventListener("mouseleave", () => {
      this.activeHex = null;
      this.activeTouch = null;
      this.activeStick = null;
      this.draw();
    });

    this.draw();
  }

  private computeHexPositions(): void {
    this.hexPositions = [];
    const labels = ["A", "B", "C", "D", "E", "F"];
    for (let i = 0; i < 6; i++) {
      const angle = -Math.PI / 2 + (i * Math.PI) / 3;
      this.hexPositions.push({
        x: this.hexCenterX + this.hexRadius * Math.cos(angle),
        y: this.hexCenterY + this.hexRadius * Math.sin(angle),
        label: labels[i],
      });
    }
  }

  private computeTouchPositions(): void {
    this.touchPositions = [];
    for (let i = 0; i < 12; i++) {
      const angle = -Math.PI / 2 + (i * Math.PI) / 6 + Math.PI / 12;
      this.touchPositions.push({
        x: this.hexCenterX + this.touchRingRadius * Math.cos(angle),
        y: this.hexCenterY + this.touchRingRadius * Math.sin(angle),
      });
    }
  }

  private computeStickPositions(): void {
    const cx = this.stickCenterX;
    const cy = this.stickCenterY;
    const spacing = 26;
    this.stickPositions = [
      { x: cx, y: cy - spacing, direction: "Up" as HexButton },
      { x: cx - spacing, y: cy, direction: "Left" as HexButton },
      { x: cx + spacing, y: cy, direction: "Right" as HexButton },
      { x: cx, y: cy + spacing, direction: "Down" as HexButton },
      { x: cx, y: cy, direction: "Fire" as HexButton },
    ];
  }

  public drawFrameBuffer(frameBuffer: Uint8Array<ArrayBufferLike>): void {
    this.screen = drawRGB565BE(frameBuffer);
    this.draw();
  }

  private draw(): void {
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);

    this.drawHexagon();
    this.drawScreen();
    this.drawTouchButtons();
    this.drawHexButtons();
    this.drawStick();
  }

  private drawHexagon(): void {
    this.ctx.beginPath();
    for (let i = 0; i < 6; i++) {
      const angle = -Math.PI / 2 + (i * Math.PI) / 3;
      const x = this.hexCenterX + this.hexRadius * Math.cos(angle);
      const y = this.hexCenterY + this.hexRadius * Math.sin(angle);
      if (i === 0) this.ctx.moveTo(x, y);
      else this.ctx.lineTo(x, y);
    }
    this.ctx.closePath();
    this.ctx.fillStyle = "#1a1a3a";
    this.ctx.fill();
    this.ctx.strokeStyle = "#2d2d5a";
    this.ctx.lineWidth = 2;
    this.ctx.stroke();
  }

  private drawScreen(): void {
    if (this.screen) {
      this.ctx.drawImage(
        this.screen,
        this.hexCenterX - 120,
        this.hexCenterY - 120,
      );
    }
  }

  private drawTouchButtons(): void {
    for (let i = 0; i < 12; i++) {
      const pos = this.touchPositions[i];
      const active = this.activeTouch === i;

      this.ctx.beginPath();
      this.ctx.arc(pos.x, pos.y, this.touchButtonRadius, 0, Math.PI * 2);

      if (active) {
        this.ctx.fillStyle = "#e74c3c";
      } else {
        this.ctx.fillStyle = "#555555";
      }
      this.ctx.fill();
      this.ctx.strokeStyle = "#888888";
      this.ctx.lineWidth = 1.5;
      this.ctx.stroke();

      // Label
      this.ctx.fillStyle = "#ffffff";
      this.ctx.font = "bold 9px monospace";
      this.ctx.textAlign = "center";
      this.ctx.textBaseline = "middle";
      this.ctx.fillText(`T${(i + 1).toString().padStart(2, "0")}`, pos.x, pos.y);
    }
  }

  private drawHexButtons(): void {
    for (let i = 0; i < 6; i++) {
      const pos = this.hexPositions[i];
      const active = this.activeHex === i;

      this.ctx.beginPath();
      this.ctx.arc(pos.x, pos.y, this.hexButtonRadius, 0, Math.PI * 2);

      if (active) {
        this.ctx.fillStyle = "#e74c3c";
      } else {
        this.ctx.fillStyle = "#336633";
      }
      this.ctx.fill();
      this.ctx.strokeStyle = "#55aa55";
      this.ctx.lineWidth = 2;
      this.ctx.stroke();

      this.ctx.fillStyle = "#ffffff";
      this.ctx.font = "bold 13px monospace";
      this.ctx.textAlign = "center";
      this.ctx.textBaseline = "middle";
      this.ctx.fillText(pos.label, pos.x, pos.y);
    }
  }

  private drawStick(): void {
    for (const btn of this.stickPositions) {
      const active = this.activeStick === btn.direction;

      this.ctx.beginPath();
      this.ctx.arc(btn.x, btn.y, this.stickButtonRadius, 0, Math.PI * 2);

      if (active) {
        this.ctx.fillStyle = "#e74c3c";
      } else if (btn.direction === "Fire") {
        this.ctx.fillStyle = "#ff4444";
      } else {
        this.ctx.fillStyle = "#666666";
      }
      this.ctx.fill();
      this.ctx.strokeStyle = "#999999";
      this.ctx.lineWidth = 1.5;
      this.ctx.stroke();

      // Arrow labels for direction buttons
      this.ctx.fillStyle = "#ffffff";
      this.ctx.font = "bold 14px monospace";
      this.ctx.textAlign = "center";
      this.ctx.textBaseline = "middle";

      let label = "";
      switch (btn.direction) {
        case "Up":
          label = "▲";
          break;
        case "Left":
          label = "◀";
          break;
        case "Fire":
          label = "●";
          break;
        case "Right":
          label = "▶";
          break;
        case "Down":
          label = "▼";
          break;
      }
      this.ctx.fillText(label, btn.x, btn.y);
    }
  }

  private handleClick(e: MouseEvent): void {
    const rect = this.canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    // Check touch buttons (inner ring)
    for (let i = 0; i < this.touchPositions.length; i++) {
      const p = this.touchPositions[i];
      if (Math.hypot(mx - p.x, my - p.y) <= this.touchButtonRadius) {
        this.touchHandler(i);
        this.activeTouch = i;
        this.draw();
        setTimeout(() => {
          this.activeTouch = null;
          this.draw();
        }, 200);
        return;
      }
    }

    // Check hex buttons (outer vertices)
    for (let i = 0; i < this.hexPositions.length; i++) {
      const p = this.hexPositions[i];
      if (Math.hypot(mx - p.x, my - p.y) <= this.hexButtonRadius) {
        this.hexHandler(i);
        this.activeHex = i;
        this.draw();
        setTimeout(() => {
          this.activeHex = null;
          this.draw();
        }, 200);
        return;
      }
    }

    // Check direction stick
    for (const btn of this.stickPositions) {
      if (Math.hypot(mx - btn.x, my - btn.y) <= this.stickButtonRadius) {
        this.stickHandler(btn.direction);
        this.activeStick = btn.direction;
        this.draw();
        setTimeout(() => {
          this.activeStick = null;
          this.draw();
        }, 200);
        return;
      }
    }
  }

  private handleMove(e: MouseEvent): void {
    const rect = this.canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    let hovering = false;
    for (const p of this.touchPositions) {
      if (Math.hypot(mx - p.x, my - p.y) <= this.touchButtonRadius) {
        hovering = true;
        break;
      }
    }
    if (!hovering) {
      for (const p of this.hexPositions) {
        if (Math.hypot(mx - p.x, my - p.y) <= this.hexButtonRadius) {
          hovering = true;
          break;
        }
      }
    }
    if (!hovering) {
      for (const btn of this.stickPositions) {
        if (Math.hypot(mx - btn.x, my - btn.y) <= this.stickButtonRadius) {
          hovering = true;
          break;
        }
      }
    }
    this.canvas.style.cursor = hovering ? "pointer" : "default";
  }

  public setHexHandler(handler: (i: number) => void): void {
    this.hexHandler = handler;
  }

  public setTouchHandler(handler: (i: number) => void): void {
    this.touchHandler = handler;
  }

  public setStickHandler(handler: (dir: HexButton) => void): void {
    this.stickHandler = handler;
  }

  public refresh(): void {
    this.draw();
  }

  public getCanvasDimensions(): { width: number; height: number } {
    return { width: this.canvas.width, height: this.canvas.height };
  }
}

import { HEIGHT, WIDTH } from "@lib";

export function drawRGB565BE(uint8Array: Uint8Array) {
  const canvas = document.createElement("canvas");

  canvas.width = WIDTH;
  canvas.height = HEIGHT;

  const ctx = canvas.getContext("2d")!;
  const imageData = ctx.createImageData(WIDTH, HEIGHT);
  const data = imageData.data;

  const screenRadius = WIDTH / 2;

  for (let i = 0; i < uint8Array.length; i += 2) {
    const byteIndex = i * 2; // Each RGB565 pixel becomes 4 RGBA bytes
    const x = (i / 2) % WIDTH;
    const y = ((i / 2) / WIDTH) | 0;

    const low = uint8Array[i];
    const high = uint8Array[i + 1];
    const rgb565 = high | (low << 8);

    const r5 = (rgb565 >> 11) & 0x1f;
    const g6 = (rgb565 >> 5) & 0x3f;
    const b5 = rgb565 & 0x1f;

    const r = (r5 * 255 + 15) / 31;
    const g = (g6 * 255 + 31) / 63;
    const b = (b5 * 255 + 15) / 31;

    const alpha = Math.sqrt(Math.pow(x - WIDTH / 2, 2) + Math.pow(y - HEIGHT / 2, 2)) < screenRadius ? 255 : 0;

    if (alpha > 0) {
      data[byteIndex + 0] = r; // R
      data[byteIndex + 1] = g; // G
      data[byteIndex + 2] = b; // B
      data[byteIndex + 3] = alpha; // A
    }
  }

  // Put the image data on the offscreen canvas
  ctx.putImageData(imageData, 0, 0);

  return canvas;
}
