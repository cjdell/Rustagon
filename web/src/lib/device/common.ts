import * as v from "valibot";
import { FieldMetadata } from "../core/index.ts";
import type { HexButton } from "../wasm/index.ts";

export const DeviceConfigSchema = v.object({
  owner_name: v.pipe(v.string(), v.minLength(1), v.maxLength(24), v.description("Who does this badge belong to?")),
  app_store_url: v.pipe(v.string(), v.url(), v.description("URL to retrieve new apps")),
  firmware_url: v.pipe(v.string(), v.url(), v.description("URL to retrieve new firmware")),
  wifi_mode: v.pipe(
    v.picklist(["Station", "AccessPoint"]),
    v.description(
      `"Access Point" mode allows you to access your badge directly by creating its own WiFi network. "Station" mode will attempt to connect to a saved WiFi network with the strongest signal.`,
    ),
  ),
  ap_ssid: v.pipe(
    v.string(),
    v.minLength(1),
    v.title("Access Point SSID"),
    v.description("Wireless network name to broadcast when the device is in Access Point mode"),
  ),
  // ap_pass: v.pipe(v.string(), v.minLength(8), v.title("Access Point Password")),
  known_wifi_networks: v.array(v.object({
    ssid: v.pipe(v.string(), v.minLength(1), v.title("SSID")),
    pass: v.pipe(v.string(), v.minLength(0), v.title("Password"), v.metadata(FieldMetadata({ password: true }))),
  })),
});

export type DeviceConfig = v.InferInput<typeof DeviceConfigSchema>;

export const DeviceFileSchema = v.object({
  name: v.string(),
  size: v.number(),
});

export type DeviceFile = v.InferInput<typeof DeviceFileSchema>;

export const WifiResultSchema = v.object({
  ssid: v.string(),
  signal_strength: v.number(),
  password_required: v.boolean(),
});

export type WifiResult = v.InferInput<typeof WifiResultSchema>;

export type HexButtonMessage = HexButton;

export interface SystemMessage {
  SystemMessage: "BootButton";
}

export type DeviceMessage = HexButtonMessage | SystemMessage;

export type FrameBufferListener = (buffer: Uint8Array) => void;

export interface DeviceApi {
  onFrameBuffer: (handler: FrameBufferListener) => void;

  schema: typeof DeviceConfigSchema;

  getDeviceConfig(): Promise<DeviceConfig>;
  saveDeviceConfig(config: DeviceConfig): Promise<void>;
  reboot(): Promise<void>;

  scanWifiNetworks(): Promise<readonly WifiResult[]>;

  sendMessage(message: DeviceMessage): Promise<void>;
  sendFile(buffer: Uint8Array): Promise<void>;

  listFiles(): Promise<readonly DeviceFile[]>;
  readFile(filename: string): Promise<Uint8Array>;
  writeFile(filename: string, bytes: Uint8Array): Promise<void>;
  deleteFile(filename: string): Promise<void>;
}
