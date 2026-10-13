//! Play Online: the Server Dashboard (Phase 7 §14, 12 October 2026),
//! over the same pieces the terminal's screen of the same name uses
//! (`netrunner_cli::tui::online`): `netrunner_client::online` for the
//! active decks and the queue's words, `hosting::Invitation` for what a
//! host gives out, `remote` for the connection, and
//! `MatchHandle::start_remote` for the match — so the board plays a game
//! online exactly as it plays one at home. The dashboard, its queue form
//! and its search are `models::online`'s to decide and this file's to
//! draw: the two active decks with their identities' faces and their
//! legality, the three doors as tiles, the column of ways elsewhere, and
//! the two overlays — a form centred on a wash, as a choice is, and the
//! search with its pulse and the time waited.
//!
//! **The connection outlives the game** (Phase 4 §7 stage 4c). Hosting
//! and connecting both end on the dashboard, attached. The [`Connected`]
//! resource holds the connection, and it is not a part of this screen's
//! tree: it stays while the board plays the game and is polled again when
//! the board leads back here, where the `BackInLobby` is waiting and the
//! next game is a door away. It goes when the person disconnects — a
//! row's Disconnect on the servers page — or leaves the screen for
//! anywhere but the board; the Default is dialled again on the next
//! visit.
//!
//! **Host runs a server inside this process and attaches to it like
//! anyone else**, over loopback: the host plays through a masked
//! `ClientView` as their opponent does, and what holds the real
//! `GameState` is the server task, not the screen. Unrated, as the
//! terminal's is: a rating is a claim by a server somebody else runs
//! (`docs/identity-and-rating.md`). The server rides in `Connected` with
//! the connection, and once a game has been played on it it lingers a few
//! seconds after it is let go, so a concession made by leaving the board
//! reaches the opponent before the server stops (`HostedServer`).
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
//!
//! **Tournaments are pages under the Server page** (Phase 4 §7 stage 6b,
//! the terminal's rows as the desktop's forms): the server's tournaments
//! listed, each a button to its page — its code, its entrants, and for a
//! key that has one the two decks to lock in and Register, or Withdraw —
//! and one held from a form of its own by the key this connection proved.
//! The deck drop-downs are the lobby's on the Server page and the entry's
//! on a tournament's (`Intent::SetCorpDeck` reads the page).

use std::sync::{mpsc, Mutex};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use bevy::prelude::*;

use netrunner_client::connection::{Goal, Link, Who};
use netrunner_client::hosting::{self, Invitation, Reach, Way};
use netrunner_client::identity::ProfileHere;
use netrunner_client::card_face::Face;
use netrunner_client::deck_builder::CardBook;
use netrunner_client::deck_picker;
use netrunner_client::online::{self, opponent_label, preference_label, rung_label, searching_line, ActiveDeck, Target};
use netrunner_client::peers::PeerBook;
use netrunner_client::tournament;
use netrunner_client::servers::{self, ServerBook};
use netrunner_core::decks::DeckFile;
use netrunner_client::peer::Relay;
use netrunner_client::play::MatchHandle;
use netrunner_client::remote::{self, Attached, AttachedEvent, ConnectEvent, Connecting};
use netrunner_client::settings::{format_name, FORMATS};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::{Side, Viewer};
use netrunner_server::protocol::swiss::Outcome as TableOutcome;
use netrunner_server::protocol::{Level, LobbyInfo, Opponent, QueueMode, TournamentState};
use netrunner_server::serve::{ServeOptions, Server};
use netrunner_server::MatchSummary;

use crate::audio::{ButtonSound, Sfx};
use crate::card_images::CardImages;
use crate::core::{ClientCore, TokioRuntime};
use crate::models::online::{lobby_title, AwardKind, Field, Intent, OnlineForm, Outcome, Page, RowProfile, Searching, ServerState};
use crate::widgets::badge::{self, BadgeGlyphs};
use crate::widgets::card_face::{spawn_face, FaceSize};
use crate::widgets::reader::Readable;
use crate::nav::{screen_root, Captures, InputCaptured, Navigate};
use crate::screens::new_game::ActiveMatch;
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::screens::deck_picker::{DeckPicked, OpenDeckPicker, Purpose};
use crate::widgets::text_field::{TextField, TextFieldEvent};
use crate::widgets::{self, ButtonKind, Pressed};

pub struct OnlinePlugin;

impl Plugin for OnlinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BadgeGlyphs>()
            .add_systems(OnEnter(AppScreen::Online), spawn)
            .add_systems(OnExit(AppScreen::Online), leave)
            .add_systems(Update, escape.in_set(Captures).after(crate::widgets::text_field::edit_text_fields).run_if(in_state(AppScreen::Online)))
            .add_systems(Update, (dev_page, dev_guest, opening, controls, chooser_wash, fields, net, round_clock_ticks, search_clock_ticks, room_ticks, refresh, turn_pulse).chain().run_if(in_state(AppScreen::Online)));
    }
}

/// How long a host's server outlives the connection it was let go with:
/// long enough for a concession to reach the opponent, over a relay too.
const HOST_LINGER: Duration = Duration::from_secs(3);

/// How long a server has to list its matches, or to say what it keeps
/// of the person for their row.
const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// Set on opening the screen afresh with nothing attached: on the first
/// frame the Default is connected to, as a press of its Connect would
/// (Phase 7 §14), or with no Default the servers page is opened and asks
/// its rows — and never again until the screen is opened afresh.
#[derive(Resource)]
struct Opening;

/// Set by the main menu's Local / P2P Play: the screen opens on the hub
/// (Phase 7 §15) rather than connecting to the Default. Taken on entry.
#[derive(Resource)]
pub struct OpenHub;

/// The server this person is hosting, and what it gives out. Dropping it
/// stops the server — a match on it ends — releases the router's port and
/// closes the ticket's endpoint.
///
/// **Once it has carried a match it lingers**: the board is left by
/// conceding (`MatchHandle::quit`), and a server stopped at once would
/// take the concession down with it, leaving the opponent to wait out the
/// reconnect before learning the game is over. So a server a game was
/// played on (`linger`) is stopped a few seconds after it is let go, on
/// the runtime, and one given up before any game is stopped at once, so
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
    /// A server that rides with the match alone — the one the `spectate`
    /// dev hook has two bots play on. A person's own server rides in
    /// [`Connected`], with the connection.
    pub hosting: Option<HostedServer>,
    pub watching: bool,
    /// Said once, in the log.
    pub notice: Option<String>,
}

/// The form, kept between visits so what was typed is still there after a
/// game.
#[derive(Resource)]
pub struct Model(pub OnlineForm);

/// The attached connection, and the server it is to when this person
/// hosts. Not removed with the screen's tree: it stays while the board
/// plays a game found on it, and goes when the person disconnects or
/// leaves the screen for anywhere else (`leave`). Dropping it closes the
/// socket — once the game's own channel has gone too — so a player looking
/// for a game leaves the lobby rather than being paired after they have
/// gone, and stops the hosted server.
#[derive(Resource)]
pub struct Connected {
    attached: Attached,
    hosting: Option<HostedServer>,
}

impl Connected {
    /// Whether this person hosts the server they are attached to.
    pub fn is_hosting(&self) -> bool {
        self.hosting.is_some()
    }
}

/// What else is running on the network for this screen. Removed on
/// leaving it, which drops a spectator's connection still waiting and a
/// dev hook's server nobody has joined.
#[derive(Resource, Default)]
struct Net {
    /// A spectator's connection, until it has its place.
    connecting: Option<Connecting>,
    /// The `spectate` dev hook's server, for the bots.
    hosting: Option<HostedServer>,
    /// A server's list on its way; behind a mutex for `HostedServer`'s
    /// reason.
    listing: Option<Mutex<mpsc::Receiver<Result<Vec<MatchSummary>, String>>>>,
    /// The invitation's lines as last drawn, so the page is redrawn only
    /// when the router or the relay has answered.
    shown: Vec<Way>,
    /// The servers' answers for their rows, as they come.
    profiles: Option<Mutex<mpsc::Receiver<(String, Result<Option<netrunner_server::protocol::Profile>, String>)>>>,
    /// The `room-guest` dev hook's other player, attached to the hook's
    /// own server so the room has someone in it to look at.
    extra: Option<Attached>,
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
    /// Make a lobby's: listed, or joined by its code.
    /// Make a friend queue's: whether its members may ask for a bot.
    LobbyBots(bool),
    WatchFrom(Side),
    Watch(usize),
    /// The `n`th thing the host gives out.
    Copy(usize),
    /// A friend lobby's code, for the group.
    CopyCode,
    /// The dashboard's doors and the queue form over it: a door, its
    /// format, its Cancel, and Find match.
    OpenQueue(QueueMode),
    QueueFormat(NsgFormat),
    CloseQueue,
    FindMatch,
    /// The dashboard's column: a screen elsewhere.
    Nav(AppScreen),
    /// The Casual form's: a friend lobby by its code. The lobby's page:
    /// out of it, and a bot where it lets one in.
    JoinById,
    LeaveLobby,
    /// The tournaments pages': the list again.
    Refresh,
    Prefer(Option<Side>),
    Opponent(Option<Level>),
    /// A friend lobby's Find a game, and the search's Abort.
    Seek,
    CancelSeek,
    /// The tournaments page's and a tournament's.
    OpenTournament(usize),
    Register,
    Unregister,
    /// A tournament's rounds: the seat at one's table, and the
    /// organizer's next round, recorded result and end.
    Sit,
    BeginRound,
    FinishTournament,
    RecordResult(usize, TableOutcome),
    OfferDraw,
    Disconnect,
    /// The servers page's, by row; `EditRow(None)` is Add a server.
    ConnectRow(usize),
    DefaultRow(usize),
    EditRow(Option<usize>),
    RemoveRow(usize),
    /// The hub's (Phase 7 §15): a saved peer joined, renamed, removed;
    /// the rename form's Save.
    QuickConnect(usize),
    RenamePeer(usize),
    RemovePeer(usize),
    SavePeer,
    /// The room's: ready to play, or not after all.
    Ready,
    Unready,
    /// The profile page's: a pop-up of the titles or the badges earned,
    /// a pick from it, and its Cancel.
    Choose(AwardKind),
    Pick(Option<usize>),
    CloseChooser,
    Ladder(NsgFormat),
}

/// The wash the award pop-up sits on; a press on it closes the pop-up.
#[derive(Component)]
struct ChooserWash;

/// A deck slot's Change… button: opens the deck picker for this purpose
/// — an active deck, on the queue form's format if one is open, or a
/// tournament's entry on the tournament's — the cursor on the deck
/// chosen now.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct DeckSlot {
    pub purpose: Purpose,
    pub chosen: String,
    pub format: NsgFormat,
}

/// The chosen deck's line in a slot, for a test to read.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeckLine(pub Purpose);

/// Where a field's box sits, so a press on it can put the editor there.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct FieldSlot(Field);

#[derive(Component)]
struct FormRoot;

/// The panel the pages are drawn in, so the dashboard can take a wider
/// one than the forms.
#[derive(Component)]
struct PanelRoot;

/// The search's pulse, turned a little each frame.
#[derive(Component)]
struct Pulse;

/// The dashboard's overlays — the queue form and the search — which a
/// redraw takes down with the page, or the search would stack on itself
/// every second.
#[derive(Component)]
struct Overlay;

/// The width of the panel a form is drawn in, and the dashboard's, which
/// has two columns and three tiles to fit.
const FORM_WIDTH: f32 = 940.0;
const DASHBOARD_WIDTH: f32 = 1080.0;

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, model: Option<ResMut<Model>>, connected: Option<Res<Connected>>, hub: Option<Res<OpenHub>>) {
    commands.init_resource::<Dirty>();
    commands.insert_resource(Net::default());
    // The active decks are read on every visit: the deck builder may have
    // changed one since; so are the saved peers.
    let active = active_decks(&core);
    let open_hub = hub.is_some();
    commands.remove_resource::<OpenHub>();
    let peers = core.peer_book();
    match model {
        Some(mut model) => {
            model.0.reopen(connected.is_some());
            model.0.apply(Intent::ActiveDecks(Box::new(active)));
            model.0.apply(Intent::Peers(peers));
            model.0.player = core.player_name();
            if open_hub && connected.is_none() {
                model.0.apply(Intent::Open(Page::Hub));
            } else if connected.is_none() {
                commands.insert_resource(Opening);
            }
        }
        None => {
            let format = core.settings.format.unwrap_or(netrunner_client::settings::DEFAULT_FORMAT);
            // Connect once starts from the Default, else the server
            // connected to last — both from the list both clients keep
            // (Phase 7 §14).
            let address = core.server_book().start_address().map_or_else(|| hosting::normalize_address("127.0.0.1"), str::to_string);
            let mut form = OnlineForm::new(address, format).with_active(active).with_player(core.player_name());
            if open_hub {
                form.apply(Intent::Open(Page::Hub));
            } else if connected.is_none() {
                commands.insert_resource(Opening);
            }
            form.apply(Intent::Servers(core.server_book()));
            form.apply(Intent::Local(local_lines(&core)));
            form.apply(Intent::Peers(peers));
            commands.insert_resource(Model(form));
        }
    }
    let form = commands.spawn((FormRoot, Node { flex_direction: FlexDirection::Column, row_gap: px(20), width: percent(100), flex_shrink: 1.0, min_height: px(0), ..default() })).id();
    let panel = commands.spawn((PanelRoot, widgets::roomy_panel(&theme, px(FORM_WIDTH)))).add_child(form).id();
    let column = commands
        .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(16), margin: UiRect::vertical(Val::Auto), max_width: percent(100), max_height: percent(100), ..default() })
        .with_children(|parent| {
            parent.spawn(widgets::title(&theme, AppScreen::Online.title()));
            parent.spawn(widgets::dim(&theme, format!("As {}. Ranked is rated, Casual is not, a friend lobby is joined by its code, and a friend is played directly from the hub.", core.player_name())));
        })
        .add_child(panel)
        .id();
    commands.spawn(screen_root(AppScreen::Online, &theme)).add_child(column);
    commands.insert_resource(Dirty(true));
}

