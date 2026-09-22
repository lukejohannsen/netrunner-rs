//! A saved game, stepped through on the board without a window: the
//! report a game saves opens under Replays, the board it opens on is the
//! board the person saw, the bar and the keys move through it, the other
//! chair is one key away, a click reads a card and never offers an
//! action, and Escape goes back to the list and then the menu.
//!
//! Driven under `MinimalPlugins` like `tests/game.rs`, whose match is real;
//! the replay is not, so once the report is saved nothing here waits on a
//! thread.

use std::time::{Duration, Instant};

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use netrunner_client::board::Control;
use netrunner_client::replay::Replay;
use netrunner_client::start::{Level, StartChoice, DEFAULT_CORP_DECK, DEFAULT_RUNNER_DECK};
use netrunner_core::rules::{GamePhase, Side};
use netrunner_desktop::core::ClientCore;
use netrunner_desktop::models::replay::Step;
use netrunner_desktop::nav::Navigate;
use netrunner_desktop::screens::game::{ActionsMenu, Click, Model, Overlay};
use netrunner_desktop::screens::new_game;
use netrunner_desktop::screens::replay::{ActiveReplay, ReplayClick, ReportRow};
use netrunner_desktop::widgets::Disabled;
use netrunner_desktop::{AppScreen, NetrunnerDesktopPlugins};

fn headless_client() -> (App, std::path::PathBuf) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("netrunner_desktop_replay_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin));
    let mut core = ClientCore::in_dir(dir.clone());
    core.settings.desktop.animation_speed = 0.0;
    app.insert_resource(core);
    app.add_plugins(NetrunnerDesktopPlugins);
    app.update();
    app.update();
    (app, dir)
}

fn screen(app: &App) -> AppScreen {
    *app.world().resource::<State<AppScreen>>().get()
}

fn wait_for(app: &mut App, what: &str, mut done: impl FnMut(&mut App) -> bool) {
    let start = Instant::now();
    while !done(app) {
        assert!(start.elapsed() < Duration::from_secs(5), "waited five seconds for {what}");
        std::thread::sleep(Duration::from_millis(5));
        app.update();
    }
}

fn press_entity(app: &mut App, entity: Entity) {
    app.world_mut().entity_mut(entity).insert(Interaction::Pressed);
    app.update();
    app.update();
}

/// A key held for one frame, as `ButtonInput` sees it.
fn key(app: &mut App, key_code: KeyCode, logical_key: Key) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput { key_code, logical_key: logical_key.clone(), state, text: None, repeat: false, window: Entity::PLACEHOLDER });
        app.update();
    }
    app.update();
}

fn entity_with(app: &mut App, wanted: &Click) -> Option<Entity> {
    let mut q = app.world_mut().query::<(Entity, &Click)>();
    q.iter(app.world()).find(|(_, c)| *c == wanted).map(|(e, _)| e)
}

fn button_labelled(app: &mut App, label: &str) -> Option<Entity> {
    let mut buttons = app.world_mut().query::<(Entity, &Click, &Children)>();
    let mut texts = app.world_mut().query::<&Text>();
    buttons.iter(app.world()).find(|(_, _, children)| children.iter().any(|child| texts.get(app.world(), child).is_ok_and(|text| text.0 == label))).map(|(e, _, _)| e)
}

fn bar_button(app: &mut App, step: Step) -> (Entity, bool) {
    let mut q = app.world_mut().query::<(Entity, &ReplayClick, Has<Disabled>)>();
    q.iter(app.world()).find(|(_, click, _)| click.0 == step).map(|(e, _, disabled)| (e, disabled)).expect("the replay bar has every step")
}

fn replay(app: &App) -> &Replay {
    &app.world().resource::<ActiveReplay>().0
}

fn count<C: Component>(app: &mut App) -> usize {
    app.world_mut().query::<&C>().iter(app.world()).count()
}

/// Plays into the Runner's first turn against the bottom rung, saves a
/// report from the options, and returns the board it was saved over.
fn a_saved_game(app: &mut App) -> netrunner_core::view::ClientView {
    let choice = StartChoice { human: Side::Runner, level: Level::Novice, style: None, corp_deck: DEFAULT_CORP_DECK.to_string(), runner_deck: DEFAULT_RUNNER_DECK.to_string() };
    let active = new_game::start_seeded(app.world().resource::<ClientCore>(), &choice, 1).expect("the default decks start a game");
    app.world_mut().insert_resource(active);
    app.world_mut().write_message(Navigate(AppScreen::Game));
    app.update();
    app.update();
    wait_for(app, "the Runner's turn", |app| {
        let model = &app.world().resource::<Model>().0;
        if !model.awaiting {
            return false;
        }
        if model.view.as_ref().is_some_and(|v| matches!(v.phase, GamePhase::Action(Side::Runner)) && v.paid_ability_window.is_none()) {
            return true;
        }
        // Keep the hand, and pass whenever the Corp's turn asks, through
        // the buttons a person would press.
        let press = match button_labelled(app, "Keep hand") {
            Some(keep) => Some(keep),
            None => entity_with(app, &Click::Control(Control::Continue)).filter(|pass| !app.world().entity(*pass).contains::<Disabled>()),
        };
        if let Some(button) = press {
            press_entity(app, button);
        }
        false
    });
    let gear = entity_with(app, &Click::Options).expect("the gear is on the board");
    press_entity(app, gear);
    let save = entity_with(app, &Click::SaveReport).expect("the options offer a bug report");
    press_entity(app, save);
    app.world().resource::<Model>().0.view.clone().expect("a board")
}

