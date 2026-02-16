use crate::tasks::wasm::context::WasmCtx;
use eyre::Error;
use std::{
  collections::HashMap,
  time::{Duration, SystemTime},
};
use wasmi::{Caller, Linker};

// Timer registry to track active timers
pub struct TimerRegistry {
  timers: HashMap<i32, SystemTime>,
  next_id: i32,
}

impl TimerRegistry {
  pub fn new() -> Self {
    Self {
      timers: HashMap::new(),
      next_id: 1,
    }
  }

  pub fn register(&mut self, duration_ms: u32) -> i32 {
    let timer_id = self.next_id;
    self.next_id += 1;

    let expiry = SystemTime::now() + Duration::from_millis(duration_ms as u64);
    self.timers.insert(timer_id, expiry);

    timer_id
  }

  pub fn check(&self, timer_id: i32) -> i32 {
    if let Some(&expiry_time) = self.timers.get(&timer_id) {
      if SystemTime::now() >= expiry_time {
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
    .func_wrap(
      "index",
      "extern_register_timer",
      |mut caller: Caller<'_, WasmCtx>, ms: u32| -> i32 {
        let timer_id = caller.data_mut().timer_registry.register(ms);
        // println!("register_timer: {ms}ms -> timer_id={timer_id}");
        timer_id
      },
    )?
    .func_wrap(
      "index",
      "extern_check_timer",
      |caller: Caller<'_, WasmCtx>, timer_id: i32| -> i32 {
        let expired = caller.data().timer_registry.check(timer_id);
        if expired == 1 {
          // println!("check_timer: timer_id={timer_id} EXPIRED");
        }
        expired
      },
    )?
    .func_wrap(
      "index",
      "extern_cancel_timer",
      |mut caller: Caller<'_, WasmCtx>, timer_id: i32| {
        caller.data_mut().timer_registry.cancel(timer_id);
        // println!("cancel_timer: timer_id={timer_id}");
      },
    )?;

  Ok(())
}