/// The active decks (`Settings::active_deck`, Phase 7 §16) as the
/// dashboard draws them and a seek brings them: each read off the one
/// list every picker reads, with its standing in every format. An id
/// that names no deck is `None`, and the dashboard says so.
fn active_decks(core: &ClientCore) -> [Option<ActiveDeck>; 2] {
    let book = CardBook::new(&core.registry, &core.catalog);
    let format = core.settings.format.unwrap_or(netrunner_client::settings::DEFAULT_FORMAT);
    let (rows, _problems) = deck_picker::rows(core.decks_dir.as_deref(), book, format);
    let find = |side: Side| {
        let id = core.settings.active_deck_or_default(side);
        rows.iter().find(|row| row.deck.id == id && row.deck.side == side).map(|row| ActiveDeck::of(row, book))
    };
    [find(Side::Corp), find(Side::Runner)]
}

/// What this machine knows of the person, for the servers page: the key
/// and its file, and the record against the bots — the local half of
/// "who I am", beside each server's half.
fn local_lines(core: &ClientCore) -> Vec<String> {
    // The fingerprint and the file it is kept in; the paragraph after
    // them is the Profile screen's.
    let mut lines: Vec<String> = netrunner_client::identity::key_lines_in(core.identity_dir.as_deref()).into_iter().take(2).collect();
    lines.push(format!("Card images in {}", core.images.dir().display()));
    if let Some(path) = &core.record_path {
        lines.push(netrunner_client::record::summary_line(path, &core.player_name()));
    }
    lines
}

/// The first frame of a visit with nothing attached: the Default is
/// connected to, or without one the servers page is opened and its rows
/// asked.
#[allow(clippy::too_many_arguments)]
fn opening(
    mut commands: Commands,
    pending: Option<Res<Opening>>,
    dev: Option<Res<crate::dev::Dev>>,
    mut model: ResMut<Model>,
    mut net: ResMut<Net>,
    mut dirty: ResMut<Dirty>,
    mut core: ResMut<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    mut navigate: MessageWriter<Navigate>,
) {
    if pending.is_none() {
        return;
    }
    commands.remove_resource::<Opening>();
    // A dev hook that opened a page of its own this frame (`dev_page`,
    // before this) is left on it; the hooks that need the Default — the
    // dashboard, its forms, the profile — are what this connects for.
    let needs_default = |page: &str| page == "dashboard" || page.starts_with("profile") || matches!(page, "queue" | "queue-ranked" | "searching" | "make-lobby" | "friend-lobby");
    if dev.as_ref().and_then(|dev| dev.online.as_deref()).is_some_and(|page| !needs_default(page)) {
        return;
    }
    let mut outcome = model.0.apply(Intent::ConnectDefault);
    if outcome == Outcome::Nothing {
        outcome = model.0.apply(Intent::Open(Page::Servers));
    }
    dirty.0 = true;
    carry_out(outcome, &mut model.0, &mut net, &mut commands, &mut core, runtime.as_deref(), &mut navigate);
}

/// Leaving the screen: the connection stays only for the board, which
/// plays a game found on it and leads back here.
fn leave(world: &mut World) {
    world.remove_resource::<Net>();
    world.remove_resource::<Opening>();
    let to_the_board = world.get_resource::<ActiveMatch>().is_some_and(|active| active.online.is_some());
    if !to_the_board {
        world.remove_resource::<Connected>();
    }
}

/// The decks offered: every deck legal in `format`, the lobby's, built-in
/// and saved. There is no deal to ask a host for.
fn deck_choices(core: &ClientCore, format: NsgFormat) -> Vec<DeckFile> {
    let decks_dir = core.decks_dir.clone().unwrap_or_else(|| std::env::temp_dir().join("netrunner-no-decks"));
    online::deck_choices(&decks_dir, &core.registry, format)
}

