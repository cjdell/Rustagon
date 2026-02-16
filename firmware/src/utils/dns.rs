use core::net::{IpAddr, Ipv4Addr};
use embassy_net::{IpAddress, Stack};
use embedded_nal_async::Dns;
use esp_println::println;
use smoltcp::wire::DnsQueryType;

pub struct DnsResolver {
  pub stack: Stack<'static>,
}

impl DnsResolver {
  pub fn new(stack: Stack<'static>) -> Self {
    Self { stack }
  }
}

impl Dns for DnsResolver {
  type Error = usize;

  async fn get_host_by_name(
    &self,
    host: &str,
    _addr_type: embedded_nal_async::AddrType,
  ) -> Result<core::net::IpAddr, usize> {
    println!("get_host_by_name: {}", host);

    if let Ok(ip) = Ipv4Addr::parse_ascii(host.as_bytes()) {
      return Ok(IpAddr::V4(ip));
    }

    if let Ok(ip) = self.stack.dns_query(host, DnsQueryType::A).await {
      let IpAddress::Ipv4(addr) = ip[0];

      return Ok(IpAddr::V4(addr));
    }

    Err(1)
  }

  async fn get_host_by_address(&self, addr: core::net::IpAddr, _result: &mut [u8]) -> Result<usize, usize> {
    println!("get_host_by_address: {}", addr);

    // Not needed
    todo!()
  }
}
