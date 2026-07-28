//! `embedded-hal` driver for the Infineon/Cypress **CY8CMBR3116** CapSense
//! Express controller (16 sensing inputs, 8 GPOs, I2C slave).
#![no_std]

use embedded_hal::i2c::I2c;

/// Default 7-bit I2C slave address.
pub const DEFAULT_ADDRESS: u8 = 0x37;

/// Number of sensing inputs (CS0..CS15).
pub const NUM_SENSORS: usize = 16;

/// Number of GPOs (GPO0..GPO7).
pub const NUM_GPOS: usize = 8;

/// Length of configuration register block (addresses 0x00..=0x7D, 126 bytes).
pub const CONFIG_LEN: usize = 126;

/// Errors returned by this driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<E> {
  I2c(E),
  Nack,
  CrcMismatch,
  ConfigSaveFailed,
  InvalidDevice,
}

impl<E> From<E> for Error<E> {
  fn from(e: E) -> Self {
    Error::I2c(e)
  }
}

/// Command opcodes for `CTRL_CMD` (0x86).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
  SaveCheckCrc = 2,
  CalcCrc = 3,
  Sleep = 7,
  ClearLatchedStatus = 8,
  ResetProxFilterPs0 = 9,
  ResetProxFilterPs1 = 10,
  Reset = 255,
}

/// Register addresses used by this driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Register {
  SensorEn = 0x00,
  ProxEn = 0x26,
  I2cAddr = 0x51,
  ConfigCrcLsb = 0x7E,
  GpoOutputState = 0x80,
  SensorId = 0x82,
  CtrlCmd = 0x86,
  CtrlCmdStatus = 0x88,
  CtrlCmdErr = 0x89,
  SystemStatus = 0x8A,
  FamilyId = 0x8F,
  DeviceId = 0x90,
  ButtonStat = 0xAA,
  LatchedButtonStat = 0xAC,
  ProxStat = 0xAE,
  LatchedProxStat = 0xAF,
  GpoData = 0xDA,
}

impl Register {
  fn addr(self) -> u8 {
    self as u8
  }
}

/// Current touch status of all 16 CapSense inputs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ButtonStatus(pub u16);

impl ButtonStatus {
  pub fn is_touched(&self, sensor: u8) -> bool {
    (self.0 >> sensor) & 1 != 0
  }

  pub fn touched(&self) -> impl Iterator<Item = u8> + '_ {
    (0..NUM_SENSORS as u8).filter(move |&n| self.is_touched(n))
  }
}

/// Edge event detected for a single sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEvent {
  None,
  Rising,
  Falling,
}

/// Result of a poll cycle: one `TouchEvent` per sensor (CS0..CS15).
#[derive(Debug, Clone)]
pub struct TouchState {
  pub events: [TouchEvent; NUM_SENSORS],
  pub buttons: ButtonStatus,
}

/// CRC-16-CCITT (poly 0x1021, init 0xFFFF) over configuration data.
pub fn crc16_ccitt(data: &[u8]) -> [u8; 2] {
  let mut crc: u16 = 0xFFFF;
  for &byte in data {
    crc ^= (byte as u16) << 8;
    for _ in 0..8 {
      if crc & 0x8000 != 0 {
        crc = (crc << 1) ^ 0x1021;
      } else {
        crc <<= 1;
      }
    }
  }
  [crc as u8, (crc >> 8) as u8]
}

