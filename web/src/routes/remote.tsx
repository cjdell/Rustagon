import { BadgeRemote, Card, DropZone } from "@components";
import { GlobalDeviceApi } from "@lib";
import { RouteSectionProps } from "@solidjs/router";

export function RemoteRoute(props: RouteSectionProps) {
  const api = GlobalDeviceApi;

  const onFileUpload = (buffer: Uint8Array) => {
    api.sendFile(buffer);
  };

  return (
    <div class="grid">
      <div class="g-col-12 g-col-md-6">
        <div class="d-flex justify-content-center">
          <BadgeRemote deviceApi={api} />
        </div>
      </div>

      <div class="g-col-12 g-col-md-6">
        <Card colour="info">
          <Card.Header text="Send File" />
          <Card.Body>
            <DropZone onFile={onFileUpload} />
          </Card.Body>
        </Card>
      </div>
    </div>
  );
}
