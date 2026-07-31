pub mod context;

pub use context::*;

use app::menu::state::{StackEvent, StackEntryType, StackEventHandle};
use app::platform::{HttpClientHandle, display::DisplayHandle};
use app::protocol::*;
use app::wasm::wasmi_runner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Receiver;
use futures::future::join;
use log::{info, warn};
use std::sync::Arc;
use wasm_protocol::{HostIpcMessage as WireHostIpcMessage, WasmIpcMessage as WireWasmIpcMessage};

pub fn spawn_wasm_runner(
    host_receiver: Receiver<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>,
    host_sender: HostIpcSender,
    stack_event_handle: StackEventHandle,
    http_client: HttpClientHandle,
    display: DisplayHandle,
    storage: app::platform::StorageHandle,
) {
    std::thread::spawn(move || {
        futures::executor::block_on(wasm_host_loop(
            host_receiver,
            host_sender,
            stack_event_handle,
            http_client,
            display,
            storage,
        ));
    });
}

async fn wasm_host_loop(
    host_receiver: Receiver<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>,
    host_sender: HostIpcSender,
    stack_event_handle: StackEventHandle,
    http_client: HttpClientHandle,
    display: DisplayHandle,
    storage: app::platform::StorageHandle,
) {
    info!("Desktop WASM runner loop started");

    loop {
        info!("wasm_host_loop: waiting for message...");
        let (_msg_id, msg) = host_receiver.receive().await;
        info!("wasm_host_loop: received msg={msg:?}");
        match msg {
            HostIpcMessage::Runtime(HostRuntimeCommand::StartWasm(filename)) => {
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
                    stack_event_handle.clone(),
                    http_client.clone(),
                    display.clone(),
                )
                .await;
                info!("wasm_host_loop: run_program returned");
            }
            HostIpcMessage::Runtime(HostRuntimeCommand::StartWasmWithBuffer(buffer)) => {
                info!("wasm_host_loop: running from buffer ({} bytes)", buffer.len());
                run_program(
                    buffer,
                    host_sender.clone(),
                    host_receiver.clone(),
                    stack_event_handle.clone(),
                    http_client.clone(),
                    display.clone(),
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

async fn run_program(
    wasm_buffer: Vec<u8>,
    host_sender: HostIpcSender,
    host_receiver: Receiver<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>,
    stack_event_handle: StackEventHandle,
    http_client: HttpClientHandle,
    display: DisplayHandle,
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
                WasmIpcMessage::Wire(WireWasmIpcMessage::HttpRequest(http_req)) => {
                    info!("run_program/ipc: handling HTTP request to {}", http_req.url);
                    let channel = app::platform::HttpEventChannel::new();
                    join(
                        http_client.request(http_req, &channel),
                        async {
                            loop {
                                match channel.receive().await {
                                    app::protocol::HttpEvent::Meta(meta) => {
                                        host_sender
                                            .send((wasm_req_id, HostIpcMessage::Wire(WireHostIpcMessage::HttpResponseMeta(meta))))
                                            .await;
                                    }
                                    app::protocol::HttpEvent::Chunk(chunk) => {
                                        host_sender
                                            .send((wasm_req_id, HostIpcMessage::Wire(WireHostIpcMessage::HttpResponseBody(chunk))))
                                            .await;
                                    }
                                    app::protocol::HttpEvent::Done => {
                                        host_sender
                                            .send((wasm_req_id, HostIpcMessage::Wire(WireHostIpcMessage::HttpResponseComplete)))
                                            .await;
                                        break;
                                    }
                                    app::protocol::HttpEvent::Error => {
                                        host_sender
                                            .send((wasm_req_id, HostIpcMessage::Wire(WireHostIpcMessage::HttpError)))
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
                    info!("run_program/ipc: Received Stopped");
                    break;
                }
                WasmIpcMessage::Started | WasmIpcMessage::MenuAppStarted => {}
            }
        }
    };

    info!("run_program: joining wasm + ipc futures");
    join(wasm_future, ipc_future).await;
    info!("run_program: join completed, cleaning up");

    *LCD_BUFFER.lock().unwrap() = None;
    stack_event_handle.send(StackEvent::Popped);
    info!("run_program: session complete");
}
