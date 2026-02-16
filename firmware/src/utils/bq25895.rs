use alloc::vec;
use alloc::vec::Vec;
use embedded_hal::i2c;

const BQ25895_ADDR: u8 = 0x6A;

#[derive(Debug, Clone, Copy)]
struct BqRegister {
  addr: u8,
  mask: u8,
  pos: u8,
}

#[derive(Debug, Clone, Copy)]
struct BqScaledRegister {
  addr: u8,
  mask: u8,
  pos: u8,
  scaling: f32,
  offset: f32,
}

// === REG00: INPUT CURRENT LIMIT & HIZ ===
const INPUT_ILIM: BqScaledRegister = BqScaledRegister {
  addr: 0x00,
  mask: 0b00111111,
  pos: 0,
  scaling: 50.0,
  offset: 100.0,
};

const EN_ILIM_PIN: BqRegister = BqRegister {
  addr: 0x00,
  mask: 0x40, // bit 6
  pos: 6,
};

const EN_HIZ: BqRegister = BqRegister {
  addr: 0x00,
  mask: 0x80, // bit 7
  pos: 7,
};

// === REG03: BOOST & CHARGE CONFIG ===
const SYS_MIN: BqScaledRegister = BqScaledRegister {
  addr: 0x03,
  mask: 0b00001110,
  pos: 1,
  scaling: 0.1,
  offset: 3.0,
};

const CHG_CONFIG: BqRegister = BqRegister {
  addr: 0x03,
  mask: 0b00010000, // bit 4
  pos: 4,
};

const OTG_CONFIG: BqRegister = BqRegister {
  addr: 0x03,
  mask: 0b00100000, // bit 5
  pos: 5,
};

const BAT_LOADEN: BqRegister = BqRegister {
  addr: 0x03,
  mask: 0b10000000, // bit 7
  pos: 7,
};

// === REG04: FAST CHARGE CURRENT ===
const ICHG: BqScaledRegister = BqScaledRegister {
  addr: 0x04,
  mask: 0b01111111,
  pos: 0,
  scaling: 64.0,
  offset: 0.0,
};

// === REG05: PRECHARGE & TERMINATION CURRENT ===
const IPRECHG: BqScaledRegister = BqScaledRegister {
  addr: 0x05,
  mask: 0b11110000,
  pos: 4,
  scaling: 64.0,
  offset: 64.0,
};

const ITERM: BqScaledRegister = BqScaledRegister {
  addr: 0x05,
  mask: 0b00001111,
  pos: 0,
  scaling: 64.0,
  offset: 64.0,
};

// === REG06: CHARGE VOLTAGE & RECHARGE THRESHOLD ===
const VREG: BqScaledRegister = BqScaledRegister {
  addr: 0x06,
  mask: 0b11111100,
  pos: 2,
  scaling: 16.0,
  offset: 3840.0,
};

const BATLOWV: BqRegister = BqRegister {
  addr: 0x06,
  mask: 0x02, // bit 1
  pos: 1,
};

const VRECHG: BqRegister = BqRegister {
  addr: 0x06,
  mask: 0x01, // bit 0
  pos: 0,
};

// === REG07: WATCHDOG & TIMER ===
const WATCHDOG: BqScaledRegister = BqScaledRegister {
  addr: 0x07,
  mask: 0b00110000, // bits 5-4 (2 bits)
  pos: 4,
  scaling: 40.0,
  offset: 0.0,
};

const EN_TERM: BqRegister = BqRegister {
  addr: 0x07,
  mask: 0x80, // bit 7
  pos: 7,
};

const CHG_TIMER: BqScaledRegister = BqScaledRegister {
  addr: 0x07,
  mask: 0b00000110,
  pos: 1,
  scaling: 3.0,
  offset: 5.0, // 5h, 8h, 12h, 20h
};

// === REG09: BOOST VOLTAGE ===
const BATFET_DIS: BqRegister = BqRegister {
  addr: 0x09,
  mask: 0b00100000, // bit 5
  pos: 5,
};

// === REG0A: BOOST VOLTAGE ===
const BOOSTV: BqScaledRegister = BqScaledRegister {
  addr: 0x0A,
  mask: 0b11110000,
  pos: 4,
  scaling: 64.0,
  offset: 4550.0,
};

