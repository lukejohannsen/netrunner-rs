//! The main menu: every way to start, and a way out.
//!
//! The entries mirror the terminal client's menu (Phase 6) plus the two
//! screens only a graphical client has a reason for — the card browser
//! and the profile — so a player who knows one client knows the other.

use bevy::prelude::*;

use crate::core::{ClientCore, Notices};
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::{self, ButtonKind, Pressed};

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::MainMenu), spawn).add_systems(Update, choose.run_if(in_state(AppScreen::MainMenu)));
    }
}

/// One menu entry, on the button that opens it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    PlayComputer,
    Online,
    Learn,
    Decks,
    Cards,
    Replays,
    Profile,
    Settings,
    About,
    Quit,
}

impl Entry {
    pub const ALL: [Entry; 10] = [
        Entry::PlayComputer,
        Entry::Online,
        Entry::Learn,
        Entry::Decks,
        Entry::Cards,
        Entry::Replays,
        Entry::Profile,
        Entry::Settings,
        Entry::About,
        Entry::Quit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Entry::PlayComputer => "Play vs Computer",
            Entry::Online => "Play Online",
            Entry::Learn => "Learn to Play",
            Entry::Decks => "Decks",
            Entry::Cards => "Cards",
            Entry::Replays => "Replays",
            Entry::Profile => "Profile",
            Entry::Settings => "Settings",
            Entry::About => "About",
            Entry::Quit => "Quit",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Entry::PlayComputer => "A casual game against a rung of the ladder, in the style your deck chooses",
            Entry::Online => "Host a game, join one by address, or watch",
            Entry::Learn => "Both lesson tracks and the starter games",
            Entry::Decks => "Build, copy and edit decks; the same files the terminal client plays",
            Entry::Cards => "Every card, with the printed text and how the engine reads it",
            Entry::Replays => "Step through a saved game on the board, from either chair",
            Entry::Profile => "Your name, your record against the ladder, and where your files live",
            Entry::Settings => "Name, format, animation, sound, card images",
            Entry::About => "Credits and licences: whose fonts, symbols, cards and art are in the client",
            Entry::Quit => "",
        }
    }

    /// The game is what the menu is for, so it is the one filled button;
    /// the way out is the one that stays quiet.
    fn kind(self) -> ButtonKind {
        match self {
            Entry::PlayComputer => ButtonKind::Primary,
            Entry::Quit => ButtonKind::Quiet,
            _ => ButtonKind::Secondary,
        }
    }

    /// Where the entry leads; `None` quits.
    pub fn screen(self) -> Option<AppScreen> {
        match self {
            Entry::PlayComputer => Some(AppScreen::NewGame),
            Entry::Online => Some(AppScreen::Online),
            Entry::Learn => Some(AppScreen::Learn),
            Entry::Decks => Some(AppScreen::Decks),
            Entry::Cards => Some(AppScreen::CardBrowser),
            Entry::Replays => Some(AppScreen::Replay),
            Entry::Profile => Some(AppScreen::Profile),
            Entry::Settings => Some(AppScreen::Settings),
            Entry::About => Some(AppScreen::About),
            Entry::Quit => None,
        }
    }
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, notices: Res<Notices>) {
    let format = netrunner_client::settings::format_name(core.settings.format.unwrap_or(netrunner_core::format::NsgFormat::Startup));
    commands.spawn((screen_root(AppScreen::MainMenu, &theme), children![
        (Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(16), margin: UiRect::vertical(Val::Auto), ..default() }, children![
            (Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(6), margin: UiRect::bottom(px(8)), ..default() }, children![
                widgets::title(&theme, "NETRUNNER"),
                widgets::dim(&theme, format!("Playing as {} · {format} format", core.player_name())),
            ]),
            (widgets::roomy_panel(&theme, px(780)), Children::spawn(SpawnIter(Entry::ALL.into_iter().map({
                let theme = theme.clone();
                move |entry| {
                    (widgets::row(20.0), children![
                        widgets::styled_button(&theme, entry.kind(), entry.label(), px(230), entry),
                        // The blurb takes whatever the row has left and wraps
                        // there, rather than pushing on the button.
                        (widgets::dim(&theme, entry.blurb()), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }),
                    ])
                }
            })))),
            widgets::notice(&theme, notices.latest().unwrap_or(""), ()),
        ]),
    ]));
}

fn choose(mut pressed: MessageReader<Pressed>, entries: Query<&Entry>, mut navigate: MessageWriter<Navigate>, mut exit: MessageWriter<AppExit>) {
    for Pressed(entity) in pressed.read() {
        let Ok(entry) = entries.get(*entity) else { continue };
        match entry.screen() {
            Some(screen) => {
                navigate.write(Navigate(screen));
            }
            None => {
                exit.write(AppExit::Success);
            }
        }
    }
}
