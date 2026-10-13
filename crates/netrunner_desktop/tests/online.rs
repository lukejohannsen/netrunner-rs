//! Play Online, without a window: the desktop hosts a game on this
//! machine from the hub and is in its room, attached to its own server;
//! it is ready there; a second player joins through the real connection
//! driver; both are seated; the board opens on the host's first view;
//! leaving the board concedes — which reaches the opponent — and leads
//! back to the room, still attached, for the next game. A second client
//! connects by address and lands on the dashboard, where it makes a
//! friend lobby of its own; another joins through the hub, both are
//! ready and play; and a ticket pasted anywhere opens the room.
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
use netrunner_desktop::models::online::{Field, Page, ServerState};
use netrunner_server::protocol::QueueMode;
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
    assert_eq!(page(&app), Page::Servers, "no Default: the servers page");
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

/// Hosts for this machine on a free port, from the hub: the room,
/// attached to the host's own server and in its queue, with the address
/// to give out beside a Copy.
fn host(app: &mut App) -> String {
    tap_control(app, Control::Open(Page::Hub));
    assert_eq!(page(app), Page::Hub);
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
    // Waiting while the host starts — or already in the room, when it
    // was up within the same frames, a race a fast runner won.
    assert!(matches!(page(app), Page::Waiting | Page::Room), "{:?} {:?}", page(app), app.world().resource::<Model>().0.notice);
    until(app, "the host's room", |app| page(app) == Page::Room && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.hosting && server.room && server.lobby.is_some()));
    assert!(app.world().get_resource::<Connected>().is_some_and(Connected::is_hosting));
    assert!(texts(app).iter().any(|text| text == "Listening for a peer…"), "{:?}", texts(app));
    let mut address = None;
    until(app, "the host's address", |app| {
        address = texts(app).into_iter().find(|text| text.starts_with("ws://127.0.0.1:"));
        address.is_some()
    });
    assert!(find::<Control>(app, |c| matches!(c, Control::Copy(_))).is_some());
    address.unwrap()
}

/// A host, a guest and a spectator: the whole of Play Online, on one
/// machine — and the host back in the room after the game, still
/// attached.
#[test]
fn a_hosted_game_seats_both_players_is_watched_and_leaving_concedes() {
    let (mut app, _dir) = headless_client("host");
    let address = host(&mut app);

    // The host is ready, with both active decks: the seek in the room's
    // queue, which Not ready after all takes back.
    tap_control(&mut app, Control::Ready);
    until(&mut app, "the host ready", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.searching.as_ref().is_some_and(|searching| searching.position == Some(1))));
    assert!(find::<Control>(&mut app, |c| *c == Control::Unready).is_some(), "ready can be taken back");
    assert!(texts(&mut app).iter().any(|text| text == "Ready. The game starts when your opponent is too."), "{:?}", texts(&mut app));

    // The opponent: a seat through the same driver, over the address.
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().unwrap();
    // The host's lobby is the table's format, Standard unless set.
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
    assert_eq!(page(&watcher), Page::Servers, "a spectator was never attached");

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
    assert_eq!(ended.expect("the guest heard in time"), Some((Some(host_side.other()), GameEndReason::Surrender)));

    // Back in the room, attached still, in its queue and not ready: the
    // next game is a Ready away.
    assert_eq!(page(&app), Page::Room);
    until(&mut app, "the host not ready", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.lobby.is_some() && !server.seeking()));
    assert!(app.world().get_resource::<Connected>().is_some());
    assert!(find::<Control>(&mut app, |c| *c == Control::Ready).is_some());
    tap_control(&mut app, Control::Disconnect);
    assert_eq!(page(&app), Page::Hub, "stopping hosting leads to the hub");
    assert!(app.world().get_resource::<Connected>().is_none(), "stopping lets the connection and the server go");
}

