//! The screens later phases build, as a heading and a way back, so every
//! menu entry leads somewhere and the navigation is tested end to end
//! before any of them exists. Each stub is replaced by its own plugin in
//! the phase that builds it (Phase 7 roadmap): the deck builder, the
//! lessons and the online lobby. The card browser was the first to go
//! (§2); the new-game form and the board went in §3, the replay in §4ah.

use bevy::prelude::*;

use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::{self, Pressed};

pub struct StubScreensPlugin;

const STUBS: [AppScreen; 4] = [AppScreen::Decks, AppScreen::DeckEditor, AppScreen::Learn, AppScreen::Online];

impl Plugin for StubScreensPlugin {
    fn build(&self, app: &mut App) {
        for screen in STUBS {
            app.add_systems(OnEnter(screen), move |commands: Commands, theme: Res<Theme>| spawn(commands, theme, screen));
        }
        app.add_systems(Update, back.run_if(|screen: Res<State<AppScreen>>| STUBS.contains(screen.get())));
    }
}

#[derive(Component)]
struct BackButton;

fn spawn(mut commands: Commands, theme: Res<Theme>, screen: AppScreen) {
    commands.spawn((screen_root(screen, &theme), children![
        widgets::heading(&theme, screen.title()),
        widgets::dim(&theme, "Not built yet — this screen arrives in a later phase (see docs/roadmap/phase-7-desktop-client.md)."),
        widgets::button(&theme, "Back", Val::Auto, BackButton),
    ]));
}

fn back(mut pressed: MessageReader<Pressed>, marks: Query<(), With<BackButton>>, mut navigate: MessageWriter<Navigate>) {
    for Pressed(entity) in pressed.read() {
        if marks.get(*entity).is_ok() {
            navigate.write(Navigate(AppScreen::MainMenu));
        }
    }
}
