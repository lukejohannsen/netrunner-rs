//! Play Online: host a game, join one by address or ticket, or watch one
//! (Phase 7 §7). The terminal's screen of the same name
//! (`netrunner_cli::tui::online`) drawn as the desktop's forms are, over
//! the same pieces: `netrunner_client::online` for the deck brought,
//! `hosting::Invitation` for what a host gives out, `remote` for the
//! connection, and `MatchHandle::start_remote` for the match — so the
//! board plays a game online exactly as it plays one at home.
//!
//! **Host runs a server inside this process and joins it like anyone
//! else**, over loopback: the host plays through a masked `ClientView` as
//! their opponent does, and what holds the real `GameState` is the server
//! task, not the screen. Unrated, as the terminal's is: a rating is a
//! claim by a server somebody else runs (`docs/identity-and-rating.md`).
//! The server outlives the screen for as long as the match does — it rides
//! in `ActiveMatch` — and a host who leaves the board concedes before it
//! stops (`HostedServer`).
//!
//! **Nothing here blocks the frame.** Hosting, dialling and listing a
//! server's matches all run on the client's tokio runtime
//! (`core::TokioRuntime`); a system polls what they report once a frame
//! (`net`), which is AGENTS.md §5's rule for Bevy's main thread. The form
//! itself is `models::online`, with no I/O in it.
//!
//! **A ticket is pasted, not typed**: a few hundred characters of base32.
//! The address field takes Ctrl+V (Cmd+V) while it is being edited, as
//! every field does (`widgets::text_field`), and has a Paste button beside
//! it; a host's ticket and addresses each have a Copy button.

use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;

use netrunner_client::connection::Goal;
use netrunner_client::hosting::{self, Invitation, Reach, Way};
use netrunner_client::online;
use netrunner_core::decks::DeckFile;
use netrunner_client::peer::Relay;
use netrunner_client::play::MatchHandle;
use netrunner_client::remote::{self, ConnectEvent, Connecting};
use netrunner_client::settings::{format_name, FORMATS};
use netrunner_server::protocol::format_lobby_id;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::{Side, Viewer};
use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};
use netrunner_server::MatchSummary;

use crate::core::{ClientCore, TokioRuntime};
use crate::models::online::{reach_pill, Field, Intent, OnlineForm, Outcome, Page};
use crate::nav::{screen_root, Captures, InputCaptured, Navigate};
use crate::screens::decks::{read_clipboard, write_clipboard_text};
use crate::screens::new_game::ActiveMatch;
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::widgets::dropdown::{spawn_dropdown, Choice, DropdownChanged};
use crate::widgets::text_field::{TextField, TextFieldEvent};
use crate::widgets::{self, ButtonKind, Pressed};

pub struct OnlinePlugin;

impl Plugin for OnlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::Online), spawn)
            .add_systems(OnExit(AppScreen::Online), |mut commands: Commands| commands.remove_resource::<Net>())
            .add_systems(Update, escape.in_set(Captures).after(crate::widgets::text_field::edit_text_fields).run_if(in_state(AppScreen::Online)))
            .add_systems(Update, (dev_page, controls, fields, net, refresh).chain().run_if(in_state(AppScreen::Online)));
    }
}

/// How long a host's server outlives the board it was left from: long
/// enough for the concession to reach the opponent, over a relay too.
const HOST_LINGER: Duration = Duration::from_secs(3);

/// How long a server has to list its matches.
const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// The server this person is hosting, and what it gives out. Dropping it
/// stops the server — a match on it ends — releases the router's port and
/// closes the ticket's endpoint.
///
/// **Once it carries a match it lingers**: the board it rides on is left
/// by conceding (`MatchHandle::quit`), and a server stopped at once would
/// take the concession down with it, leaving the opponent to wait out the
/// reconnect before learning the game is over. So a server handed to a
/// match (`linger`) is stopped a few seconds after it is let go, on the
/// runtime, and one given up while still waiting is stopped at once, so
/// its port is free to host on again.
///
/// The invitation sits behind a mutex only because the router request in
/// it is `Send` and not `Sync`, and a resource has to be both; one
/// uncontended lock a frame, as `MatchHandle` pays for its receiver.
pub struct HostedServer {
    task: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    invitation: Option<Mutex<Invitation>>,
    runtime: tokio::runtime::Handle,
    linger: bool,
}

impl Drop for HostedServer {
    fn drop(&mut self) {
        let (task, invitation) = (self.task.take(), self.invitation.take());
        if !self.linger {
            if let Some(task) = task {
                task.abort();
            }
            // Dropped inside the runtime: the router's mapping and the
            // ticket's endpoint let go of their resources there.
            let _guard = self.runtime.enter();
            drop(invitation);
            return;
        }
        self.runtime.spawn(async move {
            tokio::time::sleep(HOST_LINGER).await;
            if let Some(task) = task {
                task.abort();
            }
            drop(invitation);
        });
    }
}

