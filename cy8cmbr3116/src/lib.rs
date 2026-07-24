//! `embedded-hal` driver for the Infineon/Cypress **CY8CMBR3116** CapSense
//! Express controller (16 sensing inputs, 8 GPOs, I2C slave, no slider
//! support on this particular part number).
//!
//! This device has no on-chip firmware to write: it is entirely configured
//! and polled through its I2C register map (see the datasheet's "Register
//! Configurability" / "Host Communication Protocol" sections, and the
//! `CY8CMBR3xxx CapSense Express Controllers Registers TRM`, Doc No.
//! 001-91082). Register addresses and bit layouts below are taken directly
//! from that TRM's Section 1.5 "Register Map". Three register categories
//! exist:
//!
//! * `0x00..=0x7E` – Configuration registers (read/write, must be saved to
//!   NVM with [`Command::SaveCheckCrc`] + device reset to take effect)
//! * `0x80..=0x87` – Command / host-writable output registers
//! * `0x88..=0xFB` – Status registers (read-only: button/GPO status,
//!   diagnostics, CRC, IDs, etc.)
//!
//! The default I2C address is `0x37` (configurable `0x08..=0x77`, i.e.
//! decimal 8-119, via the `I2C_ADDR` register at `0x51`).
//!
//! **Note on sliders:** per the TRM, `SLIDERx_POSITION` and related slider
//! registers are explicitly documented as "not applicable" for the
//! CY8CMBR3116 (slider support is unique to the CY8CMBR3106S variant), so
//! this driver does not expose slider APIs.
//!
//! This driver does not attempt to model the *entire* register map (all
//! sensitivity/threshold/debounce/PWM/proximity-tuning configuration
//! registers, debug raw-count registers, etc.); those are reachable with
//! [`Cy8cmbr3116::read_register`] / [`Cy8cmbr3116::write_register`] using
//! the addresses from the TRM. Commonly used registers are exposed with
//! typed accessors below.
#![no_std]

use embedded_hal::i2c::I2c;

/// Default 7-bit I2C slave address for all CY8CMBR3xxx devices.
pub const DEFAULT_ADDRESS: u8 = 0x37;

/// Total number of sensing inputs (CS0..CS15) on the CY8CMBR3116.
pub const NUM_SENSORS: usize = 16;

/// Number of GPOs (GPO0..GPO7) on the CY8CMBR3116.
pub const NUM_GPOS: usize = 8;

/// Errors returned by this driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<E> {
    /// Underlying I2C bus error.
    I2c(E),
    /// The device NACK'd the transaction. Per the datasheet this happens
    /// while the device is in a low-power state, mid-boot (before
    /// `T_I2CBOOT`), or while executing a `SaveCheckCrc` command (up to
    /// ~220 ms). The host is expected to retry.
    Nack,
}

impl<E> From<E> for Error<E> {
    fn from(e: E) -> Self {
        Error::I2c(e)
    }
}

/// Command opcodes for the `CTRL_CMD` register (address `0x86`).
///
/// Values and semantics are taken from the TRM's `CTRL_CMD` (§1.5.80)
/// description. The device resets `CTRL_CMD` to `0` at startup and on
/// completion of any command; the host must only write while it reads back
/// as `0` (poll [`Cy8cmbr3116::command_has_error`] / raw-read `CTRL_CMD`
/// itself if you need to confirm completion before issuing the next
/// command).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    /// Calculate a CRC over the configuration registers, compare it to
    /// `CONFIG_CRC`, and if it matches, save the configuration + CRC to
    /// non-volatile memory. Takes ~220 ms (`T_SAVE`); the device NACKs I2C
    /// transactions until it completes. The new configuration only takes
    /// effect after a subsequent reset.
    SaveCheckCrc = 2,
    /// Test/debug only (not recommended for production per the TRM):
    /// calculate a CRC over the configuration registers and place the
    /// result in `CALC_CRC`, without saving anything.
    CalcCrc = 3,
    /// Stop scanning and enter the low-power (Deep Sleep) mode. The device
    /// exits this mode on the next I2C address-match event.
    Sleep = 7,
    /// Clear `LATCHED_BUTTON_STAT` and `LATCHED_PROX_STAT` to 0, and reset
    /// `LIFTOFF_SLIDER1_POSITION` / `LIFTOFF_SLIDER2_POSITION` to `0xFF`.
    ClearLatchedStatus = 8,
    /// Reset the Advanced Low-Pass filter for proximity sensor PS0.
    ResetProxFilterPs0 = 9,
    /// Reset the Advanced Low-Pass filter for proximity sensor PS1.
    ResetProxFilterPs1 = 10,
    /// Software reset. Functionally equivalent to a power-on reset or an
    /// `XRES` pulse; the device re-enters the Boot state.
    Reset = 255,
}

