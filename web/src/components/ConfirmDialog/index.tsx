import { Button, type Colour } from "@components";
import {
  createContext,
  createSignal,
  useContext,
  type Component,
  type JSX,
  Show,
} from "solid-js";

export interface ConfirmOptions {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  confirmColour?: Colour;
  cancelColour?: Colour;
}

interface ConfirmState {
  confirm: (opts: ConfirmOptions) => Promise<boolean>;
}

const ConfirmContext = createContext<ConfirmState>();

export const ConfirmDialogProvider: Component<{ children: JSX.Element }> = (props) => {
  const [options, setOptions] = createSignal<ConfirmOptions | null>(null);
  const [pending, setPending] = createSignal<((value: boolean) => void) | null>(null);

  const confirm = (opts: ConfirmOptions): Promise<boolean> => {
    return new Promise<boolean>((resolve) => {
      setOptions({ ...opts });
      setPending(() => resolve);
      document.body.classList.add("modal-open");
      document.body.style.overflow = "hidden";
    });
  };

  const close = (value: boolean) => {
    const resolve = pending();
    setOptions(null);
    setPending(null);
    document.body.classList.remove("modal-open");
    document.body.style.removeProperty("overflow");
    resolve?.(value);
  };

  return (
    <ConfirmContext.Provider value={{ confirm }}>
      {props.children}
      <Show when={options()}>
        {(o) => (
          <div
            class="modal d-block"
            style={{ "z-index": "1050", "background-color": "rgba(0, 0, 0, 0.5)" }}
            aria-modal="true"
            role="dialog"
            onClick={(e) => {
              if (e.target === e.currentTarget) close(false);
            }}
          >
            <div class="modal-dialog" role="document">
              <div
                class="modal-content"
                style={{
                  "box-shadow": "0 0.5rem 1rem rgba(0, 0, 0, 0.5)",
                }}
              >
                <div class="modal-header">
                  <h5 class="modal-title">{o().title}</h5>
                </div>
                <div class="modal-body">
                  <p class="mb-0">{o().message}</p>
                </div>
                <div class="modal-footer">
                  <Button
                    colour={o().cancelColour ?? "secondary"}
                    on:click={() => close(false)}
                  >
                    {o().cancelLabel ?? "Cancel"}
                  </Button>
                  <Button
                    colour={o().confirmColour ?? "primary"}
                    on:click={() => close(true)}
                  >
                    {o().confirmLabel ?? "OK"}
                  </Button>
                </div>
              </div>
            </div>
          </div>
        )}
      </Show>
    </ConfirmContext.Provider>
  );
};

export function useConfirm(): (opts: ConfirmOptions) => Promise<boolean> {
  const ctx = useContext(ConfirmContext);
  if (!ctx) throw new Error("useConfirm must be used within a ConfirmDialogProvider");
  return ctx.confirm;
}