/// What a game online adds to the match the board plays.
pub struct OnlineMatch {
    /// The server, when this person is the host.
    pub hosting: Option<HostedServer>,
    pub watching: bool,
    /// Said once, in the log: a host that dealt a deck other than the one
    /// brought, because it predates bringing one.
    pub notice: Option<String>,
}

/// The form, kept between visits so what was typed is still there after a
/// game.
#[derive(Resource)]
pub struct Model(pub OnlineForm);

/// What is running on the network for this screen. Removed on leaving it,
/// which drops a connection still waiting — closing its socket, so a host
/// or a daemon drops the waiter from its lobby rather than pairing someone
/// with a person who has gone — and a server nobody has joined.
#[derive(Resource, Default)]
struct Net {
    connecting: Option<Connecting>,
    hosting: Option<HostedServer>,
    /// A server's list on its way; behind a mutex for `HostedServer`'s
    /// reason.
    listing: Option<Mutex<mpsc::Receiver<Result<Vec<MatchSummary>, String>>>>,
    /// The id of the deck sent, to check against the one dealt.
    brought: Option<String>,
    /// The invitation's lines as last drawn, so the page is redrawn only
    /// when the router or the relay has answered.
    shown: Vec<Way>,
}

#[derive(Resource, Default)]
struct Dirty(bool);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub enum Control {
    Open(Page),
    Back,
    Go,
    List,
    Edit(Field),
    Paste(Field),
    Reach(Reach),
    Format(NsgFormat),
    WatchFrom(Side),
    Watch(usize),
    /// The `n`th thing the host gives out.
    Copy(usize),
}

/// The deck drop-down.
#[derive(Component)]
struct DeckDropdown;

/// Where a field's box sits, so a press on it can put the editor there.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct FieldSlot(Field);

#[derive(Component)]
struct FormRoot;

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, model: Option<ResMut<Model>>) {
    commands.init_resource::<Dirty>();
    commands.insert_resource(Net::default());
    match model {
        Some(mut model) => {
            let decks = deck_choices(&core, model.0.format);
            model.0.reopen(decks);
        }
        None => {
            let format = core.settings.format.unwrap_or(NsgFormat::Startup);
            commands.insert_resource(Model(OnlineForm::new(deck_choices(&core, format), hosting::normalize_address("127.0.0.1"), format)));
        }
    }
    let form = commands.spawn((FormRoot, Node { flex_direction: FlexDirection::Column, row_gap: px(20), width: percent(100), ..default() })).id();
    let panel = commands.spawn(widgets::roomy_panel(&theme, px(940))).add_child(form).id();
    let column = commands
        .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(16), margin: UiRect::vertical(Val::Auto), max_width: percent(100), ..default() })
        .with_children(|parent| {
            parent.spawn(widgets::title(&theme, AppScreen::Online.title()));
            parent.spawn(widgets::dim(&theme, format!("Against a person, as {}. Casual: nothing is rated, and a move cannot be taken back.", core.player_name())));
        })
        .add_child(panel)
        .id();
    commands.spawn(screen_root(AppScreen::Online, &theme)).add_child(column);
    commands.insert_resource(Dirty(true));
}

/// The decks offered: every deck legal in `format`, the lobby chosen,
/// built-in and saved. There is no deal to ask a host for.
fn deck_choices(core: &ClientCore, format: NsgFormat) -> Vec<DeckFile> {
    let decks_dir = core.decks_dir.clone().unwrap_or_else(|| std::env::temp_dir().join("netrunner-no-decks"));
    online::deck_choices(&decks_dir, &core.registry, format)
}

/// Escape steps back a page; from Home the navigation rule takes it off
/// the screen. After the text field's system, so an Escape that cancels
/// an edit does nothing more.
fn escape(keys: Res<ButtonInput<KeyCode>>, mut captured: ResMut<InputCaptured>, model: Option<ResMut<Model>>, mut net: Option<ResMut<Net>>, mut dirty: ResMut<Dirty>) {
    let Some(mut model) = model else { return };
    if captured.0 || !keys.just_pressed(KeyCode::Escape) || model.0.page == Page::Home {
        return;
    }
    captured.0 = true;
    if model.0.apply(Intent::Back) == Outcome::Stop
        && let Some(net) = net.as_mut()
    {
        stop(net);
    }
    dirty.0 = true;
}

fn stop(net: &mut Net) {
    net.connecting = None;
    net.hosting = None;
    net.shown.clear();
}

