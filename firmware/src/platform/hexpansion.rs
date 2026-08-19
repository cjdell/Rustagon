use crate::platform::drivers::{ButtonEventQueue, DeviceEventQueue, DriverEntry};
use crate::platform::DisplayHandle;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use app::platform::hexpansion::{DeviceI2c, DeviceI2cError, DeviceI2cOps, DeviceIo, HexpansionManager};
use app::types::{DeviceEvent, HexpansionEvent, HexpansionInfo};
use core::cell::RefCell;
use core::fmt;
use core::future::Future;
use core::pin::Pin;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::{Duration, Timer};
use embedded_hal::i2c::{I2c, Operation};
use log::{debug, info};

use app::utils::EventQueue;

use crate::utils::MaskedI2cBus;

const EVENT_QUEUE_DEPTH: usize = 16;
type HexpansionEventQueue = EventQueue<HexpansionEvent, EVENT_QUEUE_DEPTH>;
type SharedState = Arc<Mutex<CriticalSectionRawMutex, RefCell<[Option<HexpansionInfo>; 6]>>>;

// ============================== DeviceI2cOps for MaskedI2cBus ==============================

/// Wraps `MaskedI2cBus` so it can be type-erased through `DeviceI2c`.
#[derive(Clone)]
struct I2cBusWrapper {
  bus: MaskedI2cBus,
}

impl fmt::Debug for I2cBusWrapper {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("I2cBusWrapper").finish()
  }
}

impl DeviceI2cOps for I2cBusWrapper {
  fn transaction(&self, addr: u8, write_data: &[u8], read_data: &mut [u8]) -> Result<(), DeviceI2cError> {
    use embedded_hal::i2c::I2c;
    let mut bus = self.bus.clone();
    if read_data.is_empty() {
      // Write-only — no read phase
      bus
        .transaction(addr, &mut [Operation::Write(write_data)])
        .map_err(|_| DeviceI2cError)
    } else {
      // Combined write-then-read with REPEATED START
      bus
        .transaction(addr, &mut [Operation::Write(write_data), Operation::Read(read_data)])
        .map_err(|_| DeviceI2cError)
    }
  }

  fn write(&self, addr: u8, data: &[u8]) -> Result<(), DeviceI2cError> {
    use embedded_hal::i2c::I2c;
    let mut bus = self.bus.clone();
    bus.transaction(addr, &mut [Operation::Write(data)]).map_err(|_| DeviceI2cError)
  }

  fn read(&self, addr: u8, data: &mut [u8]) -> Result<(), DeviceI2cError> {
    use embedded_hal::i2c::I2c;
    let mut bus = self.bus.clone();
    bus.transaction(addr, &mut [Operation::Read(data)]).map_err(|_| DeviceI2cError)
  }
}

// ============================== HardwareHexpansionManager ==============================

pub struct HardwareHexpansionManager {
  events: HexpansionEventQueue,
  device_events: DeviceEventQueue,
  state: SharedState,
}

impl fmt::Debug for HardwareHexpansionManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareHexpansionManager").finish()
  }
}

impl HardwareHexpansionManager {
  pub fn new(
    spawner: Spawner,
    hx_buses: [MaskedI2cBus; 6],
    display: DisplayHandle,
    driver_table: &'static [DriverEntry],
    button_events: ButtonEventQueue,
  ) -> Self {
    let events = HexpansionEventQueue::new();
    let device_events = DeviceEventQueue::new();
    let state: SharedState = Arc::new(Mutex::new(RefCell::new(Default::default())));
    spawner.spawn(
      hexpansion_poll_task(
        hx_buses,
        events.clone(),
        state.clone(),
        display,
        PollTaskResources {
          device_events: device_events.clone(),
          button_events,
          driver_table,
          spawner,
        },
      )
      .expect("spawn hexpansion_poll_task"),
    );
    Self {
      events,
      device_events,
      state,
    }
  }
}