/// Error codes returned in `CTRL_CMD_ERR` (`0x89`) after a command
/// completes. See TRM §1.5.82.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    /// The command completed successfully.
    Success,
    /// Writing the configuration to flash failed.
    FlashWriteFailed,
    /// The CRC stored in `CONFIG_CRC` did not match the calculated CRC over
    /// the configuration registers (returned by `SaveCheckCrc`).
    CrcMismatch,
    /// The opcode written to `CTRL_CMD` was not recognized.
    InvalidCommand,
    /// A code not covered by the documented set above.
    Other(u8),
}

impl From<u8> for CommandError {
    fn from(v: u8) -> Self {
        match v {
            0 => CommandError::Success,
            253 => CommandError::FlashWriteFailed,
            254 => CommandError::CrcMismatch,
            255 => CommandError::InvalidCommand,
            other => CommandError::Other(other),
        }
    }
}

/// Registers used by this driver, with addresses taken directly from the
/// `CY8CMBR3xxx Registers TRM` (Doc No. 001-91082), Section 1.5 "Register
/// Map". This is not the full register map (see the TRM for
/// sensitivity/threshold/debounce/PWM/debug registers etc.); use
/// [`Cy8cmbr3116::read_register`] / [`Cy8cmbr3116::write_register`] with a
/// raw `u8` address for anything not listed here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Register {
    // ---- Configuration registers (0x00-0x7E) ----
    /// Per-sensor CapSense enable, CS0..CS15 (2 bytes, bit N = CSn).
    /// TRM §1.5.1.
    SensorEn = 0x00,
    /// Per-sensor proximity/button mode select, PS0/PS1 (bit N = PSn).
    /// TRM §1.5.30.
    ProxEn = 0x26,
    /// I2C slave address configuration register (bits 6:0, valid 8-119
    /// decimal / 0x08-0x77). TRM §1.5.62.
    I2cAddr = 0x51,

    // ---- Command / host-output registers (0x80-0x87) ----
    /// Host-controlled GPO output register (read/write). Writing a bit here
    /// only has effect while the corresponding GPO is host-controlled
    /// (rather than sensor-controlled) per `GPO_CFG`. TRM §1.5.78.
    GpoOutputState = 0x80,
    /// Sensor ID selector for the `DEBUG_*` registers (not general status).
    /// TRM §1.5.79.
    SensorId = 0x82,
    /// Command register that accepts opcodes from [`Command`]. TRM §1.5.80.
    CtrlCmd = 0x86,

    // ---- Status registers (0x88-0xFB) ----
    /// Result of the most recently executed command: bit 0 = error flag.
    /// TRM §1.5.81.
    CtrlCmdStatus = 0x88,
    /// Detailed error code from the most recently executed command. TRM
    /// §1.5.82.
    CtrlCmdErr = 0x89,
    /// System status; bit 0 indicates factory-default configuration is
    /// loaded. TRM §1.5.83.
    SystemStatus = 0x8A,
    /// Fixed device family ID (154 for all CY8CMBR3xxx parts) — useful as a
    /// boot/presence sanity check. TRM §1.5.85.
    FamilyId = 0x8F,
    /// 16-bit silicon/device ID. TRM §1.5.86.
    DeviceId = 0x90,
    /// Current (unlatched) button touch status, CS0..CS15 (2 bytes, bit N =
    /// CSn). TRM §1.5.95.
    ButtonStat = 0xAA,
    /// Latched button status: bits stay set until cleared with
    /// [`Command::ClearLatchedStatus`] (2 bytes, bit N = CSn). TRM §1.5.96.
    LatchedButtonStat = 0xAC,
    /// Current proximity status, 1 byte: bit 0 = PS0, bit 1 = PS1. TRM
    /// §1.5.97.
    ProxStat = 0xAE,
    /// Latched proximity status, 1 byte: bit 0 = PS0, bit 1 = PS1. TRM
    /// §1.5.98.
    LatchedProxStat = 0xAF,
    /// Actual GPO pin state as driven by the device (read-only mirror of
    /// [`Register::GpoOutputState`]; reflects PWM duty-cycle selection when
    /// PWM is enabled). Bit N = GPOn. TRM §1.5.120.
    GpoData = 0xDA,
}

