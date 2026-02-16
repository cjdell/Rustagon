extern crate alloc;

use crate::{
  lib::{HostIpcReceiver, WasmIpcSender},
  tasks::wasm::timers::TimerRegistry,
};
use alloc::vec::Vec;
use esp_alloc::ExternalMemory;
use esp_println::println;
use wasmi::{Caller, ResourceLimiter};
use wasmi_core::LimiterError;

pub struct WasmCtx {
  pub counter: u32,
  pub last_screen_update: u32,
  pub timer_registry: TimerRegistry,
  pub wasm_ipc_sender: WasmIpcSender,
  pub host_ipc_receiver: HostIpcReceiver,
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
  fn read_memory(&self, ptr: u32, len: u32) -> Vec<u8, ExternalMemory>;
  fn read_memory_into(&self, ptr: u32, buffer: &mut [u8]) -> ();
  fn write_memory(&mut self, ptr: u32, buf: &[u8]);
}

impl ReadWasmBuffer for Caller<'_, WasmCtx> {
  fn read_memory(&self, ptr: u32, len: u32) -> Vec<u8, ExternalMemory> {
    let mut buffer = Vec::new_in(ExternalMemory);
    buffer.resize(len as usize, 0u8);

    self.read_memory_into(ptr, &mut buffer);

    buffer
  }

  fn read_memory_into(&self, ptr: u32, buffer: &mut [u8]) -> () {
    let memory = self
      .get_export("memory")
      .and_then(|export| export.into_memory())
      .ok_or_else(|| wasmi::Error::new("failed to find memory export"))
      .unwrap();

    memory.read(&self, ptr as usize, buffer).map_err(|_| wasmi::Error::new("failed to read memory")).unwrap();
  }

  fn write_memory(&mut self, ptr: u32, buf: &[u8]) {
    let memory = self
      .get_export("memory")
      .and_then(|export| export.into_memory())
      .ok_or_else(|| wasmi::Error::new("failed to find memory export"))
      .unwrap();

    memory.write(self, ptr as usize, &buf).map_err(|_| wasmi::Error::new("failed to write memory")).unwrap();
  }
}
