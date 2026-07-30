pub mod context;

pub use context::*;

use crate::native::*;
use crate::utils::*;
use alloc::string::ToString;
use app::protocol::*;
use app::types::*;
use app::wasm;
use display_types::{Icon40, LcdScreen};
use embassy_time::{Duration, Timer};
use esp_println::println;
use log::{error, info};

use crate::platform::StorageHandle;

#[embassy_executor::task]
pub async fn second_core_task(storage: StorageHandle, sender: WasmIpcSender, receiver: HostIpcReceiver) {
    println!("Starting WASM on SECOND CORE...");

    loop {
        if let Err(err) = wasm_host_loop(storage.clone(), sender.clone(), receiver.clone()).await {
            error!("second_core_task: An error occurred: {err:?}");
        }

        Timer::after(Duration::from_millis(1_000)).await;
        info!("second_core_task: Restarting...");
    }
}

async fn wasm_host_loop(
    storage: StorageHandle,
    wasm_ipc_sender: WasmIpcSender,
    host_ipc_receiver: HostIpcReceiver,
) -> Result<(), anyhow::Error> {
    print_memory_info();

    loop {
        match host_ipc_receiver.receive().await.1 {
            HostIpcMessage::StartNative(app_name) => {
                wasm_ipc_sender.send((0, WasmIpcMessage::Started)).await;

                let screen = LcdScreen::Headline(Icon40::Info, "Starting app...".to_string());
                wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

                sleep(500).await;

                let screen = LcdScreen::Blank;
                wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

                let ctx = NativeAppContext::new(storage.clone(), wasm_ipc_sender, host_ipc_receiver);
                let app = NativeAppType::load_app_async(app_name, ctx);

                app.app_main().await;

                wasm_ipc_sender.send((0, WasmIpcMessage::Stopped)).await;
            }
            HostIpcMessage::StartWasm(filename) => {
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

                if let Err(err) = wasm::wasmi_runner(
                    HardwareWasmHost,
                    wasm_ipc_sender.clone(),
                    host_ipc_receiver.clone(),
                    buf,
                )
                .await
                {
                    error!("A error occurred whilst running the program: {err}");
                }

                wasm_ipc_sender.send((0, WasmIpcMessage::Stopped)).await;

                info!("Wasm: Stopped");
                print_memory_info();
            }
            HostIpcMessage::StartWasmWithBuffer(buffer) => {
                wasm_ipc_sender.send((0, WasmIpcMessage::Started)).await;

                let screen = LcdScreen::Headline(Icon40::Info, "Starting WASM...".to_string());
                wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

                sleep(500).await;

                let screen = LcdScreen::Blank;
                wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

                info!("Wasm: Started");
                print_memory_info();

                if let Err(err) = wasm::wasmi_runner(
                    HardwareWasmHost,
                    wasm_ipc_sender.clone(),
                    host_ipc_receiver.clone(),
                    buffer,
                )
                .await
                {
                    error!("A error occurred whilst running the program: {err}");
                }

                wasm_ipc_sender.send((0, WasmIpcMessage::Stopped)).await;

                info!("Wasm: Stopped");
                print_memory_info();
            }
            _ => {}
        }
    }
}
