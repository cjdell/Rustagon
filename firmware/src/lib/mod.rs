mod device;
mod i2c;
mod protocol;
mod types;

pub use device::DeviceConfigurator;
pub use i2c::{I2C_0, I2C_1, I2C_2, init_gpio, reset_device, scan_devices};
pub use protocol::{HostIpcMessage, WasmIpcMessage};
pub use types::*;

pub const FIRMWARE_VERSION: &str = env!("FIRMWARE_VERSION");

macro_rules! timeout {
  ($future:expr, $duration:expr, $prefix:literal) => {
    match embassy_futures::select::select(
      $future,
      embassy_time::Timer::after(embassy_time::Duration::from_millis($duration)),
    )
    .await
    {
      embassy_futures::select::Either::First(res) => res.map_err(|err| anyhow::anyhow!("{} Error: {err:?}", $prefix)),
      embassy_futures::select::Either::Second(()) => Err(anyhow::anyhow!("{} Error: Timed out", $prefix)),
    }
  };
}

macro_rules! timeout_result {
  ($future:expr, $duration:expr, $prefix:literal) => {
    timeout!(async { Ok::<_, anyhow::Error>($future.await) }, $duration, $prefix)
  };
}
