//! Shared `no_std` widget toolkit for menu apps.
//!
//! Small, display-agnostic building blocks (alloc only — no std) that apps
//! compose into [`LcdScreen`]s. They own *state and interaction logic*
//! (cursor, selection, focus, scrollback); rendering is a plain method that
//! returns an `LcdScreen`, so each app still decides when to signal the
//! display.
//!
//! - [`text_input::TextInput`] — single-line value + byte-index cursor
//!   (insert/backspace/delete/home/end/left/right).
//! - [`list::List`] — items + selected index (scrolling lists, root menu).
//! - [`form::Form`] — labelled [`TextInput`] fields + an action row, with
//!   up/down/fire focus navigation.
//! - [`terminal::Terminal`] — VT-style byte-stream → 8-line text buffer
//!   renderer (SSH shell output, future log/serial viewers).
//! - [`progress::Progress`] — bounded progress bar (`LcdScreen::BoundedProgress`).

pub mod form;
pub mod list;
pub mod progress;
pub mod terminal;
pub mod text_input;