/// A guest joins through the hub (Phase 7 §15): the Join page, the
/// host's room with the host named in it, the host remembered as a saved
/// peer by name; both are ready and the board opens for each; leaving
/// the board leads both back to the room, not ready, where the guest's
/// Leave and the host's Stop hosting are the hub.
#[test]
fn a_guest_joins_through_the_hub_and_both_ready_is_the_game() {
    let (mut host_app, _host_dir) = headless_client("host");
    let address = host(&mut host_app);

    let (mut guest, guest_dir) = headless_client("guest");
    tap_control(&mut guest, Control::Open(Page::Hub));
    assert!(texts(&mut guest).iter().any(|text| text.starts_with("Nobody yet")), "{:?}", texts(&mut guest));
    tap_control(&mut guest, Control::Open(Page::Join));
    assert_eq!(page(&guest), Page::Join);
    guest.world_mut().resource_mut::<Model>().0.address = address.clone();
    tap_control(&mut guest, Control::Go);
    until(&mut guest, "the guest's room", |app| page(app) == Page::Room && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.room && !server.hosting && server.lobby.is_some()));
    until(&mut guest, "the host named in the room", |app| app.world().resource::<Model>().0.room_people().1.is_some_and(|presence| presence.name == "host"));
    guest.update();
    let shown = texts(&mut guest);
    assert!(shown.iter().any(|text| text == "host (host)") && shown.iter().any(|text| text == "guest (you)"), "{shown:?}");
    assert!(find::<Control>(&mut guest, |c| matches!(c, Control::Copy(_))).is_none(), "a guest has nothing to give out");
    // The host, remembered by name, beside the key.
    let peers = netrunner_client::peers::PeerBook::load(&netrunner_client::peers::path_in(&guest_dir.join("identity"))).expect("the saved peers are written");
    assert_eq!(peers.peers.iter().map(|peer| (peer.name.as_str(), peer.target.as_str())).collect::<Vec<_>>(), [("host", address.as_str())]);
    let servers = netrunner_client::servers::ServerBook::load(&netrunner_client::servers::path_in(&guest_dir.join("identity"))).unwrap_or_default();
    assert!(servers.find(&address).is_none(), "a friend's room is never a server row: {servers:?}");
    // The host sees the guest arrive, polled.
    until(&mut host_app, "the guest in the host's room", |app| app.world().resource::<Model>().0.room_people().1.is_some_and(|presence| presence.name == "guest"));
    host_app.update();
    assert!(texts(&mut host_app).iter().any(|text| text == "guest is here"), "{:?}", texts(&mut host_app));

    // The guest is ready, preferring the Runner; the host sees it, and
    // is ready too: the game.
    tap_control(&mut guest, Control::Prefer(Some(netrunner_core::rules::Side::Runner)));
    tap_control(&mut guest, Control::Ready);
    until(&mut host_app, "the guest's Ready seen by the host", |app| app.world().resource::<Model>().0.room_people().1.is_some_and(|presence| presence.seeking && presence.prefers == Some(netrunner_core::rules::Side::Runner)));
    host_app.update();
    assert!(texts(&mut host_app).iter().any(|text| text == "guest is ready. Press Ready and the game starts."), "{:?}", texts(&mut host_app));
    tap_control(&mut host_app, Control::Ready);
    until(&mut host_app, "the host's board", |app| screen(app) == AppScreen::Game);
    until(&mut guest, "the guest's board", |app| screen(app) == AppScreen::Game);
    assert_eq!(guest.world().resource::<ActiveMatch>().handle.side(), netrunner_core::rules::Side::Runner, "the guest's preference met");
    until(&mut host_app, "the host's first view", |app| app.world().resource::<Board>().0.view.is_some());

    // The host concedes; both are back in the room, not ready.
    press(&mut host_app, KeyCode::Escape, Key::Escape);
    host_app.update();
    let confirm = find::<Click>(&mut host_app, |c| matches!(c, Click::ConfirmQuit)).expect("leaving asks first");
    tap(&mut host_app, confirm);
    until(&mut host_app, "the host's room again", |app| screen(app) == AppScreen::Online && page(app) == Page::Room);
    until(&mut host_app, "the host not ready", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| !server.seeking()));
    until(&mut guest, "the guest's game over", |app| app.world().resource::<Board>().0.over.is_some());
    press(&mut guest, KeyCode::Escape, Key::Escape);
    guest.update();
    if let Some(confirm) = find::<Click>(&mut guest, |c| matches!(c, Click::ConfirmQuit)) {
        tap(&mut guest, confirm);
    }
    until(&mut guest, "the guest's room again", |app| screen(app) == AppScreen::Online && page(app) == Page::Room);
    until(&mut guest, "the guest not ready", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| !server.seeking()));
    assert!(find::<Control>(&mut guest, |c| *c == Control::Ready).is_some(), "a rematch is a Ready away");

    tap_control(&mut guest, Control::Disconnect);
    assert_eq!(page(&guest), Page::Hub, "a guest's Leave is the hub");
    assert!(guest.world().get_resource::<Connected>().is_none());
    assert!(find::<Control>(&mut guest, |c| *c == Control::QuickConnect(0)).is_some(), "the host is a row of the hub");
    tap_control(&mut host_app, Control::Disconnect);
    assert_eq!(page(&host_app), Page::Hub);
}

