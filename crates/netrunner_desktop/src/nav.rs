//! Moving between screens.
//!
//! A screen never sets the next state itself; it writes a [`Navigate`]
//! and this module applies it, so that "what Escape does" and "what a
//! Back button does" are one rule in one place. A widget that must
//! intercept Escape — a text field being edited, an open drop-down —
//! sets [`InputCaptured`] for the frame from inside the [`Captures`]
//! set, and the rule stands down.

use bevy::prelude::*;

use crate::theme::Theme;

use crate::screens::AppScreen;

pub struct NavPlugin;

impl Plugin for NavPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Navigate>()
            .init_resource::<InputCaptured>()
            .configure_sets(Update, Captures.after(reset_captured).before(escape_goes_back))
            .add_systems(Update, (reset_captured, escape_goes_back, apply_navigation).chain());
    }
}

/// Where the systems that may capture input run: after the flag is
/// reset for the frame and before Escape is read, so a capture is seen
/// the frame it is set whatever order the widgets' systems land in.
/// Two widgets that both capture simply both set the flag; the first
/// to run also sees whether an earlier one already has.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Captures;

/// "Go to this screen." Applied at the end of the frame it is written in.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Navigate(pub AppScreen);

/// Set by a widget that is consuming keys itself this frame, so Escape
/// does not also leave the screen. Reset at the start of every frame
/// here; set only from inside [`Captures`].
#[derive(Resource, Default, Debug)]
pub struct InputCaptured(pub bool);

fn reset_captured(mut captured: ResMut<InputCaptured>) {
    captured.0 = false;
}

/// The root every screen spawns under: fills the window, stacks
/// vertically, and is despawned with its whole subtree when the screen
/// is left. A screen that spawned anything outside this root would leak
/// it into the next screen, which is why the helper exists.
///
/// Every screen but the board is drawn on [`drawn_backdrop`]: the menus
/// are glass, and glass over a flat colour is just a flat colour.
pub fn screen_root(screen: AppScreen, theme: &Theme) -> impl Bundle + use<> {
    let gradient = if screen == AppScreen::Game { BackgroundGradient::default() } else { drawn_backdrop(theme) };
    (
        Name::new(format!("{screen:?}")),
        DespawnOnExit(screen),
        // Every screen's slot for a picture behind it, dressed by
        // `backdrop` when one is installed: a screen gets it by being
        // built on this root, with no code of its own.
        crate::backdrop::ScreenBackdrop(screen),
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding: UiRect::all(px(24)),
            row_gap: px(16),
            ..default()
        },
        BackgroundColor(theme.background),
        gradient,
    )
}

/// The drawn tier of every menu screen's backdrop: a steel-teal sky
/// falling to a violet night, with two soft blooms of light, a blue one
/// high on the left and a violet one low on the right, for the glass to
/// catch. A picture installed in
/// the screen's slot draws over it (`backdrop::dress`); this is what a
/// screen with none shows, and costs one quad, so Basic graphics keeps
/// it too.
pub fn drawn_backdrop(theme: &Theme) -> BackgroundGradient {
    let bloom = |position: UiPosition, colour: Color| {
        Gradient::Radial(RadialGradient::new(
            position,
            RadialGradientShape::FarthestSide,
            vec![ColorStop::percent(colour, 0.0), ColorStop::percent(colour.with_alpha(0.0), 100.0)],
        ))
    };
    BackgroundGradient(vec![
        Gradient::Linear(LinearGradient::to_bottom(vec![ColorStop::auto(theme.backdrop_top), ColorStop::auto(theme.backdrop_bottom)])),
        bloom(UiPosition::anchor(Vec2::new(-0.3, -0.4)), theme.backdrop_bloom),
        bloom(UiPosition::anchor(Vec2::new(0.35, 0.45)), theme.backdrop_bloom_violet),
    ])
}

/// Where Escape leads from each screen. The main menu is where it stops:
/// quitting is an explicit entry, never a stray key.
pub fn back_from(screen: AppScreen) -> Option<AppScreen> {
    match screen {
        AppScreen::Boot | AppScreen::Splash | AppScreen::MainMenu => None,
        AppScreen::DeckEditor => Some(AppScreen::Decks),
        AppScreen::Guide => Some(AppScreen::Learn),
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
