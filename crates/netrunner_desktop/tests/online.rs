//! Play Online, without a window: the desktop hosts a game on this
//! machine and is attached to its own server; a second player joins it
//! through the real connection driver; both are seated; the board opens on
//! the host's first view; leaving the board concedes — which reaches the
//! opponent — and leads back to the Server page, still attached, for the
//! next game. A second client connects by address, browses the lobbies and
//! makes one of its own.
//!
//! Driven under `MinimalPlugins` like `tests/game.rs`; the server, the
//! connection and the match thread are real, so the test pumps `update`
//! until what it waits for has happened, with a bound.

use std::time::{Duration, Instant};

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use netrunner_client::hosting::Reach;
use netrunner_client::play::GameEndReason;
use netrunner_client::remote;
use netrunner_core::rules::Viewer;
use netrunner_desktop::core::ClientCore;
use netrunner_desktop::models::online::{ChairChoice, Field, Page};
use netrunner_desktop::nav::Navigate;
use netrunner_desktop::screens::game::{Click, Model as Board};
use netrunner_desktop::screens::new_game::ActiveMatch;
use netrunner_desktop::screens::online::{Connected, Control, Model};
use netrunner_desktop::{AppScreen, NetrunnerDesktopPlugins};
use netrunner_server::ServerMessage;

fn headless_client(name: &str) -> (App, std::path::PathBuf) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("netrunner_desktop_online_{}_{n}", std::process::id()));
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin));
    let mut core = ClientCore::in_dir(dir.clone());
    core.settings.desktop.animation_speed = 0.0;
    core.settings.player = Some(name.to_string());
    app.insert_resource(core);
    app.add_plugins(NetrunnerDesktopPlugins);
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::Online));
    app.update();
    app.update();
    assert_eq!(page(&app), Page::Home);
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
        assert!(Instant::now() < deadline, "timed out waiting for {what}; notice: {:?}", app.world().resource::<Model>().0.notice);
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn texts(app: &mut App) -> Vec<String> {
    app.world_mut().query::<&Text>().iter(app.world()).map(|text| text.0.clone()).collect()
}

/// Hosts for this machine on a free port: the Server page, attached to
/// the host's own server and put in its one lobby, with the address to
/// give out on the page beside a Copy.
fn host(app: &mut App) -> String {
    tap_control(app, Control::Open(Page::Host));
    assert_eq!(page(app), Page::Host);
    // The port typed into its field: 0 takes any free one.
    tap_control(app, Control::Edit(Field::Port));
    for _ in 0..4 {
        press(app, KeyCode::Backspace, Key::Backspace);
    }
    press(app, KeyCode::Digit0, Key::Character("0".into()));
    press(app, KeyCode::Enter, Key::Enter);
    app.update();
    assert_eq!(app.world().resource::<Model>().0.port, "0");
    tap_control(app, Control::Reach(Reach::ThisMachine));
    tap_control(app, Control::Go);
    assert_eq!(page(app), Page::Waiting, "{:?}", app.world().resource::<Model>().0.notice);
    until(app, "the host's own lobby", |app| page(app) == Page::Server && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.hosting && server.lobby.is_some() && !server.decks.is_empty()));
    assert!(app.world().get_resource::<Connected>().is_some_and(Connected::is_hosting));
    let mut address = None;
    until(app, "the host's address", |app| {
        address = texts(app).into_iter().find(|text| text.starts_with("ws://127.0.0.1:"));
        address.is_some()
    });
    assert!(find::<Control>(app, |c| matches!(c, Control::Copy(_))).is_some());
    address.unwrap()
}