#[allow(clippy::too_many_arguments)]
fn controls(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    mut chosen: MessageReader<DropdownChanged>,
    marks: Query<&Control>,
    dropdowns: Query<(), With<DeckDropdown>>,
    slots: Query<(Entity, &FieldSlot)>,
    mut model: ResMut<Model>,
    mut net: ResMut<Net>,
    mut dirty: ResMut<Dirty>,
    core: Res<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    theme: Res<Theme>,
    mut clipboard: Option<ResMut<bevy::clipboard::Clipboard>>,
    mut navigate: MessageWriter<Navigate>,
) {
    for DropdownChanged { dropdown, index } in chosen.read() {
        if dropdowns.contains(*dropdown) {
            model.0.apply(Intent::SetDeck(*index));
            dirty.0 = true;
        }
    }
    for Pressed(entity) in pressed.read() {
        let Ok(control) = marks.get(*entity) else { continue };
        let intent = match *control {
            Control::Open(page) => Intent::Open(page),
            Control::Back => Intent::Back,
            Control::Go => Intent::Go,
            Control::List => Intent::List,
            Control::Reach(reach) => Intent::SetReach(reach),
            Control::Format(format) => Intent::SetFormat(format),
            Control::WatchFrom(side) => Intent::SetWatchFrom(side),
            Control::Watch(index) => Intent::Watch(index),
            Control::Edit(field) => {
                // The box becomes the editor, in place; the form is not
                // redrawn until it is committed or let go.
                if let Some((slot, _)) = slots.iter().find(|(_, slot)| slot.0 == field) {
                    let current = text_of(&model.0, field).to_string();
                    commands.entity(slot).despawn_children().with_children(|parent| spawn_editor(parent, &theme, field, &current));
                }
                continue;
            }
            Control::Paste(field) => match read_clipboard(clipboard.as_deref_mut()) {
                Ok(text) => Intent::Typed(field, text.lines().map(str::trim).collect::<String>()),
                Err(reason) => Intent::Failed(reason),
            },
            Control::Copy(index) => {
                if let Some(Way::Give { what, .. }) = net.shown.get(index) {
                    let line = write_clipboard_text(clipboard.as_deref_mut(), what.clone());
                    model.0.notice = Some(line);
                    dirty.0 = true;
                }
                continue;
            }
        };
        let outcome = model.0.apply(intent);
        dirty.0 = true;
        carry_out(outcome, &mut model.0, &mut net, &core, runtime.as_deref(), &mut navigate);
    }
}

/// What a press asked of the network, started. Every failure is the
/// form's notice.
fn carry_out(outcome: Outcome, form: &mut OnlineForm, net: &mut Net, core: &ClientCore, runtime: Option<&TokioRuntime>, navigate: &mut MessageWriter<Navigate>) {
    let needs_runtime = matches!(outcome, Outcome::Host { .. } | Outcome::Join { .. } | Outcome::List { .. } | Outcome::Watch { .. });
    let Some(runtime) = runtime.filter(|_| needs_runtime) else {
        match outcome {
            Outcome::Leave => {
                navigate.write(Navigate(AppScreen::MainMenu));
            }
            Outcome::Stop => stop(net),
            Outcome::Decks(format) => form.set_decks(deck_choices(core, format)),
            Outcome::Nothing | Outcome::Redraw => {}
            _ => {
                form.apply(Intent::Failed("The network runtime did not start, so nothing can be hosted or joined".to_string()));
            }
        }
        return;
    };
    // `remote`, the server and the ticket's endpoint each spawn onto the
    // runtime they are started in.
    let _guard = runtime.0.enter();
    let player = core.player_name();
    match outcome {
        Outcome::Host { port, reach, format, deck } => {
            let relay = match reach {
                Reach::Internet => match Relay::from_setting(core.settings.relay.as_deref()) {
                    Ok(relay) => Some(relay),
                    Err(error) => {
                        form.apply(Intent::Failed(format!("The relay setting: {error}")));
                        return;
                    }
                },
                _ => None,
            };
            host(form, net, core, runtime, port, reach, format, *deck, relay);
        }
        Outcome::Join { url, lobby, password, format, deck } => {
            net.brought = Some(deck.id.clone());
            let lobby = lobby.unwrap_or_else(|| format_lobby_id(format));
            let hello = remote::seat(&player, lobby, password, *deck);
            net.connecting = Some(remote::spawn(url.clone(), Goal::Play(hello)));
            form.apply(Intent::Waiting(format!("Connecting to {}…", shortened(&url))));
        }
        Outcome::Watch { url, match_id } => {
            net.brought = None;
            net.connecting = Some(remote::spawn(url.clone(), Goal::Watch { match_id }));
            form.apply(Intent::Waiting(format!("Connecting to {}…", shortened(&url))));
        }
        Outcome::List { url } => {
            let (tx, rx) = mpsc::channel();
            runtime.0.spawn(async move {
                let answer = match tokio::time::timeout(LIST_TIMEOUT, remote::list_matches(&url)).await {
                    Ok(Ok((matches, _, _))) => Ok(matches),
                    Ok(Err(error)) => Err(error.to_string()),
                    Err(_) => Err(format!("{} did not answer within {}s", shortened(&url), LIST_TIMEOUT.as_secs())),
                };
                let _ = tx.send(answer);
            });
            net.listing = Some(Mutex::new(rx));
        }
        Outcome::Leave | Outcome::Stop | Outcome::Decks(_) | Outcome::Nothing | Outcome::Redraw => unreachable!("needs no runtime"),
    }
}