impl HexpansionManager for HardwareHexpansionManager {
  fn next_event(&self) -> Pin<Box<dyn Future<Output = HexpansionEvent> + Send + '_>> {
    Box::pin(self.events.next())
  }

  fn try_next_event(&self) -> Option<HexpansionEvent> {
    self.events.try_next()
  }

  fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)> {
    self.state.lock(|state| {
      let slots = state.borrow();
      let mut result = Vec::new();
      for (i, slot) in slots.iter().enumerate() {
        result.push(((i + 1) as u8, slot.clone()));
      }
      result
    })
  }

  fn next_device_event(&self) -> Pin<Box<dyn Future<Output = DeviceEvent> + Send + '_>> {
    Box::pin(self.device_events.next())
  }

  fn try_next_device_event(&self) -> Option<DeviceEvent> {
    self.device_events.try_next()
  }
}

// ============================== Polling task ==============================

/// Resources the polling task needs to spawn hexpansion device drivers.
struct PollTaskResources {
  device_events: DeviceEventQueue,
  button_events: ButtonEventQueue,
  driver_table: &'static [DriverEntry],
  spawner: Spawner,
}

#[embassy_executor::task]
async fn hexpansion_poll_task(
  mut hx_buses: [MaskedI2cBus; 6],
  events: HexpansionEventQueue,
  state: SharedState,
  display: DisplayHandle,
  resources: PollTaskResources,
) {
  let PollTaskResources {
    device_events,
    button_events,
    driver_table,
    spawner,
  } = resources;
  let mut states = [PortState::Empty; 6];
  let mut driver_spawned = [false; 6];

  // Initial scan
  for port in 0..6 {
    let port_state = scan_port(&mut hx_buses[port]).await;
    if port_state != PortState::Empty {
      debug!("hexpansion_poll_task: port {} initial state {:?}", port + 1, port_state);
      if let PortState::Occupied { vid, pid, name, unique_id } = &port_state {
        let name_str = name_to_string(name);
        let info = HexpansionInfo {
          port: (port + 1) as u8,
          vid: *vid,
          pid: *pid,
          unique_id: *unique_id,
          friendly_name: name_str,
        };
        state.lock(|s| s.borrow_mut()[port] = Some(info.clone()));
        events.try_push(HexpansionEvent::Inserted(info.clone()));
        spawn_driver(
          (port + 1) as u8,
          *vid,
          *pid,
          &hx_buses,
          &device_events,
          &button_events,
          driver_table,
          spawner,
        );
        driver_spawned[port] = true;
      }
      states[port] = port_state;
    }
  }

  loop {
    Timer::after(Duration::from_secs(2)).await;

    for port in 0..6 {
      let new_state = scan_port(&mut hx_buses[port]).await;

      if states[port] == PortState::Empty && new_state != PortState::Empty {
        if let PortState::Occupied { vid, pid, name, unique_id } = &new_state {
          info!(
            "hexpansion_poll_task: port {} inserted (vid=0x{vid:04x}, pid=0x{pid:04x})",
            port + 1
          );
          let name_str = name_to_string(name);
          let info = HexpansionInfo {
            port: (port + 1) as u8,
            vid: *vid,
            pid: *pid,
            unique_id: *unique_id,
            friendly_name: name_str.clone(),
          };
          state.lock(|s| s.borrow_mut()[port] = Some(info.clone()));
          events.try_push(HexpansionEvent::Inserted(info.clone()));
          let notif_text = if name_str.is_empty() {
            alloc::format!("Hxp {:02X} {:04X}:{:04X}", port + 1, vid, pid)
          } else {
            name_str
          };
          let _ = display.signal(display_types::LcdScreen::Notification(display_types::Icon40::Info, notif_text));
          spawn_driver(
            (port + 1) as u8,
            *vid,
            *pid,
            &hx_buses,
            &device_events,
            &button_events,
            driver_table,
            spawner,
          );
          driver_spawned[port] = true;
        }
        states[port] = new_state;
      } else if states[port] != PortState::Empty && new_state == PortState::Empty {
        info!("hexpansion_poll_task: port {} removed", port + 1);
        state.lock(|s| s.borrow_mut()[port] = None);
        events.try_push(HexpansionEvent::Removed { port: (port + 1) as u8 });
        let _ = display.signal(display_types::LcdScreen::Notification(
          display_types::Icon40::Info,
          alloc::format!("Hxp {} removed", port + 1),
        ));
        states[port] = PortState::Empty;
        driver_spawned[port] = false;
      }
    }
  }
}

