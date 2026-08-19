use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
pub use app::platform::wifi::{WiFiHandle, WiFiManager, WifiStatus};
pub use app::types::{WifiDesiredState, WifiMode, WifiResult};
use core::fmt;
use core::net::Ipv4Addr;
use core::pin::Pin;
use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock, signal::Signal};
use embassy_time::{Duration, Timer};
use esp_hal::time::Instant;
use esp_radio::wifi::{
  ap::{AccessPointConfig, AccessPointInfo},
  scan::ScanConfig,
  sta::StationConfig,
  AuthenticationMethod, Config, WifiController,
};
use log::{error, info};

use crate::platform::ConfigHandle;
use app::utils::WatchedValue;

use crate::platform::mdns::mdns_task;

#[derive(Clone, Debug, Default)]
pub struct WifiStats {
  pub connection_attempts: u32,
  pub successful_connections: u32,
}

const RETRY_INTERVAL: u64 = 60_000;

/// How long `scan()` waits for the connection task to service an on-demand scan
/// before falling back to the last cached results.
const SCAN_TIMEOUT: Duration = Duration::from_secs(15);

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
  /// Signalled by `scan()` to ask the connection task (which owns the controller)
  /// to perform a fresh scan.
  scan_request: Arc<Signal<CriticalSectionRawMutex, ()>>,
  /// Signalled by the connection task once a requested scan has completed.
  scan_complete: Arc<Signal<CriticalSectionRawMutex, Vec<WifiResult>>>,
  connection_attempts: Arc<core::sync::atomic::AtomicU32>,
  successful_connections: Arc<core::sync::atomic::AtomicU32>,
}

impl Default for HardwareWifiManager {
  fn default() -> Self {
    Self::new()
  }
}

impl HardwareWifiManager {
  pub fn new() -> Self {
    Self {
      status: WatchedValue::new(WifiStatus::Offline),
      desired_state: Arc::new(RwLock::new(WifiDesiredState::Offline)),
      last_scan_results: Arc::new(RwLock::new(Vec::new())),
      scan_request: Arc::new(Signal::new()),
      scan_complete: Arc::new(Signal::new()),
      connection_attempts: Arc::new(core::sync::atomic::AtomicU32::new(0)),
      successful_connections: Arc::new(core::sync::atomic::AtomicU32::new(0)),
    }
  }

  /// Internal method to update status and notify all watchers
  /// Called by the connection task when status changes
  pub(crate) async fn set_status(&self, new_status: WifiStatus) {
    self.status.set(new_status).await;
  }

  /// Cache scan results so they can be served without hitting the radio again
  pub(crate) async fn store_scan_results(&self, results: Vec<WifiResult>) {
    *self.last_scan_results.write().await = results;
  }

  /// Spawn the background connection task
  /// This must be called once during initialization
  pub fn spawn_connection_task(
    &self,
    spawner: Spawner,
    device_config: ConfigHandle,
    controller: WifiController<'static>,
    stack: embassy_net::Stack<'static>,
    ap_ip: Ipv4Addr,
  ) {
    let manager = self.clone();
    spawner.spawn(wifi_connection_task(device_config, controller, stack, ap_ip, manager, spawner).expect("spawn wifi_connection_task"));
  }
}

