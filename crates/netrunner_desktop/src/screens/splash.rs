//! The title card: what a person sees while the fonts load, for a moment,
//! and then the main menu.
//!
//! **Skipped by anything.** A key or a click goes straight to the menu —
//! a splash that makes somebody wait who has seen it a hundred times is
//! the failure — and otherwise it holds for [`SPLASH_MIN`] and until the
//! fonts have loaded, so the menu never draws a frame in the fallback
//! face. [`SPLASH_MAX`] is the other bound: a font that never arrives
//! costs a few seconds, not the client.
//!
//! **Never shown to a window nobody can see.** Boot comes here only when
//! there is a primary window and no dev hook named a screen, so the
//! headless tests and a `NETRUNNER_SCREEN` screenshot land where they
//! always did.
//!
//! **Three tiers, like everything else.** The picture behind it is the
//! `splash` backdrop slot (`crate::backdrop`), and the mark over it is
//! `backdrops/splash-logo.png`, fitted to a box in the middle. With no
//! logo the mark is the name in the theme's face with the accent under
//! it — the drawn tier, which is the fallback and, under basic graphics,
//! all there is.

use std::time::Duration;

use bevy::prelude::*;

use crate::core::ClientCore;
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::Theme;

/// The shortest a splash stays up without being skipped.
pub const SPLASH_MIN: Duration = Duration::from_millis(1500);
/// The longest it waits for the fonts.
pub const SPLASH_MAX: Duration = Duration::from_secs(5);

/// The logo file, beside the backdrops.
pub const LOGO_FILE: &str = "backdrops/splash-logo.png";

/// The box the logo is fitted into, in logical pixels.
const LOGO_BOX: Vec2 = Vec2::new(720.0, 360.0);

pub struct SplashPlugin;

impl Plugin for SplashPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::Splash), spawn).add_systems(Update, advance.run_if(in_state(AppScreen::Splash)));
    }
}

/// How long the splash has been up.
#[derive(Resource, Default)]
struct Shown(Duration);

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, images: Option<ResMut<Assets<Image>>>) {
    commands.insert_resource(Shown::default());
    let logo = images.filter(|_| !core.settings.desktop.basic_graphics).and_then(|mut images| {
        let image = crate::card_images::decode(&crate::assets::read(LOGO_FILE)?, "png")?;
        let size = image.size().as_vec2();
        Some((images.add(image), size))
    });
    let mut root = commands.spawn(screen_root(AppScreen::Splash, theme.background));
    root.entry::<Node>().and_modify(|mut node| node.justify_content = JustifyContent::Center);
    root.with_children(|parent| match logo {
        Some((image, size)) => {
            let fitted = size * (LOGO_BOX / size).min_element().min(1.0);
            parent.spawn((ImageNode::new(image), Node { width: px(fitted.x), height: px(fitted.y), ..default() }));
        }
        None => {
            parent.spawn((Text::new("NETRUNNER"), theme.font(72.0), TextColor(theme.text)));
            parent.spawn((Node { width: px(320), height: px(4), ..default() }, BackgroundColor(theme.accent)));
        }
    });
}

/// Moves on to the menu on any key or click, or once the splash has been
/// up long enough and the fonts are in.
fn advance(
    time: Res<Time>,
    mut shown: ResMut<Shown>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    theme: Res<Theme>,
    server: Option<Res<AssetServer>>,
    mut navigate: MessageWriter<Navigate>,
) {
    shown.0 += time.delta();
    let skipped = keys.is_some_and(|keys| keys.get_just_pressed().next().is_some()) || mouse.is_some_and(|mouse| mouse.get_just_pressed().next().is_some());
    let fonts_in = match (&server, &theme.font) {
        (Some(server), Some(font)) => server.is_loaded_with_dependencies(font),
        _ => true,
    };
    if skipped || (shown.0 >= SPLASH_MIN && fonts_in) || shown.0 >= SPLASH_MAX {
        navigate.write(Navigate(AppScreen::MainMenu));
    }
}