/// Starts hosting and joins the server as its first seat. Inside the
/// runtime's context (`carry_out`'s guard).
#[allow(clippy::too_many_arguments)]
fn host(form: &mut OnlineForm, net: &mut Net, core: &ClientCore, runtime: &TokioRuntime, port: u16, reach: Reach, format: NsgFormat, deck: DeckFile, relay: Option<Relay>) {
    match start_hosting(port, reach, format, relay, runtime.handle()) {
        Ok((hosting, url)) => {
            net.brought = Some(deck.id.clone());
            let hello = remote::seat_in_format(&core.player_name(), format, deck);
            net.connecting = Some(remote::spawn(url, Goal::Play(hello)));
            net.hosting = Some(hosting);
            net.shown.clear();
            form.apply(Intent::Waiting("Hosting — waiting for your opponent to join".to_string()));
        }
        Err(error) => {
            form.apply(Intent::Failed(format!("Could not host on port {port}: {error}")));
        }
    }
}

/// `NETRUNNER_ONLINE`: a page opened, or a game hosted, once, so the page
/// can be looked at (`dev`). `ticket` hosts with a ticket that names this
/// machine's own addresses — no relay and no router asked, so looking at
/// the page never touches anything outside it. `spectate[-corp]` hosts a
/// game two bots play here and watches it once they have made their
/// moves, so a spectator's board can be looked at at all: nothing else
/// puts one on this machine's screen without a second and a third client.
#[allow(clippy::too_many_arguments)]
fn dev_page(
    mut dev: Option<ResMut<crate::dev::Dev>>,
    mut done: Local<bool>,
    mut spectate: Local<Option<Mutex<mpsc::Receiver<Result<(String, uuid::Uuid), String>>>>>,
    mut model: ResMut<Model>,
    mut net: ResMut<Net>,
    mut dirty: ResMut<Dirty>,
    core: Res<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    mut navigate: MessageWriter<Navigate>,
) {
    let Some(page) = dev.as_ref().and_then(|dev| dev.online.clone()) else { return };
    let form = &mut model.0;
    // The bots have played: the match is watched as a person would.
    let ready = spectate.as_ref().and_then(|rx| rx.lock().ok()?.try_recv().ok());
    if let Some(ready) = ready {
        *spectate = None;
        dirty.0 = true;
        // The bots made the decisions; a spectator is asked none, so the
        // board's own autoplay has nothing to count.
        if let Some(dev) = dev.as_mut() {
            dev.autoplayed = dev.autoplay;
        }
        match ready {
            Ok((url, match_id)) => carry_out(Outcome::Watch { url, match_id }, form, &mut net, &core, runtime.as_deref(), &mut navigate),
            Err(reason) => {
                form.apply(Intent::Failed(reason));
            }
        }
        return;
    }
    if std::mem::replace(&mut *done, true) {
        return;
    }
    dirty.0 = true;
    match page.as_str() {
        "host" => form.apply(Intent::Open(Page::Host)),
        "join" => form.apply(Intent::Open(Page::Join)),
        "watch" => form.apply(Intent::Open(Page::Watch)),
        "hosting" | "ticket" => {
            let Some(runtime) = runtime else { return };
            let _guard = runtime.0.enter();
            form.apply(Intent::Open(Page::Host));
            let relay = (page == "ticket").then_some(Relay::Off);
            let format = form.format;
            let Some(deck) = form.chosen_deck().cloned() else { return };
            host(form, &mut net, &core, &runtime, 0, Reach::Network, format, deck, relay);
            Outcome::Nothing
        }
        "spectate" | "spectate-corp" => {
            let Some(runtime) = runtime else { return };
            let _guard = runtime.0.enter();
            form.apply(Intent::Open(Page::Watch));
            form.apply(Intent::SetWatchFrom(if page == "spectate" { Side::Runner } else { Side::Corp }));
            match start_hosting(0, Reach::ThisMachine, form.format, None, runtime.handle()) {
                Ok((hosting, url)) => {
                    // The server rides with the spectator's match, as a
                    // host's does with their own.
                    net.hosting = Some(hosting);
                    let decisions = dev.as_ref().map_or(0, |dev| dev.autoplay).max(1);
                    let format = form.format;
                    let (tx, rx) = mpsc::channel();
                    runtime.0.spawn(async move {
                        let _ = tx.send(bots_play(url, format, decisions).await);
                    });
                    *spectate = Some(Mutex::new(rx));
                    form.apply(Intent::Waiting(format!("Two bots are playing {decisions} decisions…")))
                }
                Err(error) => form.apply(Intent::Failed(format!("Could not host: {error}"))),
            }
        }
        _ => Outcome::Nothing,
    };
}