/// Escape closes the queue form, aborts a search, leaves a friend lobby
/// or steps back a page; where the model says it is the way off the
/// screen, the navigation rule takes it. After the text field's system,
/// so an Escape that cancels an edit does nothing more.
#[allow(clippy::too_many_arguments)]
fn escape(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut captured: ResMut<InputCaptured>,
    model: Option<ResMut<Model>>,
    mut net: Option<ResMut<Net>>,
    mut dirty: ResMut<Dirty>,
    mut core: ResMut<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    mut navigate: MessageWriter<Navigate>,
) {
    let Some(mut model) = model else { return };
    if captured.0 || !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    let outcome = model.0.apply(Intent::Back);
    if outcome == Outcome::Leave {
        return;
    }
    captured.0 = true;
    if let Some(net) = net.as_mut() {
        carry_out(outcome, &mut model.0, net, &mut commands, &mut core, runtime.as_deref(), &mut navigate);
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
    mut picked: MessageReader<DeckPicked>,
    marks: Query<&Control>,
    deck_slots: Query<&DeckSlot>,
    mut open_picker: MessageWriter<OpenDeckPicker>,
    slots: Query<(Entity, &FieldSlot)>,
    mut model: ResMut<Model>,
    mut net: ResMut<Net>,
    mut dirty: ResMut<Dirty>,
    mut core: ResMut<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    theme: Res<Theme>,
    mut clipboard: Option<ResMut<bevy::clipboard::Clipboard>>,
    mut navigate: MessageWriter<Navigate>,
) {
    // A deck chosen in the picker: an active deck is the settings' now
    // (the picker wrote them), so the two are read afresh; a tournament's
    // entry goes to the page's choice.
    for DeckPicked { purpose, id } in picked.read() {
        let intent = match purpose {
            Purpose::Active(_) => Some(Intent::ActiveDecks(Box::new(active_decks(&core)))),
            Purpose::Tournament(side) => {
                let Some(server) = &model.0.server else { continue };
                let position = server.tournament_side_decks(*side).iter().position(|deck| deck.id == *id);
                position.map(if *side == Side::Corp { Intent::SetCorpDeck } else { Intent::SetRunnerDeck })
            }
            Purpose::NewGame(_) => None,
        };
        if let Some(intent) = intent {
            model.0.apply(intent);
            dirty.0 = true;
        }
    }
    for Pressed(entity) in pressed.read() {
        if let Ok(slot) = deck_slots.get(*entity) {
            let side = match slot.purpose {
                Purpose::Active(side) | Purpose::Tournament(side) => side,
                Purpose::NewGame(_) => continue,
            };
            open_picker.write(OpenDeckPicker { side, format: slot.format, purpose: slot.purpose, chosen: Some(slot.chosen.clone()) });
            continue;
        }
        let Ok(control) = marks.get(*entity) else { continue };
        let intent = match *control {
            Control::Open(page) => Intent::Open(page),
            Control::Back => Intent::Back,
            Control::Go => Intent::Go,
            Control::List => Intent::List,
            Control::Reach(reach) => Intent::SetReach(reach),
            Control::Format(format) => Intent::SetFormat(format),
            Control::LobbyBots(bots) => Intent::SetLobbyBots(bots),
            Control::WatchFrom(side) => Intent::SetWatchFrom(side),
            Control::Watch(index) => Intent::Watch(index),
            Control::OpenQueue(mode) => Intent::OpenQueue(mode),
            Control::QueueFormat(format) => Intent::SetQueueFormat(format),
            Control::CloseQueue => Intent::CloseQueue,
            Control::FindMatch => Intent::FindMatch,
            Control::Nav(screen) => {
                navigate.write(Navigate(screen));
                continue;
            }
            Control::JoinById => Intent::JoinById,
            Control::LeaveLobby => Intent::LeaveLobby,
            Control::Refresh => Intent::Refresh,
            Control::Prefer(prefer) => Intent::SetPrefer(prefer),
            Control::Opponent(bot) => Intent::SetOpponent(bot),
            Control::Seek => Intent::Seek,
            Control::CancelSeek => Intent::CancelSeek,
            Control::OpenTournament(index) => Intent::OpenTournament(index),
            Control::Register => Intent::Register,
            Control::Unregister => Intent::Unregister,
            Control::Sit => Intent::Sit,
            Control::BeginRound => Intent::BeginRound,
            Control::FinishTournament => Intent::FinishTournament,
            Control::RecordResult(table, outcome) => Intent::RecordResult(table, outcome),
            Control::OfferDraw => Intent::OfferDraw,
            Control::Disconnect => Intent::Disconnect,
            Control::ConnectRow(index) => Intent::ConnectRow(index),
            Control::DefaultRow(index) => Intent::ToggleDefault(index),
            Control::EditRow(index) => Intent::EditRow(index),
            Control::RemoveRow(index) => Intent::RemoveRow(index),
            Control::QuickConnect(index) => Intent::QuickConnect(index),
            Control::RenamePeer(index) => Intent::RenamePeer(index),
            Control::RemovePeer(index) => Intent::RemovePeer(index),
            Control::SavePeer => Intent::SavePeer,
            Control::Ready => Intent::Ready,
            Control::Unready => Intent::Unready,
            Control::Choose(kind) => Intent::Choose(kind),
            Control::Pick(index) => Intent::Pick(index),
            Control::CloseChooser => Intent::CloseChooser,
            Control::Ladder(format) => Intent::ShowLadder(format),
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
            Control::CopyCode => {
                if let Some(code) = model.0.server.as_ref().and_then(ServerState::friend_lobby).map(|lobby| lobby.id.clone()) {
                    let line = write_clipboard_text(clipboard.as_deref_mut(), code);
                    model.0.notice = Some(line);
                    dirty.0 = true;
                }
                continue;
            }
        };
        let outcome = model.0.apply(intent);
        dirty.0 = true;
        carry_out(outcome, &mut model.0, &mut net, &mut commands, &mut core, runtime.as_deref(), &mut navigate);
    }
}

/// The clipboard's text, or why there is none to read. The clipboard is
/// for what nobody types — a ticket pasted in and copied out here, a
/// NetrunnerDB link pasted on the Decks screen — and a deck is a file
/// (`files`); that is what the `system_clipboard` feature is kept for.
pub(crate) fn read_clipboard(clipboard: Option<&mut bevy::clipboard::Clipboard>) -> Result<String, String> {
    let Some(clipboard) = clipboard else { return Err("There is no clipboard to read here".to_string()) };
    match clipboard.fetch_text() {
        bevy::clipboard::ClipboardRead::Ready(Ok(text)) => Ok(text),
        bevy::clipboard::ClipboardRead::Ready(Err(error)) => Err(format!("The clipboard could not be read: {error}")),
        _ => Err("The clipboard is not ready; try again".to_string()),
    }
}

/// A server was connected to: a row of the person's servers, made if it
/// is new, and where Join starts next time when nothing is the Default —
/// here and in the terminal client (Phase 7 §14). Written when the
/// connection is made, never as the address is typed, and a file that
/// cannot be written costs the connection nothing. A host's ticket is one
/// game's and is not kept.
fn remember_server(core: &ClientCore, url: &str) {
    if netrunner_client::peer::Ticket::parse(url).is_some() {
        return;
    }
    let Some(path) = core.servers_path() else { return };
    if let Err(error) = ServerBook::update(&path, |book| {
        book.connected(url, servers::now());
        Ok(())
    }) {
        warn!("the server was not kept: {error}");
    }
}

/// Puts text on the clipboard and says so — a host's ticket or address.
fn write_clipboard_text(clipboard: Option<&mut bevy::clipboard::Clipboard>, text: String) -> String {
    match clipboard.map(|clipboard| clipboard.set_text(text)) {
        Some(Ok(())) => "Copied to the clipboard".to_string(),
        Some(Err(error)) => format!("The clipboard could not be written: {error}"),
        None => "There is no clipboard to write here".to_string(),
    }
}

/// What a press asked of the network, started. Every failure is the
/// form's notice. A request on the attached connection goes through
/// `Connected`, inserted here when hosting or connecting and removed when
/// the person disconnects; the machine and the server answer it, and the
/// answers come back through `net`.
#[allow(clippy::too_many_arguments)]
fn carry_out(outcome: Outcome, form: &mut OnlineForm, net: &mut Net, commands: &mut Commands, core: &mut ClientCore, runtime: Option<&TokioRuntime>, navigate: &mut MessageWriter<Navigate>) {
    let needs_runtime = matches!(outcome, Outcome::Host { .. } | Outcome::Connect { .. } | Outcome::List { .. } | Outcome::Watch { .. } | Outcome::FetchProfiles(_));
    let Some(runtime) = runtime.filter(|_| needs_runtime) else {
        match outcome {
            Outcome::Leave => {
                navigate.write(Navigate(AppScreen::MainMenu));
            }
            Outcome::Stop => {
                stop(net);
                // A dial still under way is let go with the connection.
                commands.remove_resource::<Connected>();
            }
            Outcome::Disconnect => {
                net.shown.clear();
                commands.remove_resource::<Connected>();
            }
            Outcome::TournamentDecks(format) => form.set_tournament_decks(deck_choices(core, format)),
            Outcome::ListLobbies
            | Outcome::JoinLobby { .. }
            | Outcome::LeaveLobby
            | Outcome::CreateLobby { .. }
            | Outcome::Seek(..)
            | Outcome::CancelSeek
            | Outcome::ListTournaments
            | Outcome::CreateTournament { .. }
            | Outcome::Register { .. }
            | Outcome::Unregister { .. }
            | Outcome::Sit { .. }
            | Outcome::BeginRound { .. }
            | Outcome::FinishTournament { .. }
            | Outcome::RecordResult { .. }
            | Outcome::OfferDraw { .. }
            | Outcome::SetProfile { .. }
            | Outcome::Ladder(_) => {
                commands.queue(move |world: &mut World| ask(world, outcome));
            }
            Outcome::EditServers(edit) => {
                let written = match core.servers_path() {
                    Some(path) => ServerBook::update(&path, |book| edit.apply(book)),
                    None => Err("No data directory, so the list of servers cannot be kept".to_string()),
                };
                if let Err(reason) = written {
                    form.apply(Intent::Refused(reason));
                }
                form.apply(Intent::Servers(core.server_book()));
            }
            Outcome::EditPeers(edit) => {
                let written = match core.peers_path() {
                    Some(path) => PeerBook::update(&path, |book| edit.apply(book)),
                    None => Err("No data directory, so the saved peers cannot be kept".to_string()),
                };
                if let Err(reason) = written {
                    form.notice = Some(reason);
                }
                form.apply(Intent::Peers(core.peer_book()));
            }
            // No runtime to ask on: each row says so rather than waiting.
            Outcome::FetchProfiles(addresses) => {
                for address in addresses {
                    form.apply(Intent::RowProfile { address, answer: Err("The network runtime did not start".to_string()) });
                }
            }
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
    match outcome {
        Outcome::Host { port, reach, format } => {
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
            host(form, net, commands, core, runtime, port, reach, format, relay);
        }
        Outcome::Connect { url } => {
            // A friend's room joined from the hub is a peer, never a row of
            // the servers (`peers`); a server by its address is.
            if !matches!(form.page, Page::Hub | Page::Join) {
                remember_server(core, &url);
            }
            let credentials = match core.credentials() {
                Ok(credentials) => credentials,
                Err(error) => {
                    form.apply(Intent::Failed(format!("Your key: {error}")));
                    return;
                }
            };
            let who = Who { player_name: core.player_name(), credentials: credentials.map(Box::new) };
            commands.insert_resource(Connected { attached: remote::spawn(url.clone(), Goal::Attach(who)), hosting: None });
            form.apply(Intent::Waiting(format!("Connecting to {}…", shortened(&url))));
        }
        Outcome::Watch { url, match_id } => {
            remember_server(core, &url);
            net.connecting = Some(remote::watch(url.clone(), match_id));
            form.apply(Intent::Waiting(format!("Connecting to {}…", shortened(&url))));
        }
        // A socket each, proving the key and asking, closed after
        // (`remote::profile`); a key is never made to look, so a person
        // with none sees none asked.
        Outcome::FetchProfiles(addresses) => {
            let credentials = core.identity_dir.as_deref().map(netrunner_client::identity::existing).transpose().ok().flatten().flatten();
            let (tx, rx) = mpsc::channel();
            for address in addresses {
                let Some(credentials) = credentials.clone() else {
                    form.apply(Intent::RowProfile { address, answer: Err("No key on this machine".to_string()) });
                    continue;
                };
                let tx = tx.clone();
                runtime.0.spawn(async move {
                    let answer = match tokio::time::timeout(LIST_TIMEOUT, remote::profile(&address, &credentials)).await {
                        Ok(Ok(profile)) => Ok(profile),
                        Ok(Err(error)) => Err(error.to_string()),
                        Err(_) => Err(format!("No answer within {}s", LIST_TIMEOUT.as_secs())),
                    };
                    let _ = tx.send((address, answer));
                });
            }
            net.profiles = Some(Mutex::new(rx));
        }
        Outcome::List { url } => {
            remember_server(core, &url);
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
        _ => unreachable!("needs no runtime"),
    }
}

/// A request for the attached connection, sent when the frame's commands
/// run — so one inserted in the same frame is there to take it. The
/// machine refuses what makes no sense where it is, and the server's
/// refusals come back as events.
fn ask(world: &mut World, outcome: Outcome) {
    let Some(connected) = world.get_resource::<Connected>() else { return };
    match outcome {
        Outcome::ListLobbies => connected.attached.list_lobbies(),
        Outcome::JoinLobby { id, password } => connected.attached.join_lobby(id, password),
        Outcome::LeaveLobby => connected.attached.leave_lobby(),
        Outcome::CreateLobby { name, format, password, bots } => connected.attached.create_lobby(name, format, password, bots),
        Outcome::Seek(chair, opponent) => connected.attached.seek(chair, opponent),
        Outcome::CancelSeek => connected.attached.cancel_seek(),
        Outcome::ListTournaments => connected.attached.list_tournaments(),
        Outcome::CreateTournament { name, format } => connected.attached.create_tournament(name, format),
        Outcome::Register { tournament, corp, runner } => connected.attached.register(tournament, *corp, *runner),
        Outcome::Unregister { tournament } => connected.attached.unregister(tournament),
        Outcome::Sit { tournament, seed_commitment } => connected.attached.sit(tournament, seed_commitment),
        Outcome::BeginRound { tournament } => connected.attached.begin_round(tournament),
        Outcome::FinishTournament { tournament } => connected.attached.finish_tournament(tournament),
        Outcome::RecordResult { tournament, table, outcome } => connected.attached.record_result(tournament, table, outcome),
        Outcome::OfferDraw { tournament } => connected.attached.offer_draw(tournament),
        Outcome::SetProfile { title, badge } => connected.attached.set_profile(title, badge),
        Outcome::Ladder(format) => connected.attached.ladder(format),
        _ => {}
    }
}

/// Starts hosting and attaches to the server as its first player. Inside
/// the runtime's context (`carry_out`'s guard). A host proves no key: a
/// game on their own machine is never rated.
#[allow(clippy::too_many_arguments)]
fn host(form: &mut OnlineForm, net: &mut Net, commands: &mut Commands, core: &ClientCore, runtime: &TokioRuntime, port: u16, reach: Reach, format: NsgFormat, relay: Option<Relay>) {
    // The ticket on the person's own key, so it is the same every time
    // they host and a friend can save it (Phase 7 §15). A key that
    // cannot be made hosts on a throwaway one, as the dev hooks do.
    let key = core.credentials().ok().flatten().map(|credentials| credentials.identity.secret_bytes());
    match start_hosting(port, reach, format, relay, key, runtime.handle()) {
        Ok((hosting, url)) => {
            let who = Who { player_name: core.player_name(), credentials: None };
            commands.insert_resource(Connected { attached: remote::spawn(url, Goal::Attach(who)), hosting: Some(hosting) });
            net.shown.clear();
            form.apply(Intent::Waiting("Hosting — connecting to your own server…".to_string()));
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
    mut commands: Commands,
    mut dev: Option<ResMut<crate::dev::Dev>>,
    mut done: Local<bool>,
    mut spectate: Local<Option<Mutex<mpsc::Receiver<Result<(String, uuid::Uuid), String>>>>>,
    mut model: ResMut<Model>,
    mut net: ResMut<Net>,
    mut dirty: ResMut<Dirty>,
    mut core: ResMut<ClientCore>,
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
            Ok((url, match_id)) => carry_out(Outcome::Watch { url, match_id }, form, &mut net, &mut commands, &mut core, runtime.as_deref(), &mut navigate),
            Err(reason) => {
                form.apply(Intent::Failed(reason));
            }
        }
        return;
    }
    // The dashboard is the page once the Default has answered: nothing to
    // open, but the screenshot waits for the lobbies.
    if page == "dashboard" {
        if !*done && form.server.as_ref().is_some_and(|server| !server.lobbies.is_empty()) {
            *done = true;
            dirty.0 = true;
        }
        return;
    }
    // The profile page needs a server attached, which the Default is on
    // the first frame: wait for it.
    if page.starts_with("profile") {
        if !*done && form.server.as_ref().is_some_and(|server| server.known_profile().is_some()) {
            *done = true;
            dirty.0 = true;
            let outcome = form.apply(Intent::Open(Page::Profile));
            carry_out(outcome, form, &mut net, &mut commands, &mut core, runtime.as_deref(), &mut navigate);
            match page.as_str() {
                "profile-titles" => form.apply(Intent::Choose(AwardKind::Title)),
                "profile-badges" => form.apply(Intent::Choose(AwardKind::Badge)),
                _ => Outcome::Nothing,
            };
        }
        return;
    }
    // So do the queue form, the search and a friend lobby.
    if matches!(page.as_str(), "queue" | "queue-ranked" | "searching" | "friend-lobby" | "make-lobby") {
        if form.server.as_ref().is_none_or(|server| server.lobbies.is_empty()) {
            return;
        }
        if !std::mem::replace(&mut *done, true) {
            dirty.0 = true;
            form.apply(Intent::SetPrefer(Some(Side::Runner)));
            let outcome = match page.as_str() {
                "queue" => form.apply(Intent::OpenQueue(QueueMode::Casual)),
                "queue-ranked" => form.apply(Intent::OpenQueue(QueueMode::Ranked)),
                "searching" => {
                    form.apply(Intent::OpenQueue(QueueMode::Casual));
                    form.apply(Intent::FindMatch)
                }
                "make-lobby" => form.apply(Intent::Open(Page::MakeLobby)),
                _ => {
                    form.apply(Intent::Open(Page::MakeLobby));
                    form.apply(Intent::Typed(Field::LobbyName, "Tuesday night".to_string()));
                    form.apply(Intent::SetLobbyBots(true));
                    form.apply(Intent::Go)
                }
            };
            carry_out(outcome, form, &mut net, &mut commands, &mut core, runtime.as_deref(), &mut navigate);
        }
        return;
    }
    // The guest's room is shot once its opponent is ready, which the room
    // learns by its own poll.
    if page == "room-guest" && *done {
        if let Some(dev) = dev.as_mut()
            && dev.held
            && form.room_people().1.is_some_and(|presence| presence.seeking)
        {
            dev.held = false;
            dirty.0 = true;
        }
        return;
    }
    if std::mem::replace(&mut *done, true) {
        return;
    }
    dirty.0 = true;
    match page.as_str() {
        "hub" => form.apply(Intent::Open(Page::Hub)),
        "host" => form.apply(Intent::Open(Page::Host)),
        "join" => {
            form.apply(Intent::Open(Page::Join));
            form.apply(Intent::Typed(Field::Address, "192.168.1.50:8080".to_string()))
        }
        "servers" => {
            let outcome = form.apply(Intent::Open(Page::Servers));
            carry_out(outcome, form, &mut net, &mut commands, &mut core, runtime.as_deref(), &mut navigate);
            Outcome::Nothing
        }
        "watch" => form.apply(Intent::Open(Page::Watch)),
        // `room` is the host's room (as `hosting` was); `ticket` the same
        // with a ticket naming this machine's own addresses, on a
        // throwaway key, so looking at the page touches nothing outside it.
        "hosting" | "room" | "ticket" => {
            let Some(runtime) = runtime else { return };
            let _guard = runtime.0.enter();
            form.apply(Intent::Open(Page::Host));
            let relay = (page == "ticket").then_some(Relay::Off);
            let format = form.format;
            host(form, &mut net, &mut commands, &core, &runtime, 0, Reach::Network, format, relay);
            Outcome::Nothing
        }
        // The guest's room: a server of the hook's own on this machine,
        // joined through the Join page, with a second player in it who
        // is ready and prefers the Runner.
        "room-guest" => {
            let Some(runtime) = runtime else { return };
            let _guard = runtime.0.enter();
            match start_hosting(0, Reach::ThisMachine, form.format, None, None, runtime.handle()) {
                Ok((hosting, url)) => {
                    net.hosting = Some(hosting);
                    let who = Who { player_name: "Sam".to_string(), credentials: None };
                    net.extra = Some(remote::spawn(url.clone(), Goal::Attach(who)));
                    if let Some(dev) = dev.as_mut() {
                        dev.held = true;
                    }
                    form.apply(Intent::Open(Page::Hub));
                    form.apply(Intent::Open(Page::Join));
                    form.apply(Intent::Typed(Field::Address, url));
                    let outcome = form.apply(Intent::Go);
                    carry_out(outcome, form, &mut net, &mut commands, &mut core, Some(&runtime), &mut navigate);
                    Outcome::Nothing
                }
                Err(error) => form.apply(Intent::Failed(format!("Could not host: {error}"))),
            }
        }
        "spectate" | "spectate-corp" => {
            let Some(runtime) = runtime else { return };
            let _guard = runtime.0.enter();
            form.apply(Intent::Open(Page::Watch));
            form.apply(Intent::SetWatchFrom(if page == "spectate" { Side::Runner } else { Side::Corp }));
            match start_hosting(0, Reach::ThisMachine, form.format, None, None, runtime.handle()) {
                Ok((hosting, url)) => {
                    // The server rides with the spectator's match.
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
    let seat = |name: &str, id: &str| remote::connect(remote::seek(url.clone(), remote::seat_in_format(name, format, deck(id))), |_| {});
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

/// Binds a human-vs-human server on `port` and starts it — offering no
/// bot, since a game hosted from this machine is for the person invited,
/// and a guest's bot would play on the host's machine — returning it
/// and the loopback address the host attaches by — the terminal's
/// `start_hosting`, whose server is named here because `netrunner_client`
/// may not name it. Port 0 takes any free port.
fn start_hosting(port: u16, reach: Reach, format: NsgFormat, relay: Option<Relay>, key: Option<[u8; 32]>, runtime: tokio::runtime::Handle) -> Result<(HostedServer, String), String> {
    let options = ServeOptions { bots: Vec::new(), formats: vec![format], ..ServeOptions::default() };
    let listener = hosting::bind_listener(reach, port).map_err(|error| error.to_string())?;
    let server = Server::from_listener(listener, options).map_err(|error| error.to_string())?;
    let port = server.local_addr().map_err(|error| error.to_string())?.port();
    let acceptor = server.acceptor();
    let invitation = Invitation::start(reach, port, relay, key, move |stream, who| {
        let acceptor = acceptor.clone();
        tokio::spawn(async move { acceptor.serve(stream, &who).await });
    });
    let task = runtime.spawn(server.run());
    Ok((HostedServer { task: Some(task), invitation: Some(Mutex::new(invitation)), runtime, linger: false }, format!("ws://127.0.0.1:{port}")))
}

/// A field's editor committed or let go: the model takes the text, and
/// the form is redrawn either way, which puts the box back.
/// A press that misses the award pop-up lands on its wash and closes it,
/// as a click away closes every reading surface.
fn chooser_wash(wash: Query<&Interaction, (Changed<Interaction>, With<ChooserWash>)>, mut model: ResMut<Model>, mut dirty: ResMut<Dirty>) {
    if wash.iter().any(|interaction| *interaction == Interaction::Pressed) {
        model.0.apply(Intent::CloseChooser);
        dirty.0 = true;
    }
}

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
/// the attached connection's answers — a game found on it ends on the
/// board — and a spectator's connection, which ends on the board too, or
/// back at the form with the reason.
#[allow(clippy::too_many_arguments)]
fn net(
    mut commands: Commands,
    mut net: ResMut<Net>,
    mut connected: Option<ResMut<Connected>>,
    mut model: ResMut<Model>,
    mut dirty: ResMut<Dirty>,
    mut core: ResMut<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    mut navigate: MessageWriter<Navigate>,
) {
    let invited = connected.as_ref().and_then(|connected| connected.hosting.as_ref()).and_then(|hosting| hosting.invitation.as_ref()).and_then(|invitation| {
        invitation.lock().ok().map(|mut invitation| {
            invitation.poll();
            invitation.ways()
        })
    });
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

    // The rows' answers about the person, as each server gives one.
    let answers: Vec<_> = net.profiles.as_ref().and_then(|rx| rx.lock().ok().map(|rx| rx.try_iter().collect())).unwrap_or_default();
    for (address, answer) in answers {
        model.0.apply(Intent::RowProfile { address, answer });
        dirty.0 = true;
    }

    // The attached connection: every answer is an intent, and a game is
    // the board.
    if let Some(connected) = connected.as_mut() {
        let hosting = connected.hosting.is_some();
        while let Some(event) = connected.attached.poll() {
            dirty.0 = true;
            let intent = match event {
                // The standing is asked on attaching and after every game,
                // so the page shows the number the game just moved.
                AttachedEvent::Attached(lobbies) => {
                    connected.attached.profile();
                    Intent::Attached { lobbies, hosting }
                }
                AttachedEvent::Profile { key, profile } => Intent::Profile { key, profile: ProfileHere::from_reply(key, profile) },
                AttachedEvent::Lobbies(lobbies) => Intent::Lobbies(lobbies),
                AttachedEvent::Tournaments(tournaments) => Intent::Tournaments(tournaments),
                AttachedEvent::Tournament(info) => Intent::Tournament(info),
                AttachedEvent::TournamentRefused(reason) => Intent::Refused(reason),
                AttachedEvent::LobbyJoined(lobby) => Intent::LobbyJoined(lobby),
                AttachedEvent::LobbyLeft => Intent::LobbyLeft,
                AttachedEvent::LobbyRefused(reason) | AttachedEvent::SeekRefused(reason) | AttachedEvent::ProfileRefused(reason) => Intent::Refused(reason),
                AttachedEvent::Ladder { format, rows } => Intent::Ladder { format, rows },
                AttachedEvent::Queued(position) => Intent::Queued(position),
                AttachedEvent::SeekCancelled => Intent::SeekCancelled,
                AttachedEvent::BackInLobby(lobby) => {
                    connected.attached.profile();
                    Intent::BackInLobby(lobby)
                }
                AttachedEvent::Link(Link::Down(error)) => {
                    commands.remove_resource::<Connected>();
                    net.shown.clear();
                    Intent::Failed(error.to_string())
                }
                AttachedEvent::Link(link) => Intent::Link(link.status_line(Instant::now())),
                AttachedEvent::Joined(joined) => {
                    match MatchHandle::start_remote(core.registry.clone(), *joined, model.0.watch_from) {
                        Ok(handle) => {
                            // A game has been played on this server: a
                            // concession made by leaving the board must
                            // reach the opponent before it stops.
                            if let Some(hosting) = connected.hosting.as_mut() {
                                hosting.linger = true;
                            }
                            commands.insert_resource(ActiveMatch { handle, choice: None, lesson: None, starter: None, online: Some(OnlineMatch { hosting: None, watching: false, notice: None }) });
                            navigate.write(Navigate(AppScreen::Game));
                        }
                        Err(error) => {
                            model.0.apply(Intent::Failed(error));
                        }
                    }
                    break;
                }
            };
            let outcome = model.0.apply(intent);
            carry_out(outcome, &mut model.0, &mut net, &mut commands, &mut core, runtime.as_deref(), &mut navigate);
        }
    }

    // A spectator's connection.
    let mut outcome = None;
    if let Some(connecting) = &mut net.connecting {
        while let Some(event) = connecting.poll() {
            match event {
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
    match MatchHandle::start_remote(core.registry.clone(), *joined, model.0.watch_from) {
        Ok(handle) => {
            let hosting = net.hosting.take().map(|mut hosting| {
                hosting.linger = true;
                hosting
            });
            commands.insert_resource(ActiveMatch { handle, choice: None, lesson: None, starter: None, online: Some(OnlineMatch { hosting, watching, notice: None }) });
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
        Field::LobbyName => &form.make.name,
        Field::LobbyPassword => &form.make.password,
        Field::TournamentName => &form.make_tournament.name,
        Field::ServerName => &form.edit.name,
        Field::ServerAddress => &form.edit.address,
        Field::PeerName => &form.edit_peer.name,
    }
}

/// A tournament page with a round clock is redrawn every half minute,
/// so its countdown (`tournament::clock_line`, to the minute) moves while
/// nobody presses anything. The page is otherwise redrawn only on a
/// change, and a clock is the one thing on it that changes by itself.
fn round_clock_ticks(time: Res<Time>, mut since: Local<f32>, model: Res<Model>, mut dirty: ResMut<Dirty>) {
    let ticking = model.0.page == Page::Tournament && model.0.server.as_ref().and_then(ServerState::open_tournament).is_some_and(|info| info.clock.is_some());
    if !ticking {
        *since = 0.0;
        return;
    }
    *since += time.delta_secs();
    if *since >= 30.0 {
        *since = 0.0;
        dirty.0 = true;
    }
}

/// The search over the dashboard is redrawn every second, so the time
/// waited moves while nobody presses anything.
fn search_clock_ticks(time: Res<Time>, mut since: Local<f32>, model: Res<Model>, mut dirty: ResMut<Dirty>) {
    let searching = matches!(model.0.page, Page::Dashboard | Page::FriendLobby) && model.0.server.as_ref().is_some_and(ServerState::seeking);
    if !searching {
        *since = 0.0;
        return;
    }
    *since += time.delta_secs();
    if *since >= 1.0 {
        *since = 0.0;
        dirty.0 = true;
    }
}

/// The room is asked for its lobby every second, so the other side's
/// arrival, Ready and preference are seen without a press — the server
/// pushes nothing about a lobby's people.
fn room_ticks(
    mut commands: Commands,
    time: Res<Time>,
    mut since: Local<f32>,
    mut model: ResMut<Model>,
    mut net: ResMut<Net>,
    mut core: ResMut<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    mut navigate: MessageWriter<Navigate>,
) {
    if model.0.page != Page::Room || model.0.server.is_none() {
        *since = 0.0;
        return;
    }
    *since += time.delta_secs();
    if *since >= 1.0 {
        *since = 0.0;
        let outcome = model.0.apply(Intent::Refresh);
        carry_out(outcome, &mut model.0, &mut net, &mut commands, &mut core, runtime.as_deref(), &mut navigate);
    }
}

/// The `room-guest` dev hook's other player: attached to the hook's own
/// server, into its queue, and ready with a preference, so the guest's
/// room shows an opponent who is.
fn dev_guest(mut net: ResMut<Net>) {
    let Some(extra) = net.extra.as_mut() else { return };
    while let Some(event) = extra.poll() {
        match event {
            AttachedEvent::Attached(lobbies) => {
                if let Some(lobby) = lobbies.first() {
                    extra.join_lobby(lobby.id.clone(), None);
                }
            }
            AttachedEvent::LobbyJoined(_) => {
                let deck = |id: &str| Box::new(netrunner_core::decks::by_id(id).expect("a built-in deck"));
                extra.seek(netrunner_server::protocol::Chair::Either { corp: deck("brick_stack"), runner: deck("stolen_goods"), prefer: Some(Side::Runner) }, Opponent::Person);
            }
            _ => {}
        }
    }
}

/// The pulse turns with the clock, so a redraw does not set it back.
fn turn_pulse(time: Res<Time>, mut pulses: Query<&mut UiTransform, With<Pulse>>) {
    let angle = (time.elapsed_secs() * 2.4) % std::f32::consts::TAU;
    for mut transform in &mut pulses {
        transform.rotation = Rot2::radians(angle);
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    root: Query<Entity, With<FormRoot>>,
    mut panel: Query<&mut Node, With<PanelRoot>>,
    screen: Query<Entity, (With<DespawnOnExit<AppScreen>>, With<Node>)>,
    shown: Query<Entity, Or<(With<ChooserWash>, With<Overlay>)>>,
    theme: Res<Theme>,
    model: Res<Model>,
    net: Res<Net>,
    core: Res<ClientCore>,
    card_images: Res<CardImages>,
    mut glyphs: ResMut<BadgeGlyphs>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    if !std::mem::take(&mut dirty.0) {
        return;
    }
    let Ok(root) = root.single() else { return };
    if let Ok(mut node) = panel.single_mut() {
        node.width = px(if matches!(model.0.page, Page::Dashboard | Page::Hub | Page::Room) { DASHBOARD_WIDTH } else { FORM_WIDTH });
        // The hub's peers list scrolls inside the panel rather than
        // pushing Back off the window: the panel may shrink below its
        // content there, and its column is capped by the window.
        node.min_height = if model.0.page == Page::Hub { px(0) } else { Val::Auto };
    }
    // Every badge the page may draw, looked up once a name.
    let basic = core.settings.desktop.basic_graphics;
    let pictures: HashMap<String, Option<Handle<Image>>> = model
        .0
        .server
        .as_ref()
        .and_then(ServerState::known_profile)
        .map(|profile| profile.badges.iter().map(|award| (award.name.clone(), glyphs.of(&award.name, basic, images.as_deref_mut()))).collect())
        .unwrap_or_default();
    commands.entity(root).despawn_children().with_children(|parent| spawn_page(parent, &theme, &model.0, &net, &pictures, &core, &card_images));
    for entity in &shown {
        commands.entity(entity).despawn();
    }
    let Some(screen) = screen.iter().next() else { return };
    let choosing = model.0.server.as_ref().filter(|_| model.0.page == Page::Profile).and_then(|server| Some((server.choosing?, server.known_profile()?)));
    if let Some((kind, profile)) = choosing {
        commands.entity(screen).with_children(|parent| spawn_chooser(parent, &theme, kind, profile, &pictures));
    }
    // The dashboard's overlays: the search over everything, else the
    // queue form.
    if let Some(server) = model.0.server.as_ref().filter(|_| matches!(model.0.page, Page::Dashboard | Page::FriendLobby)) {
        if let Some(searching) = &server.searching {
            let pulse = images.as_deref_mut().map(|images| images.add(widgets::pulse_image()));
            commands.entity(screen).with_children(|parent| spawn_searching(parent, &theme, &model.0, searching, pulse));
        } else if let Some(queue) = server.queue.filter(|_| model.0.page == Page::Dashboard) {
            commands.entity(screen).with_children(|parent| spawn_queue_form(parent, &theme, &model.0, server, queue));
        }
    }
}

fn spawn_page(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, net: &Net, pictures: &HashMap<String, Option<Handle<Image>>>, core: &ClientCore, images: &CardImages) {
    match form.page {
        Page::Hub => spawn_hub(parent, theme, form),
        Page::Host => spawn_host(parent, theme, form, core, images),
        Page::Join => spawn_join(parent, theme, form, core, images),
        Page::EditPeer => spawn_edit_peer(parent, theme, form),
        Page::Room => match &form.server {
            Some(server) => spawn_room(parent, theme, form, server, net),
            None => spawn_waiting(parent, theme, form),
        },
        Page::Servers => spawn_servers(parent, theme, form),
        Page::EditServer => spawn_edit_server(parent, theme, form),
        Page::Watch => spawn_watch(parent, theme, form, net.listing.is_some()),
        Page::Waiting => spawn_waiting(parent, theme, form),
        Page::Dashboard => match &form.server {
            Some(server) => spawn_dashboard(parent, theme, form, server, net, core, images, pictures),
            None => spawn_waiting(parent, theme, form),
        },
        Page::FriendLobby => match form.server.as_ref().and_then(|server| Some((server, server.friend_lobby()?))) {
            Some((server, lobby)) => spawn_friend_lobby(parent, theme, form, server, lobby),
            None => spawn_waiting(parent, theme, form),
        },
        Page::MakeLobby => spawn_make_lobby(parent, theme, form),
        Page::Profile => match &form.server {
            Some(server) => spawn_profile(parent, theme, form, server, pictures),
            None => spawn_waiting(parent, theme, form),
        },
        Page::Tournaments | Page::Tournament | Page::MakeTournament => match &form.server {
            Some(server) => match form.page {
                Page::Tournaments => spawn_tournaments(parent, theme, server),
                Page::Tournament => spawn_tournament(parent, theme, server),
                _ => spawn_make_tournament(parent, theme, form),
            },
            None => spawn_waiting(parent, theme, form),
        },
    }
    // A notice goes on the overlay while one is up, where the person is
    // looking.
    let overlay = form.page == Page::Dashboard && form.server.as_ref().is_some_and(|server| server.queue.is_some() || server.seeking());
    if let Some(notice) = form.notice.as_ref().filter(|_| !overlay) {
        parent.spawn((widgets::notice(theme, notice.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
}

/// The Server Dashboard (Phase 7 §14): the server and the person's tag
/// across the top; the active decks beside the ways elsewhere; the three
/// doors; and for a host, what to give out.
#[allow(clippy::too_many_arguments)]
fn spawn_dashboard(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, server: &ServerState, net: &Net, core: &ClientCore, images: &CardImages, pictures: &HashMap<String, Option<Handle<Image>>>) {
    parent.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(14), ..default() }).with_children(|row| {
        let name = if server.hosting { "Hosting on this machine".to_string() } else { shortened(&form.server_name(&server.address)) };
        row.spawn((widgets::heading(theme, name), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }));
        row.spawn(widgets::dim(theme, if server.link.is_some() { "Reconnecting" } else { "Connected" }));
        // The person's tag: their name, the title worn and the badge, a
        // button to the profile page where the server keeps one.
        if let Some(profile) = server.known_profile() {
            let tag = match &profile.title {
                Some(title) => format!("{} · {title}", profile.name),
                None => profile.name.clone(),
            };
            row.spawn(widgets::row(8.0)).with_children(|tag_row| {
                if let Some(badge) = &profile.badge {
                    badge::spawn_glyph(tag_row, theme, badge, pictures.get(badge).cloned().flatten(), 28.0);
                }
                tag_row.spawn(widgets::small_button(theme, ButtonKind::Secondary, tag, Control::Open(Page::Profile)));
            });
        }
        row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Servers…", Control::Open(Page::Servers)));
    });
    if let Some(line) = &server.link {
        parent.spawn((widgets::notice(theme, line.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    // A host's addresses are the room's (Phase 7 §15); a host is never on
    // the dashboard, but the block is kept for a server that is both.
    if server.hosting {
        spawn_ways(parent, theme, net);
    }
    parent.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(28), align_items: AlignItems::FlexStart, ..default() }).with_children(|columns| {
        columns.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(10), flex_grow: 1.0, flex_basis: px(0), min_width: px(0), ..default() }).with_children(|left| {
            left.spawn(widgets::overline(theme, "Active decks"));
            for side in [Side::Corp, Side::Runner] {
                spawn_active_deck(left, theme, form, side, server, core, images);
            }
        });
        columns.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(8), width: px(260), flex_shrink: 0.0, ..default() }).with_children(|right| {
            right.spawn(widgets::overline(theme, "Navigation & profiles"));
            let mut ways: Vec<(&str, Control)> = vec![("Deck builder", Control::Nav(AppScreen::Decks)), ("Cards", Control::Nav(AppScreen::CardBrowser))];
            if !server.hosting {
                ways.push(("Server profile", Control::Open(Page::Profile)));
            }
            ways.push(("Watch a game", Control::Open(Page::Watch)));
            if !server.hosting {
                ways.push(("Local / P2P play", Control::Open(Page::Hub)));
            }
            for (label, control) in ways {
                right.spawn(widgets::styled_button(theme, ButtonKind::Secondary, label, percent(100), control));
            }
        });
    });
    section(parent, theme, "Play modes", |section| {
        section.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(16), row_gap: px(12), ..default() }).with_children(|row| {
            let width = px(320);
            let ranked = server.ranked_shut();
            row.spawn(widgets::tile_button(theme, ButtonKind::Primary, "Ranked ladder", ranked.unwrap_or("Rated, between players with keys"), width, ranked.is_some(), Control::OpenQueue(QueueMode::Ranked)));
            let casual = if server.offers(QueueMode::Casual) { "Relaxed matching, never rated; friend lobbies" } else { "This server lists no Casual queue" };
            row.spawn(widgets::tile_button(theme, if ranked.is_some() { ButtonKind::Primary } else { ButtonKind::Secondary }, "Casual / unranked", casual, width, !server.offers(QueueMode::Casual), Control::OpenQueue(QueueMode::Casual)));
            row.spawn(widgets::tile_button(theme, ButtonKind::Secondary, "Tournaments & events", "Swiss rounds with locked decks", width, false, Control::Open(Page::Tournaments)));
        });
    });
    // Back leaves the screen, and the connection with it; Disconnect lets
    // it go and stays, at the servers — a host's way to stop hosting.
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, if server.hosting { "Stop hosting" } else { "Disconnect" }, Val::Auto, Control::Disconnect));
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
    });
}

/// The face an active deck's identity is drawn with on the dashboard.
const ACTIVE_FACE: u16 = 64;

/// One active deck: its identity's face, the side, the name and the
/// identity, the formats it is legal in as badges, and Change…, which
/// opens the picker (`Purpose::Active`). A side with no deck says so.
fn spawn_active_deck(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, side: Side, server: &ServerState, core: &ClientCore, images: &CardImages) {
    let format = server.queue.map(|queue| queue.format).or_else(|| server.lobby.as_ref().map(|lobby| lobby.format)).unwrap_or(form.format);
    spawn_active_deck_in(parent, theme, form, side, format, core, images);
}

/// The tile, with the format the picker opens on: the hub's host and
/// join pages have no server to read one off.
fn spawn_active_deck_in(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, side: Side, format: NsgFormat, core: &ClientCore, images: &CardImages) {
    let active = form.active(side);
    let faction = theme.faction(active.and_then(|deck| deck.faction));
    let node = Node {
        width: percent(100),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(14),
        padding: UiRect::all(px(10)),
        border: UiRect { left: px(4), right: px(1), top: px(1), bottom: px(1) },
        border_radius: BorderRadius::all(px(12)),
        ..default()
    };
    let border = BorderColor { left: faction, right: theme.glass_border, top: theme.glass_border, bottom: theme.glass_border };
    parent.spawn((node, BackgroundColor(theme.glass_strong), border)).with_children(|tile| {
        let identity = active.and_then(|deck| core.registry.get(&deck.deck.identity).or_else(|| core.catalog.iter().find(|card| card.id == deck.deck.identity)));
        match identity {
            Some(card) => {
                let drawn = Face::of(card, &core.settings.art);
                let image = images.of(&drawn, FaceSize::Board(ACTIVE_FACE));
                spawn_face(tile, theme, &drawn, FaceSize::Board(ACTIVE_FACE), image, Readable(card.id.clone()));
            }
            None => {
                tile.spawn((Node { width: px(ACTIVE_FACE as f32), height: px(ACTIVE_FACE as f32 * 1.4), flex_shrink: 0.0, ..default() }, BackgroundColor(theme.panel)));
            }
        }
        tile.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), min_width: px(0), row_gap: px(3), ..default() }).with_children(|words| {
            words.spawn(widgets::overline(theme, format!("{side:?}")));
            match active {
                Some(deck) => {
                    words.spawn((Text::new(deck.deck.name.clone()), theme.font(size::BODY), TextColor(theme.text), DeckLine(Purpose::Active(side))));
                    words.spawn(widgets::dim(theme, deck.identity.clone()));
                    widgets::format_badges(words, theme, &deck.legal_formats());
                }
                None => {
                    words.spawn((Text::new("No deck chosen"), theme.font(size::BODY), TextColor(theme.caution), DeckLine(Purpose::Active(side))));
                    words.spawn(widgets::dim(theme, "Choose one, or build one in the deck builder."));
                }
            }
        });
        let chosen = active.map_or_else(|| core.settings.active_deck_or_default(side), |deck| deck.deck.id.clone());
        tile.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Change…", DeckSlot { purpose: Purpose::Active(side), chosen, format }));
    });
}

/// The queue form over the dashboard (Phase 7 §14): the door's title and
/// what a game there counts for; the format as pills — greyed with the
/// reason where an active deck is not legal, and never sent; the role
/// preference; under Casual the ways to the people you know; Cancel and
/// Find match. Centred on a wash, as a choice is, and the wash blocks
/// the press.
fn spawn_queue_form(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, server: &ServerState, queue: crate::models::online::QueueForm) {
    let (title, blurb) = match queue.mode {
        QueueMode::Ranked => ("Configure queue: Ranked ladder", "Rated between players with keys; your standing moves with every game."),
        QueueMode::Casual => ("Configure queue: Casual / unranked", "Never rated. Whoever is looking plays whoever else is."),
        QueueMode::Friends => ("Configure queue", ""),
    };
    parent
        .spawn((
            Overlay,
            Button,
            GlobalZIndex(20),
            Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(theme.wash.with_alpha(0.72)),
        ))
        .with_children(|wash| {
            let mut panel = wash.spawn((Interaction::None, bevy::ui::FocusPolicy::Block, widgets::roomy_panel(theme, px(820))));
            panel.entry::<Node>().and_modify(|mut node| node.max_height = percent(92));
            panel.with_children(|panel| {
                panel.spawn(widgets::heading(theme, title));
                panel.spawn((widgets::dim(theme, blurb), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                if let Some(lobby) = server.queue(queue.mode, queue.format) {
                    panel.spawn((widgets::dim(theme, format!("{} · {} looking now", server.rating_line(lobby), lobby.seeking())), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
                section(panel, theme, "Format — checked against your active decks", |section| {
                    let choices = form.queue_choices();
                    section.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), ..default() }).with_children(|row| {
                        for (format, problem) in &choices {
                            let label = netrunner_client::settings::format_label(*format);
                            match problem {
                                None => {
                                    let kind = if *format == queue.format { ButtonKind::Primary } else { ButtonKind::Secondary };
                                    row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, Control::QueueFormat(*format)));
                                }
                                Some(_) => {
                                    row.spawn(widgets::disabled_button(theme, label, Val::Auto, Control::QueueFormat(*format)));
                                }
                            }
                        }
                    });
                    for (format, problem) in &choices {
                        if let Some(problem) = problem {
                            section.spawn((Text::new(format!("{}: {problem}", netrunner_client::settings::format_label(*format))), theme.font(size::SMALL), TextColor(theme.caution), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        }
                    }
                });
                section(panel, theme, "Role soft filter — you bring both decks", |section| {
                    section.spawn(widgets::row(10.0)).with_children(|row| {
                        for prefer in [Some(Side::Corp), None, Some(Side::Runner)] {
                            let kind = if prefer == server.prefer { ButtonKind::Primary } else { ButtonKind::Secondary };
                            let label = if prefer.is_none() { "Any / Balanced (Recommended)".to_string() } else { preference_label(prefer).to_string() };
                            row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, Control::Prefer(prefer)));
                        }
                    });
                    section.spawn(widgets::dim(theme, "The queue meets a preference where it can and never refuses a game over it; two players who meet again swap chairs."));
                });
                if queue.mode == QueueMode::Casual {
                    section(panel, theme, "Play with friends", |section| {
                        section.spawn(widgets::row(12.0)).with_children(|row| {
                            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Make a friend lobby…", Control::Open(Page::MakeLobby)));
                            row.spawn(Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }).with_children(|slot| field_box(slot, theme, Field::Lobby, &form.lobby, "a lobby's code", false));
                            row.spawn(Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }).with_children(|slot| field_box(slot, theme, Field::Password, &form.password, "password, if it asks", false));
                            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Join", Control::JoinById));
                        });
                    });
                }
                if let Some(notice) = &form.notice {
                    panel.spawn((widgets::notice(theme, notice.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
                buttons(panel, |row| {
                    row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, (Control::CloseQueue, ButtonSound(Sfx::Back))));
                    row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Find match", px(220), Control::FindMatch));
                });
            });
        });
}

/// The search over the dashboard: the pulse, what is being looked for,
/// how long for, and Abort. Redrawn every second (`search_clock_ticks`).
fn spawn_searching(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, searching: &Searching, pulse: Option<Handle<Image>>) {
    let elapsed = searching.since.elapsed().as_secs();
    let (what, line) = match &searching.lobby {
        Some(lobby) if lobby.permanent() => (
            match searching.opponent {
                Opponent::Person => "Searching for an opponent…".to_string(),
                Opponent::Bot { level, .. } => format!("Seating the {} bot…", rung_label(level).to_lowercase()),
            },
            format!("{} · {}", lobby_title(lobby), searching_line(lobby.format, searching.prefer)),
        ),
        Some(lobby) => (
            match searching.opponent {
                Opponent::Person => "Waiting for someone in the lobby…".to_string(),
                Opponent::Bot { level, .. } => format!("Seating the {} bot…", rung_label(level).to_lowercase()),
            },
            format!("{} · {}", lobby_title(lobby), searching_line(lobby.format, searching.prefer)),
        ),
        None => ("Waiting for your table…".to_string(), "The tournament's format and chairs".to_string()),
    };
    parent
        .spawn((
            Overlay,
            Button,
            GlobalZIndex(20),
            Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(theme.wash.with_alpha(0.72)),
        ))
        .with_children(|wash| {
            let mut panel = wash.spawn((Interaction::None, bevy::ui::FocusPolicy::Block, widgets::roomy_panel(theme, px(640))));
            panel.entry::<Node>().and_modify(|mut node| node.align_items = AlignItems::Center);
            panel.with_children(|panel| {
                panel.spawn(widgets::heading(theme, "Jacking into the mainnet…"));
                match pulse {
                    Some(handle) => {
                        panel.spawn((Pulse, ImageNode::new(handle).with_color(theme.accent), UiTransform::IDENTITY, Node { width: px(widgets::PULSE_SIZE as f32), height: px(widgets::PULSE_SIZE as f32), margin: UiRect::vertical(px(8)), ..default() }));
                    }
                    None => {
                        panel.spawn(widgets::label(theme, "(o)"));
                    }
                }
                panel.spawn(widgets::label(theme, what));
                panel.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Center, LineBreak::WordBoundary)));
                if let Some(position) = searching.position {
                    panel.spawn(widgets::dim(theme, format!("{position} waiting in this queue")));
                }
                panel.spawn(widgets::dim(theme, format!("Elapsed {:02}:{:02}", elapsed / 60, elapsed % 60)));
                if let Some(notice) = &form.notice {
                    panel.spawn((widgets::notice(theme, notice.clone(), ()), TextLayout::new(Justify::Center, LineBreak::WordBoundary)));
                }
                panel.spawn(Node { margin: UiRect::top(px(8)), ..default() }).with_children(|row| {
                    row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Abort", px(200), (Control::CancelSeek, ButtonSound(Sfx::Back))));
                });
            });
        });
}

/// A friend lobby's page: its name and code, who is there, the role
/// preference, a bot where the maker let them in, and Find a game among
/// them. Leave is the way back to the dashboard.
fn spawn_friend_lobby(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, server: &ServerState, lobby: &LobbyInfo) {
    parent.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(14), ..default() }).with_children(|row| {
        row.spawn((widgets::heading(theme, lobby.name.clone()), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }));
        row.spawn(widgets::label(theme, format!("Code {}", lobby.id)));
        row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Copy code", Control::CopyCode));
    });
    let password = if lobby.password { ", with its password" } else { "" };
    parent.spawn((widgets::dim(theme, format!("{} · {} here, {} looking · {}", netrunner_client::settings::format_label(lobby.format), lobby.players(), lobby.seeking(), server.rating_line(lobby))), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    parent.spawn((widgets::dim(theme, format!("Your group joins by the code{password}, under Casual on their dashboard. Whoever is looking here plays whoever else is.")), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    section(parent, theme, "Role soft filter — you bring both decks", |section| {
        section.spawn(widgets::row(10.0)).with_children(|row| {
            for prefer in [Some(Side::Corp), None, Some(Side::Runner)] {
                let kind = if prefer == server.prefer { ButtonKind::Primary } else { ButtonKind::Secondary };
                row.spawn(widgets::styled_button(theme, kind, preference_label(prefer), Val::Auto, Control::Prefer(prefer)));
            }
        });
    });
    // A bot only where the lobby lets one in.
    if !lobby.bots.is_empty() {
        section(parent, theme, "Opponent", |section| {
            section.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), ..default() }).with_children(|row| {
                for bot in std::iter::once(None).chain(lobby.bots.iter().copied().map(Some)) {
                    let kind = if bot == server.bot { ButtonKind::Primary } else { ButtonKind::Secondary };
                    let label = bot.map_or_else(|| opponent_label(None), rung_label);
                    row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, Control::Opponent(bot)));
                }
            });
            section.spawn(widgets::dim(theme, "A bot plays instead of waiting for the group, at the rung chosen; never rated."));
        });
    }
    for side in [Side::Corp, Side::Runner] {
        if let Some(problem) = form.active(side).map_or(Some("no deck chosen".to_string()), |deck| deck.problem_in(lobby.format).map(str::to_string)) {
            parent.spawn((Text::new(format!("{side:?} deck: {problem} — change it on the dashboard")), theme.font(size::SMALL), TextColor(theme.caution), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
    }
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Leave lobby", Val::Auto, (Control::LeaveLobby, ButtonSound(Sfx::Back))));
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Find a game", px(220), Control::Seek));
    });
}

