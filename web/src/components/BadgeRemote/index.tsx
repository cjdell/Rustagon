import { CANVAS_HEIGHT, CANVAS_WIDTH, DeviceApi, HexButton, TouchButtons } from "@lib";
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

      hexagon.setHexHandler((i) => {
        const hexButtons: HexButton[] = ["HexA", "HexB", "HexC", "HexD", "HexE", "HexF"];
        props.deviceApi.sendMessage({ HexButton: hexButtons[i] });
      });

      hexagon.setHexReleaseHandler((i) => {
        const hexButtons: HexButton[] = ["HexA", "HexB", "HexC", "HexD", "HexE", "HexF"];
        props.deviceApi.sendMessage({ HexButton: (hexButtons[i] + "Released") as HexButton });
      });

      hexagon.setTouchHandler((i) => {
        props.deviceApi.sendMessage({ HexButton: TouchButtons[i] });
      });

      hexagon.setTouchReleaseHandler((i) => {
        props.deviceApi.sendMessage({ HexButton: (TouchButtons[i] + "Released") as HexButton });
      });

      hexagon.setStickHandler((dir: HexButton) => {
        props.deviceApi.sendMessage({ HexButton: dir });
      });

      hexagon.setStickReleaseHandler((dir: HexButton) => {
        props.deviceApi.sendMessage({ HexButton: (dir + "Released") as HexButton });
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
