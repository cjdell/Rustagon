import { assertError, GlobalDeviceApi } from "@lib";
import { createSignal } from "solid-js";
import { createResource, Show } from "solid-js";

export function IndexRoute() {
  const [error, setError] = createSignal("");

  const api = GlobalDeviceApi;

  const [deviceConfig] = createResource(async () => {
    let retries = 5;
    while (retries-- > 0) {
      try {
        return await api.getDeviceConfig();
      } catch (err) {
        assertError(err);
        setError(err.message);
      }
    }
  });

  return (
    <div>
      <h2>Welcome to Rustagon!</h2>

      <Show when={error()}>
        {(error) => <p>{error()}</p>}
      </Show>

      <Show when={deviceConfig()} fallback="Loading...">
        {(deviceConfig) => (
          <p>
            This badge belongs to <strong>{deviceConfig().owner_name}</strong>
          </p>
        )}
      </Show>

      <p>
        <a href="/config">Click here to configure WiFi networks</a>
      </p>
    </div>
  );
}
