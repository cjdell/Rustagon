pub mod context;
pub mod host;
pub mod timers;

pub use context::*;
pub use host::WasmHost;
pub use timers::*;

use crate::protocol::*;
use alloc::{boxed::Box, format, string::String, vec::Vec};
use embassy_futures::yield_now;
use log::info;
use wasmi::{Caller, Engine, Extern, Linker, Module, Store};

/// Runs a WASM binary through the wasmi interpreter.
///
/// Platform-specific operations are delegated to `host`. IPC communication
/// with the host system (HTTP requests, button events, lifecycle) happens
/// through the embassy channel pairs.
pub async fn wasmi_runner<H: WasmHost>(
    host: H,
    wasm_ipc_sender: WasmIpcSender,
    host_ipc_receiver: HostIpcReceiver,
    buf: Vec<u8>,
) -> Result<(), wasmi::Error> {
    info!("wasmi_runner: creating engine, buf={} bytes", buf.len());

    let engine = Box::new(Engine::default());

    let wasm_ctx = WasmCtx {
        counter: 1,
        last_screen_update: 0,
        timer_registry: TimerRegistry::new(),
        host_ipc_receiver: host_ipc_receiver.clone(),
        wasm_ipc_sender,
        limiter: MyLimiter,
        host,
    };

    let mut linker = <Linker<WasmCtx<H>>>::new(&engine);
    info!("wasmi_runner: registering host functions");
    register_host_functions(&mut linker)?;

    info!("wasmi_runner: compiling module");
    let module = Box::new(unsafe { Module::new_unchecked(&engine, &buf) }?);

    info!("wasmi_runner: creating store and instantiating");
    let mut store = Store::new(&engine, wasm_ctx);
    store.limiter(|ctx| &mut ctx.limiter);

    let instance = linker.instantiate_and_start(&mut store, &module)?;

    let wasm_main = instance
        .get_export(&store, "wasm_main")
        .and_then(Extern::into_func)
        .ok_or(wasmi::Error::new("WASM: `wasm_main` not found"))?;
    info!("wasmi_runner: calling wasm_main");

    let mut result = [];
    wasm_main
        .call(&mut store, &[], &mut result)
        .map_err(|err| wasmi::Error::new(format!("WASM: Error calling `wasm_main`: {err}")))?;

    info!("wasmi_runner: wasm_main completed, entering tick loop");
    let tick = instance.get_typed_func::<(u32, u32), i32>(&mut store, "tick")?;

    let mut tick_count = 0u64;
    loop {
        let (host_msg_id, host_msg_length) = match host_ipc_receiver.try_peek() {
            Ok((host_msg_id, ref host_ipc_msg)) => {
                if let HostIpcMessage::Stop = host_ipc_msg {
                    info!("WASM: Program aborted by Stop message");
                    store.data().wasm_ipc_sender.send((0, WasmIpcMessage::Stopped)).await;
                    return Ok(());
                }
                let host_msg_bytes = serde_json::to_vec(host_ipc_msg).unwrap();
                (host_msg_id, host_msg_bytes.len() as u32)
            }
            Err(_) => (0, 0),
        };

        if tick
            .call(&mut store, (host_msg_id, host_msg_length))
            .map_err(|err| wasmi::Error::new(format!("Error calling tick: {err}")))?
            != 0
        {
            break;
        }

        tick_count += 1;
        if tick_count % 1000 == 0 {
            info!("wasmi_runner: {tick_count} ticks completed");
        }

        yield_now().await;
    }

    info!("WASM: Program complete after {tick_count} ticks");
    store.data().wasm_ipc_sender.send((0, WasmIpcMessage::Stopped)).await;
    Ok(())
}