/// A host, a guest and a spectator: the whole of Play Online, on one
/// machine — and the host back on the Server page after the game, still
/// attached.
#[test]
fn a_hosted_game_seats_both_players_is_watched_and_leaving_concedes() {
    let (mut app, _dir) = headless_client("host");
    let address = host(&mut app);

    // The host looks for a game in the Corp chair with the first legal
    // Corp deck; the deck is the seek's, not the form's.
    tap_control(&mut app, Control::Seek);
    until(&mut app, "the host's seek", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.seeking == Some(1)));
    assert!(find::<Control>(&mut app, |c| *c == Control::CancelSeek).is_some(), "looking can be stopped");

    // The opponent: a seat through the same driver, over the address.
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().unwrap();
    // The host's lobby is the table's format, Casual unless set.
    let hello = remote::seat_in_format("guest", netrunner_client::settings::DEFAULT_FORMAT, netrunner_core::decks::by_id("stolen_goods").unwrap());
    let mut guest = runtime.block_on(async { tokio::time::timeout(Duration::from_secs(10), remote::connect(remote::seek(address.clone(), hello), |_| {})).await }).expect("seated in time").unwrap();

    until(&mut app, "the board", |app| screen(app) == AppScreen::Game);
    let active = app.world().resource::<ActiveMatch>();
    let host_side = active.handle.side();
    assert_eq!(host_side, netrunner_core::rules::Side::Corp, "the chair chosen");
    assert!(active.online.as_ref().is_some_and(|online| online.hosting.is_none() && !online.watching), "the host's server rides with the connection, not the match");
    assert!(app.world().get_resource::<Connected>().is_some(), "still attached while the board is up");
    assert_eq!(guest.viewer, Viewer::Player(host_side.other()), "the two are seated against each other");
    until(&mut app, "the host's first view", |app| app.world().resource::<Board>().0.view.is_some());

    // A spectator on another client: the host's matches listed, one
    // watched, from the Runner's side, and left without being asked.
    let (mut watcher, _) = headless_client("watcher");
    tap_control(&mut watcher, Control::Open(Page::Watch));
    watcher.world_mut().resource_mut::<Model>().0.address = address.clone();
    tap_control(&mut watcher, Control::WatchFrom(netrunner_core::rules::Side::Runner));
    tap_control(&mut watcher, Control::List);
    until(&mut watcher, "the host's matches", |app| !app.world().resource::<Model>().0.matches.is_empty());
    let summary = watcher.world().resource::<Model>().0.matches[0].clone();
    assert_eq!((summary.corp.as_str(), summary.runner.as_str()), ("host", "guest"));
    tap_control(&mut watcher, Control::Watch(0));
    until(&mut watcher, "the spectator's board", |app| screen(app) == AppScreen::Game && app.world().get_resource::<Board>().is_some_and(|board| board.0.view.is_some()));
    let board = &watcher.world().resource::<Board>().0;
    assert_eq!((board.side, board.view.as_ref().unwrap().viewer), (netrunner_core::rules::Side::Runner, Viewer::Spectator));
    assert!(board.view.as_ref().unwrap().legal_actions.is_empty() && !board.awaiting, "a spectator is asked nothing");
    press(&mut watcher, KeyCode::Escape, Key::Escape);
    until(&mut watcher, "the spectator back at Play Online", |app| screen(app) == AppScreen::Online);
    assert_eq!(page(&watcher), Page::Home, "a spectator was never attached");

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

    // Back on the Server page, attached still, in the lobby and not
    // looking: the next game is a press away.
    assert_eq!(page(&app), Page::Server);
    until(&mut app, "the host back in its lobby", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.lobby.is_some() && server.seeking.is_none()));
    assert!(app.world().get_resource::<Connected>().is_some());
    assert!(find::<Control>(&mut app, |c| *c == Control::Seek).is_some());
    tap_control(&mut app, Control::Disconnect);
    assert_eq!(page(&app), Page::Host);
    assert!(app.world().get_resource::<Connected>().is_none(), "disconnecting lets the connection and the server go");
}