/// Two seats at `url`, each submitting a random legal action whenever it
/// is asked, until `decisions` have been made between them — then the
/// match's address and id, for the spectator. The seats stay connected,
/// asked and unanswering, so the board the spectator opens on stays put
/// for the screenshot.
async fn bots_play(url: String, format: NsgFormat, decisions: u32) -> Result<(String, uuid::Uuid), String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use netrunner_server::ServerMessage;
    use netrunner_server::protocol::ClientMessage;

    // Each brings a built-in deck: a server deals nobody one.
    let deck = |id: &str| netrunner_core::decks::by_id(id).expect("a built-in deck");
    let seat = |name: &str, id: &str| remote::connect(&url, Goal::Play(remote::seat_in_format(name, format, deck(id))), |_| {});
    let (one, two) = tokio::join!(seat("Bot one", "brick_stack"), seat("Bot two", "stolen_goods"));
    let (one, two) = (one.map_err(|error| error.to_string())?, two.map_err(|error| error.to_string())?);
    let made = Arc::new(AtomicU32::new(0));
    let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel();
    for (index, mut joined) in [one, two].into_iter().enumerate() {
        let (made, done_tx) = (made.clone(), done_tx.clone());
        tokio::spawn(async move {
            // A xorshift, seeded per seat: random enough to reach a board,
            // and no dependency on a bot crate for a dev hook.
            let mut state = 0x9E37_79B9_7F4A_7C15_u64 ^ index as u64;
            let mut last = None;
            let _keep = joined.link;
            while let Some(message) = joined.rx.recv().await {
                match message {
                    ServerMessage::StateUpdate(view) => last = Some(view),
                    ServerMessage::ActionRejected { .. } => {}
                    _ => continue,
                }
                let Some(view) = last.as_ref().filter(|view| !view.legal_actions.is_empty()) else { continue };
                if made.load(Ordering::SeqCst) >= decisions {
                    let _ = done_tx.send(());
                    continue;
                }
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let action = view.legal_actions[(state % view.legal_actions.len() as u64) as usize].clone();
                made.fetch_add(1, Ordering::SeqCst);
                let _ = joined.tx.send(ClientMessage::SubmitAction(action));
            }
        });
    }
    drop(done_tx);
    done_rx.recv().await.ok_or("the bots' match ended before they had played")?;
    let (matches, _, _) = remote::list_matches(&url).await.map_err(|error| error.to_string())?;
    let summary = matches.first().ok_or("the host lists no match")?;
    Ok((url, summary.match_id))
}

/// An address as it is, a ticket cut to its head: a ticket is a few
/// hundred characters and a status line is one.
fn shortened(address: &str) -> String {
    if address.chars().count() > 48 { format!("{}… (a ticket)", address.chars().take(24).collect::<String>()) } else { address.to_string() }
}

/// Binds a human-vs-human server on `port` and starts it, returning it
/// and the loopback address the host joins by — the terminal's
/// `start_hosting`, whose server is named here because `netrunner_client`
/// may not name it. Port 0 takes any free port.
fn start_hosting(port: u16, reach: Reach, format: NsgFormat, relay: Option<Relay>, runtime: tokio::runtime::Handle) -> Result<(HostedServer, String), String> {
    let options = ServeOptions { bot_runner: ServeBotKind::None, formats: vec![format], ..ServeOptions::default() };
    let listener = hosting::bind_listener(reach, port).map_err(|error| error.to_string())?;
    let server = Server::from_listener(listener, options).map_err(|error| error.to_string())?;
    let port = server.local_addr().map_err(|error| error.to_string())?.port();
    let acceptor = server.acceptor();
    let invitation = Invitation::start(reach, port, relay, move |stream, who| {
        let acceptor = acceptor.clone();
        tokio::spawn(async move { acceptor.serve(stream, &who).await });
    });
    let task = runtime.spawn(server.run());
    Ok((HostedServer { task: Some(task), invitation: Some(Mutex::new(invitation)), runtime, linger: false }, format!("ws://127.0.0.1:{port}")))
}

