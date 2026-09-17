//! Screen state with no Bevy in it.
//!
//! Each model is a plain struct driven by an `Intent` enum and tested
//! under plain `cargo test`; the screen's systems only translate input
//! into intents and draw the result. It is the terminal client's
//! state-struct-plus-`key()` pattern (`netrunner_cli::tui::menu` and
//! friends) with the key codes taken out, so the same logic could drive
//! a third client, and so that what a screen *does* is pinned by tests
//! that need no window.

pub mod browser;
pub mod game;
pub mod layout;
pub mod pace;
pub mod settings;
pub mod shortcuts;