/// Tournaments from the pages (Phase 4 §7 stage 6b), at a daemon that
/// keeps things: one client holds a tournament from the form and lands on
/// its page; another opens it from the list, registers with the decks
/// its drop-downs show, is told it is entered, and withdraws; the
/// organizer's Refresh sees both. Each client's key is made in its own
/// directory the first time it connects.
#[test]
fn a_tournament_is_held_entered_and_left_from_the_pages() {
    use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};
    let data = std::env::temp_dir().join(format!("netrunner_desktop_tournament_daemon_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().unwrap();
    let address = runtime.block_on(async {
        let options = ServeOptions { bot_runner: ServeBotKind::None, seed: Some(1), data_dir: Some(data.clone()), ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
        let address = format!("ws://{}", server.local_addr().unwrap());
        tokio::spawn(server.run());
        address
    });
    let join = |app: &mut App| {
        tap_control(app, Control::Open(Page::Join));
        app.world_mut().resource_mut::<Model>().0.address = address.clone();
        tap_control(app, Control::Go);
        until(app, "the server's key answer", |app| page(app) == Page::Server && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.key.is_some()));
    };
    let tournament = |app: &App| app.world().resource::<Model>().0.server.as_ref().and_then(|server| server.open_tournament().cloned());

    let (mut organizer, _dir) = headless_client("organizer");
    join(&mut organizer);
    tap_control(&mut organizer, Control::Open(Page::Tournaments));
    until(&mut organizer, "the list", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.tournaments_listed));
    assert!(texts(&mut organizer).iter().any(|text| text == "This server holds none."));
    tap_control(&mut organizer, Control::Open(Page::MakeTournament));
    assert_eq!(page(&organizer), Page::MakeTournament);
    organizer.world_mut().resource_mut::<Model>().0.make_tournament.name = "Friday".to_string();
    tap_control(&mut organizer, Control::Go);
    until(&mut organizer, "the tournament's page", |app| page(app) == Page::Tournament && tournament(app).is_some_and(|info| info.name == "Friday"));
    let made = tournament(&organizer).unwrap();
    assert_eq!((made.id.len(), made.entrants.len()), (6, 0));
    let shown = texts(&mut organizer);
    assert!(shown.iter().any(|text| text == "You hold this tournament. Not entered yet."), "{shown:?}");
    assert!(find::<Control>(&mut organizer, |c| *c == Control::Register).is_some(), "the organizer may enter too");

    let (mut entrant, entrant_dir) = headless_client("entrant");
    join(&mut entrant);
    tap_control(&mut entrant, Control::Open(Page::Tournaments));
    until(&mut entrant, "the list with one", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.tournaments.len() == 1));
    tap_control(&mut entrant, Control::OpenTournament(0));
    until(&mut entrant, "the decks legal in its format", |app| page(app) == Page::Tournament && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| !server.tournament_decks.is_empty()));
    assert!(find::<Control>(&mut entrant, |c| *c == Control::Unregister).is_none(), "not entered yet");
    let chosen = {
        let server = entrant.world().resource::<Model>().0.server.clone().unwrap();
        (server.tournament_chosen(netrunner_core::rules::Side::Corp).unwrap().id.clone(), server.tournament_chosen(netrunner_core::rules::Side::Runner).unwrap().id.clone())
    };
    tap_control(&mut entrant, Control::Register);
    until(&mut entrant, "the entry", |app| tournament(app).is_some_and(|info| info.entrants.len() == 1));
    let shown = texts(&mut entrant);
    assert!(shown.iter().any(|text| text == "You are entered, with the two decks you committed to."), "{shown:?}");
    assert!(shown.iter().any(|text| text.starts_with("entrant · ")), "the entrant by name and key: {shown:?}");
    assert!(find::<Control>(&mut entrant, |c| *c == Control::Unregister).is_some());
    let kept = std::fs::read_to_string(entrant_dir.join("identity").join(netrunner_client::identity::REGISTRATIONS_FILE)).expect("the registration is kept beside the key");
    let line = kept.lines().last().unwrap();
    assert!(line.contains(&format!("\"corp\":\"{}\"", chosen.0)) && line.contains(&format!("\"runner\":\"{}\"", chosen.1)), "the decks the drop-downs showed: {line}");

    tap_control(&mut organizer, Control::Refresh);
    until(&mut organizer, "the entrant on the organizer's page", |app| tournament(app).is_some_and(|info| info.entrants.len() == 1));

    tap_control(&mut entrant, Control::Unregister);
    until(&mut entrant, "the withdrawal", |app| tournament(app).is_some_and(|info| info.entrants.is_empty()));
    press(&mut entrant, KeyCode::Escape, Key::Escape);
    entrant.update();
    assert_eq!(page(&entrant), Page::Tournaments);
    press(&mut entrant, KeyCode::Escape, Key::Escape);
    entrant.update();
    assert_eq!(page(&entrant), Page::Server, "Escape walks back a page at a time, attached still");
    assert!(entrant.world().get_resource::<Connected>().is_some());
    let _ = std::fs::remove_dir_all(&data);
}