/// A field's editor committed or let go: the model takes the text, and
/// the form is redrawn either way, which puts the box back.
fn fields(mut commands: Commands, edited: Query<(Entity, &TextField, &TextFieldEvent, &ChildOf)>, slots: Query<&FieldSlot>, mut model: ResMut<Model>, mut dirty: ResMut<Dirty>) {
    for (entity, _, event, child_of) in &edited {
        if let (TextFieldEvent::Committed(text), Ok(FieldSlot(field))) = (event, slots.get(child_of.parent())) {
            model.0.apply(Intent::Typed(*field, text.clone()));
        }
        commands.entity(entity).despawn();
        dirty.0 = true;
    }
}

/// Once a frame: the router's and the relay's answers, a server's list,
/// and the connection — which ends on the board, or back at the form with
/// the reason.
#[allow(clippy::too_many_arguments)]
fn net(mut commands: Commands, mut net: ResMut<Net>, mut model: ResMut<Model>, mut dirty: ResMut<Dirty>, core: Res<ClientCore>, mut navigate: MessageWriter<Navigate>) {
    let invited = net.hosting.as_ref().and_then(|hosting| hosting.invitation.as_ref()).and_then(|invitation| invitation.lock().ok().map(|mut invitation| {
        invitation.poll();
        invitation.ways()
    }));
    if let Some(ways) = invited
        && ways != net.shown
    {
        net.shown = ways;
        dirty.0 = true;
    }
    let answer = net.listing.as_ref().map(|listing| listing.lock().map_or(Err(mpsc::TryRecvError::Disconnected), |rx| rx.try_recv()));
    if let Some(answer) = answer {
        match answer {
            Ok(answer) => {
                model.0.apply(match answer {
                    Ok(matches) => Intent::Listed(matches),
                    Err(reason) => Intent::Failed(reason),
                });
                net.listing = None;
                dirty.0 = true;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => net.listing = None,
        }
    }
    let mut outcome = None;
    let hosting = net.hosting.is_some();
    if let Some(connecting) = &mut net.connecting {
        while let Some(event) = connecting.poll() {
            match event {
                // A host's page reads its invitation; the lobby line is a
                // joiner's.
                ConnectEvent::Queued(position) if !hosting => {
                    model.0.apply(Intent::Status(format!("In the lobby, waiting for an opponent ({position} waiting)…")));
                    dirty.0 = true;
                }
                ConnectEvent::Queued(_) => {}
                ConnectEvent::Link(link) => {
                    if let Some(line) = link.status_line(Instant::now()) {
                        model.0.apply(Intent::Status(line));
                        dirty.0 = true;
                    }
                }
                other => {
                    outcome = Some(other);
                    break;
                }
            }
        }
    }
    let Some(outcome) = outcome else { return };
    net.connecting = None;
    dirty.0 = true;
    let joined = match outcome {
        ConnectEvent::Joined(joined) => joined,
        ConnectEvent::Failed(error) => {
            net.hosting = None;
            model.0.apply(Intent::Failed(error.to_string()));
            return;
        }
        ConnectEvent::Queued(_) | ConnectEvent::Link(_) => unreachable!("handled in the loop"),
    };
    let watching = joined.viewer == Viewer::Spectator;
    let dealt = match joined.viewer {
        Viewer::Player(Side::Corp) => Some(joined.decks.0.clone()),
        Viewer::Player(Side::Runner) => Some(joined.decks.1.clone()),
        Viewer::Spectator => None,
    };
    let notice = match (net.brought.take(), dealt) {
        (Some(brought), Some(dealt)) if brought != dealt => Some(format!("This host dealt you {dealt:?} instead of your deck: it predates bringing your own")),
        _ => None,
    };
    match MatchHandle::start_remote(core.registry.clone(), *joined, model.0.watch_from) {
        Ok(handle) => {
            let hosting = net.hosting.take().map(|mut hosting| {
                hosting.linger = true;
                hosting
            });
            commands.insert_resource(ActiveMatch { handle, choice: None, lesson: None, starter: None, online: Some(OnlineMatch { hosting, watching, notice }) });
            navigate.write(Navigate(AppScreen::Game));
        }
        Err(error) => {
            net.hosting = None;
            model.0.apply(Intent::Failed(error));
        }
    }
}

fn text_of(form: &OnlineForm, field: Field) -> &str {
    match field {
        Field::Address => &form.address,
        Field::Lobby => &form.lobby,
        Field::Password => &form.password,
        Field::Port => &form.port,
    }
}

fn refresh(mut commands: Commands, mut dirty: ResMut<Dirty>, root: Query<Entity, With<FormRoot>>, theme: Res<Theme>, model: Res<Model>, net: Res<Net>) {
    if !std::mem::take(&mut dirty.0) {
        return;
    }
    let Ok(root) = root.single() else { return };
    commands.entity(root).despawn_children().with_children(|parent| spawn_page(parent, &theme, &model.0, &net));
}

fn spawn_page(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, net: &Net) {
    match form.page {
        Page::Home => spawn_home(parent, theme),
        Page::Host => spawn_host(parent, theme, form),
        Page::Join => spawn_join(parent, theme, form),
        Page::Watch => spawn_watch(parent, theme, form, net.listing.is_some()),
        Page::Waiting => spawn_waiting(parent, theme, form, net),
    }
    if let Some(notice) = &form.notice {
        parent.spawn((widgets::notice(theme, notice.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
}

/// The three ways in, each a pill with what it does beside it — the main
/// menu's rows.
fn spawn_home(parent: &mut ChildSpawnerCommands, theme: &Theme) {
    let ways = [
        (Page::Host, "Host a game", "Play on this machine and give your opponent an address, or a ticket that works from anywhere"),
        (Page::Join, "Join a game", "Connect to a host by the address or the ticket it gave you, or to a public server"),
        (Page::Watch, "Watch a game", "List a server's matches and watch one, seeing what both players can see"),
    ];
    for (index, (page, label, blurb)) in ways.into_iter().enumerate() {
        let kind = if index == 0 { ButtonKind::Primary } else { ButtonKind::Secondary };
        parent.spawn(widgets::row(20.0)).with_children(|row| {
            row.spawn(widgets::styled_button(theme, kind, label, px(230), Control::Open(page)));
            row.spawn((widgets::dim(theme, blurb), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }));
        });
    }
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, Control::Back));
    });
}

