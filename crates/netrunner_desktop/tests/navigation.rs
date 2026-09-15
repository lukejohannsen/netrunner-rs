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

/// The browser's thumbs in grid order — their parent's child order,
/// because a query iterates by table and an outlined thumb sits in a
/// different table from the rest.
fn thumbs_in_order(app: &mut App) -> Vec<(Entity, netrunner_core::card::CardId)> {
    use netrunner_desktop::screens::card_browser::FaceButton;
    let mut thumbs: Vec<(Entity, netrunner_core::card::CardId, Entity)> =
        app.world_mut().query::<(Entity, &FaceButton, &ChildOf)>().iter(app.world()).map(|(e, f, p)| (e, f.0, p.parent())).collect();
    let order = |app: &App, entity: Entity, parent: Entity| app.world().get::<Children>(parent).map_or(usize::MAX, |c| c.iter().position(|child| child == entity).unwrap_or(usize::MAX));
    thumbs.sort_by_key(|(e, _, p)| order(app, *e, *p));
    thumbs.into_iter().map(|(e, code, _)| (e, code)).collect()
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

/// The card browser: every printing is a face, a face opens in the
/// inspector, the arrows move the open card, typing narrows the grid,
/// and Escape clears, closes, then leaves — three presses, because the
/// search field captures the first two.
#[test]
fn the_card_browser_lists_faces_filters_as_typed_and_escapes_in_three() {
    use netrunner_desktop::screens::card_browser::FaceButton;
    use netrunner_desktop::widgets::text_field::TextField;
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::CardBrowser));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::CardBrowser);
    let faces = |app: &mut App| app.world_mut().query::<&FaceButton>().iter(app.world()).count();
    let all = faces(&mut app);
    assert!(all > 200, "{all} faces: every printing in the catalog");
    let texts = |app: &mut App| app.world_mut().query::<&Text>().iter(app.world()).map(|t| t.0.clone()).collect::<Vec<_>>();
    assert!(texts(&mut app).iter().any(|t| t.starts_with("Cost ")), "the first card is open in the inspector from the start");

    // Press a thumb the way a pointer would: the second, so the
    // inspector visibly changes.
    let (second, code) = thumbs_in_order(&mut app)[1];
    app.world_mut().entity_mut(second).insert(Interaction::Pressed);
    app.update();
    app.update();
    let spans = |app: &mut App| app.world_mut().query::<&TextSpan>().iter(app.world()).map(|t| t.0.clone()).collect::<Vec<_>>();
    assert!(spans(&mut app).iter().any(|t| t.ends_with(&format!("#{:05}", code.0))), "the inspector shows the pressed card's code");
    assert!(texts(&mut app).iter().any(|t| t.starts_with("Legal in ")), "and which formats allow it");
    let outlined = |app: &mut App| app.world_mut().query_filtered::<&FaceButton, With<Outline>>().iter(app.world()).map(|f| f.0).collect::<Vec<_>>();
    assert_eq!(outlined(&mut app), vec![code], "the pressed thumb is the outlined one");

    // The arrows move the open card: Right to the third, Left back, and
    // the outline follows without the grid being respawned.
    let third = thumbs_in_order(&mut app)[2].1;
    press(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
    app.update();
    app.update();
    assert_eq!(outlined(&mut app), vec![third], "Right moved the selection one on");
    assert!(spans(&mut app).iter().any(|t| t.ends_with(&format!("#{:05}", third.0))), "and the inspector followed");
    assert_eq!(second, thumbs_in_order(&mut app)[1].0, "the thumbs were not respawned");
    press(&mut app, KeyCode::ArrowLeft, Key::ArrowLeft);
    app.update();
    app.update();
    assert_eq!(outlined(&mut app), vec![code]);

    // Type into the search field, which is open from the start.
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).count(), 1);
    for c in ["t", "i", "t", "h", "e"] {
        press(&mut app, KeyCode::KeyT, Key::Character(c.into()));
        app.update();
    }
    app.update();
    let narrowed = faces(&mut app);
    assert!(narrowed < all && narrowed >= 1, "{narrowed} of {all} faces match \"tithe\"");

    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::CardBrowser, "the first Escape clears the search");
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).next().unwrap().text, "");
    assert_eq!(faces(&mut app), all, "the grid is whole again");
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::CardBrowser, "the second closes the field");
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).count(), 0);
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu, "the third leaves");
    assert_eq!(roots(&mut app, AppScreen::CardBrowser), 0);
}

