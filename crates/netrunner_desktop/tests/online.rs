//! Play Online, without a window: the desktop hosts a game on this
//! machine, a second player joins it through the real connection driver,
//! both are seated, the board opens on the host's first view, and leaving
//! the board concedes — which reaches the opponent before the host's
//! server stops.
//!
//! Driven under `MinimalPlugins` like `tests/game.rs`; the server, the
//! connection and the match thread are real, so the test pumps `update`
//! until what it waits for has happened, with a bound.

use std::time::{Duration, Instant};

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use netrunner_client::connection::Goal;
use netrunner_client::hosting::Reach;
use netrunner_client::play::GameEndReason;
use netrunner_client::remote;
use netrunner_core::rules::Viewer;
use netrunner_desktop::core::ClientCore;
use netrunner_desktop::models::online::{Field, Page};
use netrunner_desktop::nav::Navigate;
use netrunner_desktop::screens::game::{Click, Model as Board};
use netrunner_desktop::screens::new_game::ActiveMatch;
use netrunner_desktop::screens::online::{Control, Model};
use netrunner_desktop::{AppScreen, NetrunnerDesktopPlugins};
use netrunner_server::ServerMessage;

fn headless_client() -> (App, std::path::PathBuf) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("netrunner_desktop_online_{}_{n}", std::process::id()));
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin));
    let mut core = ClientCore::in_dir(dir.clone());
    core.settings.desktop.animation_speed = 0.0;
    core.settings.player = Some("host".to_string());
    app.insert_resource(core);
    app.add_plugins(NetrunnerDesktopPlugins);
    app.update();
    app.update();
    (app, dir)
}

fn press(app: &mut App, key_code: KeyCode, logical_key: Key) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput { key_code, logical_key: logical_key.clone(), state, text: None, repeat: false, window: Entity::PLACEHOLDER });
    }
    app.update();
}

fn tap(app: &mut App, entity: Entity) {
    app.world_mut().entity_mut(entity).insert(Interaction::Pressed);
    app.update();
    if let Ok(mut button) = app.world_mut().get_entity_mut(entity) {
        button.insert(Interaction::None);
    }
    app.update();
    app.update();
}

fn find<C: Component + Clone>(app: &mut App, wanted: impl Fn(&C) -> bool) -> Option<Entity> {
    app.world_mut().query::<(Entity, &C)>().iter(app.world()).find(|(_, c)| wanted(c)).map(|(e, _)| e)
}

fn tap_control(app: &mut App, control: Control) {
    let button = find::<Control>(app, |c| *c == control).unwrap_or_else(|| panic!("no {control:?} on the page"));
    tap(app, button);
}

fn page(app: &App) -> Page {
    app.world().resource::<Model>().0.page
}

fn screen(app: &App) -> AppScreen {
    *app.world().resource::<State<AppScreen>>().get()
}