fn spawn_host(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm) {
    parent.spawn(widgets::heading(theme, "Host a game"));
    section(parent, theme, "Who can join", |section| {
        section.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), ..default() }).with_children(|row| {
            for reach in Reach::ALL {
                let kind = if reach == form.reach { ButtonKind::Primary } else { ButtonKind::Secondary };
                row.spawn(widgets::styled_button(theme, kind, reach_pill(reach), Val::Auto, Control::Reach(reach)));
            }
        });
        section.spawn(widgets::dim(theme, capitalised(form.reach.label())));
    });
    section(parent, theme, "Port", |section| field_box(section, theme, Field::Port, &form.port, "", false));
    format_section(parent, theme, form, "The format this game is played in: your opponent joins with a deck legal in it.");
    deck_section(parent, theme, form);
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, Control::Back));
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Start hosting", px(220), Control::Go));
    });
}

fn spawn_join(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm) {
    parent.spawn(widgets::heading(theme, "Join a game"));
    section(parent, theme, "Address or ticket", |section| field_box(section, theme, Field::Address, &form.address, "the host's address, or paste its ticket", true));
    format_section(parent, theme, form, "You are paired with whoever is waiting in this format's lobby, or in the lobby named below.");
    section(parent, theme, "Lobby", |section| {
        field_box(section, theme, Field::Lobby, &form.lobby, "none — the format's own lobby", false);
        field_box(section, theme, Field::Password, &form.password, "no password", false);
    });
    deck_section(parent, theme, form);
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, Control::Back));
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Connect", px(220), Control::Go));
    });
}

fn spawn_watch(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, listing: bool) {
    parent.spawn(widgets::heading(theme, "Watch a game"));
    section(parent, theme, "Server", |section| {
        field_box(section, theme, Field::Address, &form.address, "a server's address, or a host's ticket", true);
        section.spawn(widgets::row(12.0)).with_children(|row| {
            let label = if listing { "Asking…" } else if form.listed { "Refresh" } else { "List its matches" };
            row.spawn(widgets::styled_button(theme, ButtonKind::Secondary, label, Val::Auto, Control::List));
        });
    });
    section(parent, theme, "Watch from", |section| {
        section.spawn(widgets::row(10.0)).with_children(|row| {
            for side in [Side::Corp, Side::Runner] {
                let kind = if side == form.watch_from { ButtonKind::Primary } else { ButtonKind::Secondary };
                row.spawn(widgets::styled_button(theme, kind, format!("The {side:?}'s side"), Val::Auto, Control::WatchFrom(side)));
            }
        });
        section.spawn(widgets::dim(theme, "Which side of the table is drawn nearer. A spectator sees what both players can see, and neither hand."));
    });
    if !form.matches.is_empty() {
        section(parent, theme, "Matches", |section| {
            for (index, summary) in form.matches.iter().enumerate() {
                // No decks: a server names none (`MatchSummary`).
                let lobby = format!(" · {}", format_name(summary.format));
                let line = format!("{} (Corp) vs {} (Runner){lobby} · {} min in", summary.corp, summary.runner, summary.started_secs_ago / 60);
                section.spawn(widgets::styled_button(theme, ButtonKind::Secondary, line, percent(100), Control::Watch(index)));
            }
        });
    }
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, Control::Back));
    });
}

