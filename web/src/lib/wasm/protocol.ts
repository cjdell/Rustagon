export type HostIpcMessage =
  | { HexButton: HexButton }
  | { HttpResponseMeta: HttpResponseMeta }
  | { HttpResponseBody: HttpResponseBody }
  | HttpResponseComplete;

export type HexButton =
  | "Up" | "Right" | "Fire" | "Down" | "Left"
  | "HexA" | "HexB" | "HexC" | "HexD" | "HexE" | "HexF"
  | "Touch01" | "Touch02" | "Touch03" | "Touch04" | "Touch05" | "Touch06"
  | "Touch07" | "Touch08" | "Touch09" | "Touch10" | "Touch11" | "Touch12"
  | "UpReleased" | "RightReleased" | "FireReleased" | "DownReleased" | "LeftReleased"
  | "HexAReleased" | "HexBReleased" | "HexCReleased" | "HexDReleased" | "HexEReleased" | "HexFReleased"
  | "Touch01Released" | "Touch02Released" | "Touch03Released" | "Touch04Released" | "Touch05Released" | "Touch06Released"
  | "Touch07Released" | "Touch08Released" | "Touch09Released" | "Touch10Released" | "Touch11Released" | "Touch12Released";

export type WasmIpcMessage = { HttpRequest: HttpRequest };

export interface HttpRequest {
  url: string;
  method: string;
  headers: ([string, string])[];
}

export interface HttpResponseMeta {
  status: number;
  headers: (readonly [string, string])[];
}

export type HttpResponseBody = number[]; // Bytes

export type HttpResponseComplete = "HttpResponseComplete";