/// What a host gives out (Phase 7 §15, the room; before it, the
/// dashboard): the ticket on a line of its own with Copy, the addresses,
/// and what the router or the relay said.
fn spawn_ways(parent: &mut ChildSpawnerCommands, theme: &Theme, net: &Net) {
    section(parent, theme, "Give your opponent one of these", |section| {
        if net.shown.is_empty() {
            section.spawn(widgets::dim(theme, "Preparing…"));
        }
        for (index, way) in net.shown.iter().enumerate() {
            match way {
                Way::Give { what, who, ticket } => {
                    section.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }).with_children(|give| {
                        give.spawn((widgets::dim(theme, if *ticket { format!("Ticket · {who}") } else { capitalised(who) }), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        give.spawn(widgets::row(12.0)).with_children(|row| {
                            row.spawn((
                                Text::new(what.clone()),
                                theme.font(if *ticket { size::SMALL } else { size::BODY }),
                                TextColor(theme.text),
                                // A ticket has no spaces to break at.
                                TextLayout::new(Justify::Left, LineBreak::AnyCharacter),
                                Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() },
                            ));
                            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, if *ticket { "Copy ticket" } else { "Copy" }, Control::Copy(index)));
                        });
                    });
                }
                Way::Note(note) => {
                    section.spawn((widgets::dim(theme, note.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
            }
        }
    });
}

/// The Local / P2P hub (Phase 7 §15): two tiles, Host a session and Join
/// a session, and the saved peers under them, each a Quick connect.
fn spawn_hub(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm) {
    parent.spawn(widgets::heading(theme, "Local / P2P play"));
    parent.spawn((widgets::dim(theme, "A game between two people with no server in between: one hosts it on their machine, the other joins by the address or the ticket the room shows."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    parent.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(20), align_items: AlignItems::Stretch, ..default() }).with_children(|row| {
        let tile = Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(0),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            padding: UiRect::all(px(18)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(16)),
            ..default()
        };
        row.spawn((tile.clone(), BackgroundColor(theme.glass_strong), BorderColor::all(theme.glass_border))).with_children(|card| {
            card.spawn(widgets::overline(theme, "Host a session"));
            card.spawn((widgets::dim(theme, "Run the game on this machine and invite one person by your network, a port, or a ticket."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            card.spawn(Node { flex_grow: 1.0, ..default() });
            card.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Host a new game", percent(100), Control::Open(Page::Host)));
        });
        row.spawn((tile, BackgroundColor(theme.glass_strong), BorderColor::all(theme.glass_border))).with_children(|card| {
            card.spawn(widgets::overline(theme, "Join a session"));
            card.spawn((widgets::dim(theme, "Connect straight to a friend by the address or the ticket they gave you."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            card.spawn(Node { flex_grow: 1.0, ..default() });
            card.spawn(widgets::styled_button(theme, ButtonKind::Secondary, "Join a game", percent(100), Control::Open(Page::Join)));
        });
    });
    // The list scrolls past the window — by the wheel and its bar, as the
    // award chooser's does — so a year of friends is a panel's height.
    parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(10), flex_shrink: 1.0, min_height: px(0), ..default() }).with_children(|section| {
        section.spawn(widgets::overline(theme, "Saved peers & recent connections"));
        if form.peers.peers.is_empty() {
            section.spawn((widgets::dim(theme, "Nobody yet — a game you join is remembered here, by the host's name, and a saved ticket works every time they host."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
        let now = servers::now();
        let list = section
            .spawn(Node { width: percent(100), flex_shrink: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(10), overflow: Overflow::scroll_y(), ..default() })
            .with_children(|list| {
        for (index, peer) in form.peers.peers.iter().enumerate() {
            list.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(14), flex_shrink: 0.0, ..default() }).with_children(|row| {
                row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }).with_children(|words| {
                    words.spawn(widgets::label(theme, peer.name.clone()));
                    // `shortened` says "(a ticket)" of a ticket itself.
                    words.spawn((widgets::dim(theme, format!("{} · last played {}", shortened(&peer.target), ago(now.saturating_sub(peer.last_connected)))), TextLayout::new(Justify::Left, LineBreak::AnyCharacter)));
                });
                row.spawn(widgets::small_button(theme, ButtonKind::Primary, "Quick connect", Control::QuickConnect(index)));
                row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Rename…", Control::RenamePeer(index)));
                row.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Remove", Control::RemovePeer(index)));
            });
        }
            })
            .id();
        section.spawn(Node { width: percent(100), flex_shrink: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() }).add_child(list).with_children(|row| {
            row.spawn(widgets::scrollbar(theme, list));
        });
    });
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
    });
}

/// How long ago, to the unit a person reads: what the hub says under a
/// saved peer.
fn ago(seconds: u64) -> String {
    let unit = |n: u64, one: &str, many: &str| format!("{n} {} ago", if n == 1 { one } else { many });
    match seconds {
        0..=59 => "just now".to_string(),
        60..=3599 => unit(seconds / 60, "minute", "minutes"),
        3600..=86_399 => unit(seconds / 3600, "hour", "hours"),
        _ => unit(seconds / 86_400, "day", "days"),
    }
}

/// The two active decks side by side, as the host and join pages show
/// them: the same tiles the dashboard draws, the picker opening on the
/// page's format.
fn spawn_deck_pair(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, format: NsgFormat, core: &ClientCore, images: &CardImages) {
    parent.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(14), ..default() }).with_children(|row| {
        for side in [Side::Corp, Side::Runner] {
            row.spawn(Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0), ..default() }).with_children(|slot| spawn_active_deck_in(slot, theme, form, side, format, core, images));
        }
    });
}

