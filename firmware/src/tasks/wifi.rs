use crate::{
  lib::{DeviceConfig, WifiMode, WifiResult},
  utils::state::PersistentStateService,
};
use alloc::{format, string::String, vec::Vec};
use core::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use edge_dhcp::{
  io::{self, DEFAULT_SERVER_PORT},
  server::ServerOptions,
};
use edge_nal::UdpBind;
use edge_nal_embassy::{Udp, UdpBuffers};
use embassy_net::{ConfigV4, Ipv4Cidr, Runner, Stack, StaticConfigV4};
use embassy_sync::{
  blocking_mutex::raw::CriticalSectionRawMutex,
  channel::{Channel, Receiver, Sender},
  watch::{self},
};
use embassy_time::{Duration, Timer};
use esp_hal::time::Instant;
use esp_println::{print, println};
use esp_radio::wifi::{
  AuthenticationMethod, ModeConfig, WifiController, WifiDevice,
  ap::AccessPointConfig,
  scan::{ScanConfig, ScanTypeConfig},
  sta::StationConfig,
};
use log::{error, info, warn};
use smoltcp::wire::DnsQueryType;

#[derive(Debug)]
pub enum WifiCommandMessage {
  ChangeState(WifiDesiredState),
  Scan,
  OverrideConnect(String, String),
}

#[derive(Debug)]
pub enum WifiStatusMessage {
  Connected(Ipv4Addr),
  AccessPointActive,
  NoNetworksFound,
  Interrupted,
  Disconnected,
  Reset,
}

#[derive(Debug)]
pub enum WifiDesiredState {
  Online,
  Offline,
}

pub type WifiCommandChannel = Channel<CriticalSectionRawMutex, WifiCommandMessage, 10>;
pub type WifiCommandSender = Sender<'static, CriticalSectionRawMutex, WifiCommandMessage, 10>;
pub type WifiCommandReceiver = Receiver<'static, CriticalSectionRawMutex, WifiCommandMessage, 10>;

pub type WifiStatusChannel = Channel<CriticalSectionRawMutex, WifiStatusMessage, 10>;
pub type WifiStatusSender = Sender<'static, CriticalSectionRawMutex, WifiStatusMessage, 10>;
pub type WifiStatusReceiver = Receiver<'static, CriticalSectionRawMutex, WifiStatusMessage, 10>;

pub type ScanWatch = watch::Watch<CriticalSectionRawMutex, Vec<WifiResult>, 2>;
pub type ScanSender = watch::Sender<'static, CriticalSectionRawMutex, Vec<WifiResult>, 2>;
pub type ScanReceiver = watch::Receiver<'static, CriticalSectionRawMutex, Vec<WifiResult>, 2>;

const RETRY_INTERVAL: u64 = 60_000;

