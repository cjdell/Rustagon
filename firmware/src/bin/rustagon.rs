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
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::rwlock::RwLock;
use app::menu::state::AppState;
use esp_alloc::{heap_allocator, psram_allocator};
use esp_backtrace as _;
use esp_hal::{
  i2c::master::{BusTimeout, I2c},
  interrupt::software::SoftwareInterruptControl,
  time::Rate,
  timer::timg::{MwdtStage, TimerGroup},
};
use esp_println::println;
use esp_storage::FlashStorage as EspFlashStorage;
use esp32s3_embedded_tools::flash::LittleFsFlashStorage;
use firmware::d_i2c::*;
use embedded_tools::config::ConfigFile;
use embedded_tools::config::storage::LocalFsConfigFileStorage;
use firmware::platform::{
  ConfigHandle, HardwareInputManager, HardwareLedManager, HardwarePlatform, HardwarePowerManager, HardwareStorageManager,
  HardwareWifiManager, InputHandle, LedHandle, Platform, PowerHandle, StorageHandle, WiFiHandle,
};
use firmware::platform::{
  display::{HardwareDisplayManager, LcdSignal, lcd_task},
  system::{HardwareSystemManager, SystemHandle},
};
use firmware::tasks::*;
use firmware::types::*;
use firmware::utils::*;
use log::{error, info, warn};
use picoserve::make_static;
use static_cell::StaticCell;

extern crate alloc;
extern crate core;