/// Hosting a session (Phase 7 §15): the active decks, the format as
/// pills greyed by them, the reach and the port, and Open the room.
fn spawn_host(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, core: &ClientCore, images: &CardImages) {
    parent.spawn(widgets::heading(theme, "Host a session"));
    section(parent, theme, "1. Active decks", |section| spawn_deck_pair(section, theme, form, form.format, core, images));
    section(parent, theme, "2. Format — checked against your active decks", |section| {
        let choices = form.host_choices();
        section.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), ..default() }).with_children(|row| {
            for (format, problem) in &choices {
                let label = netrunner_client::settings::format_label(*format);
                match problem {
                    None => {
                        let kind = if *format == form.format { ButtonKind::Primary } else { ButtonKind::Secondary };
                        row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, Control::Format(*format)));
                    }
                    Some(_) => {
                        row.spawn(widgets::disabled_button(theme, label, Val::Auto, Control::Format(*format)));
                    }
                }
            }
        });
        for (format, problem) in &choices {
            if let Some(problem) = problem {
                section.spawn((Text::new(format!("{}: {problem}", netrunner_client::settings::format_label(*format))), theme.font(size::SMALL), TextColor(theme.caution), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            }
        }
    });
    section(parent, theme, "3. Who can reach it", |section| {
        section.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), ..default() }).with_children(|row| {
            for reach in Reach::ALL {
                let kind = if reach == form.reach { ButtonKind::Primary } else { ButtonKind::Secondary };
                row.spawn(widgets::styled_button(theme, kind, reach.title(), Val::Auto, Control::Reach(reach)));
            }
        });
        section.spawn((widgets::dim(theme, form.reach.line()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        section.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12), ..default() }).with_children(|row| {
            row.spawn(widgets::label(theme, "Port"));
            row.spawn(Node { width: px(140), ..default() }).with_children(|slot| field_box(slot, theme, Field::Port, &form.port, "", false));
            if form.reach == Reach::Internet {
                let relay = match core.settings.relay.as_deref().map(str::trim) {
                    None | Some("") => "n0's public relays".to_string(),
                    Some("off") => "no relay: the ticket works only where a direct connection can be made".to_string(),
                    Some(relay) => relay.to_string(),
                };
                row.spawn((widgets::dim(theme, format!("Relay: {relay} (Settings)")), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            }
        });
    });
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Open the room", px(220), Control::Go));
    });
}

