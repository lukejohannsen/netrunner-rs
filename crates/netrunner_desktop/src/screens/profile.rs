//! Who the player is, as the files say: the name games are recorded
//! under, their record against the ladder on both chairs, how many card
//! images are cached, and where everything lives.
//!
//! The record lines are `netrunner_client::record::standing_lines` — the
//! same lines `netrunner_cli record` prints — so the two clients cannot
//! disagree about a number. It is a record and not a rating: a rating is
//! a server's to keep.
//!
//! **Who the player is to a server, and where they stand there** (Phase 4
//! §5 stage d). The key's fingerprint and the two facts about its file
//! (`identity::key_lines`), then the standing at every server remembered
//! in the list of servers (`servers`) — the ones that keep their key, which are
//! the ones that rate. Each is fetched on entering the screen
//! (`remote::profile`, on the runtime) and drawn as it arrives, never
//! stored: the server's book is the truth.

use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use bevy::prelude::*;

use netrunner_client::identity;
use netrunner_client::servers::ServerBook;
use netrunner_client::remote;

use crate::audio::{ButtonSound, Sfx};
use crate::core::{ClientCore, TokioRuntime};
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::{self, Pressed};

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::Profile), spawn).add_systems(Update, (buttons, standings_arrive).run_if(in_state(AppScreen::Profile)));
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileButton {
    Back,
    Settings,
}

/// Where the standings are drawn as they arrive.
#[derive(Component)]
struct StandingPanel;

/// Each remembered server's answer, as lines: its address, then its
/// standing or why it could not be had.
#[derive(Resource)]
struct Fetching(Mutex<mpsc::Receiver<Vec<String>>>);

/// How long one server is given to answer.
const STANDING_TIMEOUT: Duration = Duration::from_secs(10);

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, runtime: Option<Res<TokioRuntime>>) {
    let player = core.player_name();
    let credentials = core.credentials();
    let (key, servers) = match &credentials {
        Ok(Some(credentials)) => {
            // Every server whose key was proved: the ones that keep one,
            // which are the ones that keep ratings.
            let book = credentials.servers().and_then(|path| ServerBook::load(&path).ok()).unwrap_or_default();
            let kept = book.servers.into_iter().filter(|server| server.key.is_some()).map(|server| (server.name, server.address)).collect::<Vec<_>>();
            (identity::key_lines(credentials), kept)
        }
        Ok(None) => (vec!["No key: the OS has no data directory, so games online are unrated".to_string()], Vec::new()),
        Err(error) => (vec![format!("Your key: {error}")], Vec::new()),
    };
    let first = if servers.is_empty() {
        "Rated nowhere yet. A server that keeps ratings is remembered the first time you play there, and your standing there is shown here.".to_string()
    } else {
        format!("Your standing at {} server{}:", servers.len(), if servers.len() == 1 { "" } else { "s" })
    };
    if let (Ok(Some(credentials)), Some(runtime)) = (credentials, runtime) {
        let (tx, rx) = mpsc::channel();
        for (name, address) in servers {
            let (tx, credentials) = (tx.clone(), credentials.clone());
            runtime.0.spawn(async move {
                let answer = tokio::time::timeout(STANDING_TIMEOUT, remote::profile(&address, &credentials)).await;
                let mut lines = vec![if name == address { format!("At {address}") } else { format!("At {name} ({address})") }];
                match answer {
                    Ok(Ok(profile)) => lines.extend(identity::profile_lines(profile.as_ref())),
                    Ok(Err(error)) => lines.push(error.to_string()),
                    Err(_) => lines.push(format!("No answer within {}s", STANDING_TIMEOUT.as_secs())),
                }
                let _ = tx.send(lines);
            });
        }
        commands.insert_resource(Fetching(Mutex::new(rx)));
    }
    let standing = match &core.record_path {
        Some(path) => netrunner_client::record::standing_lines(path, &player).unwrap_or_else(|error| vec![format!("Could not read the record: {error}")]),
        None => vec!["No record file: the OS has no data directory".to_string()],
    };
    let codes = crate::downloads::every_printing();
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
        (widgets::roomy_panel(&theme, px(720)), Children::spawn(SpawnIter(key.into_iter().map({
            let theme = theme.clone();
            move |line| (widgets::dim(&theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary))
        })))),
        (widgets::roomy_panel(&theme, px(720)), StandingPanel, children![(widgets::dim(&theme, first), TextLayout::new(Justify::Left, LineBreak::WordBoundary))]),
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
            widgets::button(&theme, "Back", Val::Auto, (ProfileButton::Back, ButtonSound(Sfx::Back))),
        ]),
    ]));
}

/// Draws each server's standing as its answer arrives.
fn standings_arrive(mut commands: Commands, theme: Res<Theme>, fetching: Option<Res<Fetching>>, panel: Query<Entity, With<StandingPanel>>) {
    let (Some(fetching), Ok(panel)) = (fetching, panel.single()) else { return };
    let Ok(rx) = fetching.0.lock() else { return };
    while let Ok(lines) = rx.try_recv() {
        commands.entity(panel).with_children(|panel| {
            for (index, line) in lines.into_iter().enumerate() {
                if index == 0 {
                    panel.spawn(widgets::label(&theme, line));
                } else {
                    panel.spawn((widgets::dim(&theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
            }
        });
    }
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
