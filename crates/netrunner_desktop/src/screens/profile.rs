//! Who the player is, as the files say: the name games are recorded
//! under, their record against the ladder on both chairs, how many card
//! images are cached, and where everything lives.
//!
//! The record lines are `netrunner_client::record::standing_lines` — the
//! same lines `netrunner_cli record` prints — so the two clients cannot
//! disagree about a number. It is a record and not a rating: a rating is
//! a server's to keep, and this screen is where one a server reports will
//! be shown when there is one (Phase 4 §5).

use bevy::prelude::*;

use crate::core::ClientCore;
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::{self, Pressed};

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::Profile), spawn).add_systems(Update, buttons.run_if(in_state(AppScreen::Profile)));
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileButton {
    Back,
    Settings,
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>) {
    let player = core.player_name();
    let standing = match &core.record_path {
        Some(path) => netrunner_client::record::standing_lines(path, &player).unwrap_or_else(|error| vec![format!("Could not read the record: {error}")]),
        None => vec!["No record file: the OS has no data directory".to_string()],
    };
    let codes: Vec<_> = core.registry.iter().filter_map(|card| card.numeric_id).collect();
    let cached = core.images.cached_count(&codes);
    let path = |p: &Option<std::path::PathBuf>| p.as_ref().map_or_else(|| "unavailable".to_string(), |p| p.display().to_string());
    let paths = [
        format!("Settings   {}", path(&core.settings_path)),
        format!("Decks      {}", path(&core.decks_dir)),
        format!("Record     {}", path(&core.record_path)),
        format!("Images     {}", core.images.dir().display()),
    ];
    commands.spawn((screen_root(AppScreen::Profile, &theme), children![
        widgets::heading(&theme, AppScreen::Profile.title()),
        (widgets::roomy_panel(&theme, px(720)), children![
            widgets::label(&theme, player),
            widgets::dim(&theme, format!("{cached} of {} card images cached", codes.len())),
        ]),
        (widgets::roomy_panel(&theme, px(720)), Children::spawn(SpawnIter(standing.into_iter().map({
            let theme = theme.clone();
            move |line| widgets::label(&theme, line)
        })))),
        (widgets::roomy_panel(&theme, px(720)), Children::spawn(SpawnIter(paths.into_iter().map({
            let theme = theme.clone();
            move |line| widgets::dim(&theme, line)
        })))),
        (widgets::row(12.0), children![
            widgets::button(&theme, "Settings", Val::Auto, ProfileButton::Settings),
            widgets::button(&theme, "Back", Val::Auto, ProfileButton::Back),
        ]),
    ]));
}

fn buttons(mut pressed: MessageReader<Pressed>, marks: Query<&ProfileButton>, mut navigate: MessageWriter<Navigate>) {
    for Pressed(entity) in pressed.read() {
        match marks.get(*entity) {
            Ok(ProfileButton::Back) => {
                navigate.write(Navigate(AppScreen::MainMenu));
            }
            Ok(ProfileButton::Settings) => {
                navigate.write(Navigate(AppScreen::Settings));
            }
            Err(_) => {}
        }
    }
}