/// A ticket pasted anywhere opens the room (Phase 7 §15): a host by
/// ticket on its own key, with no relay — the ticket names this
/// machine's addresses — joined from the servers page's Connect once,
/// which is a room because a ticket is always a hosted game's; the
/// ticket is remembered as a peer.
#[test]
fn a_ticket_pasted_anywhere_opens_the_room() {
    use netrunner_server::serve::{ServeOptions, Server};
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().unwrap();
    let key = netrunner_identity::Identity::from_secret([3; 32]).secret_bytes();
    let (_host, ticket) = runtime.block_on(async {
        let options = ServeOptions { seed: Some(1), formats: vec![netrunner_client::settings::DEFAULT_FORMAT], ..ServeOptions::default() };
        let acceptor = Server::bind("127.0.0.1:0", options).await.unwrap().acceptor();
        let mut host = netrunner_client::peer::PeerHost::start(netrunner_client::peer::Relay::Off, Some(key), move |stream, who| {
            let acceptor = acceptor.clone();
            tokio::spawn(async move { acceptor.serve(stream, &who).await });
        });
        let netrunner_client::peer::Offer::Ready { ticket, .. } = host.ready().await else { panic!("no ticket") };
        (host, ticket.to_string())
    });
    let (mut guest, guest_dir) = headless_client("guest");
    guest.world_mut().resource_mut::<Model>().0.address = ticket.clone();
    tap_control(&mut guest, Control::Go);
    until(&mut guest, "the room by ticket", |app| page(app) == Page::Room && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.room && server.lobby.is_some()));
    let peers = netrunner_client::peers::PeerBook::load(&netrunner_client::peers::path_in(&guest_dir.join("identity"))).expect("the saved peers are written");
    assert_eq!(peers.peers.len(), 1);
    assert_eq!(peers.peers[0].target, ticket, "the ticket is kept: the host's key lasts");
    let servers = netrunner_client::servers::ServerBook::load(&netrunner_client::servers::path_in(&guest_dir.join("identity"))).unwrap_or_default();
    assert!(servers.servers.is_empty(), "a ticket is never a server row");
    press(&mut guest, KeyCode::Escape, Key::Escape);
    guest.update();
    assert_eq!(page(&guest), Page::Hub, "Escape in a guest's room leaves it, to the hub");
    assert!(guest.world().get_resource::<Connected>().is_none());
    runtime.shutdown_background();
}

