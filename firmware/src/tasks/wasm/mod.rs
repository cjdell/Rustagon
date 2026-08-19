pub mod context;

pub use context::*;

use crate::native::*;
use crate::utils::*;
use alloc::string::ToString;
use app::protocol::*;
use app::wasm;
use display_types::{Icon40, LcdScreen};
use log::{error, info};

use crate::platform::display::DisplayHandle;
use crate::platform::StorageHandle;

#[embassy_executor::task]
pub async fn second_core_task(storage: StorageHandle, display: DisplayHandle, sender: WasmIpcSender, receiver: HostIpcReceiver) {
  info!("WASM: Starting on second core");

  // Runs for the life of the core — the loop never fails or returns.
  wasm_host_loop(storage, display, sender, receiver).await;
}

async fn wasm_host_loop(
  storage: StorageHandle,
  display: DisplayHandle,
  wasm_ipc_sender: WasmIpcSender,
  host_ipc_receiver: HostIpcReceiver,
) {
  print_memory_info();

  loop {
    match host_ipc_receiver.receive().await.1 {
      HostIpcMessage::Runtime(HostRuntimeCommand::StartNative(app_name)) => {
        wasm_ipc_sender.send((0, WasmIpcMessage::Started)).await;

        let screen = LcdScreen::Headline(Icon40::Info, "Starting app...".to_string());
        wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

        sleep(500).await;

        let screen = LcdScreen::Blank;
        wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

        let ctx = NativeAppContext::new(storage.clone(), wasm_ipc_sender, host_ipc_receiver);
        // `app::native::NativeAppType` is an empty enum (no native apps exist yet),
        // so drive it via a temporary: a named binding trips unused_variables.
        NativeAppType::load_app_async(app_name, ctx).app_main().await;

        wasm_ipc_sender.send((0, WasmIpcMessage::Stopped)).await;
      }
      HostIpcMessage::Runtime(HostRuntimeCommand::StartWasm(filename)) => {
        wasm_ipc_sender.send((0, WasmIpcMessage::Started)).await;

        let screen = LcdScreen::Headline(Icon40::Info, "Starting WASM...".to_string());
        wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

        sleep(500).await;

        let screen = LcdScreen::Blank;
        wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

        info!("Wasm: Started");
        print_memory_info();

        let buf = storage.read_binary_chunk(filename.clone(), 0, 256 * 1024).await.unwrap();

        info!("WASM: File size: {}", buf.len());

        if let Err(err) = wasm::wasmi_runner(HardwareWasmHost::new(display.clone()), wasm_ipc_sender, host_ipc_receiver, buf).await {
          error!("A error occurred whilst running the program: {err}");
        }

        // wasmi_runner already sends WasmIpcMessage::Stopped on
        // completion or abort — do not send a second one here.
        info!("Wasm: Stopped");
        print_memory_info();
      }
      HostIpcMessage::Runtime(HostRuntimeCommand::StartWasmWithBuffer(buffer)) => {
        wasm_ipc_sender.send((0, WasmIpcMessage::Started)).await;

        let screen = LcdScreen::Headline(Icon40::Info, "Starting WASM...".to_string());
        wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

        sleep(500).await;

        let screen = LcdScreen::Blank;
        wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

        info!("Wasm: Started");
        print_memory_info();

        if let Err(err) = wasm::wasmi_runner(HardwareWasmHost::new(display.clone()), wasm_ipc_sender, host_ipc_receiver, buffer).await {
          error!("A error occurred whilst running the program: {err}");
        }

        // wasmi_runner already sends WasmIpcMessage::Stopped on
        // completion or abort — do not send a second one here.
        info!("Wasm: Stopped");
        print_memory_info();
      }
      _ => {}
    }
  }
}
