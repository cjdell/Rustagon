use esp_hal::system::{Cpu, CpuControl};
use esp_println::println;

pub struct CpuGuard<'a> {
  cpu_ctrl: &'a mut CpuControl<'a>,
  _parks: bool, // Track if we parked (to avoid double-unpark)
}

impl<'a> CpuGuard<'a> {
  pub fn new(cpu_ctrl: &'a mut CpuControl<'a>) -> Self {
    // Park the App CPU — you must call this *before* returning this guard
    unsafe {
      println!("==== PARKING SECOND CORE ====");
      cpu_ctrl.park_core(Cpu::AppCpu);
    }
    Self { cpu_ctrl, _parks: true }
  }

  pub fn release(&mut self) {
    if self._parks {
      self.cpu_ctrl.unpark_core(Cpu::AppCpu);
      println!("==== UNPARKED SECOND CORE ====");
      self._parks = false;
    }
  }
}

impl<'a> Drop for CpuGuard<'a> {
  fn drop(&mut self) {
    // ✅ Automatic cleanup: unpark if still parked
    if self._parks {
      self.cpu_ctrl.unpark_core(Cpu::AppCpu);
      println!("==== UNPARKED SECOND CORE ====");
    }
  }
}