// Maintains wifi connection, when it disconnects it tries to reconnect
#[embassy_executor::task]
pub async fn connection_task(
  device_config: PersistentStateService<DeviceConfig>,
  mut controller: WifiController<'static>,
  stack: Stack<'static>,
  ap_ip: Ipv4Addr,
  command_receiver: WifiCommandReceiver,
  status_sender: WifiStatusSender,
  scan_signal: ScanSender,
) {
  info!("Wifi: Task started");

  let mut desired_state = WifiDesiredState::Offline;
  let mut was_connected = false;
  let mut retry_in: u64 = 0;

  let mut wifi_mode = device_config.get_data().wifi_mode;

  if device_config.get_data().known_wifi_networks.len() == 0 {
    wifi_mode = WifiMode::AccessPoint;
  }

  loop {
    Timer::after(Duration::from_millis(1_000)).await;

    if let Ok(command) = command_receiver.try_receive() {
      match command {
        WifiCommandMessage::Scan => {
          // https://github.com/esp-rs/esp-hal/issues/4511
          if let Ok(results) = controller
            .scan_with_config_async(
              ScanConfig::default()
                .with_max(20)
                .with_scan_type(ScanTypeConfig::Passive(esp_hal::time::Duration::from_millis(250))),
            )
            .await
          {
            scan_signal.send(
              results
                .iter()
                .map(|r| WifiResult {
                  ssid: r.ssid.clone(),
                  signal_strength: r.signal_strength,
                  password_required: match r.auth_method {
                    Some(auth) => match auth {
                      AuthenticationMethod::None => false,
                      _ => true,
                    },
                    None => false,
                  },
                })
                .collect(),
            );
          }
        }
        WifiCommandMessage::ChangeState(state) => {
          println!("WifiCommandMessage::ChangeState: {state:?}");
          desired_state = state;
        }
        WifiCommandMessage::OverrideConnect(_ssid, _pass) => {
          // TODO
        }
      };
    }

    match desired_state {
      WifiDesiredState::Offline => {
        if controller.is_connected().unwrap_or_default() {
          match controller.disconnect_async().await {
            Ok(_) => {
              was_connected = false;

              println!("Wifi: Disconnected (planned)");
              status_sender.send(WifiStatusMessage::Disconnected).await;
            }
            Err(err) => {
              error!("Wifi: Failed to disconnect {err:?}");
              Timer::after(Duration::from_millis(5000)).await
            }
          }
        }

        if controller.is_started().unwrap_or_default() {
          match controller.stop_async().await {
            Ok(_) => {
              println!("Wifi: Stopped (planned)");
              status_sender.send(WifiStatusMessage::Disconnected).await;
            }
            Err(err) => error!("Wifi: Failed to stop {err:?}"),
          };
        }

        stack.set_config_v4(ConfigV4::None);

        loop {
          if !stack.is_config_up() {
            break;
          }

          print!("d");
          Timer::after(Duration::from_millis(100)).await;
        }
      }
      WifiDesiredState::Online => match wifi_mode {
        WifiMode::Station => {
          let mut ip_address = None;

          if !controller.is_connected().unwrap_or_default() {
            if was_connected {
              was_connected = false;
              println!("Wifi: Interrupted!");
              status_sender.send(WifiStatusMessage::Interrupted).await;
            }

            if !controller.is_started().unwrap_or_default() {
              let config = ModeConfig::Station(StationConfig::default());

              if let Err(err) = controller.set_config(&config) {
                error!("Wifi: Error setting config: {err:?}");
                continue;
              }

              if let Err(err) = controller.start_async().await {
                error!("Wifi: Error starting: {err:?}");
                continue;
              }
            }

            let mut best_network: Option<(String, String, i8)> = None;

            for _ in 0..3 {
              match controller.scan_with_config_async(ScanConfig::default()).await {
                Ok(found_networks) => {
                  println!("================");
                  for found_network in found_networks {
                    println!("{} [{}]", found_network.ssid, found_network.signal_strength);

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
                  error!("Scan Error: {err:?}")
                }
              };

              Timer::after(Duration::from_millis(1_000)).await;
            }

            if Instant::now().duration_since_epoch().as_millis() < retry_in {
              println!("Waiting...");
              continue;
            }

            match best_network {
              None => {
                println!("Wifi: No connectable networks found!");
                status_sender.send(WifiStatusMessage::NoNetworksFound).await;
                retry_in = Instant::now().duration_since_epoch().as_millis() + RETRY_INTERVAL;
                continue;
              }
              Some(best_network) => {
                let mut config = StationConfig::default().with_ssid(best_network.0);

                if best_network.1.chars().count() > 0 {
                  config = config.with_password(best_network.1);
                }

                if let Err(err) = controller.set_config(&ModeConfig::Station(config)) {
                  error!("Wifi: Error setting config: {err:?}");
                  continue;
                }

                println!("Wifi: Started");
              }
            }

            println!("Wifi: About to connect...");

            if let Err(err) = controller.connect_async().await {
              println!("Wifi: Failed to connect: {err:?}");
              Timer::after(Duration::from_millis(5_000)).await;
              status_sender.send(WifiStatusMessage::Reset).await;
              continue;
            }

            println!("Wifi: Connected!");

            stack.set_config_v4(ConfigV4::Dhcp(Default::default()));

            stack.wait_link_up().await;

            loop {
              if let Some(ip_info) = stack.config_v4() {
                ip_address = Some(ip_info.address.address());
                info!("Wifi: IP address obtained: {:?}", ip_address.unwrap());
                break;
              }

              print!(".");
              Timer::after(Duration::from_millis(100)).await;
            }
          }

          let connected = check_connectivity(stack).await;

          if was_connected != connected {
            if connected {
              info!("Wifi: DNS connection check successful");
              status_sender.send(WifiStatusMessage::Connected(ip_address.unwrap())).await;
            } else {
              status_sender.send(WifiStatusMessage::Interrupted).await;
              disconnect(&mut controller).await;
            }

            was_connected = connected;
          }
        }
        WifiMode::AccessPoint => {
          if !controller.is_started().unwrap_or_default() {
            let ap_ssid = device_config.get_data().ap_ssid;
            // let ap_pass = device_config.get_data().ap_pass;

            let config = AccessPointConfig::default().with_ssid(ap_ssid.clone());

            // https://github.com/esp-rs/esp-hal/issues/4676
            // if ap_pass.chars().count() > 0 {
            //   config = config.with_password(ap_pass.clone());
            // }

            if let Err(err) = controller.set_config(&ModeConfig::AccessPointStation(StationConfig::default(), config)) {
              error!("Wifi (AP): Error setting config: {err:?}");
            }

            if let Err(err) = controller.start_async().await {
              error!("Wifi (AP): Error starting: {err:?}");
              continue;
            }

            info!("Wifi (AP): Started");

            let config = ConfigV4::Static(StaticConfigV4 {
              address: Ipv4Cidr::new(ap_ip, 24),
              gateway: Some(ap_ip),
              dns_servers: Default::default(),
            });

            stack.set_config_v4(config);

            info!("Wifi (AP): IP address config applying...");

            stack.wait_link_up().await;

            info!("Wifi (AP): Link up");

            loop {
              if let Some(ip_info) = stack.config_v4() {
                info!("Wifi (AP): IP address configured: {:?}", ip_info.address.address());
                status_sender.send(WifiStatusMessage::AccessPointActive).await;

                was_connected = true;
                break;
              }

              print!(".");
              Timer::after(Duration::from_millis(100)).await;
            }
          }
        }
      },
    }
  }
}

async fn check_connectivity(stack: Stack<'_>) -> bool {
  let mut check_retry_count = 5;

  loop {
    match timeout!(stack.dns_query("google.com", DnsQueryType::A), 1_000, "DNS") {
      Ok(_) => {
        return true;
      }
      Err(ref err) => {
        if check_retry_count > 0 {
          check_retry_count -= 1;
          Timer::after(Duration::from_millis(1_000)).await;
          continue;
        }

        error!("Wifi: DNS query error: {err:?}");

        return false;
      }
    };
  }
}

async fn disconnect(controller: &mut WifiController<'static>) {
  match controller.disconnect_async().await {
    Ok(_) => {
      println!("Wifi: Disconnected (before reconnect attempt)");

      match controller.stop_async().await {
        Ok(_) => {
          println!("Wifi: Stopped (before reconnect attempt)")
        }
        Err(err) => error!("Wifi: Failed to stop {err:?}"),
      };
    }
    Err(err) => error!("Wifi: Failed to disconnect {err:?}"),
  }
}

// A background task, to process network events - when new packets, they need to processed, embassy-net, wraps smoltcp
#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
  runner.run().await
}

