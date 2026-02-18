mod device;
mod i2c;
mod protocol;
mod types;

pub use device::DeviceConfigurator;
pub use i2c::{I2C_0, I2C_1, I2C_2, init_gpio, reset_device, scan_devices};
pub use protocol::{HostIpcMessage, WasmIpcMessage};
pub use types::{
  DeviceConfig, DeviceState, DisplayInterface, HexButton, HexButtonChannel, HexButtonReceiver, HexButtonSender,
  HostIpcChannel, HostIpcReceiver, HostIpcSender, HttpChannel, HttpReceiver, HttpSender, HttpStatusMessage, I2cMessage,
  I2cMutux, Icon20, Icon40, Image, KnownWifiNetwork, LcdScreen, LedChannel, LedReceiver, LedRequest, LedSender,
  MenuLine, NUM_LEDS, PowerCtrl, PowerCtrlChannel, PowerCtrlReceiver, PowerCtrlSender, SystemMessage, SystemReceiver,
  SystemSender, SystemWatch, WasmIpcChannel, WasmIpcReceiver, WasmIpcSender, WebSocketIncomingChannel,
  WebSocketIncomingMessage, WebSocketIncomingReceiver, WebSocketIncomingSender, WifiMode, WifiResult,
};

pub const FIRMWARE_VERSION: &str = env!("FIRMWARE_VERSION");

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
#[macro_export]
macro_rules! mk_static {
  ($t:ty,$val:expr) => {{
    static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
    #[deny(unused_attributes)]
    let x = STATIC_CELL.uninit().write(($val));
    x
  }};
}

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