/// Joining a session (Phase 7 §15): the active decks, one field for the
/// ticket or the address with what it holds said under it, and Connect.
fn spawn_join(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, core: &ClientCore, images: &CardImages) {
    parent.spawn(widgets::heading(theme, "Join a session"));
    section(parent, theme, "1. Active decks", |section| spawn_deck_pair(section, theme, form, form.format, core, images));
    section(parent, theme, "2. Where", |section| {
        section.spawn(widgets::dim(theme, "Paste the ticket, or type the address, your opponent gave you."));
        field_box(section, theme, Field::Address, &form.address, "a ticket, or an address such as 192.168.1.50:8080", true);
        let target = form.target();
        let colour = if target == Target::Neither { theme.caution } else { theme.text_dim };
        section.spawn((Text::new(target.line()), theme.font(size::SMALL), TextColor(colour), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        section.spawn((widgets::dim(theme, "The format is the host's; a deck not legal in it is said in the room."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    });
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Connect", px(220), Control::Go));
    });
}

/// The room (Phase 7 §15), as host or guest: what to give out, the two
/// participants — each their decks, a chair preference and a Ready — and
/// the way out. Both Ready and the host's server seats them: there is no
/// Start after that. Polled every second (`room_ticks`) so the other
/// side's arrival and Ready are seen.
fn spawn_room(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, server: &ServerState, net: &Net) {
    let (me, opponent) = form.room_people();
    let lobby = server.room_lobby();
    parent.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(14), ..default() }).with_children(|row| {
        let title = if server.hosting {
            format!("Room · hosting for {}", form.reach.title().to_lowercase())
        } else {
            match opponent {
                Some(host) => format!("Room · {}'s game", host.name),
                None => format!("Room · {}", shortened(&server.address)),
            }
        };
        row.spawn((widgets::heading(theme, title), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }));
        let status = if server.link.is_some() {
            "Reconnecting".to_string()
        } else if server.hosting && opponent.is_none() {
            "Listening for a peer…".to_string()
        } else if server.hosting {
            format!("{} is here", opponent.map_or("Someone", |presence| presence.name.as_str()))
        } else {
            "Connected".to_string()
        };
        row.spawn(widgets::dim(theme, status));
    });
    if let Some(line) = &server.link {
        parent.spawn((widgets::notice(theme, line.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    if server.hosting {
        spawn_ways(parent, theme, net);
    }
    let format = lobby.map_or_else(|| "its format".to_string(), |lobby| netrunner_client::settings::format_label(lobby.format).to_string());
    section(parent, theme, format!("Participants · {format}"), |section| {
        let ready = server.seeking();
        let problem = form.ready_problem();
        // This person's row.
        section.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(8), padding: UiRect::all(px(10)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(12)), ..default() }).insert((BackgroundColor(theme.glass_strong), BorderColor::all(theme.glass_border))).with_children(|card| {
            card.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12), ..default() }).with_children(|row| {
                row.spawn(widgets::label(theme, if server.hosting { format!("{} (host, you)", form.player) } else { format!("{} (you)", form.player) }));
                let decks = [Side::Corp, Side::Runner].map(|side| form.active(side).map_or("no deck".to_string(), |deck| deck.deck.name.clone()));
                row.spawn((widgets::dim(theme, format!("{} / {}", decks[0], decks[1])), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }));
                row.spawn(widgets::dim(theme, if ready { "Ready" } else if me.is_some() { "Not ready" } else { "Joining…" }));
            });
            card.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(8), ..default() }).with_children(|row| {
                row.spawn(widgets::dim(theme, "Play as"));
                for prefer in [Some(Side::Corp), None, Some(Side::Runner)] {
                    let kind = if prefer == server.prefer { ButtonKind::Primary } else { ButtonKind::Secondary };
                    let label = if prefer.is_none() { "Either" } else { preference_label(prefer) };
                    if ready {
                        row.spawn(widgets::disabled_button(theme, label, Val::Auto, Control::Prefer(prefer)));
                    } else {
                        row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, Control::Prefer(prefer)));
                    }
                }
                row.spawn(Node { flex_grow: 1.0, ..default() });
                if ready {
                    row.spawn(widgets::styled_button(theme, ButtonKind::Secondary, "Not ready after all", px(220), (Control::Unready, ButtonSound(Sfx::Back))));
                } else if problem.is_some() {
                    row.spawn(widgets::disabled_button(theme, "Ready", px(220), Control::Ready));
                } else {
                    row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Ready", px(220), Control::Ready));
                }
            });
            if let Some(problem) = &problem {
                card.spawn((Text::new(problem.clone()), theme.font(size::SMALL), TextColor(theme.caution), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            }
        });
        // The opponent's row.
        section.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12), padding: UiRect::all(px(10)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(12)), ..default() }).insert((BackgroundColor(theme.glass), BorderColor::all(theme.glass_border))).with_children(|row| {
            match opponent {
                Some(presence) => {
                    row.spawn(widgets::label(theme, if server.hosting { presence.name.clone() } else { format!("{} (host)", presence.name) }));
                    let state = match (presence.seeking, presence.prefers) {
                        (true, Some(side)) => format!("prefers the {side:?} · ready"),
                        (true, None) => "either chair · ready".to_string(),
                        (false, _) => "not ready yet".to_string(),
                    };
                    row.spawn((widgets::dim(theme, state), Node { flex_grow: 1.0, ..default() }));
                }
                None => {
                    row.spawn(widgets::label(theme, "Opponent"));
                    row.spawn((widgets::dim(theme, if server.hosting { "waiting for a connection…" } else { "the host is not in the room yet…" }), Node { flex_grow: 1.0, ..default() }));
                }
            }
        });
        let line = match (ready, opponent.is_some_and(|presence| presence.seeking)) {
            (true, true) => Some("Both ready: the game is starting…".to_string()),
            (false, true) => Some(format!("{} is ready. Press Ready and the game starts.", opponent.map_or("Your opponent", |presence| presence.name.as_str()))),
            (true, false) => Some("Ready. The game starts when your opponent is too.".to_string()),
            (false, false) => None,
        };
        if let Some(line) = line {
            section.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
    });
    if let Some(notice) = &form.notice {
        parent.spawn((widgets::notice(theme, notice.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, if server.hosting { "Stop hosting" } else { "Leave" }, Val::Auto, (Control::Disconnect, ButtonSound(Sfx::Back))));
    });
}