#[test]
fn a_saved_game_opens_on_the_board_it_was_saved_from_and_steps_from_either_chair() {
    let (mut app, dir) = headless_client();
    let saved = a_saved_game(&mut app);

    // "Watch it", beside where the report went, opens it on the board.
    let watch = entity_with(&mut app, &Click::WatchReport).expect("a saved report can be watched");
    press_entity(&mut app, watch);
    wait_for(&mut app, "the replay board", |app| screen(app) == AppScreen::Game && app.world().contains_resource::<ActiveReplay>() && app.world().contains_resource::<Model>());
    let model = &app.world().resource::<Model>().0;
    let at = model.replay.clone().expect("the board is a replay");
    assert_eq!(model.side, Side::Runner, "a report opens from the person's chair");
    assert_eq!(model.view.as_ref(), Some(&saved), "at the moment it was saved");
    assert_eq!(at.cursor, at.len, "which is its end");
    assert!(!model.awaiting, "a replay never awaits");
    assert!(model.actions.entries.is_empty(), "and offers nothing");
    assert!(!model.log.is_empty(), "the log reads to that point");
    assert!(app.world_mut().query::<(&Click, Has<Disabled>)>().iter(app.world()).all(|(click, _)| !matches!(click, Click::Control(_) | Click::Entry(_))), "no control bar and no decisions");
    let (_, disabled) = bar_button(&mut app, Step::Forward(1));
    assert!(disabled, "nothing after the end");

    // Back one on the bar: the board is the record's position before.
    let (back, disabled) = bar_button(&mut app, Step::Back(1));
    assert!(!disabled);
    press_entity(&mut app, back);
    assert_eq!(replay(&app).cursor(), at.len - 1);
    let model = &app.world().resource::<Model>().0;
    assert_eq!(model.view.as_ref(), Some(replay(&app).view()), "the board follows the record");
    assert_eq!(model.log, replay(&app).log(), "and so does the log");
    assert_eq!(model.replay.as_ref().map(|at| at.cursor), Some(at.len - 1));

    // Forward one on the key: played through the pacer, as the match was.
    key(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
    wait_for(&mut app, "the step to be applied", |app| app.world().resource::<Model>().0.view.as_ref() == Some(replay(app).view()));
    assert_eq!(replay(&app).cursor(), at.len);
    assert_eq!(app.world().resource::<Model>().0.log, replay(&app).log(), "the paced step logged what the record says");

    // Home, then the other chair.
    key(&mut app, KeyCode::Home, Key::Home);
    assert_eq!(replay(&app).cursor(), 0);
    key(&mut app, KeyCode::KeyS, Key::Character("s".into()));
    let model = &app.world().resource::<Model>().0;
    assert_eq!(model.side, Side::Corp, "S is the other chair");
    assert_eq!(model.view.as_ref(), Some(replay(&app).view()));
    assert_eq!(replay(&app).view().corp.hq_cards.as_ref().map(Vec::len), Some(5), "the Corp's chair sees its own hand");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_click_on_a_replay_reads_the_card_and_escape_goes_back_through_the_list() {
    let (mut app, dir) = headless_client();
    a_saved_game(&mut app);
    // Leave the game and find the report under Replays, from the menu.
    app.world_mut().write_message(Navigate(AppScreen::MainMenu));
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::Replay));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Replay);
    let row = app.world_mut().query_filtered::<Entity, With<ReportRow>>().iter(app.world()).next().expect("the report is listed");
    assert_eq!(count::<ReportRow>(&mut app), 1);
    press_entity(&mut app, row);
    wait_for(&mut app, "the replay board", |app| screen(app) == AppScreen::Game && app.world().contains_resource::<Model>());

    // A primary click on a card reads it: a sheet, never a menu.
    let target = app
        .world_mut()
        .query::<(Entity, &Click)>()
        .iter(app.world())
        .find(|(_, click)| matches!(click, Click::Target(netrunner_client::board::Target::Pile(_))))
        .map(|(entity, _)| entity)
        .expect("a pile is on the board");
    press_entity(&mut app, target);
    assert_eq!(count::<ActionsMenu>(&mut app), 0, "a replay opens no menu");
    assert!(app.world().resource::<Model>().0.sheet.is_some() || app.world().resource::<Model>().0.inspecting.is_some(), "the click read it");
    assert_eq!(count::<Overlay>(&mut app), 1);

    // Escape closes the sheet, then leaves without asking, to the list.
    key(&mut app, KeyCode::Escape, Key::Escape);
    assert_eq!(count::<Overlay>(&mut app), 0);
    assert_eq!(screen(&app), AppScreen::Game);
    key(&mut app, KeyCode::Escape, Key::Escape);
    assert_eq!(screen(&app), AppScreen::Replay, "a replay goes back to its list");
    assert!(!app.world().contains_resource::<ActiveReplay>());
    key(&mut app, KeyCode::Escape, Key::Escape);
    assert_eq!(screen(&app), AppScreen::MainMenu);

    let _ = std::fs::remove_dir_all(dir);
}
