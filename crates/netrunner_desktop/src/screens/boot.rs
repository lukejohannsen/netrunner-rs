//! The first frame: build the client core, start the font loading, put a
//! camera up, and go to the menu.
//!
//! A test inserts its own `ClientCore` before the first update, pointed
//! at a temp directory; boot keeps one it finds and builds one only when
//! there is none, so the test never touches the developer's files.

use bevy::prelude::*;

use crate::core::{ClientCore, Notices, TokioRuntime};
use crate::nav::Navigate;
use crate::screens::AppScreen;
use crate::theme::{Theme, FONT_PATH, SYMBOL_FONT_PATH};

pub struct BootPlugin;

impl Plugin for BootPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppScreen>().add_systems(OnEnter(AppScreen::Boot), boot);
    }
}

fn boot(world: &mut World) {
    if !world.contains_resource::<ClientCore>() {
        let (core, notices) = ClientCore::load();
        world.resource_mut::<Notices>().0.extend(notices);
        world.insert_resource(core);
    }
    if !world.contains_resource::<TokioRuntime>() {
        match TokioRuntime::new() {
            Ok(runtime) => world.insert_resource(runtime),
            Err(error) => world.resource_mut::<Notices>().push(error),
        }
    }
    // Only where fonts exist as assets: a headless test has no text plugin
    // and so no `Assets<Font>`, and asking the server for a handle to an
    // unregistered asset type is a panic, not a pending load.
    if world.contains_resource::<Assets<Font>>() {
        let font = world.resource::<AssetServer>().load(FONT_PATH);
        let symbols = world.resource::<AssetServer>().load(SYMBOL_FONT_PATH);
        let mut theme = world.resource_mut::<Theme>();
        theme.font = Some(font);
        theme.symbol_font = Some(symbols);
    }
    world.spawn((Name::new("Camera"), Camera2d));
    let first = world.get_resource::<crate::dev::Dev>().map_or(AppScreen::MainMenu, |dev| dev.first_screen());
    world.write_message(Navigate(first));
}
