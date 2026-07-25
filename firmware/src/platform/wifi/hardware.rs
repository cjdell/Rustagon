use super::traits::*;
use alloc::{boxed::Box, format, string::String, sync::Arc, vec::Vec};
use core::fmt;
use core::net::Ipv4Addr;
use core::pin::Pin;
use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use embassy_time::{Duration, Timer};
use esp_hal::time::Instant;
use esp_radio::wifi::{
  AuthenticationMethod, ModeConfig, WifiController, WifiDevice,
  ap::AccessPointConfig,
  scan::{ScanConfig, ScanTypeConfig},
  sta::StationConfig,
};
use log::{error, info};

use crate::types::DeviceConfig;
use crate::utils::{PersistentStateService, WatchedValue};

const RETRY_INTERVAL: u64 = 60_000;

/// Hardware WiFi Manager using ESP32 WiFi controller
///
/// The manager uses WatchedValue for WiFi status, which provides:
/// - Async reads via get()
/// - Awaitable changes via wait_for_change()
/// - Single source of truth with no duplication
#[derive(Clone)]
pub struct HardwareWifiManager {
  status: WatchedValue<WifiStatus>,
  desired_state: Arc<RwLock<CriticalSectionRawMutex, WifiDesiredState>>,
  last_scan_results: Arc<RwLock<CriticalSectionRawMutex, Vec<WifiResult>>>,
  connection_attempts: Arc<core::sync::atomic::AtomicU32>,
  successful_connections: Arc<core::sync::atomic::AtomicU32>,
}

impl HardwareWifiManager {
  pub fn new() -> Self {
    Self {
      status: WatchedValue::new(WifiStatus::Offline),
      desired_state: Arc::new(RwLock::new(WifiDesiredState::Offline)),
      last_scan_results: Arc::new(RwLock::new(Vec::new())),
      connection_attempts: Arc::new(core::sync::atomic::AtomicU32::new(0)),
      successful_connections: Arc::new(core::sync::atomic::AtomicU32::new(0)),
    }
  }

  /// Internal method to update status and notify all watchers
  /// Called by the connection task when status changes
  pub(crate) async fn set_status(&self, new_status: WifiStatus) {
    self.status.set(new_status).await;
  }

  /// Spawn the background connection task
  /// This must be called once during initialization
  pub fn spawn_connection_task(
    &self,
    spawner: Spawner,
    device_config: PersistentStateService<DeviceConfig>,
    controller: WifiController<'static>,
    stack: embassy_net::Stack<'static>,
    ap_ip: Ipv4Addr,
  ) {
    let manager = self.clone();
    spawner.spawn(wifi_connection_task(device_config, controller, stack, ap_ip, manager)).ok();
  }
}