/// Pumps the client until `done`, or fails after ten seconds.
fn until(app: &mut App, what: &str, mut done: impl FnMut(&mut App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(app) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn texts(app: &mut App) -> Vec<String> {
    app.world_mut().query::<&Text>().iter(app.world()).map(|text| text.0.clone()).collect()
}

/// A host, a guest and a spectator: the whole of Play Online, on one
/// machine.
#[test]
fn a_hosted_game_seats_both_players_is_watched_and_leaving_concedes() {
    let (mut app, _dir) = headless_client();
    app.world_mut().write_message(Navigate(AppScreen::Online));
    app.update();
    app.update();
    assert_eq!(page(&app), Page::Home);
    tap_control(&mut app, Control::Open(Page::Host));
    assert_eq!(page(&app), Page::Host);

    // The port typed into its field: 0 takes any free one.
    tap_control(&mut app, Control::Edit(Field::Port));
    for _ in 0..4 {
        press(&mut app, KeyCode::Backspace, Key::Backspace);
    }
    press(&mut app, KeyCode::Digit0, Key::Character("0".into()));
    press(&mut app, KeyCode::Enter, Key::Enter);
    app.update();
    assert_eq!(app.world().resource::<Model>().0.port, "0");
    tap_control(&mut app, Control::Reach(Reach::ThisMachine));
    tap_control(&mut app, Control::Go);
    assert_eq!(page(&app), Page::Waiting, "{:?}", app.world().resource::<Model>().0.notice);

    // The address to give out is on the page, with a Copy beside it.
    let mut address = None;
    until(&mut app, "the host's address", |app| {
        address = texts(app).into_iter().find(|text| text.starts_with("ws://127.0.0.1:"));
        address.is_some()
    });
    assert!(find::<Control>(&mut app, |c| matches!(c, Control::Copy(_))).is_some());
    let address = address.unwrap();

    // The opponent: a seat through the same driver, over the address.
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().unwrap();
    let hello = remote::connect_message("guest", None, None, None);
    let mut guest = runtime.block_on(async { tokio::time::timeout(Duration::from_secs(10), remote::connect(&address, Goal::Play(hello), |_| {})).await }).expect("seated in time").unwrap();

    until(&mut app, "the board", |app| screen(app) == AppScreen::Game);
    let active = app.world().resource::<ActiveMatch>();
    let host_side = active.handle.side();
    assert!(active.online.as_ref().is_some_and(|online| online.hosting.is_some() && !online.watching), "the host's server rides with the match");
    assert_eq!(guest.viewer, Viewer::Player(host_side.other()), "the two are seated against each other");
    until(&mut app, "the host's first view", |app| app.world().resource::<Board>().0.view.is_some());

    // A spectator on another client: the host's matches listed, one
    // watched, from the Runner's side, and left without being asked.
    let (mut watcher, _) = headless_client();
    watcher.world_mut().write_message(Navigate(AppScreen::Online));
    watcher.update();
    watcher.update();
    tap_control(&mut watcher, Control::Open(Page::Watch));
    watcher.world_mut().resource_mut::<Model>().0.address = address.clone();
    tap_control(&mut watcher, Control::WatchFrom(netrunner_core::rules::Side::Runner));
    tap_control(&mut watcher, Control::List);
    until(&mut watcher, "the host's matches", |app| !app.world().resource::<Model>().0.matches.is_empty());
    let summary = watcher.world().resource::<Model>().0.matches[0].clone();
    assert_eq!((summary.corp.as_str(), summary.runner.as_str()), if host_side == netrunner_core::rules::Side::Corp { ("host", "guest") } else { ("guest", "host") });
    tap_control(&mut watcher, Control::Watch(0));
    until(&mut watcher, "the spectator's board", |app| screen(app) == AppScreen::Game && app.world().get_resource::<Board>().is_some_and(|board| board.0.view.is_some()));
    let board = &watcher.world().resource::<Board>().0;
    assert_eq!((board.side, board.view.as_ref().unwrap().viewer), (netrunner_core::rules::Side::Runner, Viewer::Spectator));
    assert!(board.view.as_ref().unwrap().legal_actions.is_empty() && !board.awaiting, "a spectator is asked nothing");
    press(&mut watcher, KeyCode::Escape, Key::Escape);
    until(&mut watcher, "the spectator back at Play Online", |app| screen(app) == AppScreen::Online);

    // Leaving concedes, and the concession reaches the guest before the
    // host's server lets go.
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    let confirm = find::<Click>(&mut app, |c| matches!(c, Click::ConfirmQuit)).expect("leaving asks first");
    tap(&mut app, confirm);
    until(&mut app, "Play Online again", |app| screen(app) == AppScreen::Online);
    let ended = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(message) = guest.rx.recv().await {
                if let ServerMessage::GameEnded { winner, reason } = message {
                    return Some((winner, reason));
                }
            }
            None
        })
        .await
    });
    assert_eq!(ended.expect("the guest heard in time"), Some((host_side.other(), GameEndReason::Surrender)));
}