const VBAT: BqScaledRegister = BqScaledRegister {
  addr: 0x0E,
  mask: 0b01111111,
  pos: 0,
  scaling: 20.0,
  offset: 2304.0,
};

const VSYS: BqScaledRegister = BqScaledRegister {
  addr: 0x0F,
  mask: 0b01111111,
  pos: 0,
  scaling: 20.0,
  offset: 2304.0,
};

// === REG11: VBUS ===
const VBUS: BqScaledRegister = BqScaledRegister {
  addr: 0x11,
  mask: 0b01111111,
  pos: 0,
  scaling: 100.0,
  offset: 2600.0,
};

// === REG12: ICHGR ===
const ICHGR: BqScaledRegister = BqScaledRegister {
  addr: 0x12,
  mask: 0b01111111,
  pos: 0,
  scaling: 50.0,
  offset: 0.0,
};

// === REG13: IDPM_LIM ===
const IDPM_LIM: BqScaledRegister = BqScaledRegister {
  addr: 0x13,
  mask: 0b00111111,
  pos: 0,
  scaling: 50.0,
  offset: 100.0,
};

// === REG14: RESET ===
const REG_RST: BqRegister = BqRegister {
  addr: 0x14,
  mask: 0x80, // bit 7
  pos: 7,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChargeStatus {
  NotCharging,
  PreCharging,
  FastCharging,
  Terminated,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputStatus {
  NoInput,
  UsbSdp,
  UsbCdp,
  UsbDcp,
  MaxChargeDcp,
  UnknownAdapter,
  NonStdAdapter,
  Otg,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SystemStatus {
  Normal,
  InVsysMinRegulation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BatteryFault {
  Normal,
  OverVoltage,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoostFault {
  Normal,
  Overloaded,
  LowBattery,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChargeFault {
  Normal,
  InputFault,
  ThermalShutdown,
  SafetyTimerExpired,
}

#[derive(Debug)]
pub struct BqState {
  pub charge_status: ChargeStatus,
  pub input_status: InputStatus,
  pub system_status: SystemStatus,
  pub battery_fault: BatteryFault,
  pub boost_fault: BoostFault,
  pub charge_fault: ChargeFault,
  pub vbat: f32,
  pub vsys: f32,
  pub vbus: f32,
  pub ichrg: f32,
  pub vreg: f32,
  pub boostv: f32,
  pub input_current_limit: f32,
  pub is_ico_optimized: bool,
}

impl BqState {
  pub fn from_raw(
    status: u8,
    fault: u8,
    vbat: f32,
    vsys: f32,
    vbus: f32,
    ichrg: f32,
    vreg: f32,
    boostv: f32,
    ilim: f32,
    ico_opt: bool,
  ) -> Self {
    // Decode CHRG_STAT (bits 4-3 of REG0B)
    let charge_status = match (status >> 3) & 0x03 {
      0 => ChargeStatus::NotCharging,
      1 => ChargeStatus::PreCharging,
      2 => ChargeStatus::FastCharging,
      3 => ChargeStatus::Terminated,
      _ => ChargeStatus::NotCharging,
    };

    // Decode VBUS_STAT (bits 2-0 of REG0B)
    let input_status = match status & 0x07 {
      0 => InputStatus::NoInput,
      1 => InputStatus::UsbSdp,
      2 => InputStatus::UsbCdp,
      3 => InputStatus::UsbDcp,
      4 => InputStatus::MaxChargeDcp,
      5 => InputStatus::UnknownAdapter,
      6 => InputStatus::NonStdAdapter,
      7 => InputStatus::Otg,
      _ => InputStatus::NoInput,
    };

    // Decode VSYS_STAT (bit 0 of REG0B)
    let system_status = if status & 0x01 != 0 {
      SystemStatus::InVsysMinRegulation
    } else {
      SystemStatus::Normal
    };

    // Decode BAT_FAULT (bit 3 of REG0C)
    let battery_fault = if fault & 0x08 != 0 {
      BatteryFault::OverVoltage
    } else {
      BatteryFault::Normal
    };

    // Decode BOOST_FAULT (bit 6 of REG0C)
    let boost_fault = if fault & 0x40 != 0 {
      if fault & 0x10 != 0 {
        BoostFault::LowBattery
      } else {
        BoostFault::Overloaded
      }
    } else {
      BoostFault::Normal
    };

    // Decode CHRG_FAULT (bits 2-1 of REG0C)
    let charge_fault = match (fault >> 1) & 0x03 {
      0 => ChargeFault::Normal,
      1 => ChargeFault::InputFault,
      2 => ChargeFault::ThermalShutdown,
      3 => ChargeFault::SafetyTimerExpired,
      _ => ChargeFault::Normal,
    };

    Self {
      charge_status,
      input_status,
      system_status,
      battery_fault,
      boost_fault,
      charge_fault,
      vbat,
      vsys,
      vbus,
      ichrg,
      vreg,
      boostv,
      input_current_limit: ilim,
      is_ico_optimized: ico_opt,
    }
  }
}

pub struct Bq25895<I2C> {
  i2c: I2C,
}

impl<I2C, E> Bq25895<I2C>
where
  I2C: i2c::I2c<Error = E>,
{
  pub fn new(i2c: I2C) -> Self {
    Self { i2c }
  }

  fn write_register(&mut self, reg: u8, value: u8) -> Result<(), &'static str> {
    let buf = [reg, value];
    self.i2c.write(BQ25895_ADDR, &buf).map_err(|_| "I2C write failed")
  }

  fn read_registers(&mut self, start_reg: u8, count: usize) -> Result<Vec<u8>, &'static str> {
    let mut buf = vec![0u8; count];
    self.i2c.write(BQ25895_ADDR, &[start_reg]).map_err(|_| "I2C write failed")?;
    self.i2c.read(BQ25895_ADDR, &mut buf).map_err(|_| "I2C read failed")?;
    Ok(buf)
  }

  fn read_register(&mut self, reg: u8) -> Result<u8, &'static str> {
    let mut buf = [0u8; 1];
    self.i2c.write(BQ25895_ADDR, &[reg]).map_err(|_| "I2C write failed")?;
    self.i2c.read(BQ25895_ADDR, &mut buf).map_err(|_| "I2C read failed")?;
    Ok(buf[0])
  }

  fn write_bits(&mut self, reg: BqRegister, value: u8) -> Result<(), &'static str> {
    let current = self.read_register(reg.addr)?;
    let masked = current & (!reg.mask);
    let set_val = masked | ((value & 1) << reg.pos);
    self.write_register(reg.addr, set_val)?;
    Ok(())
  }

  fn read_scaled(&mut self, reg: BqScaledRegister) -> Result<f32, &'static str> {
    let raw_val = self.read_register(reg.addr)?;
    Ok(((((raw_val & reg.mask) >> reg.pos) as f32) * reg.scaling) + reg.offset)
  }

  fn write_scaled(&mut self, reg: BqScaledRegister, value: f32) -> Result<(), &'static str> {
    let current = self.read_register(reg.addr)?;
    let masked = current & (!reg.mask);
    let raw_val = (((value - reg.offset) / reg.scaling) as i32).max(0) as u8;
    let set_val = masked | ((raw_val << reg.pos) & reg.mask);
    self.write_register(reg.addr, set_val)?;
    Ok(())
  }

  pub fn init(&mut self) -> Result<(), &'static str> {
    // 1. Reset chip
    self.write_bits(REG_RST, 1)?;

    // 2. Configure registers (from original working config, now corrected per datasheet)
    self.write_register(0x02, 0x60)?;
    self.write_register(0x03, 0x10)?;
    self.write_register(0x04, 0x18)?;
    self.write_register(0x05, 0x00)?;
    self.write_register(0x07, 0x8C)?;

    Ok(())
  }

  pub fn enable_hiz(&mut self, enable: bool) -> Result<(), &'static str> {
    self.write_bits(EN_HIZ, if enable { 1 } else { 0 })
  }

  pub fn enable_boost(&mut self, enable: bool) -> Result<(), &'static str> {
    self.write_bits(OTG_CONFIG, if enable { 1 } else { 0 })
  }

  pub fn enable_charge(&mut self, enable: bool) -> Result<(), &'static str> {
    self.write_bits(CHG_CONFIG, if enable { 1 } else { 0 })
  }

  pub fn enable_battery_load(&mut self, enable: bool) -> Result<(), &'static str> {
    self.write_bits(BAT_LOADEN, if enable { 1 } else { 0 })
  }

  pub fn disable_batfet(&mut self, enable: bool) -> Result<(), &'static str> {
    self.write_bits(BATFET_DIS, if enable { 1 } else { 0 })
  }

  pub fn set_sys_min_voltage(&mut self, voltage: f32) -> Result<(), &'static str> {
    self.write_scaled(SYS_MIN, voltage)
  }

  pub fn set_charge_current(&mut self, ma: f32) -> Result<(), &'static str> {
    self.write_scaled(ICHG, ma)
  }

  pub fn set_precharge_current(&mut self, ma: f32) -> Result<(), &'static str> {
    self.write_scaled(IPRECHG, ma)
  }

  pub fn set_termination_current(&mut self, ma: f32) -> Result<(), &'static str> {
    self.write_scaled(ITERM, ma)
  }

  pub fn set_charge_voltage(&mut self, voltage: f32) -> Result<(), &'static str> {
    self.write_scaled(VREG, voltage)
  }

  pub fn set_recharge_threshold(&mut self, enable_200mv: bool) -> Result<(), &'static str> {
    self.write_bits(VRECHG, if enable_200mv { 1 } else { 0 })
  }

  pub fn set_battery_low_threshold(&mut self, low_3v: bool) -> Result<(), &'static str> {
    self.write_bits(BATLOWV, if low_3v { 1 } else { 0 })
  }

  pub fn set_watchdog_timeout(&mut self, seconds: u8) -> Result<(), &'static str> {
    let value = match seconds {
      0 => 0,
      40 => 1,
      80 => 2,
      160 => 3,
      _ => 1, // default 40s
    };
    self.write_scaled(WATCHDOG, value as f32)
  }

  pub fn set_charge_timer(&mut self, hours: u8) -> Result<(), &'static str> {
    let value = match hours {
      5 => 0,
      8 => 1,
      12 => 2,
      20 => 3,
      _ => 2, // default 12h
    };
    self.write_scaled(CHG_TIMER, value as f32)
  }

  pub fn set_boost_voltage(&mut self, voltage: f32) -> Result<(), &'static str> {
    self.write_scaled(BOOSTV, voltage)
  }

  pub fn set_input_current_limit(&mut self, limit_ma: f32) -> Result<(), &'static str> {
    self.write_scaled(INPUT_ILIM, limit_ma)?;

    if limit_ma > 1500.0 {
      self.write_bits(EN_ILIM_PIN, 1)?;
    } else {
      self.write_bits(EN_ILIM_PIN, 0)?;
    }

    Ok(())
  }

  pub fn update_state(&mut self) -> Result<BqState, &'static str> {
    let regs = self.read_registers(0x0B, 12)?; // Read 0x0B onwards

    let status = regs[0]; // REG0B
    let fault = regs[1]; // REG0C

    // Decode VBAT (REG0E)
    let vbat = self.read_scaled(VBAT)? / 1000.0;

    // Decode VSYS (REG0F)
    let vsys = self.read_scaled(VSYS)? / 1000.0;

    // Decode VBUS (REG11)
    let vbus = self.read_scaled(VBUS)? / 1000.0;

    // Decode ICHRG (REG12)
    let ichrg = self.read_scaled(ICHGR)?;

    // Decode VREG
    let vreg_val = self.read_scaled(VREG)? / 1000.0;

    // Decode BOOSTV (from REG0A)
    let boostv_val = self.read_scaled(BOOSTV)? / 1000.0;

    // Decode input current limit (from REG00)
    let ilim = (regs[0] & 0x3F) as f32 * 50.0 + 100.0;

    // Decode ICO optimized status (REG14 bit 6)
    let ico_opt = self.read_register(0x14)?;
    let ico_optimized = (ico_opt & 0x40) != 0;

    Ok(BqState::from_raw(
      status,
      fault,
      vbat,
      vsys,
      vbus,
      ichrg,
      vreg_val,
      boostv_val,
      ilim,
      ico_optimized,
    ))
  }
}