// Heterogeneous driver resources (queues, buses, table, spawner) — bundling
// them just to satisfy the lint would obscure what the driver actually gets.
#[allow(clippy::too_many_arguments)]
fn spawn_driver(
  port: u8,
  vid: u16,
  pid: u16,
  hx_buses: &[MaskedI2cBus; 6],
  device_events: &DeviceEventQueue,
  button_events: &ButtonEventQueue,
  driver_table: &'static [DriverEntry],
  spawner: Spawner,
) {
  let idx = (port as usize).wrapping_sub(1);
  if idx >= 6 {
    return;
  }

  for entry in driver_table {
    if entry.vid == vid && entry.pid == pid {
      info!("hexpansion: spawning driver for {vid:04x}:{pid:04x} on port {port}");
      let i2c = DeviceI2c::new(Arc::new(I2cBusWrapper {
        bus: hx_buses[idx].clone(),
      }));
      let io = DeviceIo { port, i2c, vid, pid };
      (entry.factory)(io, device_events.clone(), button_events.clone(), spawner);
      return;
    }
  }

  info!("hexpansion: no driver for {vid:04x}:{pid:04x} on port {port}");
}

// ============================== Port scanning ==============================

#[derive(Debug, Clone, Copy, PartialEq)]
enum PortState {
  Empty,
  Occupied { vid: u16, pid: u16, name: [u8; 9], unique_id: u32 },
}

fn name_to_string(name: &[u8; 9]) -> String {
  core::str::from_utf8(&name[..name.iter().position(|&b| b == 0).unwrap_or(9)])
    .unwrap_or("")
    .to_string()
}

async fn scan_port(bus: &mut MaskedI2cBus) -> PortState {
  let (eeprom_addr, addr_len) = match detect_eeprom_addr(bus) {
    Some(result) => result,
    None => return PortState::Empty,
  };
  debug!("hexpansion_scan: EEPROM found at 0x{eeprom_addr:02x} (addr_len={addr_len})");

  let mut header_buf = [0u8; 32];
  let mem_addr: &[u8] = if addr_len == 2 { &[0x00, 0x00] } else { &[0x00] };

  if bus
    .transaction(eeprom_addr, &mut [Operation::Write(mem_addr), Operation::Read(&mut header_buf)])
    .is_err()
  {
    return PortState::Empty;
  }

  if &header_buf[0..4] != b"THEX" {
    return PortState::Empty;
  }

  let checksum = header_buf[31];
  let mut calc = 0x55u8;
  for &b in &header_buf[1..31] {
    calc ^= b;
  }
  if calc != checksum {
    return PortState::Empty;
  }

  let vid = u16::from_le_bytes([header_buf[16], header_buf[17]]);
  let pid = u16::from_le_bytes([header_buf[18], header_buf[19]]);
  let unique_id = u16::from_le_bytes([header_buf[20], header_buf[21]]) as u32;
  let mut name = [0u8; 9];
  name.copy_from_slice(&header_buf[22..31]);
  debug!("hexpansion_scan: valid header vid=0x{vid:04x} pid=0x{pid:04x} uid={unique_id}");

  PortState::Occupied { vid, pid, name, unique_id }
}

fn detect_eeprom_addr(bus: &mut MaskedI2cBus) -> Option<(u8, u8)> {
  let devices = scan_i2c_bus(bus);

  if devices.contains(&0x57) && !devices.contains(&0x50) {
    return Some((0x57, 2));
  }

  let all_present = (0x50..=0x57).all(|addr| devices.contains(&addr));
  if all_present {
    return Some((0x50, 1));
  }

  if devices.contains(&0x50) {
    return Some((0x50, 2));
  }

  None
}

fn scan_i2c_bus(bus: &mut MaskedI2cBus) -> Vec<u8> {
  let mut found = Vec::new();
  let mut probe = [0u8; 1];
  for addr in 0x08..=0x77 {
    if bus.transaction(addr, &mut [Operation::Read(&mut probe)]).is_ok() {
      found.push(addr);
    }
  }
  found
}
