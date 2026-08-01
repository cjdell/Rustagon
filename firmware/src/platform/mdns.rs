//! mDNS responder — advertises the device as `<device_name>.local`

use alloc::string::String;
use core::net::{Ipv4Addr, Ipv6Addr};
use edge_mdns::{buf::VecBufAccess, domain::base::Ttl, io::MdnsIoError, HostAnswersMdnsHandler};
use edge_nal::UdpSplit as _;
use edge_nal_embassy::UdpError;
use embassy_net::Stack;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use log::{info, warn};

/// A background task that responds to mDNS queries for `<device_name>.local`,
/// pointing at `our_ip`. When the WiFi link drops the responder errors out and
/// the task exits, so it can be re-spawned on the next connection.
#[embassy_executor::task]
pub async fn mdns_task(stack: Stack<'static>, device_name: String, our_ip: Ipv4Addr) {
  Timer::after(Duration::from_millis(1_000)).await;

  let hostname = sanitize_hostname(&device_name);

  info!("mDNS: Responding as {hostname}.local at {our_ip}");

  if let Err(err) = mdns_runner(stack, &hostname, our_ip).await {
    warn!("mDNS: Responder stopped with error: {err:?}");
  }
}

/// Run the mDNS responder for `hostname.local` until the link drops or an I/O
/// error occurs.
pub async fn mdns_runner(stack: Stack<'static>, hostname: &str, our_ip: Ipv4Addr) -> Result<(), MdnsIoError<UdpError>> {
  let udp_buffers: edge_nal_embassy::UdpBuffers<5, 1024, 1024, 5> = edge_nal_embassy::UdpBuffers::new();
  let udp = edge_nal_embassy::Udp::new(stack, &udp_buffers);

  let (recv_buf, send_buf) = (VecBufAccess::<NoopRawMutex, 1500>::new(), VecBufAccess::<NoopRawMutex, 1500>::new());

  let mut socket = edge_mdns::io::bind(&udp, edge_mdns::io::IPV4_DEFAULT_SOCKET, Some(Ipv4Addr::UNSPECIFIED), None).await?;

  let (recv, send) = socket.split();

  let host = edge_mdns::host::Host {
    hostname,
    ipv4: our_ip,
    ipv6: Ipv6Addr::UNSPECIFIED,
    ttl: Ttl::from_secs(60),
  };

  // A way to notify the mDNS responder that the data in `Host` had changed.
  // We don't use it, because the data is fixed for the lifetime of the task.
  let signal = Signal::<NoopRawMutex, ()>::new();

  let mdns = edge_mdns::io::Mdns::new(
    Some(Ipv4Addr::UNSPECIFIED),
    None,
    recv,
    send,
    recv_buf,
    send_buf,
    esp_hal::rng::Rng::new(),
    &signal,
  );

  mdns.run(HostAnswersMdnsHandler::new(&host)).await
}

/// DNS-SD hostname labels are 1-63 octets of `[a-zA-Z0-9-]` and may not begin or
/// end with a hyphen. Normalise the configured device name so the advertised
/// `.local` hostname is always valid.
fn sanitize_hostname(name: &str) -> String {
  let mut out = String::new();
  let mut last = '\0';

  for c in name.chars().flat_map(|c| c.to_lowercase()) {
    let c = if c.is_ascii_alphanumeric() { c } else { '-' };

    if out.is_empty() && c == '-' {
      continue;
    }
    if c == '-' && last == '-' {
      continue;
    }
    if out.len() == 63 {
      break;
    }

    last = c;
    out.push(c);
  }

  while out.ends_with('-') {
    out.pop();
  }

  if out.is_empty() {
    out.push_str("rustagon");
  }

  out
}
