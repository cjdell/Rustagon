use crate::lib::{SystemMessage, SystemSender};
use esp_hal::{
  gpio::{Input, InputConfig, Pull},
  peripherals::GPIO0,
};
use log::info;

#[embassy_executor::task]
pub async fn system_task(pin: GPIO0<'static>, system_sender: SystemSender) {
  let mut input = Input::new(pin, InputConfig::default().with_pull(Pull::Up));

  loop {
    input.wait_for_falling_edge().await;
    info!("Boot pin pressed!");
    system_sender.send(SystemMessage::BootButton);
  }
}
