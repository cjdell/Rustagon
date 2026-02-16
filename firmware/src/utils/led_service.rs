use alloc::vec::Vec;
use esp_hal::{
  Async,
  gpio::Level,
  peripherals::{GPIO21, RMT},
  rmt::{Channel, PulseCode, Rmt, Tx, TxChannelConfig, TxChannelCreator as _},
  time::Rate,
};

// SK6812/WS2812 timing constants (all in nanoseconds)
const SK68XX_CODE_PERIOD: u32 = 1250; // 800kHz
const SK68XX_T0H_NS: u32 = 400;
const SK68XX_T0L_NS: u32 = SK68XX_CODE_PERIOD - SK68XX_T0H_NS;
const SK68XX_T1H_NS: u32 = 850;
const SK68XX_T1L_NS: u32 = SK68XX_CODE_PERIOD - SK68XX_T1H_NS;

const BITS_PER_LED: usize = 24; // 8 bits each for G, R, B

pub struct LedService<'a> {
  channel: Channel<'a, Async, Tx>,
  zero: PulseCode,
  one: PulseCode,
}

#[derive(Debug, Clone, Copy)]
pub struct LedState {
  pub r: u8,
  pub g: u8,
  pub b: u8,
}

impl LedService<'_> {
  pub fn new() -> Self {
    let rmt = Rmt::new(unsafe { RMT::steal() }, Rate::from_mhz(80)).unwrap().into_async();

    let config = TxChannelConfig::default()
      .with_clk_divider(1)
      .with_idle_output_level(Level::Low)
      .with_memsize(8)
      .with_carrier_modulation(false)
      .with_idle_output(false);

    let channel = rmt.channel0.configure_tx(&config).unwrap().with_pin(unsafe { GPIO21::steal() });

    let clocks = esp_hal::clock::Clocks::get();
    let src_clock_mhz = clocks.apb_clock.as_mhz();

    let zero = Self::create_pulse_code(SK68XX_T0H_NS, SK68XX_T0L_NS, src_clock_mhz);
    let one = Self::create_pulse_code(SK68XX_T1H_NS, SK68XX_T1L_NS, src_clock_mhz);

    Self { channel, zero, one }
  }

  pub async fn send(&mut self, leds: &[LedState]) {
    let mut data = Vec::with_capacity(leds.len() * BITS_PER_LED + 1);

    for led in leds {
      self.encode_led(&mut data, led);
    }

    data.push(PulseCode::end_marker());

    self.channel.transmit(&data).await.expect("RMT transmit failure");
  }

  fn create_pulse_code(high_ns: u32, low_ns: u32, src_clock_mhz: u32) -> PulseCode {
    PulseCode::new(
      Level::High,
      ((high_ns * src_clock_mhz) / 1000) as u16,
      Level::Low,
      ((low_ns * src_clock_mhz) / 1000) as u16,
    )
  }

  fn encode_led(&self, data: &mut Vec<PulseCode>, led: &LedState) {
    // SK6812/WS2812 expect GRB order
    self.encode_byte(data, led.g);
    self.encode_byte(data, led.r);
    self.encode_byte(data, led.b);
  }

  fn encode_byte(&self, data: &mut Vec<PulseCode>, byte: u8) {
    for i in 0..8 {
      let bit = (byte >> (7 - i)) & 1;
      data.push(if bit == 1 { self.one } else { self.zero });
    }
  }
}
