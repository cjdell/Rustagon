extern crate alloc;

#[macro_use]
pub mod helper;
pub mod allocator;
pub mod graphics;
pub mod http;
pub mod protocol;
pub mod sleep;
pub mod tasks;

#[macro_export]
macro_rules! mk_static {
  ($t:ty,$val:expr) => {{
    static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
    #[deny(unused_attributes)]
    let x = STATIC_CELL.uninit().write(($val));
    x
  }};
}