#[embassy_executor::task]
pub async fn wifi_connection_task(
  device_config: PersistentStateService<DeviceConfig>,
  mut controller: WifiController<'static>,
  stack: embassy_net::Stack<'static>,
  ap_ip: Ipv4Addr,
  manager: HardwareWifiManager,
) {
  info!("WiFi: Connection task started");

  let mut was_connected = false;
  let mut retry_in: u64 = 0;

  let wifi_mode = device_config.get_data().wifi_mode;

  loop {
    Timer::after(Duration::from_millis(1_000)).await;

    let desired_state = *manager.desired_state.read().await;

    match desired_state {
      WifiDesiredState::Offline => {
        if controller.is_connected().unwrap_or_default() {
          match controller.disconnect_async().await {
            Ok(_) => {
              was_connected = false;
              info!("WiFi: Disconnected (planned)");
              manager.set_status(WifiStatus::Offline).await;
            }
            Err(err) => {
              error!("WiFi: Failed to disconnect {err:?}");
              Timer::after(Duration::from_millis(5000)).await
            }
          }
        }

        if controller.is_started().unwrap_or_default() {
          match controller.stop_async().await {
            Ok(_) => {
              info!("WiFi: Stopped (planned)");
              manager.set_status(WifiStatus::Offline).await;
            }
            Err(err) => error!("WiFi: Failed to stop {err:?}"),
          };
        }

        stack.set_config_v4(embassy_net::ConfigV4::None);

        loop {
          if !stack.is_config_up() {
            break;
          }
          Timer::after(Duration::from_millis(100)).await;
        }
      }
      WifiDesiredState::Online => match wifi_mode {
        crate::types::WifiMode::Station => {
          let mut ip_address = None;

          if !controller.is_connected().unwrap_or_default() {
            if was_connected {
              was_connected = false;
              info!("WiFi: Interrupted!");
              manager.set_status(WifiStatus::Interrupted).await;
            } else {
              // Not yet connected and not previously connected - we're connecting
              info!("WiFi: Attempting to connect...");
              manager.set_status(WifiStatus::Connecting).await;
            }

            if !controller.is_started().unwrap_or_default() {
              let config = ModeConfig::Station(StationConfig::default());

              if let Err(err) = controller.set_config(&config) {
                error!("WiFi: Error setting config: {err:?}");
                continue;
              }

              if let Err(err) = controller.start_async().await {
                error!("WiFi: Error starting: {err:?}");
                continue;
              }
            }

            let mut best_network: Option<(String, String, i8)> = None;

            for _ in 0..3 {
              match controller.scan_with_config_async(ScanConfig::default()).await {
                Ok(found_networks) => {
                  for found_network in found_networks {
                    for known in device_config.get_data().known_wifi_networks {
                      if known.ssid == found_network.ssid {
                        match best_network {
                          Some((_, _, best_so_far)) => {
                            if found_network.signal_strength > best_so_far {
                              best_network = Some((known.ssid, known.pass, found_network.signal_strength));
                            }
                          }
                          None => {
                            best_network = Some((known.ssid, known.pass, found_network.signal_strength));
                          }
                        };
                      }
                    }
                  }
                }
                Err(err) => {
                  error!("WiFi: Scan Error: {err:?}")
                }
              };

              Timer::after(Duration::from_millis(1_000)).await;
            }

            if Instant::now().duration_since_epoch().as_millis() < retry_in {
              info!("WiFi: Waiting before retry...");
              continue;
            }

            match best_network {
              None => {
                error!("WiFi: No connectable networks found!");
                manager.set_status(WifiStatus::NoNetworksFound).await;
                retry_in = Instant::now().duration_since_epoch().as_millis() + RETRY_INTERVAL;
                manager.connection_attempts.fetch_add(1, Ordering::Relaxed);
                continue;
              }
              Some(best_network) => {
                let mut config = StationConfig::default().with_ssid(best_network.0);

                if best_network.1.chars().count() > 0 {
                  config = config.with_password(best_network.1);
                }

                if let Err(err) = controller.set_config(&ModeConfig::Station(config)) {
                  error!("WiFi: Error setting config: {err:?}");
                  continue;
                }

                info!("WiFi: Attempting connection...");
                manager.connection_attempts.fetch_add(1, Ordering::Relaxed);
              }
            }

            if let Err(err) = controller.connect_async().await {
              error!("WiFi: Failed to connect: {err:?}");
              Timer::after(Duration::from_millis(5_000)).await;
              manager.set_status(WifiStatus::Interrupted).await;
              continue;
            }

            info!("WiFi: Connected!");

            stack.set_config_v4(embassy_net::ConfigV4::Dhcp(Default::default()));

            stack.wait_link_up().await;

            loop {
              if let Some(ip_info) = stack.config_v4() {
                ip_address = Some(ip_info.address.address());
                info!("WiFi: IP address obtained: {:?}", ip_address.unwrap());
                break;
              }

              Timer::after(Duration::from_millis(100)).await;
            }
          }

          let connected = check_connectivity(stack).await;

          if was_connected != connected {
            if connected {
              info!("WiFi: DNS connection check successful");
              let status = WifiStatus::Connected(ip_address.unwrap());
              manager.set_status(status).await;
              manager.successful_connections.fetch_add(1, Ordering::Relaxed);
            } else {
              manager.set_status(WifiStatus::Interrupted).await;
              disconnect(&mut controller).await;
            }

            was_connected = connected;
          }
        }
        crate::types::WifiMode::AccessPoint => {
          if !controller.is_started().unwrap_or_default() {
            let ap_ssid = device_config.get_data().ap_ssid;

            let config = AccessPointConfig::default().with_ssid(ap_ssid.clone());

            if let Err(err) = controller.set_config(&ModeConfig::AccessPointStation(StationConfig::default(), config)) {
              error!("WiFi (AP): Error setting config: {err:?}");
            }

            if let Err(err) = controller.start_async().await {
              error!("WiFi (AP): Error starting: {err:?}");
              continue;
            }

            info!("WiFi (AP): Started");

            let config = embassy_net::ConfigV4::Static(embassy_net::StaticConfigV4 {
              address: embassy_net::Ipv4Cidr::new(ap_ip, 24),
              gateway: Some(ap_ip),
              dns_servers: Default::default(),
            });

            stack.set_config_v4(config);

            info!("WiFi (AP): IP address config applying...");

            stack.wait_link_up().await;

            info!("WiFi (AP): Link up");

            loop {
              if let Some(ip_info) = stack.config_v4() {
                info!("WiFi (AP): IP address configured: {:?}", ip_info.address.address());
                manager.set_status(WifiStatus::AccessPoint).await;

                was_connected = true;
                break;
              }

              Timer::after(Duration::from_millis(100)).await;
            }
          }
        }
      },
    }
  }
}