impl Register {
    #[inline]
    fn addr(self) -> u8 {
        self as u8
    }
}

/// Snapshot of the current touch status of all 16 CapSense buttons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ButtonStatus(pub u16);

impl ButtonStatus {
    /// Returns `true` if sensor `CSn` (0..=15) is currently touched.
    pub fn is_touched(&self, sensor: u8) -> bool {
        debug_assert!((sensor as usize) < NUM_SENSORS);
        (self.0 >> sensor) & 1 != 0
    }

    /// Iterator over all touched sensor indices.
    pub fn touched(&self) -> impl Iterator<Item = u8> + '_ {
        (0..NUM_SENSORS as u8).filter(move |&n| self.is_touched(n))
    }
}

/// Snapshot of the two proximity sensor statuses (PS0, PS1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProximityStatus(pub u8);

impl ProximityStatus {
    /// `true` if PS0 detects proximity/touch.
    pub fn ps0(&self) -> bool {
        self.0 & 0x01 != 0
    }
    /// `true` if PS1 detects proximity/touch.
    pub fn ps1(&self) -> bool {
        self.0 & 0x02 != 0
    }
}

/// Snapshot of the 8 GPO states (either the device-driven actual state from
/// `GPO_DATA`, or the host-requested state from `GPO_OUTPUT_STATE`,
/// depending on which accessor produced it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpoStatus(pub u8);

impl GpoStatus {
    /// `true` if GPOn (0..=7) is high / active per the source register's
    /// convention (see `GPO_CFG.ACTIVE_STATE` in the TRM for how this maps
    /// to a physically active-low or active-high pin).
    pub fn is_active(&self, gpo: u8) -> bool {
        debug_assert!((gpo as usize) < NUM_GPOS);
        (self.0 >> gpo) & 1 != 0
    }

    /// Returns the bit for GPOn set/cleared, useful for building a value to
    /// write to `GPO_OUTPUT_STATE`.
    pub fn with_gpo(mut self, gpo: u8, active: bool) -> Self {
        debug_assert!((gpo as usize) < NUM_GPOS);
        if active {
            self.0 |= 1 << gpo;
        } else {
            self.0 &= !(1 << gpo);
        }
        self
    }
}

