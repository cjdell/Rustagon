pub mod context;
pub mod timers;

use crate::{
  lib::{DisplayInterface, HostIpcMessage, HostIpcReceiver, Icon40, LcdScreen, WasmIpcMessage, WasmIpcSender},
  native::{NativeApp, NativeAppContext, NativeAppType},
  tasks::{
    lcd::{BUFFER, SPI_DISPLAY_INTERFACE},
    wasm::{
      context::{MyLimiter, ReadWasmBuffer as _, WasmCtx},
      timers::{TimerRegistry, add_timers_to_linker},
    },
  },
  utils::{
    VecHelper,
    graphics::{SCREEN_HEIGHT, SCREEN_WIDTH},
    local_fs::LocalFs,
    print_memory_info, sleep,
  },
};
use alloc::{
  boxed::Box,
  format,
  string::{String, ToString},
  vec::Vec,
};
use core::slice::from_raw_parts_mut;
use display_interface::{DataFormat, WriteOnlyDataCommand};
use embassy_futures::yield_now;
use embassy_time::{Duration, Timer};
use esp_alloc::ExternalMemory;
use esp_backtrace as _;
use esp_hal::{
  gpio::{AnyPin, Level, Output},
  time::Instant,
};
use esp_println::{print, println};
use gc9a01::command::Command;
use log::{error, info};
use wasmi::{Caller, Engine, Extern, Linker, Module, Store};

#[embassy_executor::task]
pub async fn second_core_task(local_fs: LocalFs, sender: WasmIpcSender, receiver: HostIpcReceiver) {
  println!("Starting WASM on SECOND CORE...");

  loop {
    if let Err(err) = wasmi_runner(local_fs.clone(), sender, receiver).await {
      error!("second_core_task: An error occurred: {err:?}");
    }

    Timer::after(Duration::from_millis(1_000)).await;
    info!("second_core_task: Restarting...");
  }
}

