#![no_std]
#![no_main]
#![feature(
  addr_parse_ascii,
  impl_trait_in_assoc_type,
  error_generic_member_access,
  future_join,
  allocator_api,
  box_vec_non_null,
  async_trait_bounds,
  impl_trait_in_bindings,
  substr_range
)]
#![recursion_limit = "256"]

use alloc::{borrow::ToOwned as _, string::ToString as _, sync::Arc};
use core::{net::Ipv4Addr, str::FromStr};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use esp_alloc::{heap_allocator, psram_allocator};
use esp_backtrace as _;
use esp_hal::{
  i2c::master::{BusTimeout, I2c},
  interrupt::software::SoftwareInterruptControl,
  peripherals::FLASH,
  time::Rate,
  timer::timg::{MwdtStage, TimerGroup},
};
use esp_println::println;
use esp_storage::FlashStorage;
use firmware::types::*;
use firmware::utils::*;
use firmware::{
  d_i2c::*,
  platform::{
    HardwareInputManager, HardwareLedManager, HardwarePlatform, HardwarePowerManager, HardwareWifiManager, InputHandle, LedHandle,
    Platform, PowerHandle, WiFiHandle,
  },
};
use firmware::{
  platform::system::{HardwareSystemManager, SystemHandle},
  tasks::*,
};
use log::{error, info, warn};
use picoserve::make_static;
use static_cell::StaticCell;