/// Default configuration for 12 touch buttons + 2 proximity sensors.
///
/// Values are from the badge-2024-software reference configuration
/// (registers 0x00-0x7D, 126 bytes).
pub const DEFAULT_CONFIG: [u8; CONFIG_LEN] = [
  // 0x00 – SENSOR_EN LSB (CS0-CS7)
  0xFF,
  // 0x01 – SENSOR_EN MSB (CS8-CS15)
  0xFF,
  // 0x02-0x05 FSS_EN / TOGGLE_EN
  0x00, 0x00, 0x00, 0x00,
  // 0x06-0x07 LED_ON_EN
  0x00, 0x00,
  // 0x08 – SENSITIVITY0
  0x40,
  // 0x09 – SENSITIVITY1
  0xCF,
  // 0x0A – SENSITIVITY2
  0x3F,
  // 0x0B – SENSITIVITY3
  0x3C,
  // 0x0C-0x1B – FINGER_THRESHOLD0-15
  0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
  0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x7F,
  // 0x1C-0x25 – debounce / hysteresis / LBR / NNT / NT / unused
  0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  // 0x26 – PROX_EN
  0x03,
  // 0x27-0x3A – proximity config
  0x00, 0x06, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00,
  0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1E,
  0x1E, 0x00, 0x00, 0x1E, 0x1E,
  // 0x3B-0x3F – buzzer / GPO
  0x00, 0x00, 0x01, 0x01,
  // 0x40 – GPO_CFG
  0x00,
  // 0x41-0x48 – PWM_DUTYCYCLE_CFG0-7
  0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
  // 0x49-0x4C – SPO_CFG
  0x00, 0x00, 0x00, 0x00,
  // 0x4D – DEVICE_CFG0
  0x02,
  // 0x4E – DEVICE_CFG1
  0x01,
  // 0x4F – DEVICE_CFG2
  0x0A,
  // 0x50 – DEVICE_CFG3
  0x00,
  // 0x51 – I2C_ADDR
  0x37,
  // 0x52 – REFRESH_CTRL
  0x01,
  // 0x53-0x7D
  0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00,
];

/// Driver for the CY8CMBR3116 CapSense Express controller.
pub struct Cy8cmbr3116<I2C> {
  i2c: I2C,
  address: u8,
  prev_buttons: u16,
}