/// Tournaments from the pages (Phase 4 §7 stage 6b), at a daemon that
/// keeps things: one client holds a tournament from the form and lands on
/// its page; another opens it from the list, registers with the decks
/// its drop-downs show, is told it is entered, and withdraws; the
/// organizer's Refresh sees both. Each client's key is made in its own
/// directory the first time it connects.
#[test]
fn a_tournament_is_held_entered_and_left_from_the_pages() {
    use netrunner_server::serve::{ServeOptions, Server};
    let data = std::env::temp_dir().join(format!("netrunner_desktop_tournament_daemon_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().unwrap();
    let address = runtime.block_on(async {
        let options = ServeOptions { seed: Some(1), data_dir: Some(data.clone()), ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
        let address = format!("ws://{}", server.local_addr().unwrap());
        tokio::spawn(server.run());
        address
    });
    let join = |app: &mut App| {
        app.world_mut().resource_mut::<Model>().0.address = address.clone();
        tap_control(app, Control::Go);
        until(app, "the server's key answer", |app| page(app) == Page::Dashboard && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.key.is_some()));
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
    assert_eq!(page(&entrant), Page::Dashboard, "Escape walks back a page at a time, attached still");
    assert!(entrant.world().get_resource::<Connected>().is_some());
    let _ = std::fs::remove_dir_all(&data);
}

/// The rounds from the pages (Phase 4 §7 stage 6b): the organizer, entered
/// too, begins round 1 once a second entrant is in, which the entrant
/// learns unasked; both sit at their table and the board opens for each;
/// the organizer's concession is the table's result and leads back to
/// the tournament's page, where the standings stand and the next round
/// is offered; round 2 is recorded as a tie by the organizer and the
/// tournament ended, its standings final.
#[test]
fn a_round_is_begun_sat_at_played_and_ended_from_the_pages() {
    use netrunner_server::protocol::swiss::Outcome;
    use netrunner_server::protocol::TournamentState;
    use netrunner_server::serve::{ServeOptions, Server};
    let data = std::env::temp_dir().join(format!("netrunner_desktop_rounds_daemon_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().unwrap();
    let address = runtime.block_on(async {
        let options = ServeOptions { seed: Some(1), data_dir: Some(data.clone()), ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
        let address = format!("ws://{}", server.local_addr().unwrap());
        tokio::spawn(server.run());
        address
    });
    let join = |app: &mut App| {
        app.world_mut().resource_mut::<Model>().0.address = address.clone();
        tap_control(app, Control::Go);
        until(app, "the server's key answer", |app| page(app) == Page::Dashboard && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.key.is_some()));
    };
    let tournament = |app: &App| app.world().resource::<Model>().0.server.as_ref().and_then(|server| server.open_tournament().cloned());
    let register = |app: &mut App, entrants: usize| {
        until(app, "the decks legal in its format", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| !server.tournament_decks.is_empty()));
        tap_control(app, Control::Register);
        until(app, "the entry", |app| tournament(app).is_some_and(|info| info.entrants.len() == entrants));
    };

    let (mut organizer, _dir) = headless_client("organizer");
    join(&mut organizer);
    tap_control(&mut organizer, Control::Open(Page::Tournaments));
    tap_control(&mut organizer, Control::Open(Page::MakeTournament));
    organizer.world_mut().resource_mut::<Model>().0.make_tournament.name = "Rounds".to_string();
    tap_control(&mut organizer, Control::Go);
    until(&mut organizer, "the tournament's page", |app| page(app) == Page::Tournament && tournament(app).is_some());
    assert!(find::<Control>(&mut organizer, |c| *c == Control::BeginRound).is_none(), "nobody to pair yet");
    register(&mut organizer, 1);

    let (mut entrant, _dir2) = headless_client("entrant");
    join(&mut entrant);
    tap_control(&mut entrant, Control::Open(Page::Tournaments));
    until(&mut entrant, "the list with one", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.tournaments.len() == 1));
    tap_control(&mut entrant, Control::OpenTournament(0));
    register(&mut entrant, 2);
    // The organizer is told unasked, and offered the first round.
    until(&mut organizer, "the second entrant, pushed", |app| tournament(app).is_some_and(|info| info.entrants.len() == 2));
    tap_control(&mut organizer, Control::BeginRound);
    until(&mut organizer, "round 1", |app| tournament(app).is_some_and(|info| info.state == TournamentState::Playing { round: 1 }));
    until(&mut entrant, "round 1, pushed", |app| tournament(app).is_some_and(|info| info.state == TournamentState::Playing { round: 1 }));
    let round = tournament(&organizer).unwrap().rounds[0].clone();
    assert_eq!((round.tables.len(), round.bye), (1, None));
    let shown = texts(&mut organizer);
    assert!(shown.iter().any(|text| text.starts_with("Round 1: you play")), "{shown:?}");
    assert!(shown.iter().any(|text| text.starts_with("1. ")), "the standings stand where the entrants did: {shown:?}");

    // Both sit; the board opens for each on the pairing's sides.
    tap_control(&mut organizer, Control::Sit);
    until(&mut organizer, "the organizer seated", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(ServerState::seeking));
    assert!(find::<Control>(&mut organizer, |c| *c == Control::CancelSeek).is_some(), "a seat is stood up from");
    tap_control(&mut entrant, Control::Sit);
    until(&mut organizer, "the organizer's board", |app| screen(app) == AppScreen::Game);
    until(&mut entrant, "the entrant's board", |app| screen(app) == AppScreen::Game);
    let organizer_key = organizer.world().resource::<Model>().0.server.as_ref().unwrap().key.unwrap();
    let expected = if round.tables[0].corp == organizer_key { netrunner_core::rules::Side::Corp } else { netrunner_core::rules::Side::Runner };
    assert_eq!(organizer.world().resource::<ActiveMatch>().handle.side(), expected, "the pairing's side");
    until(&mut organizer, "the organizer's first view", |app| app.world().resource::<Board>().0.view.is_some());

    // The organizer concedes: the table's result, and the tournament's
    // page again with the next round offered.
    press(&mut organizer, KeyCode::Escape, Key::Escape);
    organizer.update();
    let confirm = find::<Click>(&mut organizer, |c| matches!(c, Click::ConfirmQuit)).expect("leaving asks first");
    tap(&mut organizer, confirm);
    until(&mut organizer, "Play Online again", |app| screen(app) == AppScreen::Online);
    assert_eq!(page(&organizer), Page::Tournament, "back from a tournament game, its page");
    let winner = if expected == netrunner_core::rules::Side::Corp { Outcome::RunnerWon } else { Outcome::CorpWon };
    until(&mut organizer, "the table's result", |app| tournament(app).is_some_and(|info| info.rounds[0].tables[0].result == Some(winner)));
    until(&mut organizer, "the seat let go", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| !server.seeking()));
    let shown = texts(&mut organizer);
    assert!(shown.iter().any(|text| text.contains("· 3 pts ·")), "{shown:?}");
    assert!(find::<Control>(&mut organizer, |c| *c == Control::BeginRound).is_some());
    assert!(find::<Control>(&mut organizer, |c| *c == Control::FinishTournament).is_some());

    // Round 2 is recorded by the organizer as a tie and the tournament
    // ended; the entrant, pumped, sees it all unasked.
    tap_control(&mut organizer, Control::BeginRound);
    until(&mut organizer, "round 2", |app| tournament(app).is_some_and(|info| info.state == TournamentState::Playing { round: 2 }));
    assert!(find::<Control>(&mut organizer, |c| *c == Control::RecordResult(0, Outcome::Tie)).is_some());
    tap_control(&mut organizer, Control::RecordResult(0, Outcome::Tie));
    until(&mut organizer, "the tie", |app| tournament(app).is_some_and(|info| info.rounds[1].tables[0].result == Some(Outcome::Tie)));
    tap_control(&mut organizer, Control::FinishTournament);
    until(&mut organizer, "the end", |app| tournament(app).is_some_and(|info| info.state == TournamentState::Finished));
    // An overline is drawn in capitals.
    assert!(texts(&mut organizer).iter().any(|text| text == "FINAL STANDINGS"));
    // The entrant is still on the board, which the push waits behind:
    // the game over, Escape leads back to the tournament's page, where
    // everything that happened since is read.
    until(&mut entrant, "the entrant's game over", |app| app.world().resource::<Board>().0.over.is_some());
    press(&mut entrant, KeyCode::Escape, Key::Escape);
    entrant.update();
    if let Some(confirm) = find::<Click>(&mut entrant, |c| matches!(c, Click::ConfirmQuit)) {
        tap(&mut entrant, confirm);
    }
    until(&mut entrant, "the entrant back on the tournament's page", |app| screen(app) == AppScreen::Online && page(app) == Page::Tournament);
    until(&mut entrant, "the entrant told of the end", |app| tournament(app).is_some_and(|info| info.state == TournamentState::Finished));
    let _ = std::fs::remove_dir_all(&data);
}

/// A second client connects to the host by address and lands on the
/// dashboard, where the host's one Casual queue is listed; under the
/// Casual door it makes a friend lobby of its own and lands on its page;
/// leaving the lobby is the dashboard again, and Escape there is off the
/// screen, the connection let go.
#[test]
fn a_guest_lands_on_the_dashboard_and_makes_a_friend_lobby() {
    let (mut host_app, _dir) = headless_client("host");
    let address = host(&mut host_app);

    let (mut guest, _dir) = headless_client("guest");
    guest.world_mut().resource_mut::<Model>().0.address = address.clone();
    tap_control(&mut guest, Control::Go);
    // Waiting while the connection is made — or already on the
    // dashboard, when the host answered within the same frames: a fast
    // runner did, and the test that asserted Waiting alone lost that race.
    assert!(matches!(page(&guest), Page::Waiting | Page::Dashboard), "{:?}", page(&guest));
    // The server joined is a row of the person's servers from the moment
    // the connection is made, and where Connect once starts next time
    // while none is the Default, in both clients (Phase 7 §14).
    let book = netrunner_client::servers::ServerBook::load(&netrunner_client::servers::path_in(&_dir.join("identity"))).expect("the list of servers is written");
    assert_eq!(book.start_address(), Some(address.as_str()), "the address joined is kept");
    until(&mut guest, "the host's queues", |app| page(app) == Page::Dashboard && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| !server.lobbies.is_empty()));
    let server = guest.world().resource::<Model>().0.server.clone().unwrap();
    assert!(!server.hosting && server.lobby.is_none(), "a guest is in no queue until it looks");
    assert_eq!(server.lobbies.len(), 1, "the host's server has the one queue, its format's");
    let format = server.lobbies[0].format;
    let shown = texts(&mut guest);
    assert!(shown.iter().any(|text| text == "ACTIVE DECKS") && shown.iter().any(|text| text == "Ranked ladder"), "{shown:?}");
    assert!(find::<Control>(&mut guest, |c| matches!(c, Control::Nav(AppScreen::Decks))).is_some(), "the deck builder is a row of the column");

    // A friend lobby of the guest's own, from the Casual door, which the
    // server puts them in.
    tap_control(&mut guest, Control::OpenQueue(QueueMode::Casual));
    tap_control(&mut guest, Control::Open(Page::MakeLobby));
    assert_eq!(page(&guest), Page::MakeLobby);
    guest.world_mut().resource_mut::<Model>().0.make.name = "Friday".to_string();
    tap_control(&mut guest, Control::LobbyBots(true));
    tap_control(&mut guest, Control::Go);
    until(&mut guest, "the made lobby", |app| page(app) == Page::FriendLobby && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.lobby.as_ref().is_some_and(|lobby| lobby.name == "Friday")));
    let made = guest.world().resource::<Model>().0.server.as_ref().unwrap().lobby.clone().unwrap();
    assert!(!made.permanent() && made.id.len() == 6 && made.format == format, "{made:?}");
    assert!(made.bots.is_empty(), "a hosted server offers no bot, whatever its friend lobby asks: {made:?}");
    assert!(find::<Control>(&mut guest, |c| *c == Control::Seek).is_some(), "in the lobby, the game to look for");
    assert!(find::<Control>(&mut guest, |c| matches!(c, Control::Opponent(_))).is_none(), "and no bot to ask for");
    assert!(texts(&mut guest).iter().any(|text| text == &format!("Code {}", made.id)));

    // Out of it: the dashboard again.
    tap_control(&mut guest, Control::LeaveLobby);
    until(&mut guest, "the dashboard", |app| page(app) == Page::Dashboard && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.lobby.is_none()));

    press(&mut guest, KeyCode::Escape, Key::Escape);
    until(&mut guest, "the main menu", |app| screen(app) == AppScreen::MainMenu);
    assert!(guest.world().get_resource::<Connected>().is_none(), "leaving the screen lets the connection go");
}