#[embassy_executor::task]
pub async fn captive_task(stack: Stack<'static>, ap_ip: Ipv4Addr) {
  info!("Captive: Task started");

  loop {
    let udp_buffers: edge_nal_embassy::UdpBuffers<5, 1024, 1024, 5> = edge_nal_embassy::UdpBuffers::new();

    let udp = edge_nal_embassy::Udp::new(stack, &udp_buffers);

    let mut tx_buf = [0; 1500];
    let mut rx_buf = [0; 1500];

    edge_captive::io::run(
      &udp,
      SocketAddr::new(core::net::IpAddr::V4(Ipv4Addr::UNSPECIFIED), 53),
      &mut tx_buf,
      &mut rx_buf,
      ap_ip,
      core::time::Duration::from_secs(60),
    )
    .await
    .unwrap();

    info!("Captive: Stopped");
  }
}

#[embassy_executor::task]
pub async fn dhcp_task(stack: Stack<'static>, ap_ip: Ipv4Addr) {
  info!("DHCP: Task started");

  let mut buf = [0u8; 1500];

  let mut gw_buf = [Ipv4Addr::UNSPECIFIED];
  let dns = [ap_ip];

  let buffers = UdpBuffers::<3, 1024, 1024, 10>::new();
  let unbound_socket = Udp::new(stack, &buffers);
  let mut bound_socket = unbound_socket
    .bind(SocketAddr::V4(SocketAddrV4::new(
      Ipv4Addr::UNSPECIFIED,
      DEFAULT_SERVER_PORT,
    )))
    .await
    .unwrap();

  loop {
    let captive_url = format!("http://{ap_ip}/");

    let mut options = ServerOptions::new(ap_ip, Some(&mut gw_buf));
    options.dns = &dns;
    options.captive_url = Some(&captive_url);

    if let Err(err) = io::server::run(
      &mut edge_dhcp::server::Server::<_, 64>::new_with_et(ap_ip),
      &options,
      &mut bound_socket,
      &mut buf,
    )
    .await
    {
      warn!("DHCP: Server error: {err:?}");
    }

    Timer::after(Duration::from_millis(500)).await;
    println!("DHCP offered");
  }
}
