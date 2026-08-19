use alloc::collections::BTreeMap;

pub struct TimerRegistry {
  timers: BTreeMap<i32, u64>,
  next_id: i32,
}

impl Default for TimerRegistry {
  fn default() -> Self {
    Self::new()
  }
}

impl TimerRegistry {
  pub fn new() -> Self {
    Self {
      timers: BTreeMap::new(),
      next_id: 1,
    }
  }

  pub fn register(&mut self, duration_ms: u32, now_ms: u64) -> i32 {
    let timer_id = self.next_id;
    self.next_id += 1;
    self.timers.insert(timer_id, now_ms + duration_ms as u64);
    timer_id
  }

  pub fn check(&self, timer_id: i32, now_ms: u64) -> i32 {
    if let Some(&expiry_time) = self.timers.get(&timer_id) {
      if now_ms >= expiry_time {
        return 1;
      }
      return 0;
    }
    0
  }

  pub fn cancel(&mut self, timer_id: i32) {
    self.timers.remove(&timer_id);
  }
}
