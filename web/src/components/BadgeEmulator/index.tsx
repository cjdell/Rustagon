import { CANVAS_HEIGHT, CANVAS_WIDTH, HexButton, TouchButtons, WasmRuntimeRemote } from "@lib";
import * as Comlink from "comlink";
import { createEffect, createResource } from "solid-js";
import { HexagonCanvasManager } from "../helper.ts";
import "./style.scss";

interface Props {
  buffer?: Uint8Array | null;
}

export function BadgeEmulator(props: Props) {
  let canvas: HTMLCanvasElement | null = null;

  const [wasmRuntime] = createResource(() => new WasmRuntimeRemote());

  createEffect(() => {
    const runtime = wasmRuntime();

    if (runtime && canvas) {
      const hexagon = new HexagonCanvasManager(canvas);

      hexagon.setHexHandler((i) => {
        const hexButtons: HexButton[] = ["HexA", "HexB", "HexC", "HexD", "HexE", "HexF"];
        runtime.sendHostIpcMessage({ HexButton: hexButtons[i] });
      });

      hexagon.setHexReleaseHandler((i) => {
        const hexButtons: HexButton[] = ["HexA", "HexB", "HexC", "HexD", "HexE", "HexF"];
        runtime.sendHostIpcMessage({ HexButton: (hexButtons[i] + "Released") as HexButton });
      });

      hexagon.setTouchHandler((i) => {
        runtime.sendHostIpcMessage({ HexButton: TouchButtons[i] });
      });

      hexagon.setTouchReleaseHandler((i) => {
        runtime.sendHostIpcMessage({ HexButton: (TouchButtons[i] + "Released") as HexButton });
      });

      hexagon.setStickHandler((dir: HexButton) => {
        runtime.sendHostIpcMessage({ HexButton: dir });
      });

      hexagon.setStickReleaseHandler((dir: HexButton) => {
        runtime.sendHostIpcMessage({ HexButton: (dir + "Released") as HexButton });
      });

      runtime.addFrameBufferHandler(Comlink.proxy((frameBuffer: Uint8Array) => {
        hexagon.drawFrameBuffer(frameBuffer);
      }));
    }
  });

  createEffect(() => {
    const runtime = wasmRuntime();

    if (runtime && props.buffer) {
      runtime.start(props.buffer.buffer as ArrayBuffer);
    }
  });

  return (
    <div class="BadgeEmulator">
      <canvas ref={(c) => canvas = c} id="badge" width={CANVAS_WIDTH} height={CANVAS_HEIGHT} />
    </div>
  );
}
