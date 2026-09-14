//! Moving between screens.
//!
//! A screen never sets the next state itself; it writes a [`Navigate`]
//! and this module applies it, so that "what Escape does" and "what a
//! Back button does" are one rule in one place. A screen that must
//! intercept Escape — a text field being edited, a modal — sets
//! [`InputCaptured`] for the frame, and the rule stands down.

use bevy::prelude::*;

use crate::screens::AppScreen;

pub struct NavPlugin;

impl Plugin for NavPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Navigate>()
            .init_resource::<InputCaptured>()
            .add_systems(Update, (escape_goes_back, apply_navigation).chain());
    }
}

/// "Go to this screen." Applied at the end of the frame it is written in.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Navigate(pub AppScreen);

/// Set by a screen that is consuming keys itself this frame, so Escape
/// does not also leave the screen. Cleared every frame by whoever set it.
#[derive(Resource, Default, Debug)]
pub struct InputCaptured(pub bool);

/// The root every screen spawns under: fills the window, stacks
/// vertically, and is despawned with its whole subtree when the screen
/// is left. A screen that spawned anything outside this root would leak
/// it into the next screen, which is why the helper exists.
pub fn screen_root(screen: AppScreen, background: Color) -> impl Bundle + use<> {
    (
        Name::new(format!("{screen:?}")),
        DespawnOnExit(screen),
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding: UiRect::all(px(24)),
            row_gap: px(16),
            ..default()
        },
        BackgroundColor(background),
    )
}

/// Where Escape leads from each screen. The main menu is where it stops:
/// quitting is an explicit entry, never a stray key.
pub fn back_from(screen: AppScreen) -> Option<AppScreen> {
    match screen {
        AppScreen::Boot | AppScreen::MainMenu => None,
        AppScreen::DeckEditor => Some(AppScreen::Decks),
        AppScreen::Game | AppScreen::Replay => Some(AppScreen::MainMenu),
        _ => Some(AppScreen::MainMenu),
    }
}

fn escape_goes_back(
    keys: Res<ButtonInput<KeyCode>>,
    captured: Res<InputCaptured>,
    screen: Res<State<AppScreen>>,
    mut navigate: MessageWriter<Navigate>,
) {
    if captured.0 || !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if let Some(back) = back_from(*screen.get()) {
        navigate.write(Navigate(back));
    }
}

fn apply_navigation(mut messages: MessageReader<Navigate>, mut next: ResMut<NextState<AppScreen>>) {
    if let Some(Navigate(screen)) = messages.read().last() {
        next.set(*screen);
    }
}
