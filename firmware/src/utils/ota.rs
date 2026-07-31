use crc::{Algorithm, Crc};
use embedded_storage::{ReadStorage, Storage};
use esp_storage::FlashStorage;

static ALGO: Algorithm<u32> = Algorithm {
  width: 32,
  poly: 0x04c11db7,
  init: 0,
  refin: true,
  refout: true,
  xorout: 0,
  check: 0,
  residue: 0,
};

/// CRC-32/LE as computed by the ESP-IDF bootloader's `esp_rom_crc32_le(0, ..)`.
fn checksum(data: &[u8]) -> u32 {
  let crc = Crc::<u32>::new(&ALGO);
  let mut digest = crc.digest();
  digest.update(data);
  digest.finalize()
}

#[derive(Debug)]
pub struct Ota<'a> {
  flash: &'a mut FlashStorage<'static>,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum Slot {
  None,
  Slot0,
  Slot1,
}

impl Slot {
  pub fn number(&self) -> usize {
    match self {
      Slot::None => 0,
      Slot::Slot0 => 0,
      Slot::Slot1 => 1,
    }
  }

  pub fn next(&self) -> Slot {
    match self {
      Slot::None => Slot::Slot0,
      Slot::Slot0 => Slot::Slot1,
      Slot::Slot1 => Slot::Slot0,
    }
  }
}

impl<'a> Ota<'a> {
  pub fn new(flash: &'a mut FlashStorage<'static>) -> Ota<'a> {
    Ota { flash }
  }

  pub fn free(self) {}

  pub fn current_slot(&mut self) -> Slot {
    let (seq0, seq1) = self.get_slot_seq();

    if seq0 == 0xffffffff && seq1 == 0xffffffff {
      Slot::None
    } else if seq0 == 0xffffffff {
      Slot::Slot1
    } else if seq1 == 0xffffffff {
      Slot::Slot0
    } else if seq0 > seq1 {
      Slot::Slot0
    } else {
      Slot::Slot1
    }
  }

  fn get_slot_seq(&mut self) -> (u32, u32) {
    let mut buffer1 = [0u8; 0x20];
    let mut buffer2 = [0u8; 0x20];
    self.flash.read(0xd000, &mut buffer1).unwrap();
    self.flash.read(0xe000, &mut buffer2).unwrap();
    let mut seq0bytes = [0u8; 4];
    let mut seq1bytes = [0u8; 4];
    seq0bytes[..].copy_from_slice(&buffer1[..4]);
    seq1bytes[..].copy_from_slice(&buffer2[..4]);
    let seq0 = u32::from_le_bytes(seq0bytes);
    let seq1 = u32::from_le_bytes(seq1bytes);
    (seq0, seq1)
  }

  /// The slot to write the next OTA update into.
  ///
  /// With empty otadata the bootloader boots the first app partition (ota_0),
  /// so treat that as current and target the other slot. Never write to the
  /// slot the running firmware is executing from.
  pub fn target_slot(&mut self) -> Slot {
    match self.current_slot() {
      Slot::None => Slot::Slot1,
      slot => slot.next(),
    }
  }

  pub fn set_current_slot(&mut self, slot: Slot) {
    let (seq0, seq1) = self.get_slot_seq();

    let new_seq = {
      if seq0 == 0xffffffff && seq1 == 0xffffffff {
        1
      } else if seq0 == 0xffffffff {
        seq1 + 1
      } else if seq1 == 0xffffffff {
        seq0 + 1
      } else {
        u32::max(seq0, seq1) + 1
      }
    };
    let new_seq_le = new_seq.to_le_bytes();

    let mut buffer1 = [0xffu8; 0x20];
    let mut buffer2 = [0xffu8; 0x20];

    self.flash.read(0xd000, &mut buffer1).unwrap();
    self.flash.read(0xe000, &mut buffer2).unwrap();

    if slot == Slot::Slot0 {
      buffer1[..4].copy_from_slice(&new_seq_le);
      let crc = checksum(&buffer1[..28]).to_le_bytes();
      buffer1[28..].copy_from_slice(&crc);
    } else {
      buffer2[..4].copy_from_slice(&new_seq_le);
      let crc = checksum(&buffer2[..28]).to_le_bytes();
      buffer2[28..].copy_from_slice(&crc);
    }

    self.flash.write(0xd000, &buffer1).unwrap();
    self.flash.write(0xe000, &buffer2).unwrap();
  }

  pub fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), esp_storage::FlashStorageError> {
    self.flash.write(addr, data)
  }
}
