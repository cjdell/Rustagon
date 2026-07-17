#![no_std]
#![feature(
  addr_parse_ascii,
  impl_trait_in_assoc_type,
  error_generic_member_access,
  future_join,
  allocator_api,
  box_vec_non_null,
  async_trait_bounds,
  impl_trait_in_bindings,
  substr_range
)]
#![recursion_limit = "256"]

extern crate alloc;
extern crate core;

mod apps;
pub mod d_i2c;
mod device;
mod native;
mod protocol;
pub mod tasks;
pub mod types;
#[macro_use]
pub mod utils;

pub const FIRMWARE_VERSION: &str = env!("FIRMWARE_VERSION");