#[embassy_executor::task]
pub async fn wifi_connection_task(
  device_config: ConfigHandle,
  mut controller: WifiController<'static>,
  stack: embassy_net::Stack<'static>,
  ap_ip: Ipv4Addr,
  manager: HardwareWifiManager,
  spawner: Spawner,
) {
  info!("WiFi: Connection task started");

  let mut was_connected = false;
  let mut retry_in: u64 = 0;
  let mut ap_started = false;

  let wifi_mode = device_config.get_data().await.wifi_mode;

  loop {
    Timer::after(Duration::from_millis(1_000)).await;

    // Serve any pending on-demand scan before doing connection work
    service_scan_request(&mut controller, &manager).await;

    let desired_state = manager.desired_state.read().await.clone();

    match desired_state {
      WifiDesiredState::Offline => {
        if controller.is_connected() {
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

        // esp-radio 0.18 has no `stop` API (the controller stays started and is
        // (re)started by `set_config` as needed); we only tear down the stack.
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

          if !controller.is_connected() {
            if was_connected {
              was_connected = false;
              info!("WiFi: Interrupted!");
              manager.set_status(WifiStatus::Interrupted).await;
            } else {
              // Not yet connected and not previously connected - we're connecting
              info!("WiFi: Attempting to connect...");
              manager.set_status(WifiStatus::Connecting).await;
            }

            // `set_config` starts the controller if it isn't already running.
            let config = Config::Station(StationConfig::default());

            if let Err(err) = controller.set_config(&config) {
              error!("WiFi: Error setting config: {err:?}");
              continue;
            }

            let mut best_network: Option<(String, String, i8)> = None;

            for _ in 0..3 {
              match controller.scan_async(&ScanConfig::default()).await {
                Ok(found_networks) => {
                  manager.store_scan_results(to_wifi_results(&found_networks)).await;

                  for found_network in found_networks {
                    for known in device_config.get_data().await.known_wifi_networks {
                      if known.ssid.as_str() == found_network.ssid.as_str() {
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

                if let Err(err) = controller.set_config(&Config::Station(config)) {
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

            // Advertise the device over mDNS once we have an address
            let device_name = device_config.get_data().await.device_name;
            if let Ok(token) = mdns_task(stack, device_name, ip_address.unwrap()) {
              spawner.spawn(token);
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
          if !ap_started {
            let ap_ssid = device_config.get_data().await.ap_ssid;

            let config = AccessPointConfig::default().with_ssid(ap_ssid.clone());

            if let Err(err) = controller.set_config(&Config::AccessPointStation(StationConfig::default(), config)) {
              error!("WiFi (AP): Error setting config: {err:?}");
            } else {
              // `set_config` starts the controller; remember that the AP is up so
              // we don't re-apply the config every loop iteration.
              ap_started = true;
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

              // Advertise the device over mDNS with the AP's static address
              let device_name = device_config.get_data().await.device_name;
              if let Ok(token) = mdns_task(stack, device_name, ap_ip) {
                spawner.spawn(token);
              }
            }
          }
        }
      },
    }
  }
}

/// Convert raw scan output into platform `WifiResult`s, strongest first and de-duplicated
/// by SSID (the same network is often seen on multiple channels/bands).
fn to_wifi_results(found_networks: &[AccessPointInfo]) -> Vec<WifiResult> {
  let mut results: Vec<WifiResult> = Vec::new();

  for found_network in found_networks {
    let ssid = found_network.ssid.as_str();

    if ssid.is_empty() {
      continue;
    }

    let password_required = !matches!(found_network.auth_method, None | Some(AuthenticationMethod::None));

    match results.iter_mut().find(|r| r.ssid == ssid) {
      Some(existing) => {
        if found_network.signal_strength > existing.signal_strength {
          existing.signal_strength = found_network.signal_strength;
          existing.password_required = password_required;
        }
      }
      None => results.push(WifiResult {
        ssid: String::from(ssid),
        signal_strength: found_network.signal_strength,
        password_required,
      }),
    }
  }

  results.sort_by_key(|r| core::cmp::Reverse(r.signal_strength));
  results
}

/// Service a pending `scan()` request from the application (e.g. the HTTP wifi scan
/// endpoint). The connection task owns the controller, so all scans must go through here.
async fn service_scan_request(controller: &mut WifiController<'static>, manager: &HardwareWifiManager) {
  if !manager.scan_request.signaled() {
    return;
  }

  manager.scan_request.reset();

  info!("WiFi: Servicing on-demand scan request");

  let results = match controller.scan_async(&ScanConfig::default()).await {
    Ok(found_networks) => to_wifi_results(&found_networks),
    Err(err) => {
      error!("WiFi: Scan Error: {err:?}");
      manager.last_scan_results.read().await.clone()
    }
  };

  manager.store_scan_results(results.clone()).await;
  manager.scan_complete.signal(results);
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

  fn set_desired_state(&self, state: WifiDesiredState) -> Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>> {
    let desired_state = self.desired_state.clone();
    Box::pin(async move {
      *desired_state.write().await = state;
    })
  }

  fn scan(&self) -> Pin<Box<dyn core::future::Future<Output = Result<Vec<WifiResult>, ()>> + Send + '_>> {
    let scan_results = self.last_scan_results.clone();
    let scan_request = self.scan_request.clone();
    let scan_complete = self.scan_complete.clone();

    Box::pin(async move {
      scan_complete.reset();
      scan_request.signal(());

      match embassy_time::with_timeout(SCAN_TIMEOUT, scan_complete.wait()).await {
        Ok(results) => Ok(results),
        Err(_) => {
          error!("WiFi: Scan request timed out, returning cached results");
          Ok(scan_results.read().await.clone())
        }
      }
    })
  }
}
