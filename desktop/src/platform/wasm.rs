use app::apps::common::WASM_LAUNCHING;
use app::menu::state::AppState;
use app::platform::{HttpClientHandle, display::DisplayHandle};
use app::protocol::*;
use app::wasm::host::WasmHost;
use app::wasm::wasmi_runner;
use core::sync::atomic::Ordering;
use display_types::LcdScreen;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use futures::future::join;
use log::{error, info, warn};
use std::sync::Arc;
use std::sync::Mutex;

#[allow(dead_code)]
pub static LCD_BUFFER: Mutex<Option<Vec<u8>>> = Mutex::new(None);

#[derive(Debug)]
#[allow(dead_code)]
pub struct DesktopWasmHost;

impl WasmHost for DesktopWasmHost {
    fn write_stdout(&mut self, text: &str) {
        print!("{text}");
    }

    fn get_millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn set_lcd_buffer(&mut self, buffer: &[u8]) {
        if let Ok(mut lcd) = LCD_BUFFER.lock() {
            *lcd = Some(buffer.to_vec());
        }
    }

    fn set_gpio(&mut self, pin_number: u32, state: u32) {
        log::info!("set_gpio: pin={pin_number} state={state}");
    }
}

/// Runs one WASM session: the interpreter + the IPC handler concurrently.
async fn run_one_wasm(
    wasm_buffer: Vec<u8>,
    host_sender: HostIpcSender,
    host_receiver: HostIpcReceiver,
    http_client: HttpClientHandle,
    display: DisplayHandle,
    app_state: Arc<RwLock<CriticalSectionRawMutex, AppState>>,
) {
    let wasm_channel = Box::leak(Box::new(WasmIpcChannel::new()));
    let wasm_receiver = wasm_channel.receiver();
    let wasm_sender = wasm_channel.sender();

    info!("run_one_wasm: starting wasmi_runner ({} bytes)", wasm_buffer.len());

    let ipc_sender = wasm_sender.clone();
    let wasm_future = wasmi_runner(DesktopWasmHost, wasm_sender, host_receiver, wasm_buffer);

    let ipc_future = async {
        // Send Started once the IPC handler is live (channel capacity 1)
        ipc_sender.try_send((0, WasmIpcMessage::Started)).ok();

        loop {
            let (wasm_req_id, msg) = wasm_receiver.receive().await;
            info!("run_one_wasm/ipc: received WasmIpcMessage::{msg:?}");
            match msg {
                WasmIpcMessage::HttpRequest(http_req) => {
                    info!("run_one_wasm/ipc: handling HTTP request to {}", http_req.url);
                    let channel = app::platform::HttpEventChannel::new();
                    join(
                        http_client.request(http_req, &channel),
                        async {
                            loop {
                                match channel.receive().await {
                                    app::protocol::HttpEvent::Meta(meta) => {
                                        host_sender
                                            .send((wasm_req_id, HostIpcMessage::HttpResponseMeta(meta)))
                                            .await;
                                    }
                                    app::protocol::HttpEvent::Chunk(chunk) => {
                                        host_sender
                                            .send((wasm_req_id, HostIpcMessage::HttpResponseBody(chunk)))
                                            .await;
                                    }
                                    app::protocol::HttpEvent::Done => {
                                        host_sender
                                            .send((wasm_req_id, HostIpcMessage::HttpResponseComplete))
                                            .await;
                                        break;
                                    }
                                    app::protocol::HttpEvent::Error => {
                                        host_sender
                                            .send((wasm_req_id, HostIpcMessage::HttpError))
                                            .await;
                                        break;
                                    }
                                }
                            }
                        },
                    )
                    .await;
                }
                WasmIpcMessage::LcdScreen(screen) => {
                    let _ = display.signal(screen);
                }
                WasmIpcMessage::Stopped => {
                    *app_state.write().await = AppState::None;
                    info!("run_one_wasm/ipc: Received Stopped");
                    break;
                }
                WasmIpcMessage::Started | WasmIpcMessage::MenuAppStarted => {
                    *app_state.write().await = AppState::HostedApp;
                }
            }
        }
    };

    info!("run_one_wasm: joining wasm + ipc futures");
    join(wasm_future, ipc_future).await;
    info!("run_one_wasm: join completed, cleaning up");

    // Clear the LCD buffer so the minifb render loop falls back to the menu screen
    info!("run_one_wasm: clearing LCD_BUFFER");
    *LCD_BUFFER.lock().unwrap() = None;

    WASM_LAUNCHING.store(false, Ordering::Release);
    *app_state.write().await = AppState::None;
    info!("run_one_wasm: session complete");
}

/// Background loop that listens for WASM launch requests and runs them.
pub async fn wasm_runner_loop(
    host_receiver: HostIpcReceiver,
    host_sender: HostIpcSender,
    http_client: HttpClientHandle,
    display: DisplayHandle,
    storage: app::platform::StorageHandle,
    app_state: Arc<RwLock<CriticalSectionRawMutex, AppState>>,
) {
    info!("Desktop WASM runner loop started");

    loop {
        info!("wasm_runner_loop: waiting for message...");
        let (_msg_id, msg) = host_receiver.receive().await;
        info!("wasm_runner_loop: received msg={msg:?}");
        match msg {
            HostIpcMessage::StartWasm(filename) => {
                info!("wasm_runner_loop: loading wasm file");
                let buf = storage
                    .read_binary_chunk(filename, 0, 256 * 1024)
                    .await
                    .unwrap_or_default();
                if buf.is_empty() {
                    warn!("wasm_runner_loop: wasm file not found or empty");
                    continue;
                }
                info!("wasm_runner_loop: file loaded, {} bytes", buf.len());
                run_one_wasm(
                    buf,
                    host_sender.clone(),
                    host_receiver.clone(),
                    http_client.clone(),
                    display.clone(),
                    app_state.clone(),
                )
                .await;
                info!("wasm_runner_loop: run_one_wasm returned");
            }
            HostIpcMessage::StartWasmWithBuffer(buffer) => {
                info!("wasm_runner_loop: running from buffer ({} bytes)", buffer.len());
                run_one_wasm(
                    buffer,
                    host_sender.clone(),
                    host_receiver.clone(),
                    http_client.clone(),
                    display.clone(),
                    app_state.clone(),
                )
                .await;
                info!("wasm_runner_loop: run_one_wasm returned");
            }
            other => {
                info!("wasm_runner_loop: ignoring unexpected message: {other:?}");
            }
        }
    }
}
