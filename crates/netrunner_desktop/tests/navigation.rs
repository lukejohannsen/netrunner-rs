//! The client, without a window: the plugin set `main` runs, under
//! `MinimalPlugins`, driven by messages and key presses.
//!
//! What this pins is the shape every screen shares — boot lands on the
//! menu, a `Navigate` enters a screen under a root that `DespawnOnExit`
//! takes down, Escape leads back — so a screen that breaks the pattern
//! fails here before it fails on a desk. The `ClientCore` is pointed at
//! a temp directory so the test never reads the developer's own files.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use netrunner_desktop::core::ClientCore;
use netrunner_desktop::nav::Navigate;
use netrunner_desktop::{AppScreen, NetrunnerDesktopPlugins};

fn headless_client() -> (App, std::path::PathBuf) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("netrunner_desktop_test_{}_{n}", std::process::id()));
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin));
    app.insert_resource(ClientCore::in_dir(dir.clone()));
    app.add_plugins(NetrunnerDesktopPlugins);
    (app, dir)
}

/// A key tap the way winit delivers one: a press and a release, as
/// messages the input plugin turns into `ButtonInput` state in
/// `PreUpdate`, so `just_pressed` is true for exactly the next `Update`
/// and the key is not still held when the next tap comes.
fn press(app: &mut App, key_code: KeyCode, logical_key: Key) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: logical_key.clone(),
            state,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
    }
}

fn screen(app: &App) -> AppScreen {
    *app.world().resource::<State<AppScreen>>().get()
}

fn roots(app: &mut App, screen: AppScreen) -> usize {
    app.world_mut().query::<&DespawnOnExit<AppScreen>>().iter(app.world()).filter(|root| root.0 == screen).count()
}

#[test]
fn boot_lands_on_the_main_menu_with_its_root_spawned() {
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
    assert_eq!(roots(&mut app, AppScreen::MainMenu), 1);
}

#[test]
fn navigating_swaps_the_screen_root_and_escape_leads_back() {
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::Profile));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Profile);
    assert_eq!(roots(&mut app, AppScreen::Profile), 1, "the profile spawned under its root");
    assert_eq!(roots(&mut app, AppScreen::MainMenu), 0, "the menu's root was despawned on exit");

    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
    assert_eq!(roots(&mut app, AppScreen::Profile), 0);
    assert_eq!(roots(&mut app, AppScreen::MainMenu), 1);
}

/// Every screen a menu entry leads to exists, so no entry is a dead end.
#[test]
fn every_screen_can_be_entered_and_left() {
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    for target in [
        AppScreen::Profile,
        AppScreen::Settings,
        AppScreen::Decks,
        AppScreen::DeckEditor,
        AppScreen::CardBrowser,
        AppScreen::NewGame,
        AppScreen::Learn,
        AppScreen::Online,
        AppScreen::Game,
        AppScreen::Replay,
    ] {
        app.world_mut().write_message(Navigate(target));
        app.update();
        app.update();
        assert_eq!(screen(&app), target);
        assert_eq!(roots(&mut app, target), 1, "{target:?} spawned under one root");
        app.world_mut().write_message(Navigate(AppScreen::MainMenu));
        app.update();
        app.update();
        assert_eq!(roots(&mut app, target), 0, "{target:?} was taken down");
    }
}

/// Escape on the main menu goes nowhere: quitting is an entry, never a
/// stray key.
#[test]
fn escape_on_the_main_menu_stays() {
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
}

/// The settings screen's name field: Edit opens it, typing fills it,
/// Escape cancels it *without* leaving the screen (the field captures
/// input), and a second Escape then leaves.
#[test]
fn a_text_field_captures_escape_until_it_closes() {
    use netrunner_desktop::widgets::text_field::TextField;
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::Settings));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Settings);
    // Press the Edit button the way a pointer would: find the themed
    // button whose label reads "Edit" and mark it pressed.
    let mut buttons = app.world_mut().query::<(Entity, &netrunner_desktop::widgets::Themed, &Children)>();
    let mut texts = app.world_mut().query::<&Text>();
    let edit = buttons
        .iter(app.world())
        .find(|(_, _, children)| children.iter().any(|child| texts.get(app.world(), child).is_ok_and(|text| text.0 == "Edit")))
        .map(|(entity, _, _)| entity)
        .expect("the settings screen has an Edit button");
    app.world_mut().entity_mut(edit).insert(Interaction::Pressed);
    app.update();
    app.update();
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).count(), 1, "Edit opened the name field");
    press(&mut app, KeyCode::KeyC, Key::Character("c".into()));
    app.update();
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).next().unwrap().text, "c");
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Settings, "Escape cancelled the edit, not the screen");
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).count(), 0);
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
}