impl<I2C, E> Cy8cmbr3116<I2C>
where
  I2C: I2c<Error = E>,
{
  pub fn new(i2c: I2C) -> Self {
    Self { i2c, address: DEFAULT_ADDRESS, prev_buttons: 0 }
  }

  pub fn new_with_address(i2c: I2C, address: u8) -> Self {
    Self { i2c, address, prev_buttons: 0 }
  }

  pub fn release(self) -> I2C {
    self.i2c
  }

  // ---- Low-level register access -----------------------------------

  pub fn set_pointer(&mut self, reg: u8) -> Result<(), Error<E>> {
    self.i2c.write(self.address, &[reg]).map_err(Error::I2c)
  }

  pub fn read_register(&mut self, reg: u8, buf: &mut [u8]) -> Result<(), Error<E>> {
    self.i2c.write_read(self.address, &[reg], buf).map_err(Error::I2c)
  }

  pub fn read_u8(&mut self, reg: Register) -> Result<u8, Error<E>> {
    let mut buf = [0u8; 1];
    self.read_register(reg.addr(), &mut buf)?;
    Ok(buf[0])
  }

  pub fn read_u16(&mut self, reg: Register) -> Result<u16, Error<E>> {
    let mut buf = [0u8; 2];
    self.read_register(reg.addr(), &mut buf)?;
    Ok(u16::from_le_bytes(buf))
  }

  pub fn write_register(&mut self, reg: u8, data: &[u8]) -> Result<(), Error<E>> {
    let mut frame = [0u8; 129];
    frame[0] = reg;
    frame[1..=data.len()].copy_from_slice(data);
    self.i2c.write(self.address, &frame[..=data.len()]).map_err(Error::I2c)
  }

  pub fn write_u8(&mut self, reg: Register, value: u8) -> Result<(), Error<E>> {
    self.write_register(reg.addr(), &[value])
  }

  // ---- Status queries ------------------------------------------------

  pub fn button_status(&mut self) -> Result<ButtonStatus, Error<E>> {
    Ok(ButtonStatus(self.read_u16(Register::ButtonStat)?))
  }

  pub fn latched_button_status(&mut self) -> Result<ButtonStatus, Error<E>> {
    Ok(ButtonStatus(self.read_u16(Register::LatchedButtonStat)?))
  }

  pub fn probe(&mut self) -> Result<bool, Error<E>> {
    Ok(self.read_u8(Register::FamilyId)? == 154)
  }

  // ---- Commands -------------------------------------------------------

  pub fn command(&mut self, cmd: Command) -> Result<(), Error<E>> {
    self.write_u8(Register::CtrlCmd, cmd as u8)
  }

  pub fn clear_latched_status(&mut self) -> Result<(), Error<E>> {
    self.command(Command::ClearLatchedStatus)
  }

  pub fn software_reset(&mut self) -> Result<(), Error<E>> {
    self.command(Command::Reset)
  }

  // ---- Initialisation -------------------------------------------------

  /// Initialise the touch controller.
  ///
  /// 1. Probes the device (checks Family ID == 154).
  /// 2. Reads the stored config CRC.
  /// 3. If it differs from the CRC of `config`, writes the full 126-byte
  ///    configuration block + CRC, issues `SaveCheckCrc`, and resets.
  ///
  /// The caller is responsible for asserting the hardware reset pin (XRES or
  /// a GPIO connected to the reset line) before calling this method.  After
  /// a successful init the device is scanning and will respond to touch.
  ///
  /// `delay_ms` should sleep for the given number of milliseconds (platform
  /// specific – e.g. `embassy_time::Timer::after` on embassy).
  pub fn init(&mut self, mut delay_ms: impl FnMut(u64)) -> Result<(), Error<E>> {
    if !self.probe()? {
      return Err(Error::InvalidDevice);
    }

    // Read stored CRC from registers 0x7E-0x7F
    let mut stored_crc = [0u8; 2];
    self.read_register(Register::ConfigCrcLsb.addr(), &mut stored_crc)?;
    let calculated_crc = crc16_ccitt(&DEFAULT_CONFIG);

    if stored_crc == calculated_crc {
      return Ok(());
    }

    // Write configuration block (126 bytes) + CRC (2 bytes)
    let mut config_with_crc = [0u8; CONFIG_LEN + 2];
    config_with_crc[..CONFIG_LEN].copy_from_slice(&DEFAULT_CONFIG);
    config_with_crc[CONFIG_LEN..].copy_from_slice(&calculated_crc);
    self.write_register(0x00, &config_with_crc)?;

    // Save to NVM
    self.command(Command::SaveCheckCrc)?;
    delay_ms(500);

    // Check for save error
    if self.command_has_error()? {
      return Err(Error::ConfigSaveFailed);
    }

    // Reset to apply new config
    self.software_reset()?;
    delay_ms(50);

    // Verify device is back
    if !self.probe()? {
      return Err(Error::InvalidDevice);
    }

    Ok(())
  }

  /// Read whether the last command reported an error.
  pub fn command_has_error(&mut self) -> Result<bool, Error<E>> {
    Ok(self.read_u8(Register::CtrlCmdStatus)? & 0x01 != 0)
  }

  // ---- Polling / edge detection --------------------------------------

  /// Read the full 6-byte status block at 0xAA (button state, latch,
  /// proximity state, latch) in a single transaction, detect rising/falling
  /// edges since the last poll, and reset the latches.
  ///
  /// `TouchState.events[i]` is `Rising` or `Falling` for sensor `CSi` that
  /// changed since the previous call. `TouchState.buttons` is the *current*
  /// (unlatched) state.
  pub fn poll(&mut self) -> Result<TouchState, Error<E>> {
    // Read 6 bytes: button_stat[2], latched_button_stat[2],
    //               prox_stat[1], latched_prox_stat[1]
    let mut buf = [0u8; 6];
    self.read_register(Register::ButtonStat.addr(), &mut buf)?;

    let current = u16::from_le_bytes([buf[0], buf[1]]);
    let latched = u16::from_le_bytes([buf[2], buf[3]]);

    let mut events = [TouchEvent::None; NUM_SENSORS];

    for i in 0..NUM_SENSORS as u8 {
      let bit = 1u16 << i;
      let was_touched = self.prev_buttons & bit != 0;
      let now_touched = current & bit != 0;
      let was_latched = latched & bit != 0;

      if was_latched && !was_touched {
        events[i as usize] = if now_touched { TouchEvent::Rising } else { TouchEvent::Falling };
      } else if was_touched && !now_touched {
        events[i as usize] = TouchEvent::Falling;
      }
    }

    self.prev_buttons = current;

    // Clear latched status so new touches can be detected
    self.clear_latched_status()?;

    Ok(TouchState { events, buttons: ButtonStatus(current) })
  }

  /// Reset edge-detection state without reading the sensor (useful after a
  /// long gap in polling where we want to ignore stale latches).
  pub fn reset_state(&mut self) -> Result<(), Error<E>> {
    let current = self.button_status()?.0;
    self.prev_buttons = current;
    self.clear_latched_status()
  }
}
