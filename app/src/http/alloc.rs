use alloc::vec::Vec;

#[cfg(feature = "extern-alloc")]
pub fn allocate_http_buffer(size: usize) -> Vec<u8> {
  use esp_alloc::ExternalMemory;
  let mut v = Vec::new_in(ExternalMemory);
  v.resize(size, 0);
  let (ptr, len, cap, _) = v.into_raw_parts_with_alloc();
  unsafe { Vec::from_raw_parts(ptr, len, cap) }
}

#[cfg(not(feature = "extern-alloc"))]
pub fn allocate_http_buffer(size: usize) -> Vec<u8> {
  vec![0u8; size]
}
