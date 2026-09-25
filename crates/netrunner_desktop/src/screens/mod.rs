//! The screens, one state and one plugin each.
//!
//! A screen is entered by writing `nav::Navigate(AppScreen::X)`. Its
//! plugin spawns everything under `nav::screen_root` on `OnEnter`, which
//! also takes it all down on exit, and runs its systems under
//! `run_if(in_state(AppScreen::X))`. The set below is the whole client;
//! a screen that is not built yet is a stub (`stubs`) with a heading and
//! a way back, so every menu entry leads somewhere from the first PR.

pub mod about;
pub mod boot;
pub mod card_browser;
pub mod deck_editor;
pub mod decks;
pub mod game;
pub mod learn;
pub mod main_menu;
pub mod new_game;
pub mod profile;
pub mod replay;
pub mod settings;
pub mod splash;
pub mod stubs;

use bevy::prelude::*;

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppScreen {
    /// Loads the client core and the theme, then goes to the menu. Never
    /// drawn.
    #[default]
    Boot,
    /// The title card while the fonts load: a moment, skipped by any key
    /// or click, and never shown to a window nobody can see.
    Splash,
    MainMenu,
    Profile,
    Settings,
    Decks,
    DeckEditor,
    CardBrowser,
    NewGame,
    Learn,
    Online,
    Game,
    Replay,
    /// Whose work is in the client, and under what terms.
    About,
}

impl AppScreen {
    pub const ALL: [AppScreen; 14] = [
        AppScreen::Boot,
        AppScreen::Splash,
        AppScreen::MainMenu,
        AppScreen::Profile,
        AppScreen::Settings,
        AppScreen::Decks,
        AppScreen::DeckEditor,
        AppScreen::CardBrowser,
        AppScreen::NewGame,
        AppScreen::Learn,
        AppScreen::Online,
        AppScreen::Game,
        AppScreen::Replay,
        AppScreen::About,
    ];

    /// The screen named by its variant or its heading, any case —
    /// `cardbrowser` and `cards` both — for `NETRUNNER_SCREEN`.
    pub fn from_name(name: &str) -> Option<AppScreen> {
        let name = name.trim();
        AppScreen::ALL.into_iter().find(|screen| format!("{screen:?}").eq_ignore_ascii_case(name) || screen.title().eq_ignore_ascii_case(name))
    }

    /// The name of the screen's backdrop slot (`backdrop`), which is also
    /// its file name under `assets/backdrops/`: `None` for a screen that
    /// has no backdrop of its own — Boot is never drawn, and the board
    /// stands on its table.
    ///
    /// **Exhaustive on purpose**: a new screen does not compile until it
    /// has a slot, and a test then fails until the slot is in the guide,
    /// so there is always somewhere to put a picture before anybody has
    /// drawn one.
    pub fn asset_key(self) -> Option<&'static str> {
        match self {
            AppScreen::Boot | AppScreen::Game => None,
            AppScreen::Splash => Some("splash"),
            AppScreen::MainMenu => Some("main-menu"),
            AppScreen::Profile => Some("profile"),
            AppScreen::Settings => Some("settings"),
            AppScreen::Decks => Some("decks"),
            AppScreen::DeckEditor => Some("deck-editor"),
            AppScreen::CardBrowser => Some("cards"),
            AppScreen::NewGame => Some("new-game"),
            AppScreen::Learn => Some("learn"),
            AppScreen::Online => Some("online"),
            AppScreen::Replay => Some("replay"),
            AppScreen::About => Some("about"),
        }
    }

    /// The heading a screen shows.
    pub fn title(self) -> &'static str {
        match self {
            AppScreen::Boot | AppScreen::Splash => "",
            AppScreen::MainMenu => "Netrunner",
            AppScreen::Profile => "Profile",
            AppScreen::Settings => "Settings",
            AppScreen::Decks => "Decks",
            AppScreen::DeckEditor => "Deck editor",
            AppScreen::CardBrowser => "Cards",
            AppScreen::NewGame => "Play vs Computer",
            AppScreen::Learn => "Learn to Play",
            AppScreen::Online => "Play Online",
            AppScreen::Game => "Game",
            AppScreen::Replay => "Replay",
            AppScreen::About => "About",
        }
    }
}
