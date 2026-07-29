use crate::tasks::wasm::timers::TimerRegistry;
use std::{
  sync::{
    Arc,
    mpmc::{Receiver, Sender},
  },
  time::Instant,
};
use tokio::sync::RwLock;
use wasmi::{Caller, ResourceLimiter};
use wasmi_core::LimiterError;

pub struct WasmCtx {
  pub start: Instant,
  pub counter: u32,
  pub lcd_buffer: Arc<RwLock<Vec<u32>>>,
  pub timer_registry: TimerRegistry,
  pub wasm_ipc_sender: Sender<(u32, Vec<u8>)>,
  pub host_ipc_receiver: Receiver<(u32, Vec<u8>)>,
  pub limiter: MyLimiter,
}

pub struct MyLimiter;

impl ResourceLimiter for MyLimiter {
  fn memory_growing(&mut self, current: usize, desired: usize, _maximum: Option<usize>) -> Result<bool, LimiterError> {
    println!("memory_growing: current={current} desired={desired}");
    Ok(true)
  }

  fn table_growing(&mut self, current: usize, desired: usize, _maximum: Option<usize>) -> Result<bool, LimiterError> {
    println!("table_growing: current={current} desired={desired}");
    Ok(true)
  }

  fn instances(&self) -> usize {
    100
  }

  fn tables(&self) -> usize {
    100
  }

  fn memories(&self) -> usize {
    100
  }
}

pub trait ReadWasmBuffer {
  fn read_memory(&self, ptr: u32, len: u32) -> Vec<u8>;
  fn write_memory(&mut self, ptr: u32, buf: &[u8]);
}

impl ReadWasmBuffer for Caller<'_, WasmCtx> {
  fn read_memory(&self, ptr: u32, len: u32) -> Vec<u8> {
    let memory = self
      .get_export("memory")
      .and_then(|export| export.into_memory())
      .ok_or_else(|| wasmi::Error::new("failed to find memory export"))
      .unwrap();

    let mut buffer = vec![0u8; len as usize];

    memory
      .read(&self, ptr as usize, &mut buffer)
      .map_err(|_| wasmi::Error::new("failed to read memory"))
      .unwrap();

    buffer
  }

  fn write_memory(&mut self, ptr: u32, buf: &[u8]) {
    let memory = self
      .get_export("memory")
      .and_then(|export| export.into_memory())
      .ok_or_else(|| wasmi::Error::new("failed to find memory export"))
      .unwrap();

    memory
      .write(self, ptr as usize, &buf)
      .map_err(|_| wasmi::Error::new("failed to write memory"))
      .unwrap();
  }
}