/// Driver for the CY8CMBR3116 CapSense Express controller.
///
/// `I2C` must implement the `embedded-hal` 1.0 [`embedded_hal::i2c::I2c`]
/// trait. The driver owns the bus (or a shared-bus proxy) and the 7-bit
/// slave address.
pub struct Cy8cmbr3116<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C, E> Cy8cmbr3116<I2C>
where
    I2C: I2c<Error = E>,
{
    /// Create a new driver instance using the default I2C address (`0x37`).
    pub fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: DEFAULT_ADDRESS,
        }
    }

    /// Create a new driver instance with a custom (previously configured)
    /// I2C address.
    pub fn new_with_address(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    /// Release the underlying I2C peripheral.
    pub fn release(self) -> I2C {
        self.i2c
    }

    // ---- Low-level register access -----------------------------------

    /// Set the device's internal register pointer without reading data
    /// back. Mirrors the "Setting the Device Data Pointer" write-only
    /// sequence in the datasheet.
    pub fn set_pointer(&mut self, reg: u8) -> Result<(), Error<E>> {
        self.i2c
            .write(self.address, &[reg])
            .map_err(Error::I2c)
    }

    /// Read `buf.len()` bytes starting at register `reg`, using a repeated
    /// START (`write_read`), matching the "Read Operation" sequence in the
    /// datasheet.
    pub fn read_register(&mut self, reg: u8, buf: &mut [u8]) -> Result<(), Error<E>> {
        self.i2c
            .write_read(self.address, &[reg], buf)
            .map_err(Error::I2c)
    }

    /// Read a single byte register.
    pub fn read_u8(&mut self, reg: Register) -> Result<u8, Error<E>> {
        let mut buf = [0u8; 1];
        self.read_register(reg.addr(), &mut buf)?;
        Ok(buf[0])
    }

    /// Read a little-endian 16-bit register (matches the byte ordering of
    /// consecutive status registers such as `BUTTON_STAT`).
    pub fn read_u16(&mut self, reg: Register) -> Result<u16, Error<E>> {
        let mut buf = [0u8; 2];
        self.read_register(reg.addr(), &mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    /// Write `data` to consecutive registers starting at `reg`. Matches the
    /// "Write Operation" sequence: `[reg, data[0], data[1], ...]`.
    pub fn write_register(&mut self, reg: u8, data: &[u8]) -> Result<(), Error<E>> {
        // Configuration registers are contiguous 0x00-0x7E; build the frame
        // on a small stack buffer to avoid requiring `alloc`.
        const MAX_FRAME: usize = 33; // 1 addr byte + up to 32 data bytes
        let mut frame = [0u8; MAX_FRAME];
        assert!(
            data.len() + 1 <= MAX_FRAME,
            "write_register: data too large for internal buffer"
        );
        frame[0] = reg;
        frame[1..=data.len()].copy_from_slice(data);
        self.i2c
            .write(self.address, &frame[..=data.len()])
            .map_err(Error::I2c)
    }

    /// Write a single byte to a configuration register.
    pub fn write_u8(&mut self, reg: Register, value: u8) -> Result<(), Error<E>> {
        self.write_register(reg.addr(), &[value])
    }

    // ---- Status -----------------------------------------------------------

    /// Read the current (unlatched) status of all 16 CapSense buttons
    /// (`BUTTON_STAT`, `0xAA`).
    pub fn button_status(&mut self) -> Result<ButtonStatus, Error<E>> {
        Ok(ButtonStatus(self.read_u16(Register::ButtonStat)?))
    }

    /// Read the latched button status (`LATCHED_BUTTON_STAT`, `0xAC`): a
    /// bit stays set once a touch occurs until explicitly cleared with
    /// [`Self::clear_latched_status`]. Useful for not missing brief touches
    /// between polls.
    pub fn latched_button_status(&mut self) -> Result<ButtonStatus, Error<E>> {
        Ok(ButtonStatus(self.read_u16(Register::LatchedButtonStat)?))
    }

    /// Read current proximity sensor (PS0/PS1) status (`PROX_STAT`,
    /// `0xAE`, 1 byte).
    pub fn proximity_status(&mut self) -> Result<ProximityStatus, Error<E>> {
        Ok(ProximityStatus(self.read_u8(Register::ProxStat)?))
    }

    /// Read latched proximity sensor status (`LATCHED_PROX_STAT`, `0xAF`).
    /// Cleared the same way as latched button status, via
    /// [`Self::clear_latched_status`].
    pub fn latched_proximity_status(&mut self) -> Result<ProximityStatus, Error<E>> {
        Ok(ProximityStatus(self.read_u8(Register::LatchedProxStat)?))
    }

    /// Read the GPO pin states actually being driven by the device
    /// (`GPO_DATA`, `0xDA`, read-only). If a GPO is configured for PWM,
    /// its bit reflects the instantaneous duty-cycle phase rather than a
    /// static level. Disabled GPOs read as `0`.
    ///
    /// Note: per the TRM this register is not applicable on the
    /// CY8CMBR3106S variant; it is valid on the CY8CMBR3116.
    pub fn gpo_data(&mut self) -> Result<GpoStatus, Error<E>> {
        Ok(GpoStatus(self.read_u8(Register::GpoData)?))
    }

    /// Read back the host-requested GPO output state (`GPO_OUTPUT_STATE`,
    /// `0x80`, read/write). Only GPOs configured as host-controlled (rather
    /// than sensor-controlled, see `GPO_CFG` in the TRM) are actually
    /// driven from this register's contents.
    pub fn gpo_output_state(&mut self) -> Result<GpoStatus, Error<E>> {
        Ok(GpoStatus(self.read_u8(Register::GpoOutputState)?))
    }

    /// Drive host-controlled GPOs directly by writing `GPO_OUTPUT_STATE`
    /// (`0x80`). Has effect only for GPOs configured as host-controlled.
    pub fn set_gpo_output_state(&mut self, state: GpoStatus) -> Result<(), Error<E>> {
        self.write_u8(Register::GpoOutputState, state.0)
    }

    /// Read the fixed device family ID (`FAMILY_ID`, `0x8F`). Per the TRM
    /// this is always `154` for CY8CMBR3xxx parts; a mismatch (or a bus
    /// error/NACK) indicates the device isn't present/responding at this
    /// address.
    pub fn family_id(&mut self) -> Result<u8, Error<E>> {
        self.read_u8(Register::FamilyId)
    }

    /// Read the 16-bit silicon/device ID (`DEVICE_ID`, `0x90`).
    pub fn device_id(&mut self) -> Result<u16, Error<E>> {
        self.read_u16(Register::DeviceId)
    }

    /// Convenience probe: confirms the device responds and reports the
    /// expected family ID (154). Returns `Ok(true)` on a match, `Ok(false)`
    /// if a different ID was read back (unexpected device at this
    /// address), or an [`Error`] on bus failure/NACK.
    pub fn probe(&mut self) -> Result<bool, Error<E>> {
        const EXPECTED_FAMILY_ID: u8 = 154;
        Ok(self.family_id()? == EXPECTED_FAMILY_ID)
    }

    // ---- Commands -----------------------------------------------------------

    /// Issue a command by writing its opcode to `CTRL_CMD` (`0x86`).
    ///
    /// The device resets `CTRL_CMD` to `0` at startup and on completion of
    /// any command; writing while a command is still in progress (register
    /// reads back non-zero) has undefined results per the TRM. For
    /// [`Command::SaveCheckCrc`] specifically, expect the device to NACK
    /// subsequent I2C transactions for up to ~220 ms (`T_SAVE`) while the
    /// write completes; retry on [`Error::Nack`].
    pub fn command(&mut self, cmd: Command) -> Result<(), Error<E>> {
        self.write_u8(Register::CtrlCmd, cmd as u8)
    }

    /// Convenience wrapper: save the current configuration registers to
    /// non-volatile memory (CRC-checked against `CONFIG_CRC`). The new
    /// configuration only takes effect after a subsequent reset
    /// ([`Self::reset`], `XRES`, or power-cycle) per the datasheet's
    /// "Register Configurability" section.
    pub fn save_configuration(&mut self) -> Result<(), Error<E>> {
        self.command(Command::SaveCheckCrc)
    }

    /// Issue a software reset (`CTRL_CMD = 255`). The device re-enters the
    /// Boot state and the host must wait `T_I2CBOOT` (15 ms typ.) before
    /// further I2C traffic.
    pub fn reset(&mut self) -> Result<(), Error<E>> {
        self.command(Command::Reset)
    }

    /// Force the device into Deep Sleep (`CTRL_CMD = 7`). Any I2C address
    /// match wakes it, but per the datasheet the device NACKs until it
    /// fully transitions to the Active state; retry as needed.
    pub fn sleep(&mut self) -> Result<(), Error<E>> {
        self.command(Command::Sleep)
    }

    /// Clear latched button/proximity status and slider liftoff position
    /// registers (`CTRL_CMD = 8`).
    pub fn clear_latched_status(&mut self) -> Result<(), Error<E>> {
        self.command(Command::ClearLatchedStatus)
    }

    /// Read back whether the most recently executed command reported an
    /// error, from `CTRL_CMD_STATUS` (`0x88`, bit 0).
    pub fn command_has_error(&mut self) -> Result<bool, Error<E>> {
        Ok(self.read_u8(Register::CtrlCmdStatus)? & 0x01 != 0)
    }

    /// Read the detailed error code of the most recently executed command
    /// from `CTRL_CMD_ERR` (`0x89`).
    pub fn command_error(&mut self) -> Result<CommandError, Error<E>> {
        Ok(CommandError::from(self.read_u8(Register::CtrlCmdErr)?))
    }

    /// Read whether the factory-default configuration is currently loaded
    /// (`SYSTEM_STATUS`, `0x8A`, bit 0).
    pub fn is_factory_default(&mut self) -> Result<bool, Error<E>> {
        Ok(self.read_u8(Register::SystemStatus)? & 0x01 != 0)
    }

    // ---- Configuration --------------------------------------------------

    /// Read the per-sensor CapSense enable bits (`SENSOR_EN`, `0x00`,
    /// bit N = CSn).
    pub fn sensor_enable(&mut self) -> Result<ButtonStatus, Error<E>> {
        Ok(ButtonStatus(self.read_u16(Register::SensorEn)?))
    }

    /// Write the per-sensor CapSense enable bits (`SENSOR_EN`, `0x00`).
    /// Requires [`Self::save_configuration`] + [`Self::reset`] to persist
    /// and take effect.
    pub fn set_sensor_enable(&mut self, mask: ButtonStatus) -> Result<(), Error<E>> {
        self.write_register(Register::SensorEn.addr(), &mask.0.to_le_bytes())
    }

    /// Read which of sensors 0/1 are configured as proximity sensors vs.
    /// plain buttons (`PROX_EN`, `0x26`, bit 0 = PS0, bit 1 = PS1).
    pub fn proximity_enable(&mut self) -> Result<ProximityStatus, Error<E>> {
        Ok(ProximityStatus(self.read_u8(Register::ProxEn)?))
    }

    /// Configure whether sensors 0/1 act as proximity sensors
    /// (`PROX_EN`, `0x26`). Requires [`Self::save_configuration`] +
    /// [`Self::reset`] to persist and take effect.
    pub fn set_proximity_enable(&mut self, mask: ProximityStatus) -> Result<(), Error<E>> {
        self.write_u8(Register::ProxEn, mask.0 & 0x03)
    }

    /// Reconfigure the device's I2C slave address (valid range: decimal
    /// 8-119, i.e. `0x08..=0x77`). This writes the `I2C_ADDR` configuration
    /// register (`0x51`); call [`Self::save_configuration`] followed by
    /// [`Self::reset`] for it to persist and take effect, then construct a
    /// new driver instance with [`Self::new_with_address`].
    pub fn set_i2c_address(&mut self, new_address: u8) -> Result<(), Error<E>> {
        assert!(
            (0x08..=0x77).contains(&new_address),
            "I2C address out of the 0x08-0x77 (8-119 decimal) range supported by CY8CMBR3xxx"
        );
        self.write_u8(Register::I2cAddr, new_address & 0x7F)
    }

    /// Current address this driver instance is configured to use.
    pub fn address(&self) -> u8 {
        self.address
    }
}
