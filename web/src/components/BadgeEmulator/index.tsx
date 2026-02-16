import { Buttons, CANVAS_HEIGHT, CANVAS_WIDTH, WasmRuntimeRemote } from "@lib";
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

      hexagon.setPointHandler((i) => {
        runtime.sendHostIpcMessage({ HexButton: Buttons[i] });
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
