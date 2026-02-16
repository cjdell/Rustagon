import { isIpAddress } from "../core/index.ts";
import { BadgeDeviceApi } from "./badge.ts";
import { DeviceApi } from "./common.ts";
import { DummyDeviceApi } from "./dummy.ts";

export * from "./badge.ts";
export * from "./common.ts";
export * from "./dummy.ts";

export function getDeviceApi(): DeviceApi {
  const { hostname } = globalThis.location;

  // return new BadgeDeviceApi("http://192.168.49.143"); // FOR TESTING

  if (hostname === "127.0.0.1" || hostname === "localhost" || hostname === "demo.rustagon.chrisdell.info") {
    return new DummyDeviceApi();
  } else {
    return new BadgeDeviceApi();
  }
}
