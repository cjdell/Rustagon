use aw9523b::Aw9523b;
use core::convert::Infallible;
use embedded_hal::digital::OutputPin;

pub struct DummyOutput;

impl embedded_hal::digital::ErrorType for DummyOutput {
  type Error = Infallible;
}

impl OutputPin for DummyOutput {
  fn set_low(&mut self) -> Result<(), Self::Error> {
    Ok(())
  }

  fn set_high(&mut self) -> Result<(), Self::Error> {
    Ok(())
  }
}

pub struct Aw9523bGpioPin<I2C> {
  pin: aw9523b::Pin,
  aw9523b: Aw9523b<I2C>,
}

impl<I2C, E> Aw9523bGpioPin<I2C>
where
  I2C: embedded_hal::i2c::I2c<Error = E>,
{
  /// Create new instance of the device
  pub fn new(i2c: I2C, addr: u8, pin: aw9523b::Pin) -> Self {
    let mut aw9523b = Aw9523b::new(i2c, addr);

    aw9523b.pin_gpio_mode(pin).ok();
    aw9523b.set_io_direction(pin, aw9523b::Dir::OUTPUT).ok();

    Self { aw9523b, pin }
  }
}

#[derive(Debug)]
pub enum Aw9523bGpioPinError {
  Unknown,
}

impl embedded_hal::digital::Error for Aw9523bGpioPinError {
  fn kind(&self) -> embedded_hal::digital::ErrorKind {
    embedded_hal::digital::ErrorKind::Other
  }
}

impl<I2C, E> embedded_hal::digital::ErrorType for Aw9523bGpioPin<I2C>
where
  I2C: embedded_hal::i2c::I2c<Error = E>,
{
  type Error = Aw9523bGpioPinError;
}

impl<I2C, E> OutputPin for Aw9523bGpioPin<I2C>
where
  I2C: embedded_hal::i2c::I2c<Error = E>,
{
  fn set_low(&mut self) -> Result<(), Self::Error> {
    self.aw9523b.set_pin_low(self.pin).map_err(|_err| Aw9523bGpioPinError::Unknown)
  }

  fn set_high(&mut self) -> Result<(), Self::Error> {
    self.aw9523b.set_pin_high(self.pin).map_err(|_err| Aw9523bGpioPinError::Unknown)
  }
}
