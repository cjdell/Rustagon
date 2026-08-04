//! Helpers for allocating buffers in external memory (PSRAM on the firmware).
//!
//! On targets with the `extern-alloc` feature (firmware) large buffers are
//! placed in external memory so they don't exhaust the small internal SRAM
//! heap, and large value types (SSH session state, big structs) can be kept
//! off the stack. On other targets these functions are thin wrappers over the
//! default allocator.

use alloc::{boxed::Box, vec, vec::Vec};

/// Allocate a zeroed `Vec<u8>` whose backing store lives in external memory
/// (PSRAM on the firmware). Falls back to the default heap elsewhere.
///
/// Generalisation of the former HTTP file-transfer buffer allocation, so any
/// subsystem can grab a large PSRAM-backed buffer.
pub fn external_vec(size: usize) -> Vec<u8> {
  #[cfg(feature = "extern-alloc")]
  {
    use esp_alloc::ExternalMemory;
    let mut v = Vec::new_in(ExternalMemory);
    v.resize(size, 0);
    let (ptr, len, cap, _) = v.into_raw_parts_with_alloc();
    // SAFETY: `ExternalMemory` and the global allocator share the same
    // `EspHeap`; deallocation dispatches on the pointer range, so a plain
    // `Vec` can own memory allocated for external (PSRAM) memory.
    unsafe { Vec::from_raw_parts(ptr, len, cap) }
  }
  #[cfg(not(feature = "extern-alloc"))]
  {
    vec![0u8; size]
  }
}

/// Move a large value into external memory (PSRAM on the firmware), returning
/// a plain `Box<T>`. Use for big state structs (e.g. the SSH session) that
/// would otherwise sit on the stack as locals or be copied during menu stack
/// manipulation. Falls back to the default allocator elsewhere.
pub fn external_box<T>(value: T) -> Box<T> {
  #[cfg(feature = "extern-alloc")]
  {
    use esp_alloc::ExternalMemory;
    let boxed = Box::new_in(value, ExternalMemory);
    let (raw, _) = Box::<T, ExternalMemory>::into_raw_with_allocator(boxed);
    // SAFETY: as for `external_vec` — the global allocator's dealloc
    // dispatches on the pointer and both sides use `size_of::<T>()` layout.
    unsafe { Box::from_raw(raw) }
  }
  #[cfg(not(feature = "extern-alloc"))]
  {
    Box::new(value)
  }
}