/// A Default is connected to on opening the screen (Phase 7 §14), and the
/// profile page there wears a title the server's store says was earned,
/// picked from the pop-up of those earned, and reads the ladder (Phase 4
/// §10).
#[test]
fn the_default_is_connected_to_on_opening_and_the_profile_page_wears_a_title() {
    use netrunner_client::servers::{ServerBook, ServerEdit};
    let dir = std::env::temp_dir().join(format!("netrunner_desktop_online_profile_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // This client's key, made first, so the daemon's store can be told
    // what it has earned before the daemon starts.
    let credentials = netrunner_client::identity::Credentials::in_dir(&dir.join("identity")).unwrap();
    let key = credentials.identity.public_key().rating_id();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let url = runtime.block_on(async {
        let data = dir.join("daemon");
        std::fs::create_dir_all(&data).unwrap();
        let store = netrunner_store::Store::open(&netrunner_store::sqlite_url(&data.join(netrunner_store::SQLITE_FILE))).await.unwrap();
        store.saw_player(&key, "ann", 1).await.unwrap();
        store.grant(&netrunner_store::Award { key: key.clone(), kind: "title".into(), name: "Gauntlet Runner".into(), earned_at: 2, source: "event:abc".into() }).await.unwrap();
        store.close().await;
        let options = netrunner_server::serve::ServeOptions { seed: Some(1), data_dir: Some(data), ..Default::default() };
        let server = netrunner_server::serve::Server::bind("127.0.0.1:0", options).await.unwrap();
        let url = format!("ws://{}", server.local_addr().unwrap());
        tokio::spawn(server.run());
        url
    });
    let path = netrunner_client::servers::path_in(&dir.join("identity"));
    ServerBook::update(&path, |book| {
        ServerEdit::Add { name: "Club".into(), address: url.clone() }.apply(book)?;
        ServerEdit::Default(Some(url.clone())).apply(book)
    })
    .unwrap();

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin));
    let mut core = ClientCore::in_dir(dir.clone());
    core.settings.desktop.animation_speed = 0.0;
    core.settings.player = Some("ann".to_string());
    app.insert_resource(core);
    app.add_plugins(NetrunnerDesktopPlugins);
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::Online));
    app.update();
    app.update();
    until(&mut app, "the Default, attached", |app| page(app) == Page::Dashboard && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.known_profile().is_some()));
    assert!(texts(&mut app).iter().any(|text| text == "ann"), "the person's tag on the dashboard: {:?}", texts(&mut app));
    assert_eq!(app.world().resource::<Model>().0.attached_address(), Some(url.as_str()));

    tap_control(&mut app, Control::Open(Page::Profile));
    assert_eq!(page(&app), Page::Profile);
    until(&mut app, "the ladder", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.ladder.is_some()));
    // What was earned is a pop-up of its own, a pick from which is worn.
    assert!(find::<Control>(&mut app, |c| *c == Control::Pick(Some(0))).is_none(), "nothing listed until Change… is pressed");
    tap_control(&mut app, Control::Choose(netrunner_desktop::models::online::AwardKind::Title));
    tap_control(&mut app, Control::Pick(Some(0)));
    until(&mut app, "the title worn", |app| app.world().resource::<Model>().0.server.as_ref().and_then(|server| server.known_profile()).is_some_and(|profile| profile.title.as_deref() == Some("Gauntlet Runner")));
    assert!(app.world().resource::<Model>().0.notice.is_none(), "{:?}", app.world().resource::<Model>().0.notice);

    // The servers page while attached: the row is the connected one.
    press(&mut app, KeyCode::Escape, Key::Escape);
    tap_control(&mut app, Control::Open(Page::Servers));
    assert!(find::<Control>(&mut app, |c| *c == Control::Disconnect).is_some(), "the connected row's Disconnect");
    assert!(texts(&mut app).iter().any(|text| text == "CLUB · DEFAULT · CONNECTED"), "an overline is drawn in capitals: {:?}", texts(&mut app));
    drop(app);
    runtime.shutdown_background();
    let _ = std::fs::remove_dir_all(&dir);
}