extern crate alloc;
extern crate core;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
  esp_println::logger::init_logger_from_env();

  let config = esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max());
  let peripherals = esp_hal::init(config);

  heap_allocator!(#[esp_hal::ram(reclaimed)] size: 72 * 1024);
  heap_allocator!(size: 72 * 1024);

  psram_allocator!(peripherals.PSRAM, esp_hal::psram);

  let timg0 = TimerGroup::new(peripherals.TIMG0);
  let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
  esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

  // Enable the watchdog so we reboot if there's a problem
  let mut wdt = timg0.wdt;
  wdt.set_timeout(MwdtStage::Stage0, esp_hal::time::Duration::from_millis(30_000));
  wdt.enable();

  println!("Init!");
  print_memory_info();

  let i2c = I2c::new(
    peripherals.I2C0,
    esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(100)).with_timeout(BusTimeout::BusCycles(133_000)),
  )
  .unwrap()
  .with_sda(peripherals.GPIO45)
  .with_scl(peripherals.GPIO46);

  let multiplexed_i2c_bus = MultiplexedI2cBus::new(i2c);

  let sys_bus = multiplexed_i2c_bus.new_masked_i2c_bus(MultiplexedI2cBus::SYS_BUS);
  let top_bus = multiplexed_i2c_bus.new_masked_i2c_bus(MultiplexedI2cBus::TOP_BUS);

  // I2C won't work until reset
  reset_device(peripherals.GPIO9);

  scan_devices(sys_bus.clone());

  init_gpio(sys_bus.clone(), I2C_0).await;
  init_gpio(sys_bus.clone(), I2C_1).await;
  init_gpio(sys_bus.clone(), I2C_2).await;

  // let system_watch: &mut embassy_sync::watch::Watch<CriticalSectionRawMutex, SystemMessage, 1> = make_static!(SystemWatch, SystemWatch::new());
  let lcd_signal = make_static!(LcdSignal, LcdSignal::new());
  let i2c_channel = make_static!(HexButtonChannel, HexButtonChannel::new());
  let button_event_channel = make_static!(ButtonEventChannel, ButtonEventChannel::new());
  let wasm_ipc_channel = make_static!(WasmIpcChannel, WasmIpcChannel::new());
  let host_ipc_channel = make_static!(HostIpcChannel, HostIpcChannel::new());
  let http_channel = make_static!(HttpChannel, HttpChannel::new());
  let web_socket_incoming_channel = make_static!(WebSocketIncomingChannel, WebSocketIncomingChannel::new());

  let i2c_publisher = i2c_channel.publisher().unwrap();
  let button_event_sender = button_event_channel.sender();
  let button_event_receiver = button_event_channel.receiver();

  spawner.spawn(lcd_task(sys_bus.clone(), lcd_signal)).ok();

  lcd_signal.signal(LcdScreen::Headline(Icon40::Info, "Init".to_string()));
  sleep(1_000).await;
  lcd_signal.signal(LcdScreen::Splash);
  sleep(1_000).await;

  lcd_signal.signal(LcdScreen::Headline(Icon40::Info, "Checking filesystem...".to_owned()));

  let flash = make_static!(FlashStorage, FlashStorage::new(peripherals.FLASH));

  let local_fs = match LocalFs::new(flash) {
    Ok(local_fs) => {
      info!("Local OK");
      lcd_signal.signal(LcdScreen::Headline(Icon40::Info, "Filesystem OK".to_owned()));
      sleep(100).await;
      local_fs
    }
    Err(err) => {
      error!("Filesystem Error: {err:?}");
      wdt.disable();

      lcd_signal.signal(LcdScreen::Headline(Icon40::Warn, "Format may take a while".to_owned()));
      sleep(2_000).await;

      lcd_signal.signal(LcdScreen::Headline(Icon40::Info, "Reformatting...".to_owned()));
      sleep(100).await;

      let flash = make_static!(FlashStorage, FlashStorage::new(unsafe { FLASH::steal() }));
      LocalFs::make_new_filesystem(flash);
      warn!("New File System Created! Rebooting...");
      lcd_signal.signal(LcdScreen::Headline(Icon40::Info, "Format Complete!".to_string()));
      sleep(1_000).await;

      esp_hal::system::software_reset();
    }
  };

  let free_clusters = local_fs.stats().unwrap().free_clusters();
  info!("Filesystem verified. Free clusters: {free_clusters}");

  if free_clusters == 0 {
    lcd_signal.signal(LcdScreen::Headline(Icon40::Error, "Filesystem Unwritable!".to_string()));
    sleep(2_000).await;
  }

  let mut device_state = DeviceState::new(local_fs.clone(), "device.jsn".to_string(), DeviceConfig::default());
  if let Err(err) = device_state.init() {
    warn!("Could not correctly initialise device config. Using defaults. Error: {err:?}");
  }

  match device_state.get_json() {
    Ok(json) => info!("JSON: {json}"),
    Err(err) => error!("JSON Error: {err:?}"),
  }

  let (controller, interfaces) = esp_radio::wifi::new(peripherals.WIFI, Default::default()).unwrap();

  let wifi_interface = match device_state.get_data().wifi_mode {
    WifiMode::Station => interfaces.station,
    WifiMode::AccessPoint => interfaces.access_point,
  };

  let rng = esp_hal::rng::Rng::new();
  let seed = (rng.random() as u64) << 32 | rng.random() as u64;

  // Init network stack
  let (stack, runner) = embassy_net::new(
    wifi_interface,
    embassy_net::Config::dhcpv4(Default::default()),
    make_static!(embassy_net::StackResources<8>, embassy_net::StackResources::<8>::new()),
    seed,
  );

  print_memory_info();

  println!("Starting connection...");
  lcd_signal.signal(LcdScreen::Progress("Connecting...".to_string()));

  print_memory_info();

  let ap_ip = Ipv4Addr::from_str("192.168.1.1").expect("Failed to parse AP IP!");

  // Create and initialize WiFi manager
  // (Status is managed internally via WatchedValue)
  let wifi_manager = HardwareWifiManager::new();

  wifi_manager.spawn_connection_task(spawner, device_state.clone(), controller, stack, ap_ip);

  spawner.spawn(net_task(runner)).ok();

  if let WifiMode::AccessPoint = device_state.get_data().wifi_mode {
    spawner.spawn(dhcp_task(stack, ap_ip)).ok();
    spawner.spawn(captive_task(stack, ap_ip)).ok();
  }

  print_memory_info();

  // Initialize platform with hardware managers
  let led_manager = Arc::new(HardwareLedManager::new(&spawner, sys_bus.clone()));
  let led = LedHandle::new(led_manager);

  let power_manager = Arc::new(HardwarePowerManager::new(sys_bus.clone()));
  let power = PowerHandle::new(power_manager);

  let wifi = WiFiHandle::new(wifi_manager);

  let input = InputHandle::new(HardwareInputManager::new(
    spawner,
    sys_bus.clone(),
    top_bus.clone(),
    button_event_sender,
    button_event_receiver,
  ));

  let system = SystemHandle::new(HardwareSystemManager::new(spawner, peripherals.GPIO0));

  let platform = HardwarePlatform::new_with_managers(led, power, wifi, input, system);

  let _ = platform.led_manager().request(LedRequest::Breathe(LedState { r: 255, g: 0, b: 0 }));

  static APP_CORE_STACK: StaticCell<esp_hal::system::Stack<16384>> = StaticCell::new();
  let app_core_stack = APP_CORE_STACK.init(esp_hal::system::Stack::new());

  let wasm_sender = wasm_ipc_channel.sender();
  let host_receiver = host_ipc_channel.receiver();

  let local_fs_2nd_core = local_fs.clone();

  esp_rtos::start_second_core(peripherals.CPU_CTRL, sw_int.software_interrupt1, app_core_stack, move || {
    static EXECUTOR: StaticCell<esp_rtos::embassy::Executor> = StaticCell::new();
    let executor = EXECUTOR.init(esp_rtos::embassy::Executor::new());

    executor.run(|spawner| {
      spawner.spawn(second_core_task(local_fs_2nd_core, wasm_sender, host_receiver)).ok();
    });
  });

  print_memory_info();

  start_http(
    spawner,
    stack,
    local_fs.clone(),
    device_state.clone(),
    http_channel.sender(),
    web_socket_incoming_channel.sender(),
    platform.clone(),
  );

  print_memory_info();

  let runner_ctx = MenuRunnerContext {
    stack,
    local_fs: local_fs.clone(),
    device_state: device_state.clone(),
    http_event_receiver: http_channel.receiver(),
    host_ipc_sender: host_ipc_channel.sender(),
    wasm_ipc_channel,
    lcd_signal,
    platform: platform.clone(),
  };

  // Spawn WiFi monitor task to handle status changes
  spawner.spawn(wifi_monitor_task(platform, lcd_signal, device_state.clone())).ok();

  spawner.spawn(menu_task(runner_ctx)).ok();

  spawner
    .spawn(websocket_input_forwarder_task(
      web_socket_incoming_channel.receiver(),
      i2c_channel.publisher().unwrap(),
    ))
    .ok();

  loop {
    sleep(1_000).await;
    wdt.feed();
  }
}

#[embassy_executor::task]
async fn websocket_input_forwarder_task(
  web_socket_incoming_receiver: WebSocketIncomingReceiver,
  hex_button_sender: HexButtonSender,
  // system_sender: SystemSender,
) {
  loop {
    match web_socket_incoming_receiver.receive().await {
      WebSocketIncomingMessage::HexButton(hex_button) => {
        hex_button_sender.publish(I2cMessage::HexButton(hex_button)).await;
      }
      WebSocketIncomingMessage::SystemMessage(button) => {
        // system_sender.send(button);  // TODO!!!
      }
    }
  }
}
