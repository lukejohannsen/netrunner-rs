//! The main menu: every way to start, and a way out.
//!
//! The entries mirror the terminal client's menu (Phase 6) plus the two
//! screens only a graphical client has a reason for — the card browser
//! and the profile — so a player who knows one client knows the other.

use bevy::ecs::spawn::SpawnWith;
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
            Entry::Online => "Host a game, join one by address or ticket, or watch one",
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

/// The box the wordmark is fitted into on a window with room for it.
const LOGO_BOX: Vec2 = Vec2::new(560.0, 120.0);

/// The shortest the wordmark is drawn. On a short window it gives way to
/// the entries — a 720-high window had no height to spare even for the
/// text title — down to this, rather than pushing Quit off the window.
const LOGO_MIN_HEIGHT: f32 = 40.0;

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, notices: Res<Notices>, images: Option<ResMut<Assets<Image>>>) {
    let format = netrunner_client::settings::format_name(core.settings.format.unwrap_or(netrunner_core::format::NsgFormat::Startup));
    let logo = widgets::logo(images, core.settings.desktop.basic_graphics, LOGO_BOX);
    let playing_as = format!("Playing as {} · {format} format", core.player_name());
    // The wordmark and the line under it are the column's own children,
    // not a box of their own: the column is held to the window, and a box
    // round them would have had to shrink with the wordmark and let the
    // line under it spill onto the panel.
    let heading = SpawnWith({
        let theme = theme.clone();
        move |parent: &mut ChildSpawner| {
            let tucked = UiRect::bottom(px(-10));
            match logo {
                // Sized by its height, with the width following, so it
                // shrinks as one picture; the panel under it never shrinks
                // below its entries, so the wordmark is what gives way.
                Some((image, size)) => parent.spawn((
                    image,
                    Node {
                        height: px(size.y),
                        min_height: px(LOGO_MIN_HEIGHT),
                        aspect_ratio: Some(size.x / size.y),
                        flex_shrink: 1.0,
                        margin: tucked,
                        ..default()
                    },
                )),
                None => parent.spawn((widgets::title(&theme, "NETRUNNER"), widgets::title_shadow(), Node { margin: tucked, ..default() })),
            };
            parent.spawn((widgets::dim(&theme, playing_as), Node { margin: UiRect::bottom(px(8)), ..default() }));
        }
    });
    let panel = (
        widgets::roomy_panel(&theme, px(780)),
        Children::spawn(SpawnIter(Entry::ALL.into_iter().map({
            let theme = theme.clone();
            move |entry| {
                (widgets::row(20.0), children![
                    widgets::styled_button(&theme, entry.kind(), entry.label(), px(230), entry),
                    // The blurb takes whatever the row has left and wraps
                    // there, rather than pushing on the button.
                    (widgets::dim(&theme, entry.blurb()), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }),
                ])
            }
        }))),
    );
    let notice = widgets::notice(&theme, notices.latest().unwrap_or(""), ());
    commands.spawn((screen_root(AppScreen::MainMenu, &theme), children![(
        Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(16), margin: UiRect::vertical(Val::Auto), max_height: percent(100), ..default() },
        Children::spawn((heading, Spawn(panel), Spawn(notice))),
    )]));
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
