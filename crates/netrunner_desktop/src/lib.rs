//! The Bevy desktop client of the Netrunner engine.
//!
//! **What this crate is.** A client in the sense AGENTS.md §3 gives the
//! word: it renders a `ClientView` and submits a `PlayerAction` chosen
//! from `legal_actions`, never touching `GameState` and never deciding a
//! rule. Everything it needs that is not rendering — the settings file,
//! the deck store, the rating book, the card pool, the match it is
//! playing — comes from `netrunner_client`, which it shares with the
//! terminal client so the two are one game with two faces.
//!
//! **How it is shaped**, so that the next screen looks like the last one
//! (the conventions AGENTS.md records under "Desktop client conventions"):
//!
//! - One [`screens::AppScreen`] state per screen and one `Plugin` per
//!   screen. `OnEnter` spawns the screen under [`nav::screen_root`], whose
//!   `DespawnOnExit` takes the whole tree down again; `Update` systems run
//!   under `run_if(in_state(..))`.
//! - Screen *state* that can be tested without a window lives in
//!   [`models`], as a plain struct driven by an `Intent` enum — the
//!   terminal client's state-struct-plus-`key()` pattern, minus the key
//!   codes. The Bevy systems translate input into intents and draw the
//!   result.
//! - Bevy runs on the main thread. Anything that blocks — a bot's search,
//!   a socket — runs elsewhere (`core::TokioRuntime`, a thread) and
//!   reaches a system through a channel it polls.
//! - Assets come in three tiers: a procedural one that always works, an
//!   optional file under `assets/` that is prettier, and a user override
//!   under `<data dir>/netrunner/assets/`. Nothing that is not the
//!   project's to license is committed.

// A Bevy query with two or three components and a filter is the normal
// shape of a system parameter, and naming each one is noise that hides
// what the system reads; the lint is meant for types a reader cannot
// parse, which a `Query` is not.
#![allow(clippy::type_complexity)]

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub mod core;
pub mod models;
pub mod nav;
pub mod screens;
pub mod theme;
pub mod widgets;

pub use screens::AppScreen;

pub const WINDOW_TITLE: &str = "Netrunner";

/// Every plugin the client is made of, in one group, so `main` and a
/// headless test build the same application.
pub struct NetrunnerDesktopPlugins;

impl PluginGroup for NetrunnerDesktopPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(core::CorePlugin)
            .add(theme::ThemePlugin)
            .add(nav::NavPlugin)
            .add(widgets::WidgetsPlugin)
            .add(screens::boot::BootPlugin)
            .add(screens::main_menu::MainMenuPlugin)
            .add(screens::profile::ProfilePlugin)
            .add(screens::settings::SettingsPlugin)
            .add(screens::stubs::StubScreensPlugin)
    }
}
