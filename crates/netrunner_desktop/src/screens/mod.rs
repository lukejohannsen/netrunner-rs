//! The screens, one state and one plugin each.
//!
//! A screen is entered by writing `nav::Navigate(AppScreen::X)`. Its
//! plugin spawns everything under `nav::screen_root` on `OnEnter`, which
//! also takes it all down on exit, and runs its systems under
//! `run_if(in_state(AppScreen::X))`. The set below is the whole client;
//! a screen that is not built yet is a stub (`stubs`) with a heading and
//! a way back, so every menu entry leads somewhere from the first PR.

pub mod boot;
pub mod main_menu;
pub mod profile;
pub mod settings;
pub mod stubs;

use bevy::prelude::*;

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppScreen {
    /// Loads the client core and the theme, then goes to the menu. Never
    /// drawn.
    #[default]
    Boot,
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
}

impl AppScreen {
    /// The heading a screen shows.
    pub fn title(self) -> &'static str {
        match self {
            AppScreen::Boot => "",
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
        }
    }
}
