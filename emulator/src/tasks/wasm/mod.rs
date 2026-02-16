pub mod context;
pub mod protocol;
pub mod timers;

use crate::{
  print_memory_usage,
  tasks::wasm::{
    context::{MyLimiter, ReadWasmBuffer, WasmCtx},
    timers::{TimerRegistry, add_timers_to_linker},
  },
};
use std::{
  env, fs,
  ops::Sub,
  slice::from_raw_parts,
  sync::{
    Arc,
    mpmc::{Receiver, Sender},
  },
  time::{Duration, Instant},
};
use tokio::sync::RwLock;
use wasmi::*;
use zerocopy::FromBytes;

const WIDTH: usize = 240;
const HEIGHT: usize = 240;

pub fn wasmi_runner(
  lcd_buffer: Arc<RwLock<Vec<u32>>>,
  wasm_ipc_sender: Sender<(u32, Vec<u8>)>,
  host_ipc_receiver: Receiver<(u32, Vec<u8>)>,
) {
  let wasm_ctx = WasmCtx {
    start: Instant::now(),
    counter: 1,
    lcd_buffer: lcd_buffer.clone(),
    timer_registry: TimerRegistry::new(),
    host_ipc_receiver: host_ipc_receiver.clone(),
    wasm_ipc_sender,
    limiter: MyLimiter,
  };

  run_program(wasm_ctx, host_ipc_receiver).unwrap();
}

fn rgb565_to_rgb888(mut rgb565: u16) -> u32 {
  rgb565 = rgb565.to_be();

  // Extract the 5-bit red component (bits 11-15)
  let red = ((rgb565 >> 11) & 0x1F) as u8;
  // Extract the 6-bit green component (bits 5-10)
  let green = ((rgb565 >> 5) & 0x3F) as u8;
  // Extract the 5-bit blue component (bits 0-4)
  let blue = (rgb565 & 0x1F) as u8;

  // Convert 5-bit values (0-31) to 8-bit values (0-255)
  // Multiply by 255/31 ≈ 8.2258, which is equivalent to (value * 255 + 15) / 31
  // This provides better rounding than simple multiplication
  let red_8bit = (red as u32 * 255 + 15) / 31;
  let green_8bit = (green as u32 * 255 + 31) / 63; // 63 = 2^6 - 1
  let blue_8bit = (blue as u32 * 255 + 15) / 31;

  // Combine into u32 RGB888 format (RRGGBB, with 8 bits per channel)
  (red_8bit << 16) | (green_8bit << 8) | blue_8bit

  // ((red as u32) << (16 + 3)) + ((green as u32) << (8 + 2)) + ((blue as u32) << 3)
}

fn run_program(wasm_ctx: WasmCtx, host_ipc_receiver: Receiver<(u32, Vec<u8>)>) -> Result<(), wasmi::Error> {
  let args: Vec<String> = env::args().collect();

  let filename = args[1].clone();

  println!("Loading File: {}", filename);
  let wasm = fs::read(filename).expect("Could not load WASM file.");

  let engine = Engine::new(Config::default().set_max_stack_height(1024 * 1024));

  let module = Module::new(&engine, wasm)?;

  let mut store = Store::new(&engine, wasm_ctx);

  store.limiter(|ctx| &mut ctx.limiter);

  print_memory_usage();

  let mut linker = <Linker<WasmCtx>>::new(&engine);

  linker
    .func_wrap("index", "extern_write_stdout", host_println)?
    .func_wrap(
      "index",
      "extern_set_gpio",
      |_caller: Caller<'_, WasmCtx>, pin_number: u32, state: u32| {
        println!("set_gpio: {pin_number}={state}");
      },
    )?
    .func_wrap("index", "extern_get_millis", |caller: Caller<'_, WasmCtx>| {
      Instant::now().duration_since(caller.data().start).as_millis() as u32
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

        let buf: &[u16] = FromBytes::ref_from_bytes(unsafe {
          from_raw_parts(memory.data_ptr(&caller).add(ptr as usize), WIDTH * HEIGHT * 2)
        })
        .unwrap();

        let lcd_buffer = &mut caller.data_mut().lcd_buffer.blocking_write();
        let mut i = 0;

        for b in buf {
          (*lcd_buffer)[i] = rgb565_to_rgb888(*b);
          i += 1;
        }
      },
    )?
    .func_wrap(
      "index",
      "extern_write_wasm_ipc_message",
      |mut caller: Caller<'_, WasmCtx>, ptr: u32, len: u32| -> u32 {
        let wasm_ipc_sender = &caller.data().wasm_ipc_sender;

        let wasm_msg = caller.read_memory(ptr, len);
        let wasm_msg_id = caller.data().counter + 1;

        wasm_ipc_sender.try_send((wasm_msg_id, wasm_msg)).unwrap();

        caller.data_mut().counter = wasm_msg_id;

        wasm_msg_id
      },
    )?
    .func_wrap(
      "index",
      "extern_read_host_ipc_message",
      |mut caller: Caller<'_, WasmCtx>, host_msg_id_a: u32, ptr: u32| {
        let host_ipc_receiver = &caller.data().host_ipc_receiver;

        let (host_msg_id_b, host_msg_bytes) = host_ipc_receiver.try_recv().unwrap();

        if host_msg_id_a != host_msg_id_b {
          panic!("Mismatched host IDs! {host_msg_id_a} {host_msg_id_b}");
        }

        caller.write_memory(ptr, &host_msg_bytes);
      },
    )?;

  add_timers_to_linker(&mut linker).unwrap();

  let instance = linker.instantiate_and_start(&mut store, &module)?;

  print_memory_usage();

  let wasm_main = instance
    .get_export(&store, "wasm_main")
    .and_then(Extern::into_func)
    .unwrap();

  let mut result = [];

  wasm_main.call(&mut store, &[], &mut result).unwrap();

  let mut last_print = Instant::now().sub(Duration::from_secs(1));

  let tick = instance.get_typed_func::<(u32, u32), i32>(&mut store, "tick")?;
  let get_memory_usage = instance.get_typed_func::<(), u32>(&mut store, "get_memory_usage")?;

  loop {
    let (host_msg_id, host_msg_length) = match host_ipc_receiver.try_recv() {
      Ok((host_msg_id, buf)) => (host_msg_id, buf.len() as u32),
      Err(_) => (0, 0),
    };

    let now = Instant::now();

    if now - last_print > Duration::from_millis(1_000) {
      // print_memory_usage();
      let bytes = get_memory_usage.call(&mut store, ())?;
      println!("Current memory usage: {bytes} bytes");
      last_print = now;
    }

    if tick
      .call(&mut store, (host_msg_id, host_msg_length))
      .map_err(|err| wasmi::Error::new(format!("Error Calling tick: {err}")))?
      != 0
    {
      break;
    }
  }

  println!("==== Program Complete ====");

  Ok(())
}

// Define a host function that can read strings from wasm memory
fn host_println(caller: Caller<'_, WasmCtx>, ptr: i32, len: u32) -> Result<(), wasmi::Error> {
  // Get access to the wasm linear memory
  let memory = caller
    .get_export("memory")
    .and_then(|export| export.into_memory())
    .ok_or_else(|| wasmi::Error::new("failed to find memory export"))?;

  // Read the string data from wasm memory
  let mut buffer = vec![0u8; len as usize];
  memory
    .read(&caller, ptr as usize, &mut buffer)
    .map_err(|_| wasmi::Error::new("failed to read memory"))?;

  // Convert to UTF-8 string
  let string = String::from_utf8_lossy(&mut buffer);
  print!("{}", string);

  Ok(())
}
