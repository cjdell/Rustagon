use std::process;
use sysinfo::{RefreshKind, System};

pub fn print_memory_usage() {
  let mut sys = System::new_with_specifics(RefreshKind::everything());

  let pid = process::id() as usize;

  sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid.into()]), true);

  if let Some(process) = sys.process(pid.into()) {
    let memory_bytes = process.memory();
    let memory_kb = memory_bytes as f64 / 1024.0;

    println!("Current memory usage: {memory_kb} KB");
  }
}
