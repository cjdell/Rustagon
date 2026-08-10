use alloc::format;
use core::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use edge_dhcp::{
  io::{self, DEFAULT_SERVER_PORT},
  server::ServerOptions,
};
use edge_nal::UdpBind;
use edge_nal_embassy::{Udp, UdpBuffers};
use embassy_net::{ConfigV4, Ipv4Cidr, Runner, Stack, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_radio::wifi::Interface;
use log::info;

// A background task, to process network events - when new packets, they need to processed, embassy-net, wraps smoltcp
#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
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
    .bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DEFAULT_SERVER_PORT)))
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
      log::warn!("DHCP: Server error: {err:?}");
    }

    Timer::after(Duration::from_millis(500)).await;
    println!("DHCP offered");
  }
}
