use crate::tasks::wasm::context::WasmCtx;
use alloc::collections::BTreeMap;
use anyhow::Error;
use embassy_time::Instant;
use wasmi::{Caller, Linker};

extern crate alloc;
extern crate core;

// Timer registry to track active timers
pub struct TimerRegistry {
  timers: BTreeMap<i32, u64>,
  next_id: i32,
}

impl TimerRegistry {
  pub fn new() -> Self {
    Self {
      timers: BTreeMap::new(),
      next_id: 1,
    }
  }

  pub fn register(&mut self, duration_ms: u32) -> i32 {
    let timer_id = self.next_id;
    self.next_id += 1;

    let expiry = Instant::now().as_millis() + duration_ms as u64;
    self.timers.insert(timer_id, expiry);

    timer_id
  }

  pub fn check(&self, timer_id: i32) -> i32 {
    if let Some(&expiry_time) = self.timers.get(&timer_id) {
      if Instant::now().as_millis() >= expiry_time {
        return 1; // Timer expired
      }
      return 0; // Timer still running
    }
    0 // Timer not found (treat as expired or invalid)
  }

  pub fn cancel(&mut self, timer_id: i32) {
    self.timers.remove(&timer_id);
  }
}

pub fn add_timers_to_linker(linker: &mut Linker<WasmCtx>) -> Result<(), Error> {
  linker
    .func_wrap("index", "extern_register_timer", |mut caller: Caller<'_, WasmCtx>, ms: u32| -> i32 {
      let timer_id = caller.data_mut().timer_registry.register(ms);
      // println!("register_timer: {ms}ms -> timer_id={timer_id}");
      timer_id
    })?
    .func_wrap("index", "extern_check_timer", |mut caller: Caller<'_, WasmCtx>, timer_id: i32| -> i32 {
      let expired = caller.data().timer_registry.check(timer_id);
      if expired == 1 {
        caller.data_mut().timer_registry.cancel(timer_id);
        // println!("check_timer: timer_id={timer_id} EXPIRED");
      }
      expired
    })?
    .func_wrap("index", "extern_cancel_timer", |mut caller: Caller<'_, WasmCtx>, timer_id: i32| {
      caller.data_mut().timer_registry.cancel(timer_id);
      // println!("cancel_timer: timer_id={timer_id}");
    })?;

  Ok(())
}
