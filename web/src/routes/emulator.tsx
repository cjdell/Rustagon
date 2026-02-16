import { BadgeEmulator, Card, DropZone } from "@components";
import { GlobalDeviceApi } from "@lib";
import { RouteSectionProps } from "@solidjs/router";
import { createResource } from "solid-js";

export function EmulatorRoute(props: RouteSectionProps) {
  const api = GlobalDeviceApi;

  const [buffer, { mutate }] = createResource(async () => props.params.filename ? await api.readFile(props.params.filename) : null);

  const onFileUpload = (buffer: Uint8Array) => {
    mutate(buffer);
  };

  return (
    <div class="grid">
      <div class="g-col-12 g-col-md-6">
        <div class="d-flex justify-content-center">
          <BadgeEmulator buffer={buffer()} />
        </div>
      </div>

      <div class="g-col-12 g-col-md-6">
        <Card colour="info">
          <Card.Header text="Emulate File" />
          <Card.Body>
            <DropZone onFile={onFileUpload} />
          </Card.Body>
        </Card>
      </div>
    </div>
  );
}
