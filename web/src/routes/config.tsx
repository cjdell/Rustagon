import { Button, Card, MagicFields } from "@components";
import { DeviceConfig, GlobalDeviceApi, WifiResult } from "@lib";
import { createResource, createSignal, For, Show } from "solid-js";
import { Suspense } from "solid-js/web";
import * as v from "valibot";

export function ConfigRoute() {
  const api = GlobalDeviceApi;

  const [deviceConfig, { mutate }] = createResource(() => api.getDeviceConfig());
  const [submittedCount, setSubmittedCount] = createSignal(0);
  const [scanResults, setScanResults] = createSignal<readonly WifiResult[]>();
  const [addingScanResult, setAddingScanResult] = createSignal<number | null>(null);
  const [addingScanResultPassword, setAddingScanResultPassword] = createSignal("");

  const onChange = (data: Partial<DeviceConfig>) => mutate({ ...deviceConfig()!, ...data });

  const onSave = async () => {
    setSubmittedCount(submittedCount() + 1);

    await api.saveDeviceConfig(v.parse(api.schema, deviceConfig()));
  };

  const onSaveAndReboot = async () => {
    setSubmittedCount(submittedCount() + 1);

    await api.saveDeviceConfig(v.parse(api.schema, deviceConfig()));

    await api.reboot();
  };

  const onAddNetwork = () => {
    addNetwork("", "");
  };

  const onScan = async () => {
    setScanResults(await api.scanWifiNetworks());
  };

  const onAddScanResult = (result: WifiResult, idx: number) => {
    if (addingScanResult() === idx) {
      if (addingScanResultPassword().length === 0) return;

      addNetwork(result.ssid, addingScanResultPassword());

      setAddingScanResult(null);
      setAddingScanResultPassword("");

      if (confirm("Save and reboot?")) {
        onSaveAndReboot();
      }
    }

    if (result.password_required) {
      setAddingScanResult(idx);
      return;
    }

    addNetwork(result.ssid, "");

    if (confirm("Save and reboot?")) {
      onSaveAndReboot();
    }
  };

  const addNetwork = (ssid: string, pass: string) => {
    const data = deviceConfig()!;

    mutate({ ...data, wifi_mode: "Station", known_wifi_networks: [...data.known_wifi_networks, { ssid, pass }] });
  };

  return (
    <div class="grid">
      <Show when={deviceConfig()?.known_wifi_networks.length ?? 0 > 0}>
        <div class="g-col-12 g-col-md-6">
          <Card colour="warning">
            <Card.Header text="Device Config" />
            <Card.Body>
              <Suspense>
                <Show when={deviceConfig()}>
                  {(deviceConfig) => (
                    <MagicFields
                      schema={api.schema}
                      data={deviceConfig()}
                      onChange={onChange}
                      validation={submittedCount() > 0}
                    />
                  )}
                </Show>
              </Suspense>
            </Card.Body>
            <Card.Footer>
              <Button colour="info" on:click={() => onAddNetwork()}>Add Network</Button>
              <Button colour="primary" on:click={() => onSave()}>Save</Button>
              <Button colour="warning" on:click={() => onSaveAndReboot()}>Save and Reboot</Button>
            </Card.Footer>
          </Card>
        </div>
      </Show>
      <div class="g-col-12 g-col-md-6">
        <Card colour="info">
          <Card.Header text="Found Wifi Networks" />
          <Card.Body>
            <div class="d-flex flex-column gap-2">
              <Show when={scanResults()} fallback={`Click "Scan" to search for WiFi networks`}>
                {(scanResults) => (
                  <For each={scanResults()}>
                    {(result, idx) => (
                      <div class="d-flex flex-column gap-1">
                        <div class="d-flex justify-content-between align-items-center">
                          <div class="d-flex gap-1">
                            <div class="fw-bold">[{result.signal_strength}]</div>
                            <div>{result.ssid}</div>
                            <div class="fw-bold">{result.password_required ? "[Secure]" : "[Open]"}</div>
                          </div>
                          <Button
                            colour={addingScanResult() !== idx() ? "info" : "primary"}
                            on:click={() => onAddScanResult(result, idx())}
                          >
                            {addingScanResult() !== idx() ? "Add" : "Save"}
                          </Button>
                        </div>
                        <Show when={addingScanResult() === idx()}>
                          <input
                            type="text"
                            placeholder="Password"
                            class="form-control"
                            value={addingScanResultPassword()}
                            on:change={(e) => setAddingScanResultPassword(e.target.value)}
                          />
                        </Show>
                      </div>
                    )}
                  </For>
                )}
              </Show>
            </div>
          </Card.Body>
          <Card.Footer>
            <Button colour="primary" on:click={() => onScan()}>Scan</Button>
            <Button colour="primary" on:click={() => onAddNetwork()}>Add Manually</Button>
          </Card.Footer>
        </Card>
      </div>
    </div>
  );
}