/// A second client connects to the host by address: the host's one lobby
/// is listed, a lobby of the guest's own is made and joined, the host's
/// is joined back, and Escape disconnects.
#[test]
fn a_guest_browses_the_hosts_lobbies_and_makes_one() {
    let (mut host_app, _dir) = headless_client("host");
    let address = host(&mut host_app);

    let (mut guest, _dir) = headless_client("guest");
    tap_control(&mut guest, Control::Open(Page::Join));
    guest.world_mut().resource_mut::<Model>().0.address = address.clone();
    tap_control(&mut guest, Control::Go);
    assert_eq!(page(&guest), Page::Waiting);
    // The server joined is where Join starts next time, in both clients
    // (Phase 6 §3): kept in the settings file as the connection is made.
    let kept = netrunner_client::settings::Settings::load(&_dir.join(netrunner_client::settings::SETTINGS_FILE)).expect("the settings file is written");
    assert_eq!(kept.server.as_deref(), Some(address.as_str()), "the address joined is kept");
    until(&mut guest, "the host's lobbies", |app| page(app) == Page::Server && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| !server.lobbies.is_empty()));
    let server = guest.world().resource::<Model>().0.server.clone().unwrap();
    assert!(!server.hosting && server.lobby.is_none(), "a guest picks a lobby");
    assert_eq!(server.lobbies.len(), 1, "the host's server has the one lobby, its format's");
    let format = server.lobbies[0].format;

    // A closed lobby of the guest's own, which the server puts them in.
    tap_control(&mut guest, Control::Open(Page::MakeLobby));
    assert_eq!(page(&guest), Page::MakeLobby);
    guest.world_mut().resource_mut::<Model>().0.make.name = "Friday".to_string();
    tap_control(&mut guest, Control::Closed(true));
    tap_control(&mut guest, Control::Go);
    until(&mut guest, "the made lobby", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.lobby.as_ref().is_some_and(|lobby| lobby.name == "Friday")));
    let made = guest.world().resource::<Model>().0.server.as_ref().unwrap().lobby.clone().unwrap();
    assert!(made.closed && !made.permanent && made.id.len() == 6 && made.format == format, "{made:?}");
    assert_eq!(page(&guest), Page::Server);
    assert!(find::<Control>(&mut guest, |c| matches!(c, Control::Chair(ChairChoice::Runner))).is_some(), "in a lobby, the game to look for");

    // Back to the host's lobby by its row.
    tap_control(&mut guest, Control::JoinLobby(0));
    until(&mut guest, "the host's lobby", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.lobby.as_ref().is_some_and(|lobby| lobby.permanent)));
    // The host sees the guest arrive on a refresh.
    tap_control(&mut host_app, Control::Refresh);
    until(&mut host_app, "two in the lobby", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.lobbies.first().is_some_and(|lobby| lobby.players == 2)));

    press(&mut guest, KeyCode::Escape, Key::Escape);
    guest.update();
    assert_eq!(page(&guest), Page::Join, "Escape from the Server page disconnects");
    assert!(guest.world().get_resource::<Connected>().is_none());
    press(&mut guest, KeyCode::Escape, Key::Escape);
    guest.update();
    assert_eq!(page(&guest), Page::Home);
}