/// Hosting: everything to give out, each with a Copy; joining: where the
/// connection is. Either way, Stop.
fn spawn_waiting(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, net: &Net) {
    let hosting = net.hosting.is_some();
    parent.spawn(widgets::heading(theme, if hosting { "Hosting — waiting for your opponent" } else { "Waiting" }));
    if hosting {
        parent.spawn(widgets::dim(theme, "Give your opponent one of these. The game starts when they join."));
        for (index, way) in net.shown.iter().enumerate() {
            match way {
                Way::Give { what, who, ticket } => {
                    parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }).with_children(|give| {
                        give.spawn(widgets::overline(theme, if *ticket { format!("Ticket · {who}") } else { capitalised(who) }));
                        give.spawn(widgets::row(12.0)).with_children(|row| {
                            row.spawn((
                                Text::new(what.clone()),
                                theme.font(if *ticket { size::SMALL } else { size::BODY }),
                                TextColor(theme.text),
                                // A ticket has no spaces to break at.
                                TextLayout::new(Justify::Left, LineBreak::AnyCharacter),
                                Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() },
                            ));
                            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Copy", Control::Copy(index)));
                        });
                    });
                }
                Way::Note(note) => {
                    parent.spawn((widgets::dim(theme, note.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
            }
        }
    } else {
        parent.spawn((widgets::dim(theme, form.status.clone()), TextLayout::new(Justify::Left, LineBreak::AnyCharacter)));
    }
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Stop", Val::Auto, Control::Back));
    });
}

/// The format, as pills: four choices are a row, not a drop-down.
fn format_section(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, blurb: &str) {
    section(parent, theme, "Format", |section| {
        section.spawn(widgets::row(10.0)).with_children(|row| {
            for format in FORMATS {
                let kind = if format == form.format { ButtonKind::Primary } else { ButtonKind::Secondary };
                row.spawn(widgets::styled_button(theme, kind, capitalised(format_name(format)), Val::Auto, Control::Format(format)));
            }
        });
        section.spawn(widgets::dim(theme, blurb));
    });
}

fn deck_section(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm) {
    section(parent, theme, "Your deck — its side is your seat", |section| {
        let choices = form.decks.iter().map(|deck| Choice::plain(online::label(deck))).collect();
        spawn_dropdown(section, theme, "", choices, form.deck, DeckDropdown);
    });
}

/// A field as a box showing its text, which a press turns into the
/// editor; `paste` puts a Paste beside it, for a ticket.
fn field_box(parent: &mut ChildSpawnerCommands, theme: &Theme, field: Field, value: &str, placeholder: &str, paste: bool) {
    parent.spawn(widgets::row(12.0)).with_children(|row| {
        row.spawn((FieldSlot(field), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() })).with_children(|slot| {
            let (shown, colour) = if value.is_empty() { (placeholder.to_string(), theme.text_dim) } else { (shortened(value), theme.text) };
            slot.spawn((
                Button,
                widgets::Themed,
                Control::Edit(field),
                widgets::field_node(percent(100)),
                BackgroundColor(theme.glass),
                BorderColor::all(theme.glass_border),
                children![(Text::new(shown), theme.font(size::BODY), TextColor(colour), TextLayout::new(Justify::Left, LineBreak::AnyCharacter))],
            ));
        });
        if paste {
            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Paste", Control::Paste(field)));
        }
    });
}

/// The editor in a field's place: the text so far, the keys going to it
/// (Enter keeps, Escape lets go, Ctrl+V pastes).
fn spawn_editor(parent: &mut ChildSpawnerCommands, theme: &Theme, field: Field, current: &str) {
    parent.spawn((
        TextField { text: current.to_string(), max_len: field.max_len() },
        widgets::field_node(percent(100)),
        BackgroundColor(theme.glass_strong),
        BorderColor::all(theme.accent),
        children![(Text::new(format!("{current}|")), theme.font(size::BODY), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::AnyCharacter))],
    ));
}

fn section(parent: &mut ChildSpawnerCommands, theme: &Theme, name: impl Into<String>, content: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(10), ..default() }).with_children(|section| {
        section.spawn(widgets::overline(theme, name));
        content(section);
    });
}

fn buttons(parent: &mut ChildSpawnerCommands, content: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, justify_content: JustifyContent::FlexEnd, column_gap: px(12), margin: UiRect::top(px(8)), ..default() })
        .with_children(content);
}

fn capitalised(words: &str) -> String {
    let mut chars = words.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}