/// The filter drop-downs: pressing a head opens its list, choosing an
/// entry narrows the grid and closes the list, and Escape closes an
/// open list before the search field gets it.
#[test]
fn the_card_browser_filters_through_drop_downs_that_escape_closes_first() {
    use netrunner_desktop::screens::card_browser::{FaceButton, Filter};
    use netrunner_desktop::widgets::dropdown::{Dropdown, Head, Item};
    use netrunner_desktop::widgets::text_field::TextField;
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::CardBrowser));
    app.update();
    app.update();
    let faces = |app: &mut App| app.world_mut().query::<&FaceButton>().iter(app.world()).count();
    let all = faces(&mut app);
    let items = |app: &mut App| app.world_mut().query::<&Item>().iter(app.world()).count();
    assert_eq!(items(&mut app), 0, "every list starts closed");
    assert_eq!(app.world_mut().query::<&Filter>().iter(app.world()).count(), 5, "one drop-down per filter");

    let side = app.world_mut().query::<(Entity, &Filter)>().iter(app.world()).find(|(_, f)| **f == Filter::Side).map(|(e, _)| e).unwrap();
    let head = app.world_mut().query::<(Entity, &Head)>().iter(app.world()).find(|(_, h)| h.0 == side).map(|(e, _)| e).unwrap();
    app.world_mut().entity_mut(head).insert(Interaction::Pressed);
    app.update();
    app.update();
    assert_eq!(items(&mut app), 3, "Both, Corp, Runner");
    assert!(app.world_mut().query::<&Dropdown>().iter(app.world()).any(|d| d.open));

    // Type with the list open ("dam", which both sides print), then
    // Escape: the list closes and the search keeps its text.
    for c in ["d", "a", "m"] {
        press(&mut app, KeyCode::KeyD, Key::Character(c.into()));
        app.update();
    }
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(items(&mut app), 0, "Escape closed the list");
    assert_eq!(screen(&app), AppScreen::CardBrowser);
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).next().unwrap().text, "dam", "the field did not also clear");
    let narrowed = faces(&mut app);
    assert!(narrowed < all);

    // Open it again and choose Runner: the grid narrows to the Runner's
    // cards that match, and the list closes.
    let head = app.world_mut().query::<(Entity, &Head)>().iter(app.world()).find(|(_, h)| h.0 == side).map(|(e, _)| e).unwrap();
    app.world_mut().entity_mut(head).insert(Interaction::Pressed);
    app.update();
    app.update();
    let runner = app.world_mut().query::<(Entity, &Item)>().iter(app.world()).find(|(_, i)| i.0 == side && i.1 == 2).map(|(e, _)| e).unwrap();
    app.world_mut().entity_mut(runner).insert(Interaction::Pressed);
    app.update();
    app.update();
    assert_eq!(items(&mut app), 0, "choosing closed the list");
    let runner_faces = faces(&mut app);
    assert!(runner_faces < narrowed && runner_faces > 0, "{runner_faces} Runner faces of {narrowed}");
    let side = app.world_mut().query::<(Entity, &Filter)>().iter(app.world()).find(|(_, f)| **f == Filter::Side).map(|(e, _)| e).unwrap();
    assert_eq!(app.world_mut().query::<&Dropdown>().get(app.world(), side).unwrap().selected, 2, "the respawned drop-down shows Runner");
}
