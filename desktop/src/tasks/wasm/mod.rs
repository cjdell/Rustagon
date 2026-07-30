pub mod context;

pub use context::*;

use app::apps::common::WASM_LAUNCHING;
use app::menu::state::AppState;
use app::platform::{HttpClientHandle, display::DisplayHandle};
use app::protocol::*;
use app::wasm::wasmi_runner;
use core::sync::atomic::Ordering;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use futures::future::join;
use log::{info, warn};
use std::sync::Arc;

/// Spawns the desktop WASM runner on a background thread.
/// Mirrors firmware's `second_core_task` (Embassy task on core 1).
pub fn spawn_wasm_runner(
    host_receiver: HostIpcReceiver,
    host_sender: HostIpcSender,
    http_client: HttpClientHandle,
    display: DisplayHandle,
    storage: app::platform::StorageHandle,
    app_state: Arc<RwLock<CriticalSectionRawMutex, AppState>>,
) {
    std::thread::spawn(move || {
        futures::executor::block_on(wasm_host_loop(
            host_receiver,
            host_sender,
            http_client,
            display,
            storage,
            app_state,
        ));
    });
}

/// Message dispatch loop. Mirrors firmware's `wasm_host_loop`.
async fn wasm_host_loop(
    host_receiver: HostIpcReceiver,
    host_sender: HostIpcSender,
    http_client: HttpClientHandle,
    display: DisplayHandle,
    storage: app::platform::StorageHandle,
    app_state: Arc<RwLock<CriticalSectionRawMutex, AppState>>,
) {
    info!("Desktop WASM runner loop started");

    loop {
        info!("wasm_host_loop: waiting for message...");
        let (_msg_id, msg) = host_receiver.receive().await;
        info!("wasm_host_loop: received msg={msg:?}");
        match msg {
            HostIpcMessage::StartWasm(filename) => {
                info!("wasm_host_loop: loading wasm file");
                let buf = storage
                    .read_binary_chunk(filename, 0, 256 * 1024)
                    .await
                    .unwrap_or_default();
                if buf.is_empty() {
                    warn!("wasm_host_loop: wasm file not found or empty");
                    continue;
                }
                info!("wasm_host_loop: file loaded, {} bytes", buf.len());
                run_program(
                    buf,
                    host_sender.clone(),
                    host_receiver.clone(),
                    http_client.clone(),
                    display.clone(),
                    app_state.clone(),
                )
                .await;
                info!("wasm_host_loop: run_program returned");
            }
            HostIpcMessage::StartWasmWithBuffer(buffer) => {
                info!("wasm_host_loop: running from buffer ({} bytes)", buffer.len());
                run_program(
                    buffer,
                    host_sender.clone(),
                    host_receiver.clone(),
                    http_client.clone(),
                    display.clone(),
                    app_state.clone(),
                )
                .await;
                info!("wasm_host_loop: run_program returned");
            }
            other => {
                info!("wasm_host_loop: ignoring unexpected message: {other:?}");
            }
        }
    }
}

/// Runs one WASM program: interpreter + IPC handler concurrently.
/// Mirrors firmware's `run_program` (without the linker/setup — that's in `app::wasm`).
async fn run_program(
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

    info!("run_program: starting wasmi_runner ({} bytes)", wasm_buffer.len());

    let ipc_sender = wasm_sender.clone();
    let wasm_future = wasmi_runner(DesktopWasmHost, wasm_sender, host_receiver, wasm_buffer);

    let ipc_future = async {
        ipc_sender.try_send((0, WasmIpcMessage::Started)).ok();

        loop {
            let (wasm_req_id, msg) = wasm_receiver.receive().await;
            info!("run_program/ipc: received WasmIpcMessage::{msg:?}");
            match msg {
                WasmIpcMessage::HttpRequest(http_req) => {
                    info!("run_program/ipc: handling HTTP request to {}", http_req.url);
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
                    info!("run_program/ipc: Received Stopped");
                    break;
                }
                WasmIpcMessage::Started | WasmIpcMessage::MenuAppStarted => {
                    *app_state.write().await = AppState::HostedApp;
                }
            }
        }
    };

    info!("run_program: joining wasm + ipc futures");
    join(wasm_future, ipc_future).await;
    info!("run_program: join completed, cleaning up");

    *LCD_BUFFER.lock().unwrap() = None;

    WASM_LAUNCHING.store(false, Ordering::Release);
    *app_state.write().await = AppState::None;
    info!("run_program: session complete");
}