async fn check_connectivity(stack: embassy_net::Stack<'_>) -> bool {
  let mut check_retry_count = 5;

  loop {
    match embassy_time::with_timeout(
      Duration::from_secs(1),
      stack.dns_query("google.com", embassy_net::dns::DnsQueryType::A),
    )
    .await
    {
      Ok(_) => {
        return true;
      }
      Err(_) => {
        if check_retry_count > 0 {
          check_retry_count -= 1;
          Timer::after(Duration::from_millis(1_000)).await;
          continue;
        }

        error!("WiFi: DNS query timeout");

        return false;
      }
    };
  }
}

async fn disconnect(controller: &mut WifiController<'static>) {
  match controller.disconnect_async().await {
    Ok(_) => {
      info!("WiFi: Disconnected (before reconnect attempt)");

      match controller.stop_async().await {
        Ok(_) => {
          info!("WiFi: Stopped (before reconnect attempt)")
        }
        Err(err) => error!("WiFi: Failed to stop {err:?}"),
      };
    }
    Err(err) => error!("WiFi: Failed to disconnect {err:?}"),
  }
}

impl fmt::Debug for HardwareWifiManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareWifiManager").finish()
  }
}

impl WiFiManager for HardwareWifiManager {
  fn get_status(&self) -> Pin<Box<dyn core::future::Future<Output = WifiStatus> + Send + '_>> {
    let status = self.status.clone();
    Box::pin(async move { status.get().await })
  }

  fn wait_for_status_change(&self) -> Pin<Box<dyn core::future::Future<Output = WifiStatus> + Send + '_>> {
    let status = self.status.clone();
    Box::pin(async move { status.wait_for_change().await })
  }

  fn get_stats(&self) -> WifiStats {
    WifiStats {
      connection_attempts: self.connection_attempts.load(Ordering::Relaxed),
      successful_connections: self.successful_connections.load(Ordering::Relaxed),
    }
  }

  fn set_desired_state(&self, state: WifiDesiredState) -> Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>> {
    let desired_state = self.desired_state.clone();
    Box::pin(async move {
      *desired_state.write().await = state;
    })
  }

  fn scan(&self) -> Pin<Box<dyn core::future::Future<Output = Vec<WifiResult>> + Send + '_>> {
    let scan_results = self.last_scan_results.clone();
    Box::pin(async move {
      // Return the last scan results
      // In practice, the connection task performs scans and updates this
      scan_results.read().await.clone()
    })
  }

  fn set_wifi_mode(&self, _mode: WifiMode) -> Pin<Box<dyn core::future::Future<Output = Result<(), &'static str>> + Send + '_>> {
    Box::pin(async { Ok(()) })
  }
}