const VFS_PARTITION_OFFSET: u32 = 0x00290000;

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

  let lcd_signal = make_static!(LcdSignal, LcdSignal::new());
  let i2c_channel = make_static!(HexButtonChannel, HexButtonChannel::new());
  let wasm_ipc_channel = make_static!(WasmIpcChannel, WasmIpcChannel::new());
  let host_ipc_channel = make_static!(HostIpcChannel, HostIpcChannel::new());
  let http_channel = make_static!(HttpChannel, HttpChannel::new());
  let web_socket_incoming_channel = make_static!(WebSocketIncomingChannel, WebSocketIncomingChannel::new());

  let _i2c_publisher = i2c_channel.publisher().unwrap();

  spawner.spawn(lcd_task(sys_bus.clone(), lcd_signal)).ok();

  let display_manager = alloc::sync::Arc::new(HardwareDisplayManager::new(lcd_signal));
  let display = firmware::platform::display::DisplayHandle::new(display_manager.clone());

  let _ = display.signal(LcdScreen::Headline(Icon40::Info, "Init".to_string()));
  sleep(1_000).await;
  let _ = display.signal(LcdScreen::Splash);
  sleep(1_000).await;

  let _ = display.signal(LcdScreen::Headline(Icon40::Info, "Checking filesystem...".to_owned()));

  // Create shared flash storage with auto-park for multicore safety
  let flash = Arc::new(RwLock::new(
    EspFlashStorage::new(peripherals.FLASH).multicore_auto_park(),
  ));

  // Try to mount the filesystem
  let littlefs_storage = LittleFsFlashStorage::new(flash.clone(), VFS_PARTITION_OFFSET);

  let storage_formatter = HardwareStorageManager::new(flash.clone(), VFS_PARTITION_OFFSET);

  let (storage, config_handle) = match embedded_tools::local_fs::LocalFs::new(littlefs_storage) {
    Ok(local_fs) => {
      info!("Filesystem OK");
      let _ = display.signal(LcdScreen::Headline(Icon40::Info, "Filesystem OK".to_owned()));
      sleep(100).await;

      let fs_for_config = local_fs.clone();
      let storage_handle = StorageHandle::new(Arc::new(local_fs));

      let config_file = ConfigFile::new(
        LocalFsConfigFileStorage::new(fs_for_config, "device.jsn".to_string()),
        DeviceConfig::default(),
      ).await;
      let config_handle = ConfigHandle::new(Arc::new(config_file));

      (storage_handle, config_handle)
    }
    Err(_) => {
      error!("Filesystem Error: Corrupt. Formatting...");
      wdt.disable();

      let _ = display.signal(LcdScreen::Headline(Icon40::Warn, "Format may take a while".to_owned()));
      sleep(2_000).await;

      let _ = display.signal(LcdScreen::Headline(Icon40::Info, "Reformatting...".to_owned()));
      sleep(100).await;

      // Format the filesystem
      {
        let mut format_storage = LittleFsFlashStorage::new(flash.clone(), VFS_PARTITION_OFFSET);
        embedded_tools::local_fs::LocalFs::format(&mut format_storage).unwrap();
      }
      warn!("New File System Created! Rebooting...");
      let _ = display.signal(LcdScreen::Headline(Icon40::Info, "Format Complete!".to_string()));
      sleep(1_000).await;

      esp_hal::system::software_reset();
    }
  };

  let (controller, interfaces) = esp_radio::wifi::new(peripherals.WIFI, Default::default()).unwrap();

  let wifi_mode = config_handle.get_data().await.wifi_mode;

  let wifi_interface = match wifi_mode {
    firmware::types::WifiMode::Station => interfaces.station,
    firmware::types::WifiMode::AccessPoint => interfaces.access_point,
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
  let _ = display.signal(LcdScreen::Progress("Connecting...".to_string()));

  print_memory_info();

  let ap_ip = Ipv4Addr::from_str("192.168.1.1").expect("Failed to parse AP IP!");

  // Create and initialize WiFi manager
  let wifi_manager = HardwareWifiManager::new();
  wifi_manager.spawn_connection_task(spawner, config_handle.clone(), controller, stack, ap_ip);

  spawner.spawn(net_task(runner)).ok();

  if let firmware::types::WifiMode::AccessPoint = wifi_mode {
    spawner.spawn(dhcp_task(stack, ap_ip)).ok();
    spawner.spawn(captive_task(stack, ap_ip)).ok();
  }

  print_memory_info();

  // Initialize platform with hardware managers
  let led_manager = Arc::new(HardwareLedManager::new(&spawner, sys_bus.clone()));
  let led = LedHandle::new(led_manager);

  let power_manager = Arc::new(HardwarePowerManager::new(sys_bus.clone()));
  let power = PowerHandle::new(power_manager);

  let wifi = WiFiHandle::new(Arc::new(wifi_manager));

  let input = InputHandle::new(Arc::new(HardwareInputManager::new(spawner, sys_bus.clone(), top_bus.clone())));

  let system = SystemHandle::new(Arc::new(HardwareSystemManager::new(spawner, peripherals.GPIO0)));

  let platform = HardwarePlatform::new_with_managers(display, led, power, wifi, input, system, storage.clone(), config_handle.clone(), storage_formatter.clone());

  let _ = platform.led_manager().request(LedRequest::Breathe(LedState { r: 255, g: 0, b: 0 }));

  static APP_CORE_STACK: StaticCell<esp_hal::system::Stack<16384>> = StaticCell::new();
  let app_core_stack = APP_CORE_STACK.init(esp_hal::system::Stack::new());

  let wasm_sender = wasm_ipc_channel.sender();
  let host_receiver = host_ipc_channel.receiver();

  let storage_2nd_core = storage.clone();

  esp_rtos::start_second_core(peripherals.CPU_CTRL, sw_int.software_interrupt1, app_core_stack, move || {
    static EXECUTOR: StaticCell<esp_rtos::embassy::Executor> = StaticCell::new();
    let executor = EXECUTOR.init(esp_rtos::embassy::Executor::new());

    executor.run(|spawner| {
      spawner.spawn(second_core_task(storage_2nd_core, wasm_sender, host_receiver)).ok();
    });
  });

  print_memory_info();

  start_http(
    spawner,
    stack,
    storage.clone(),
    http_channel.sender(),
    web_socket_incoming_channel.sender(),
    platform.clone(),
  );

  print_memory_info();

  let http_client = firmware::platform::http::HardwareHttpClient::new(stack);
  let platform = platform.with_http_client(app::platform::HttpClientHandle::new(Arc::new(http_client)));

  let app_state = Arc::new(RwLock::<CriticalSectionRawMutex, AppState>::new(AppState::None));

  let runner_ctx = MenuRunnerContext {
    stack,
    storage: storage.clone(),
    host_ipc_sender: host_ipc_channel.sender(),
    platform: platform.clone(),
    app_state: app_state.clone(),
  };

  let platform_for_ws = platform.clone();

  // Spawn WiFi monitor task to handle status changes
  spawner.spawn(wifi_monitor_task(platform)).ok();

  // Spawn IPC handler for WASM IPC and HTTP events
  spawner.spawn(ipc_handler_task(
    wasm_ipc_channel,
    http_channel.receiver(),
    host_ipc_channel.sender(),
    platform_for_ws.clone(),
    app_state,
  )).ok();

  spawner.spawn(menu_task(runner_ctx)).ok();

  spawner
    .spawn(websocket_input_forwarder_task(
      web_socket_incoming_channel.receiver(),
      platform_for_ws,
    ))
    .ok();

  loop {
    sleep(1_000).await;
    wdt.feed();
  }
}

#[embassy_executor::task]
/// Forwards remote control events received over the WebSocket into the platform's
/// input/system event queues, so they are indistinguishable from physical presses.
async fn websocket_input_forwarder_task(web_socket_incoming_receiver: WebSocketIncomingReceiver, platform: HardwarePlatform) {
  loop {
    match web_socket_incoming_receiver.receive().await {
      WebSocketIncomingMessage::HexButton(hex_button) => {
        platform.input_manager().inject_button(hex_button).await;
      }
      WebSocketIncomingMessage::SystemMessage(message) => {
        platform.system_manager().inject(message).await;
      }
    }
  }
}