/// Renaming a saved peer.
fn spawn_edit_peer(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm) {
    parent.spawn(widgets::heading(theme, "Rename peer"));
    parent.spawn((widgets::dim(theme, shortened(&form.edit_peer.target)), TextLayout::new(Justify::Left, LineBreak::AnyCharacter)));
    section(parent, theme, "Name", |section| field_box(section, theme, Field::PeerName, &form.edit_peer.name, "what you call them", false));
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Save", px(220), Control::SavePeer));
    });
}

/// The person's servers (Phase 7 §14): a row each — its name, whether it
/// is the Default, its address, what it keeps of the person — with its
/// Connect (or, for the one attached, Disconnect), Make default, Edit and
/// Remove; then what this machine knows, and an address or a ticket to
/// connect to once, which is how a host's game is joined.
fn spawn_servers(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm) {
    parent.spawn(widgets::heading(theme, "Servers"));
    let attached = form.attached_address();
    if form.book.servers.is_empty() {
        parent.spawn((widgets::dim(theme, "No server yet. Add one by its address, or connect to one below: a server you connect to is listed here, named by its address until you name it."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    for (index, row) in form.book.servers.iter().enumerate() {
        let here = attached == Some(row.address.as_str());
        let is_default = form.book.default.as_deref() == Some(row.address.as_str());
        parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }).with_children(|card| {
            let mut tags = Vec::new();
            if is_default {
                tags.push("Default");
            }
            if here {
                tags.push("Connected");
            }
            let title = if tags.is_empty() { row.name.clone() } else { format!("{} · {}", row.name, tags.join(" · ")) };
            card.spawn(widgets::overline(theme, title));
            if row.name != row.address {
                card.spawn(widgets::dim(theme, shortened(&row.address)));
            }
            let lines: Vec<String> = match (here, form.server.as_ref(), form.row_profiles.get(&row.address)) {
                (true, Some(server), _) => netrunner_client::identity::profile_lines(server.known_profile()),
                (_, _, Some(RowProfile::Lines(lines))) => lines.clone(),
                (_, _, Some(RowProfile::Failed(reason))) => vec![reason.clone()],
                (_, _, Some(RowProfile::Asking)) => vec!["Asking…".to_string()],
                _ if row.key.is_none() => vec!["Not met yet: what it keeps of you is shown once you have connected".to_string()],
                _ => Vec::new(),
            };
            for line in lines {
                card.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            }
            card.spawn(widgets::row(10.0)).with_children(|buttons| {
                if here {
                    buttons.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Disconnect", Control::Disconnect));
                } else {
                    buttons.spawn(widgets::small_button(theme, ButtonKind::Primary, "Connect", Control::ConnectRow(index)));
                }
                buttons.spawn(widgets::small_button(theme, if is_default { ButtonKind::Primary } else { ButtonKind::Secondary }, if is_default { "Default" } else { "Make default" }, Control::DefaultRow(index)));
                buttons.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Edit…", Control::EditRow(Some(index))));
                buttons.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Remove", Control::RemoveRow(index)));
            });
        });
    }
    parent.spawn(widgets::row(12.0)).with_children(|row| {
        row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Add a server…", Control::EditRow(None)));
        row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Local / P2P play…", Control::Open(Page::Hub)));
        row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Watch a game…", Control::Open(Page::Watch)));
    });
    if !form.local.is_empty() {
        section(parent, theme, "On this machine", |section| {
            for line in &form.local {
                section.spawn((widgets::dim(theme, line.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            }
        });
    }
    section(parent, theme, "Connect once", |section| {
        field_box(section, theme, Field::Address, &form.address, "a server's address, or paste a host's ticket", true);
        section.spawn(widgets::dim(theme, "A server connected to by its address is listed above; a host's ticket opens their room, and the hub remembers it."));
    });
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        row.spawn(widgets::styled_button(theme, ButtonKind::Secondary, "Connect", px(220), Control::Go));
    });
}

/// The pop-up a title or a badge is picked from (Phase 7 §14): over the
/// screen on a wash a press on which closes it, as Escape does; what was
/// earned in a list that scrolls — by the wheel and its bar — so a
/// hundred of them are a pop-up's height, not a page's; None first, the
/// one worn lit. Titles are rows of words; badges are a grid of their
/// glyphs, each named under it. A pick is worn at once.
fn spawn_chooser(parent: &mut ChildSpawnerCommands, theme: &Theme, kind: AwardKind, profile: &netrunner_server::protocol::Profile, pictures: &HashMap<String, Option<Handle<Image>>>) {
    let (earned, worn, heading) = match kind {
        AwardKind::Title => (&profile.titles, profile.title.as_deref(), "Wear a title"),
        AwardKind::Badge => (&profile.badges, profile.badge.as_deref(), "Wear a badge"),
    };
    parent
        .spawn((
            ChooserWash,
            Button,
            GlobalZIndex(25),
            Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(theme.wash.with_alpha(0.72)),
        ))
        .with_children(|wash| {
            let mut panel = wash.spawn((Interaction::None, bevy::ui::FocusPolicy::Block, widgets::roomy_panel(theme, px(720))));
            panel.entry::<Node>().and_modify(|mut node| node.max_height = percent(80));
            panel.with_children(|panel| {
                panel.spawn(widgets::heading(theme, heading));
                panel.spawn(widgets::dim(theme, format!("{} earned here. The one you wear is shown beside your name on this server.", earned.len())));
                let grid = kind == AwardKind::Badge;
                let list = panel
                    .spawn((
                        bevy::ui_widgets::ScrollArea,
                        Node {
                            width: percent(100),
                            flex_shrink: 1.0,
                            min_height: px(0),
                            flex_direction: if grid { FlexDirection::Row } else { FlexDirection::Column },
                            flex_wrap: if grid { FlexWrap::Wrap } else { FlexWrap::NoWrap },
                            row_gap: px(8),
                            column_gap: px(10),
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                    ))
                    .with_children(|list| {
                        let pill = |chosen: bool| if chosen { ButtonKind::Primary } else { ButtonKind::Secondary };
                        if !grid {
                            list.spawn(widgets::styled_button(theme, pill(worn.is_none()), "None", percent(100), Control::Pick(None)));
                            for (index, award) in earned.iter().enumerate() {
                                list.spawn(widgets::styled_button(theme, pill(worn == Some(award.name.as_str())), award.name.clone(), percent(100), Control::Pick(Some(index))));
                            }
                            return;
                        }
                        let tile = |chosen: bool, pick: Option<usize>| {
                            (
                                Button,
                                widgets::Themed,
                                ButtonSound(Sfx::Toggle),
                                Control::Pick(pick),
                                Node {
                                    width: px(120),
                                    flex_direction: FlexDirection::Column,
                                    align_items: AlignItems::Center,
                                    row_gap: px(6),
                                    padding: UiRect::all(px(10)),
                                    border: UiRect::all(px(if chosen { 2.0 } else { 1.0 })),
                                    border_radius: BorderRadius::all(px(12)),
                                    ..default()
                                },
                                BackgroundColor(theme.glass_strong),
                                BorderColor::all(if chosen { theme.accent } else { theme.glass_border }),
                            )
                        };
                        list.spawn(tile(worn.is_none(), None)).with_children(|tile| {
                            tile.spawn(Node { width: px(56), height: px(56), ..default() });
                            tile.spawn(widgets::dim(theme, "None"));
                        });
                        for (index, award) in earned.iter().enumerate() {
                            list.spawn(tile(worn == Some(award.name.as_str()), Some(index))).with_children(|tile| {
                                badge::spawn_glyph(tile, theme, &award.name, pictures.get(&award.name).cloned().flatten(), 56.0);
                                tile.spawn((widgets::dim(theme, award.name.clone()), TextLayout::new(Justify::Center, LineBreak::WordBoundary)));
                            });
                        }
                    })
                    .id();
                panel.spawn(Node { width: percent(100), flex_shrink: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() }).add_child(list).with_children(|row| {
                    row.spawn(widgets::scrollbar(theme, list));
                });
                panel.spawn(Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::FlexEnd, margin: UiRect::top(px(8)), ..default() }).with_children(|row| {
                    row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, (Control::CloseChooser, ButtonSound(Sfx::Back))));
                });
            });
        });
}

/// Adding a server, or editing one: its name and its address. A new
/// address is another server, whose key is not the old one's.
fn spawn_edit_server(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm) {
    let heading = match &form.edit.original {
        Some(_) => format!("Edit {}", form.edit.name),
        None => "Add a server".to_string(),
    };
    parent.spawn(widgets::heading(theme, heading));
    section(parent, theme, "Name", |section| field_box(section, theme, Field::ServerName, &form.edit.name, "what you call it", false));
    section(parent, theme, "Address", |section| {
        field_box(section, theme, Field::ServerAddress, &form.edit.address, "host or host:port", true);
        if form.edit.original.is_some() {
            section.spawn(widgets::dim(theme, "A new address is another server: the key this one proved is forgotten, and the next connection meets the new one's."));
        }
    });
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Save", px(220), Control::Go));
    });
}

