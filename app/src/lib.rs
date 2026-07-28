#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "extern-alloc", feature(allocator_api))]
#![feature(async_trait_bounds)]
#![allow(async_fn_in_trait)]

extern crate alloc;

pub mod apps;
#[cfg(feature = "http-server")]
pub mod http;
pub mod menu;
pub mod native;
pub mod platform;
pub mod protocol;
pub mod types;
pub mod utils;