/// An event through the pages (Phase 4 §11): the daemon names the
/// operator's key, the operator holds a gauntlet from the form and lands
/// on its page, an entrant opens it from the list and enters, both find a
/// match in its pool and reach the board, the operator concedes, and both
/// are back on the event's page with the tracks the server pushed — the
/// entrant's win the first milestone, a title in the profile at once.
#[test]
fn an_event_is_held_entered_and_a_match_found_from_the_pages() {
    use netrunner_server::protocol::AwardKind;
    use netrunner_server::serve::{ServeOptions, Server};
    let data = std::env::temp_dir().join(format!("netrunner_desktop_event_daemon_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    // The operator's client first, so its key is made before the daemon
    // is told whose it is.
    let (mut operator, operator_dir) = headless_client("operator");
    let operator_key = netrunner_client::identity::Credentials::in_dir(&operator_dir.join("identity")).unwrap().identity.public_key();
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().unwrap();
    let address = runtime.block_on(async {
        let options = ServeOptions { seed: Some(1), data_dir: Some(data.clone()), operators: vec![operator_key], ..ServeOptions::default() };
        let server = Server::bind("127.0.0.1:0", options).await.expect("an ephemeral port binds");
        let address = format!("ws://{}", server.local_addr().unwrap());
        tokio::spawn(server.run());
        address
    });
    let join = |app: &mut App| {
        app.world_mut().resource_mut::<Model>().0.address = address.clone();
        tap_control(app, Control::Go);
        until(app, "the server's key answer", |app| page(app) == Page::Dashboard && app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.key.is_some()));
    };
    let event = |app: &App| app.world().resource::<Model>().0.server.as_ref().and_then(|server| server.open_event().cloned());
    let wins = |app: &App| event(app).and_then(|info| info.mine).map(|mine| (mine.wins, mine.losses));

    join(&mut operator);
    tap_control(&mut operator, Control::Open(Page::Tournaments));
    until(&mut operator, "the list", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.events_listed && server.tournaments_listed));
    assert!(texts(&mut operator).iter().any(|text| text == "This server holds no event."));
    tap_control(&mut operator, Control::Open(Page::MakeEvent));
    assert_eq!(page(&operator), Page::MakeEvent, "the operator's key may");
    {
        let mut model = operator.world_mut().resource_mut::<Model>();
        let make = &mut model.0.make_event;
        make.name = "The Red Planet Gauntlet".to_string();
        make.wins = "2".to_string();
        make.milestones = vec![(Some(AwardKind::Title), "Net-Slinger".to_string()), (Some(AwardKind::Badge), "red-planet".to_string())];
    }
    tap_control(&mut operator, Control::Go);
    until(&mut operator, "the event's page", |app| page(app) == Page::Event && event(app).is_some_and(|info| info.name == "The Red Planet Gauntlet"));
    assert!(find::<Control>(&mut operator, |c| *c == Control::EnterEvent).is_some(), "not entered yet");
    assert!(find::<Control>(&mut operator, |c| *c == Control::CloseEvent).is_some(), "the operator's");
    tap_control(&mut operator, Control::EnterEvent);
    until(&mut operator, "the operator entered", |app| wins(app) == Some((0, 0)));
    assert!(find::<Control>(&mut operator, |c| *c == Control::FindEventMatch).is_some());

    let (mut entrant, _dir2) = headless_client("entrant");
    join(&mut entrant);
    tap_control(&mut entrant, Control::Open(Page::Tournaments));
    until(&mut entrant, "the list with one", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| server.events.len() == 1));
    assert!(find::<Control>(&mut entrant, |c| *c == Control::Open(Page::MakeEvent)).is_none(), "not the operator's key");
    tap_control(&mut entrant, Control::OpenEvent(0));
    assert_eq!(page(&entrant), Page::Event);
    tap_control(&mut entrant, Control::EnterEvent);
    until(&mut entrant, "the entrant entered", |app| wins(app) == Some((0, 0)));
    assert!(texts(&mut entrant).iter().any(|text| text == "Win 1 · title \"Net-Slinger\" · one more win"), "{:?}", texts(&mut entrant));

    // Both look in the pool; the board opens for each.
    tap_control(&mut operator, Control::FindEventMatch);
    until(&mut operator, "the operator in the pool", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(ServerState::seeking));
    assert_eq!(page(&operator), Page::Event, "the search is over the event's page");
    tap_control(&mut entrant, Control::FindEventMatch);
    until(&mut operator, "the operator's board", |app| screen(app) == AppScreen::Game);
    until(&mut entrant, "the entrant's board", |app| screen(app) == AppScreen::Game);
    until(&mut operator, "the operator's first view", |app| app.world().resource::<Board>().0.view.is_some());

    // The operator concedes: the entrant's win, and both back on the page.
    press(&mut operator, KeyCode::Escape, Key::Escape);
    operator.update();
    let confirm = find::<Click>(&mut operator, |c| matches!(c, Click::ConfirmQuit)).expect("leaving asks first");
    tap(&mut operator, confirm);
    until(&mut operator, "Play Online again", |app| screen(app) == AppScreen::Online);
    assert_eq!(page(&operator), Page::Event, "back from a game in the pool, the event's page");
    until(&mut operator, "the operator's loss, pushed", |app| wins(app) == Some((0, 1)));
    until(&mut operator, "the seat let go", |app| app.world().resource::<Model>().0.server.as_ref().is_some_and(|server| !server.seeking()));
    until(&mut entrant, "the entrant's game over", |app| app.world().resource::<Board>().0.over.is_some());
    press(&mut entrant, KeyCode::Escape, Key::Escape);
    entrant.update();
    if let Some(confirm) = find::<Click>(&mut entrant, |c| matches!(c, Click::ConfirmQuit)) {
        tap(&mut entrant, confirm);
    }
    until(&mut entrant, "the entrant back on the event's page", |app| screen(app) == AppScreen::Online && page(app) == Page::Event);
    until(&mut entrant, "the entrant's win, pushed", |app| wins(app) == Some((1, 0)));
    let shown = texts(&mut entrant);
    assert!(shown.iter().any(|text| text == "Win 1 · title \"Net-Slinger\" · earned"), "{shown:?}");
    assert!(shown.iter().any(|text| text == "1 win, 0 losses · one more win clears it"), "{shown:?}");
    until(&mut entrant, "the title in the profile", |app| app.world().resource::<Model>().0.server.as_ref().and_then(ServerState::known_profile).is_some_and(|profile| profile.titles.iter().any(|award| award.name == "Net-Slinger")));
    let _ = std::fs::remove_dir_all(&data);
}
