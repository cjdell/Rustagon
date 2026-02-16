import { Buttons, CANVAS_HEIGHT, CANVAS_WIDTH, DeviceApi } from "@lib";
import { createEffect } from "solid-js";
import { Button } from "../Button/index.tsx";
import { HexagonCanvasManager } from "../helper.ts";
import "./style.scss";

interface Props {
  deviceApi: DeviceApi;
}

export function BadgeRemote(props: Props) {
  let canvas: HTMLCanvasElement | null = null;

  createEffect(() => {
    if (canvas) {
      const hexagon = new HexagonCanvasManager(canvas);

      hexagon.setPointHandler((i) => {
        props.deviceApi.sendMessage({ HexButton: Buttons[i] });
      });

      props.deviceApi.onFrameBuffer((frameBuffer) => {
        hexagon.drawFrameBuffer(frameBuffer);
      });
    }
  });

  const onBootClick = () => {
    props.deviceApi.sendMessage({ SystemMessage: "BootButton" });
  };

  return (
    <div class="BadgeRemote">
      <canvas ref={(c) => canvas = c} id="badge" width={CANVAS_WIDTH} height={CANVAS_HEIGHT} />
      <Button colour="warning" on:click={() => onBootClick()}>Quit (boop)</Button>
    </div>
  );
}