// In this simple example we are going to compile the below Wasm source,
// instantiate a Wasm module from it and call its exported "hello" function.
async fn wasmi_runner(
  local_fs: LocalFs,
  wasm_ipc_sender: WasmIpcSender,
  host_ipc_receiver: HostIpcReceiver,
) -> Result<(), anyhow::Error> {
  print_memory_info();

  loop {
    println!("wasmi_runner loop");

    let wasm_ctx = WasmCtx {
      counter: 1,
      last_screen_update: 0,
      timer_registry: TimerRegistry::new(),
      host_ipc_receiver: host_ipc_receiver.clone(),
      wasm_ipc_sender,
      limiter: MyLimiter,
    };

    match host_ipc_receiver.receive().await.1 {
      HostIpcMessage::StartNative(app_name) => {
        wasm_ipc_sender.send((0, WasmIpcMessage::Started)).await;

        let screen = LcdScreen::Headline(Icon40::Info, "Starting app...".to_string());
        wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

        sleep(500).await;

        let screen = LcdScreen::Blank;
        wasm_ipc_sender.send((0, WasmIpcMessage::LcdScreen(screen))).await;

        let ctx = NativeAppContext::new(local_fs.clone(), wasm_ipc_sender, host_ipc_receiver);
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

        let buf = local_fs.read_binary_chunk(&filename, 0, 256 * 1024).unwrap(); // TODO

        info!("WASM: File size: {}", buf.len());

        if let Err(err) = run_program(host_ipc_receiver, wasm_ctx, VecHelper::to_global_vec(buf)).await {
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

        if let Err(err) = run_program(host_ipc_receiver, wasm_ctx, buffer).await {
          error!("A error occurred whilst running the program: {err}");
        }

        wasm_ipc_sender.send((0, WasmIpcMessage::Stopped)).await;

        info!("Wasm: Stopped");
        print_memory_info();
      }
      _ => {}
    };
  }
}

pub async fn run_program<'a>(
  host_ipc_receiver: HostIpcReceiver,
  wasm_ctx: WasmCtx,
  buf: Vec<u8>,
) -> Result<(), wasmi::Error> {
  let engine = Box::new_in(Engine::default(), ExternalMemory);

  let mut linker = <Linker<WasmCtx>>::new(&engine);

  add_timers_to_linker(&mut linker).unwrap();

  linker
    .func_wrap("index", "extern_write_stdout", write_stdout)?
    .func_wrap(
      "index",
      "extern_set_gpio",
      |_caller: Caller<'_, WasmCtx>, pin_number: u32, state: u32| {
        // println!("set_gpio: {pin_number}={state}");

        let pin = unsafe { AnyPin::steal(pin_number.try_into().unwrap()) };
        let mut output = Output::new(pin, Level::High, Default::default());
        output.set_level(if state == 0 { Level::Low } else { Level::High });
      },
    )?
    .func_wrap("index", "extern_get_millis", |_caller: Caller<'_, WasmCtx>| {
      Instant::now().duration_since_epoch().as_millis() as u32
    })?
    .func_wrap(
      "index",
      "extern_set_lcd_buffer",
      |mut caller: Caller<'_, WasmCtx>, ptr: u32| {
        let memory = caller
          .get_export("memory")
          .and_then(|export| export.into_memory())
          .ok_or_else(|| wasmi::Error::new("failed to find memory export"))
          .unwrap();

        let wasm_buffer =
          unsafe { core::slice::from_raw_parts(memory.data_ptr(&caller).add(ptr as usize), 240 * 240 * 2) };

        let interface: &mut DisplayInterface = unsafe { core::mem::transmute(SPI_DISPLAY_INTERFACE) };

        Command::ColumnAddressSet(0, SCREEN_WIDTH as u16 - 1).send(interface).unwrap();
        Command::RowAddressSet(0, SCREEN_HEIGHT as u16 - 1).send(interface).unwrap();
        Command::MemoryWrite.send(interface).unwrap();

        interface.send_data(DataFormat::U8(wasm_buffer)).ok();

        let now = Instant::now().duration_since_epoch().as_millis() as u32;

        // Also write the shadow buffer so remote screen viewing works. Low frame rate to reduce overhead.
        if now - (*caller.data()).last_screen_update > 250 {
          (*caller.data_mut()).last_screen_update = now;

          let raw_buffer = unsafe { from_raw_parts_mut(BUFFER, (SCREEN_WIDTH * SCREEN_HEIGHT * 2) as usize) };

          raw_buffer.clone_from_slice(wasm_buffer);
        }
      },
    )?
    .func_wrap(
      "index",
      "extern_write_wasm_ipc_message",
      |mut caller: Caller<'_, WasmCtx>, ptr: u32, len: u32| -> u32 {
        let wasm_ipc_sender = &caller.data().wasm_ipc_sender;

        let wasm_msg_bytes = caller.read_memory(ptr, len);
        let wasm_msg_id = caller.data().counter + 1;

        let wasm_ipc_msg: WasmIpcMessage = serde_json::from_slice(&VecHelper::to_global_vec(wasm_msg_bytes)).unwrap();

        wasm_ipc_sender.try_send((wasm_msg_id, wasm_ipc_msg)).unwrap();

        caller.data_mut().counter = wasm_msg_id;

        wasm_msg_id
      },
    )?
    .func_wrap(
      "index",
      "extern_read_host_ipc_message",
      |mut caller: Caller<'_, WasmCtx>, host_msg_id_a: u32, ptr: u32| {
        let host_ipc_receiver = &caller.data().host_ipc_receiver;

        let (host_msg_id_b, host_ipc_msg) = host_ipc_receiver.try_receive().unwrap();

        if host_msg_id_a != host_msg_id_b {
          panic!("Mismatched host IDs! {host_msg_id_a} {host_msg_id_b}");
        }

        let host_msg_bytes = serde_json::to_vec(&host_ipc_msg).unwrap();

        caller.write_memory(ptr, &host_msg_bytes);
      },
    )?;

  // Now we can compile the above Wasm module with the given Wasm source.
  let module = Box::new_in(unsafe { Module::new_unchecked(&engine, &buf) }?, ExternalMemory);

  print_memory_info();

  let mut store = Store::new(&engine, wasm_ctx);

  store.limiter(|ctx| &mut ctx.limiter);

  print_memory_info();

  let instance = linker.instantiate_and_start(&mut store, &module)?;

  print_memory_info();

  let wasm_main = instance
    .get_export(&store, "wasm_main")
    .and_then(Extern::into_func)
    .ok_or(wasmi::Error::new(format!("WASM: `wasm_main` not found")))?;

  let mut result = [];

  wasm_main
    .call(&mut store, &[], &mut result)
    .map_err(|err| wasmi::Error::new(format!("WASM: Error calling `wasm_main`: {err}")))?;

  print_memory_info();

  let mut last_print = Instant::now().duration_since_epoch().as_millis();

  let tick = instance.get_typed_func::<(u32, u32), i32>(&mut store, "tick")?;
  let get_memory_usage = instance.get_typed_func::<(), u32>(&mut store, "get_memory_usage")?;

  loop {
    let (host_msg_id, host_msg_length) = match host_ipc_receiver.try_peek() {
      Ok((host_msg_id, host_ipc_msg)) => {
        if let HostIpcMessage::Stop = host_ipc_msg {
          println!("==== Program ABORTED ====");
          return Ok(());
        }
        // TODO
        let host_msg_bytes = serde_json::to_vec(&host_ipc_msg).unwrap();
        (host_msg_id, host_msg_bytes.len() as u32)
      }
      Err(_) => (0, 0),
    };

    if tick
      .call(&mut store, (host_msg_id, host_msg_length))
      .map_err(|err| wasmi::Error::new(format!("Error Calling tick: {err}")))?
      != 0
    {
      break;
    }

    // if let Some(msg) = host_lifecycle_receiver.try_get() {
    //   match msg {
    //     HostLifecycleMessage::Stop => {
    //       println!("==== Program ABORTED ====");
    //       return Ok(());
    //     }
    //     _ => {}
    //   }
    // }

    // Timer::after(Duration::from_millis(100)).await;

    yield_now().await;

    let now = Instant::now().duration_since_epoch().as_millis();

    if now - last_print > 1000 {
      print_memory_info();

      let bytes = get_memory_usage.call(&mut store, ())?;

      println!("WASM app allocator usage: {bytes} bytes");

      last_print = now;
    }
  }

  println!("==== Program Complete ====");

  Ok(())
}

fn write_stdout(caller: Caller<'_, WasmCtx>, ptr: u32, len: u32) -> Result<(), wasmi::Error> {
  let buffer = caller.read_memory(ptr, len);

  let string = String::from_utf8_lossy(&buffer);

  print!("{}", string);

  Ok(())
}

// #[embassy_executor::task]
// async fn run_http(http_registry_loop: HttpRegistry) -> ! {
//   loop {
//     if let Err(err) = http_registry_loop.run().await {
//       println!("HttpError: {err:?}");
//     }

//     Timer::after(Duration::from_millis(500)).await;
//   }
// }