fn register_host_functions<H: WasmHost>(
    linker: &mut Linker<WasmCtx<H>>,
) -> Result<(), wasmi::Error> {
    linker
        .func_wrap(
            "index",
            "extern_write_stdout",
            |mut caller: Caller<'_, WasmCtx<H>>, ptr: u32, len: u32| {
                let buffer = ReadWasmBuffer::read_memory(&caller, ptr, len);
                let string = String::from_utf8_lossy(&buffer);
                caller.data_mut().host.write_stdout(&string);
            },
        )?
        .func_wrap(
            "index",
            "extern_set_gpio",
            |mut caller: Caller<'_, WasmCtx<H>>, pin_number: u32, state: u32| {
                caller.data_mut().host.set_gpio(pin_number, state);
            },
        )?
        .func_wrap(
            "index",
            "extern_get_millis",
            |caller: Caller<'_, WasmCtx<H>>| -> u32 {
                caller.data().host.get_millis() as u32
            },
        )?
        .func_wrap(
            "index",
            "extern_set_lcd_buffer",
            |mut caller: Caller<'_, WasmCtx<H>>, ptr: u32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .ok_or_else(|| wasmi::Error::new("failed to find memory export"))
                    .unwrap();
                let wasm_buffer = unsafe {
                    core::slice::from_raw_parts(
                        memory.data_ptr(&caller).add(ptr as usize),
                        240 * 240 * 2,
                    )
                };
                caller.data_mut().host.set_lcd_buffer(wasm_buffer);
            },
        )?
        .func_wrap(
            "index",
            "extern_write_wasm_ipc_message",
            |mut caller: Caller<'_, WasmCtx<H>>, ptr: u32, len: u32| -> u32 {
                let wasm_msg_bytes =
                    ReadWasmBuffer::read_memory(&caller, ptr, len);
                let wasm_msg_id = caller.data().counter + 1;

                let wasm_ipc_msg: WasmIpcMessage =
                    serde_json::from_slice(&wasm_msg_bytes).unwrap();

                caller
                    .data()
                    .wasm_ipc_sender
                    .try_send((wasm_msg_id, wasm_ipc_msg))
                    .unwrap();

                caller.data_mut().counter = wasm_msg_id;
                wasm_msg_id
            },
        )?
        .func_wrap(
            "index",
            "extern_read_host_ipc_message",
            |mut caller: Caller<'_, WasmCtx<H>>, host_msg_id_a: u32, ptr: u32| {
                let (host_msg_id_b, host_ipc_msg) = caller
                    .data()
                    .host_ipc_receiver
                    .try_receive()
                    .unwrap();

                if host_msg_id_a != host_msg_id_b {
                    panic!("Mismatched host IDs! {host_msg_id_a} {host_msg_id_b}");
                }

                let host_msg_bytes = serde_json::to_vec(&host_ipc_msg).unwrap();
                ReadWasmBuffer::write_memory(&mut caller, ptr, &host_msg_bytes);
            },
        )?;

    register_timer_functions(linker)?;

    Ok(())
}

fn register_timer_functions<H: WasmHost>(
    linker: &mut Linker<WasmCtx<H>>,
) -> Result<(), wasmi::Error> {
    linker
        .func_wrap(
            "index",
            "extern_register_timer",
            |mut caller: Caller<'_, WasmCtx<H>>, ms: u32| -> i32 {
                let now_ms = caller.data().host.get_millis();
                caller.data_mut().timer_registry.register(ms, now_ms)
            },
        )?
        .func_wrap(
            "index",
            "extern_check_timer",
            |mut caller: Caller<'_, WasmCtx<H>>, timer_id: i32| -> i32 {
                let now_ms = caller.data().host.get_millis();
                let expired = caller.data().timer_registry.check(timer_id, now_ms);
                if expired == 1 {
                    caller.data_mut().timer_registry.cancel(timer_id);
                }
                expired
            },
        )?
        .func_wrap(
            "index",
            "extern_cancel_timer",
            |mut caller: Caller<'_, WasmCtx<H>>, timer_id: i32| {
                caller.data_mut().timer_registry.cancel(timer_id);
            },
        )?;

    Ok(())
}
