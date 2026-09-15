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
//!   reaches a system through a channel it polls. The match is the
//!   first: `netrunner_client::play::MatchHandle` runs the session on
//!   its own thread and the board polls it once a frame.
//! - The board renders a `ClientView` and submits what it chose from
//!   `netrunner_client::board::ActionMap`, built from `legal_actions`;
//!   what to highlight comes from `board::diff`'s `Transition`s, never
//!   from comparing what is on screen.
//! - Assets come in three tiers: a procedural one that always works, an
//!   optional file under `assets/` that is prettier, and a user override
//!   under `<data dir>/netrunner/assets/`. Nothing that is not the
//!   project's to license is committed.

// A Bevy query with two or three components and a filter is the normal
// shape of a system parameter, and naming each one is noise that hides
// what the system reads; the lint is meant for types a reader cannot
// parse, which a `Query` is not.
#![allow(clippy::type_complexity)]
// A system's parameters are the things it reads and writes, and a screen
// with eleven things on it has a system with eleven of them; bundling them
// into a `SystemParam` struct moves the list, it does not shorten it.
#![allow(clippy::too_many_arguments)]

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub mod assets;
pub mod card_back;
pub mod card_images;
pub mod core;
pub mod dev;
pub mod downloads;
pub mod icon_font;
pub mod models;
pub mod nav;
pub mod screens;
pub mod theme;
pub mod widgets;

pub use screens::AppScreen;

pub const WINDOW_TITLE: &str = "Netrunner";

/// The observers behind `ScrollArea` and `Scrollbar`, for an app built
/// without `DefaultPlugins`. With them (the `ui` feature brings
/// `bevy_ui_widgets`, and `DefaultPlugins` adds its plugin group) they
/// are already there, and adding a plugin twice is a panic at start-up;
/// without them — the headless tests — the components would be inert
/// markers. So: added only where absent.
struct ScrollPlugins;

impl Plugin for ScrollPlugins {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy::ui_widgets::ScrollAreaPlugin>() {
            app.add_plugins(bevy::ui_widgets::ScrollAreaPlugin);
        }
        if !app.is_plugin_added::<bevy::ui_widgets::ScrollbarPlugin>() {
            app.add_plugins(bevy::ui_widgets::ScrollbarPlugin);
        }
    }
}

/// Every plugin the client is made of, in one group, so `main` and a
/// headless test build the same application.
pub struct NetrunnerDesktopPlugins;

impl PluginGroup for NetrunnerDesktopPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(core::CorePlugin)
            .add(dev::DevPlugin)
            .add(theme::ThemePlugin)
            .add(nav::NavPlugin)
            .add(widgets::WidgetsPlugin)
            .add(ScrollPlugins)
            .add(card_images::CardImagesPlugin)
            .add(downloads::DownloadsPlugin)
            .add(icon_font::IconFontPlugin)
            .add(screens::boot::BootPlugin)
            .add(screens::main_menu::MainMenuPlugin)
            .add(screens::profile::ProfilePlugin)
            .add(screens::settings::SettingsPlugin)
            .add(screens::card_browser::CardBrowserPlugin)
            .add(screens::new_game::NewGamePlugin)
            .add(screens::game::GamePlugin)
            .add(screens::stubs::StubScreensPlugin)
    }
}