/// What the server attached to keeps of the person (Phase 4 §10): the
/// standing per format, the title and the badge worn with how many were
/// earned and a Change… each, and the ladder of a format.
fn spawn_profile(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, server: &ServerState, pictures: &HashMap<String, Option<Handle<Image>>>) {
    parent.spawn(widgets::heading(theme, format!("Your profile at {}", shortened(&form.server_name(&server.address)))));
    let Some(profile) = server.known_profile() else {
        let why = match server.profile {
            ProfileHere::NoKey => "You connected without a key, so this server keeps nothing of you.",
            ProfileHere::Unkept => "This server keeps nothing: it rates no game and grants nothing.",
            _ => "Asking…",
        };
        parent.spawn(widgets::dim(theme, why));
        buttons(parent, |row| {
            row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        });
        return;
    };
    section(parent, theme, "Standing", |section| {
        let lines = netrunner_client::identity::standing_lines(&profile.standings);
        if lines.is_empty() {
            section.spawn(widgets::dim(theme, "No rated game here yet."));
        }
        for line in lines {
            section.spawn(widgets::dim(theme, line));
        }
    });
    // A line a kind: what is worn, how many were earned, and Change… to
    // the pop-up that lists them — never a pill each, which a year of
    // events would make a wall of.
    section(parent, theme, "Title", |section| {
        section.spawn(widgets::row(12.0)).with_children(|row| {
            row.spawn((widgets::label(theme, profile.title.clone().unwrap_or_else(|| "None worn".to_string())), Node { flex_grow: 1.0, min_width: px(0), ..default() }));
            if profile.titles.is_empty() {
                row.spawn(widgets::dim(theme, "None earned here yet"));
            } else {
                row.spawn(widgets::dim(theme, format!("{} earned", profile.titles.len())));
                row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Change…", Control::Choose(AwardKind::Title)));
            }
        });
    });
    section(parent, theme, "Badge", |section| {
        section.spawn(widgets::row(12.0)).with_children(|row| {
            if let Some(name) = &profile.badge {
                badge::spawn_glyph(row, theme, name, pictures.get(name).cloned().flatten(), 40.0);
            }
            row.spawn((widgets::label(theme, profile.badge.clone().unwrap_or_else(|| "None worn".to_string())), Node { flex_grow: 1.0, min_width: px(0), ..default() }));
            if profile.badges.is_empty() {
                row.spawn(widgets::dim(theme, "None earned here yet"));
            } else {
                row.spawn(widgets::dim(theme, format!("{} earned", profile.badges.len())));
                row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Change…", Control::Choose(AwardKind::Badge)));
            }
        });
    });
    section(parent, theme, "Ladder", |section| {
        section.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), ..default() }).with_children(|pills| {
            for format in FORMATS {
                let kind = if format == server.ladder_format { ButtonKind::Primary } else { ButtonKind::Secondary };
                pills.spawn(widgets::styled_button(theme, kind, capitalised(format_name(format)), Val::Auto, Control::Ladder(format)));
            }
        });
        match &server.ladder {
            None => {
                section.spawn(widgets::dim(theme, "Asking…"));
            }
            Some(rows) if rows.is_empty() => {
                section.spawn(widgets::dim(theme, format!("Nobody has {} rated games in {} here yet.", netrunner_server::protocol::LADDER_MIN_GAMES, capitalised(format_name(server.ladder_format)))));
            }
            Some(rows) => {
                for (place, row) in rows.iter().enumerate() {
                    let mine = server.key.as_ref() == Some(&row.key);
                    let line = netrunner_client::identity::ladder_line(place + 1, row);
                    section.spawn(if mine { widgets::label(theme, format!("{line} · you")) } else { widgets::label(theme, line) });
                }
            }
        }
    });
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
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
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
    });
}

/// Dialling, or hosting until attached, or a spectator waiting for a
/// place: where the connection is, and Stop.
fn spawn_waiting(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm) {
    parent.spawn(widgets::heading(theme, "Waiting"));
    parent.spawn((widgets::dim(theme, form.status.clone()), TextLayout::new(Justify::Left, LineBreak::AnyCharacter)));
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Stop", Val::Auto, Control::Back));
    });
}

/// The deck this player would lock in for `side` in the open tournament,
/// from those legal in its format. Its own purpose, so the model can
/// tell whose choice a pick is.
fn tournament_deck_dropdown(parent: &mut ChildSpawnerCommands, theme: &Theme, server: &ServerState, side: Side, format: NsgFormat) {
    let chosen = match side {
        Side::Corp => server.tournament_corp,
        Side::Runner => server.tournament_runner,
    };
    deck_slot_of(parent, theme, server.tournament_side_decks(side), chosen, side, format, Purpose::Tournament(side));
}

/// The chosen deck for a side, in a line, with Change…, which opens the
/// deck picker (`screens::deck_picker`, Phase 7 §16) on the lobby's or
/// the tournament's format. It replaced a drop-down of names.
fn deck_slot_of(parent: &mut ChildSpawnerCommands, theme: &Theme, decks: Vec<&DeckFile>, chosen: usize, side: Side, format: NsgFormat, purpose: Purpose) {
    let Some(deck) = decks.get(chosen) else {
        parent.spawn(widgets::dim(theme, format!("No {side:?} deck is legal in {}: build one under Decks.", capitalised(format_name(format)))));
        return;
    };
    parent.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12), ..default() }).with_children(|row| {
        row.spawn(widgets::overline(theme, format!("{side:?} deck")));
        row.spawn((Text::new(online::label(deck)), theme.font(size::BODY), TextColor(theme.text), DeckLine(purpose)));
        row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Change…", DeckSlot { purpose, chosen: deck.id.clone(), format }));
    });
}

/// The server's tournaments, each a button to its page, and the way to
/// hold one.
fn spawn_tournaments(parent: &mut ChildSpawnerCommands, theme: &Theme, server: &ServerState) {
    parent.spawn(widgets::heading(theme, format!("Tournaments at {}", shortened(&server.address))));
    parent.spawn((widgets::dim(theme, "Run as Null Signal Games run theirs: each entrant locks two decks, one a side, for the whole event behind a signed commitment; the lists stay private. The rounds come in a later stage."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    section(parent, theme, "Tournaments", |section| {
        match (server.tournaments_listed, server.tournaments.is_empty()) {
            (false, _) => section.spawn(widgets::dim(theme, "Asking the server…")),
            (true, true) => section.spawn(widgets::dim(theme, "This server holds none.")),
            (true, false) => section.spawn(widgets::dim(theme, "Open one to see who has entered, and to enter.")),
        };
        for (index, info) in server.tournaments.iter().enumerate() {
            section.spawn(widgets::styled_button(theme, ButtonKind::Secondary, tournament::line(info), percent(100), Control::OpenTournament(index)));
        }
        section.spawn(widgets::row(12.0)).with_children(|row| {
            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Refresh", Control::Refresh));
            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Hold a tournament…", Control::Open(Page::MakeTournament)));
        });
    });
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
    });
}

/// One tournament: its code, format and state, who holds it and where
/// this key stands in it, its entrants, and — for a key that may — the
/// two decks to lock in and Register, with Withdraw for an entrant.
fn spawn_tournament(parent: &mut ChildSpawnerCommands, theme: &Theme, server: &ServerState) {
    let Some(info) = server.open_tournament() else {
        parent.spawn(widgets::heading(theme, "Tournament"));
        parent.spawn(widgets::dim(theme, "The server no longer lists it."));
        buttons(parent, |row| {
            row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        });
        return;
    };
    parent.spawn(widgets::heading(theme, info.name.clone()));
    parent.spawn(widgets::dim(theme, format!("Code {} · {} · {}", info.id, capitalised(format_name(info.format)), tournament::state_label(info.state))));
    parent.spawn(widgets::dim(theme, format!("Held by {}", info.organizer.fingerprint())));
    parent.spawn((widgets::dim(theme, tournament::standing_line(info, server.key.as_ref())), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    match info.state {
        TournamentState::Registering => {
            section(parent, theme, if info.entrants.is_empty() { "Entered".to_string() } else { format!("Entered ({})", info.entrants.len()) }, |section| {
                if info.entrants.is_empty() {
                    section.spawn(widgets::dim(theme, "Nobody has entered yet."));
                }
                for entrant in &info.entrants {
                    section.spawn(widgets::label(theme, tournament::entrant_line(entrant)));
                }
            });
        }
        // In the rounds the standings stand where the entrants did: they
        // fold off the rounds the server published (`swiss::standings`),
        // so what is drawn here is what anyone holding them would compute.
        TournamentState::Playing { .. } | TournamentState::Finished => {
            section(parent, theme, if info.state == TournamentState::Finished { "Final standings" } else { "Standings" }, |section| {
                for (index, standing) in info.standings().iter().enumerate() {
                    section.spawn(widgets::label(theme, tournament::standing_row(info, index + 1, standing)));
                }
                // Whether every secret the server revealed is the one it
                // committed to (Phase 4 §7 stage 6c).
                if let Some(line) = tournament::seeds_line(info) {
                    section.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
            });
            if let Some(current) = info.current_round() {
                section(parent, theme, format!("Round {}", info.rounds.len()), |section| {
                    if let Some(line) = tournament::round_line(info, server.key.as_ref()) {
                        section.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    }
                    if let Some(line) = tournament::clock_line(info, unix_now()) {
                        section.spawn((widgets::label(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    }
                    for (index, table) in current.tables.iter().enumerate() {
                        section.spawn(widgets::row(12.0)).with_children(|row| {
                            row.spawn((widgets::label(theme, tournament::table_line(info, index, table)), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }));
                            // The organizer records a table nobody played.
                            if server.tables_to_record().contains(&index) {
                                for (outcome, label) in [(TableOutcome::CorpWon, "Corp won"), (TableOutcome::RunnerWon, "Runner won"), (TableOutcome::Tie, "Tie")] {
                                    row.spawn(widgets::small_button(theme, ButtonKind::Secondary, label, Control::RecordResult(index, outcome)));
                                }
                            }
                        });
                    }
                    if let Some(bye) = tournament::bye_line(info, current) {
                        section.spawn(widgets::dim(theme, bye));
                    }
                    if server.may_sit() {
                        section.spawn(widgets::row(12.0)).with_children(|row| match server.searching.as_ref().and_then(|searching| searching.position) {
                            Some(_) => {
                                row.spawn(widgets::dim(theme, "Seated — waiting for your opponent to sit…"));
                                row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Stand up", Val::Auto, Control::CancelSeek));
                            }
                            None => {
                                row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Sit at your table", px(240), Control::Sit));
                            }
                        });
                    }
                    // An intentional draw (2.5.8) is two offers: the
                    // opponent's back is the agreement, and the table a
                    // tie. The round line says when one stands.
                    if server.may_offer_draw() {
                        section.spawn(widgets::row(12.0)).with_children(|row| {
                            row.spawn(widgets::styled_button(theme, ButtonKind::Secondary, "Offer a draw", Val::Auto, Control::OfferDraw));
                            row.spawn(widgets::dim(theme, "A tie if your opponent offers one too."));
                        });
                    }
                    // A drop is the same message as a withdrawal; the
                    // server tells a player with a game under way to
                    // concede it on the board first.
                    if server.may_drop() {
                        section.spawn(widgets::row(12.0)).with_children(|row| {
                            row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Drop from the tournament", Val::Auto, Control::Unregister));
                            row.spawn(widgets::dim(theme, "Your results stand; you are paired no more."));
                        });
                    }
                });
            }
        }
    }
    if server.is_organizer() && (server.next_round().is_some() || server.may_finish()) {
        section(parent, theme, "Organizer", |section| {
            section.spawn(widgets::row(12.0)).with_children(|row| {
                if let Some(label) = server.next_round() {
                    row.spawn(widgets::styled_button(theme, ButtonKind::Primary, label, Val::Auto, Control::BeginRound));
                }
                if server.may_finish() {
                    row.spawn(widgets::styled_button(theme, ButtonKind::Secondary, "End the tournament", Val::Auto, Control::FinishTournament));
                }
            });
            section.spawn(widgets::dim(theme, if info.state == TournamentState::Registering { "Beginning the first round closes registration and pairs everyone entered." } else { "The next round pairs by the standings; ending it makes them final." }));
        });
    }
    if server.may_register() {
        section(parent, theme, "Your entry", |section| {
            section.spawn((widgets::dim(theme, "The two decks you play for the whole event. The server checks them against the format and publishes a commitment to them, never the lists."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            tournament_deck_dropdown(section, theme, server, Side::Corp, info.format);
            tournament_deck_dropdown(section, theme, server, Side::Runner, info.format);
            section.spawn(widgets::row(12.0)).with_children(|row| {
                let label = if server.is_entered() { "Register again with these decks" } else { "Register" };
                row.spawn(widgets::styled_button(theme, ButtonKind::Primary, label, Val::Auto, Control::Register));
                if server.is_entered() {
                    row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Withdraw", Val::Auto, Control::Unregister));
                }
            });
        });
    }
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Refresh", Control::Refresh));
    });
}

/// Seconds since the Unix epoch, this machine's clock, for the round's
/// countdown.
fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

fn spawn_make_tournament(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm) {
    parent.spawn(widgets::heading(theme, "Hold a tournament"));
    parent.spawn((widgets::dim(theme, "Held by your key, which has the final say over it. Entrants register two decks, one a side, locked for the whole event."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    section(parent, theme, "Name", |section| field_box(section, theme, Field::TournamentName, &form.make_tournament.name, "what the entrants see", false));
    format_section(parent, theme, form.make_tournament.format, "The format every entrant's decks must be legal in.");
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Hold", px(220), Control::Go));
    });
}

fn spawn_make_lobby(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm) {
    parent.spawn(widgets::heading(theme, "Make a friend lobby"));
    parent.spawn((widgets::dim(theme, "Never listed and never rated: your group joins it by the code the server gives it, under Casual, and plays whoever else is looking there."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    section(parent, theme, "Name", |section| field_box(section, theme, Field::LobbyName, &form.make.name, "what the others see", false));
    format_section(parent, theme, form.make.format, "The format games in this lobby are played in.");
    section(parent, theme, "Bots", |section| {
        section.spawn(widgets::row(10.0)).with_children(|row| {
            for (bots, label) in [(false, "People only"), (true, "Bots on request")] {
                let kind = if bots == form.make.bots { ButtonKind::Primary } else { ButtonKind::Secondary };
                row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, Control::LobbyBots(bots)));
            }
        });
        section.spawn(widgets::dim(theme, if form.make.bots { "Anyone here may play a bot instead of waiting for the group — the only place a bot sits." } else { "Games here are between the people who join." }));
    });
    section(parent, theme, "Password", |section| field_box(section, theme, Field::LobbyPassword, &form.make.password, "none", false));
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Make", px(220), Control::Go));
    });
}

/// The format, as pills: a handful of choices is a row, not a drop-down.
fn format_section(parent: &mut ChildSpawnerCommands, theme: &Theme, chosen: NsgFormat, blurb: &str) {
    section(parent, theme, "Format", |section| {
        section.spawn(widgets::row(10.0)).with_children(|row| {
            for format in FORMATS {
                let kind = if format == chosen { ButtonKind::Primary } else { ButtonKind::Secondary };
                row.spawn(widgets::styled_button(theme, kind, capitalised(format_name(format)), Val::Auto, Control::Format(format)));
            }
        });
        section.spawn(widgets::dim(theme, blurb));
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
        TextField::new(current.to_string(), field.max_len()),
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
