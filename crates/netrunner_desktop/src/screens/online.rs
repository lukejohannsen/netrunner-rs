//! Play Online: host a game, connect to a server by address or ticket, or
//! watch a game (Phase 7 §7). The terminal's screen of the same name
//! (`netrunner_cli::tui::online`) drawn as the desktop's forms are, over
//! the same pieces: `netrunner_client::online` for the decks brought,
//! `hosting::Invitation` for what a host gives out, `remote` for the
//! connection, and `MatchHandle::start_remote` for the match — so the
//! board plays a game online exactly as it plays one at home.
//!
//! **The connection outlives the game** (Phase 4 §7 stage 4c). Hosting
//! and connecting both end on the Server page, attached: the lobbies
//! listed, one joined, a lobby made or a closed one joined by its code,
//! and in a lobby a game looked for in a chair with that chair's deck or
//! decks. The [`Connected`] resource holds the connection, and it is not a
//! part of this screen's tree: it stays while the board plays the game
//! and is polled again when the board leads back here, where the
//! `BackInLobby` is waiting and the next game is a chair and a deck away.
//! It goes when the person disconnects — Escape or the button on the
//! Server page — or leaves the screen for anywhere but the board.
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
use netrunner_client::online;
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
use netrunner_server::protocol::{LobbyInfo, TournamentState};
use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};
use netrunner_server::MatchSummary;

use crate::audio::{ButtonSound, Sfx};
use crate::core::{ClientCore, TokioRuntime};
use crate::models::online::{lobby_title, reach_pill, AwardKind, ChairChoice, Field, Intent, OnlineForm, Outcome, Page, RowProfile, ServerState};
use crate::widgets::badge::{self, BadgeGlyphs};
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
            .add_systems(Update, (dev_page, auto_connect, controls, chooser_wash, fields, net, round_clock_ticks, refresh).chain().run_if(in_state(AppScreen::Online)));
    }
}

/// How long a host's server outlives the connection it was let go with:
/// long enough for a concession to reach the opponent, over a relay too.
const HOST_LINGER: Duration = Duration::from_secs(3);

/// How long a server has to list its matches, or to say what it keeps
/// of the person for their row.
const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// Set on opening the screen afresh when a server is the Default: it is
/// connected to on the first frame, as a press of its Connect would
/// (Phase 7 §14), and never again until the screen is opened afresh.
#[derive(Resource)]
struct AutoConnect;

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
    Closed(bool),
    /// Make a lobby's: whether its games count.
    Casual(bool),
    WatchFrom(Side),
    Watch(usize),
    /// The `n`th thing the host gives out.
    Copy(usize),
    /// The Server page's.
    JoinLobby(usize),
    JoinById,
    LeaveLobby,
    Refresh,
    Chair(ChairChoice),
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
/// on the lobby's or the tournament's format, the cursor on the deck
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

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, model: Option<ResMut<Model>>, connected: Option<Res<Connected>>) {
    commands.init_resource::<Dirty>();
    commands.insert_resource(Net::default());
    match model {
        Some(mut model) => model.0.reopen(connected.is_some()),
        None => {
            let format = core.settings.format.unwrap_or(netrunner_client::settings::DEFAULT_FORMAT);
            // Join starts from the Default, else the server connected to
            // last — both from the list both clients keep (Phase 7 §14).
            let address = core.server_book().start_address().map_or_else(|| hosting::normalize_address("127.0.0.1"), str::to_string);
            let preferred = [core.settings.active_deck(Side::Corp).map(str::to_string), core.settings.active_deck(Side::Runner).map(str::to_string)];
            let mut form = OnlineForm::new(address, format).with_preferred(preferred);
            let book = core.server_book();
            if book.default_server().is_some() && connected.is_none() {
                commands.insert_resource(AutoConnect);
            }
            form.apply(Intent::Servers(book));
            form.apply(Intent::Local(local_lines(&core)));
            commands.insert_resource(Model(form));
        }
    }
    let form = commands.spawn((FormRoot, Node { flex_direction: FlexDirection::Column, row_gap: px(20), width: percent(100), ..default() })).id();
    let panel = commands.spawn(widgets::roomy_panel(&theme, px(940))).add_child(form).id();
    let column = commands
        .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(16), margin: UiRect::vertical(Val::Auto), max_width: percent(100), ..default() })
        .with_children(|parent| {
            parent.spawn(widgets::title(&theme, AppScreen::Online.title()));
            parent.spawn(widgets::dim(&theme, format!("Against a person, as {}. A lobby says whether its games are rated; a move cannot be taken back.", core.player_name())));
        })
        .add_child(panel)
        .id();
    commands.spawn(screen_root(AppScreen::Online, &theme)).add_child(column);
    commands.insert_resource(Dirty(true));
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

/// The Default, connected to on the first frame of a fresh visit.
#[allow(clippy::too_many_arguments)]
fn auto_connect(
    mut commands: Commands,
    pending: Option<Res<AutoConnect>>,
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
    commands.remove_resource::<AutoConnect>();
    let outcome = model.0.apply(Intent::ConnectDefault);
    dirty.0 = true;
    carry_out(outcome, &mut model.0, &mut net, &mut commands, &mut core, runtime.as_deref(), &mut navigate);
}

/// Leaving the screen: the connection stays only for the board, which
/// plays a game found on it and leads back here.
fn leave(world: &mut World) {
    world.remove_resource::<Net>();
    world.remove_resource::<AutoConnect>();
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

/// Escape steps back a page; from Home the navigation rule takes it off
/// the screen. After the text field's system, so an Escape that cancels
/// an edit does nothing more.
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
    if captured.0 || !keys.just_pressed(KeyCode::Escape) || model.0.page == Page::Home {
        return;
    }
    captured.0 = true;
    let outcome = model.0.apply(Intent::Back);
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
    // A deck chosen in the picker: the side's choice goes to it, on the
    // page the purpose names. The picker offers every deck of the side,
    // legal or not; one the lobby's list does not hold is not legal in
    // its format, and the picker refused it already.
    for DeckPicked { purpose, id } in picked.read() {
        let Some(server) = &model.0.server else { continue };
        let intent = match purpose {
            Purpose::Lobby(Side::Corp) => server.corp_decks().iter().position(|deck| deck.id == *id).map(Intent::SetCorpDeck),
            Purpose::Lobby(Side::Runner) => server.runner_decks().iter().position(|deck| deck.id == *id).map(Intent::SetRunnerDeck),
            Purpose::Tournament(side) => {
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
                Purpose::Lobby(side) | Purpose::Tournament(side) => side,
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
            Control::Closed(closed) => Intent::SetClosed(closed),
            Control::Casual(casual) => Intent::SetCasual(casual),
            Control::WatchFrom(side) => Intent::SetWatchFrom(side),
            Control::Watch(index) => Intent::Watch(index),
            Control::JoinLobby(index) => Intent::JoinLobby(index),
            Control::JoinById => Intent::JoinById,
            Control::LeaveLobby => Intent::LeaveLobby,
            Control::Refresh => Intent::Refresh,
            Control::Chair(chair) => Intent::SetChair(chair),
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
            Outcome::Decks(format) => form.set_decks(deck_choices(core, format)),
            Outcome::TournamentDecks(format) => form.set_tournament_decks(deck_choices(core, format)),
            Outcome::ListLobbies
            | Outcome::JoinLobby { .. }
            | Outcome::LeaveLobby
            | Outcome::CreateLobby { .. }
            | Outcome::Seek(_)
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
            remember_server(core, &url);
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
        Outcome::CreateLobby { name, format, closed, password, casual } => connected.attached.create_lobby(name, format, closed, password, casual),
        Outcome::Seek(chair) => connected.attached.seek(chair),
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
    match start_hosting(port, reach, format, relay, runtime.handle()) {
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
    if std::mem::replace(&mut *done, true) {
        return;
    }
    dirty.0 = true;
    match page.as_str() {
        "host" => form.apply(Intent::Open(Page::Host)),
        "join" | "servers" => {
            let outcome = form.apply(Intent::Open(Page::Servers));
            carry_out(outcome, form, &mut net, &mut commands, &mut core, runtime.as_deref(), &mut navigate);
            Outcome::Nothing
        }
        "watch" => form.apply(Intent::Open(Page::Watch)),
        "hosting" | "ticket" => {
            let Some(runtime) = runtime else { return };
            let _guard = runtime.0.enter();
            form.apply(Intent::Open(Page::Host));
            let relay = (page == "ticket").then_some(Relay::Off);
            let format = form.format;
            host(form, &mut net, &mut commands, &core, &runtime, 0, Reach::Network, format, relay);
            Outcome::Nothing
        }
        "spectate" | "spectate-corp" => {
            let Some(runtime) = runtime else { return };
            let _guard = runtime.0.enter();
            form.apply(Intent::Open(Page::Watch));
            form.apply(Intent::SetWatchFrom(if page == "spectate" { Side::Runner } else { Side::Corp }));
            match start_hosting(0, Reach::ThisMachine, form.format, None, runtime.handle()) {
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

/// Binds a human-vs-human server on `port` and starts it, returning it
/// and the loopback address the host attaches by — the terminal's
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

#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    root: Query<Entity, With<FormRoot>>,
    screen: Query<Entity, (With<DespawnOnExit<AppScreen>>, With<Node>)>,
    shown: Query<Entity, With<ChooserWash>>,
    theme: Res<Theme>,
    model: Res<Model>,
    net: Res<Net>,
    core: Res<ClientCore>,
    mut glyphs: ResMut<BadgeGlyphs>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    if !std::mem::take(&mut dirty.0) {
        return;
    }
    let Ok(root) = root.single() else { return };
    // Every badge the page may draw, looked up once a name.
    let basic = core.settings.desktop.basic_graphics;
    let pictures: HashMap<String, Option<Handle<Image>>> = model
        .0
        .server
        .as_ref()
        .and_then(ServerState::known_profile)
        .map(|profile| profile.badges.iter().map(|award| (award.name.clone(), glyphs.of(&award.name, basic, images.as_deref_mut()))).collect())
        .unwrap_or_default();
    commands.entity(root).despawn_children().with_children(|parent| spawn_page(parent, &theme, &model.0, &net, &pictures));
    for entity in &shown {
        commands.entity(entity).despawn();
    }
    let choosing = model.0.server.as_ref().filter(|_| model.0.page == Page::Profile).and_then(|server| Some((server.choosing?, server.known_profile()?)));
    if let (Some((kind, profile)), Some(screen)) = (choosing, screen.iter().next()) {
        commands.entity(screen).with_children(|parent| spawn_chooser(parent, &theme, kind, profile, &pictures));
    }
}

fn spawn_page(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, net: &Net, pictures: &HashMap<String, Option<Handle<Image>>>) {
    match form.page {
        Page::Home => spawn_home(parent, theme),
        Page::Host => spawn_host(parent, theme, form),
        Page::Servers => spawn_servers(parent, theme, form),
        Page::EditServer => spawn_edit_server(parent, theme, form),
        Page::Watch => spawn_watch(parent, theme, form, net.listing.is_some()),
        Page::Waiting => spawn_waiting(parent, theme, form),
        Page::Server => match &form.server {
            Some(server) => spawn_server(parent, theme, form, server, net),
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
    if let Some(notice) = &form.notice {
        parent.spawn((widgets::notice(theme, notice.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
}

/// The three ways in, each a pill with what it does beside it — the main
/// menu's rows.
fn spawn_home(parent: &mut ChildSpawnerCommands, theme: &Theme) {
    let ways = [
        (Page::Host, "Host a game", "Play on this machine and give your opponent an address, or a ticket that works from anywhere"),
        (Page::Servers, "Join a server", "Your servers, each by name and one the Default this screen connects to; or a host's address or ticket, once"),
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
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
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
    format_section(parent, theme, form.format, "The format this game is played in: your server has one lobby, and your opponent brings a deck legal in it. You choose your own deck when you look for the game.");
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Start hosting", px(220), Control::Go));
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
        section.spawn(widgets::dim(theme, "A server connected to by its address is listed above; a host's ticket is one game's and is not kept."));
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

/// Attached: what a host gives out, the lobbies, a closed one's code, and
/// in a lobby the game to look for.
fn spawn_server(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &OnlineForm, server: &ServerState, net: &Net) {
    parent.spawn(widgets::heading(theme, if server.hosting { "Hosting on this machine".to_string() } else { format!("At {}", shortened(&form.server_name(&server.address))) }));
    if let Some(line) = &server.link {
        parent.spawn((widgets::notice(theme, line.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    if server.hosting {
        parent.spawn(widgets::dim(theme, "Give your opponent one of these. They connect, join your lobby and look for a game, as you do below."));
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
    }
    section(parent, theme, "Lobbies", |section| {
        if server.lobbies.is_empty() {
            section.spawn(widgets::dim(theme, "This server lists no lobby."));
        }
        for (index, lobby) in server.lobbies.iter().enumerate() {
            let here = server.lobby.as_ref().is_some_and(|current| current.id == lobby.id);
            section.spawn(widgets::row(12.0)).with_children(|row| {
                let (kind, label) = if here { (ButtonKind::Primary, "Here") } else { (ButtonKind::Secondary, "Join") };
                row.spawn(widgets::styled_button(theme, kind, label, px(110), Control::JoinLobby(index)));
                row.spawn((widgets::label(theme, lobby_line(lobby)), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }));
            });
        }
        section.spawn(widgets::row(12.0)).with_children(|row| {
            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Refresh", Control::Refresh));
            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Make a lobby…", Control::Open(Page::MakeLobby)));
            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Tournaments…", Control::Open(Page::Tournaments)));
            if !server.hosting {
                row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Your profile…", Control::Open(Page::Profile)));
            }
            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Servers…", Control::Open(Page::Servers)));
        });
    });
    section(parent, theme, "A closed lobby", |section| {
        section.spawn(widgets::row(12.0)).with_children(|row| {
            row.spawn(Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }).with_children(|slot| field_box(slot, theme, Field::Lobby, &form.lobby, "its code", false));
            row.spawn(Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }).with_children(|slot| field_box(slot, theme, Field::Password, &form.password, "password, if it asks for one", false));
            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Join", Control::JoinById));
        });
    });
    if let Some(lobby) = &server.lobby {
        section(parent, theme, format!("Find a game in {}", lobby_title(lobby)), |section| {
            section.spawn((widgets::dim(theme, server.rating_line(lobby)), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            section.spawn(widgets::row(10.0)).with_children(|row| {
                for chair in ChairChoice::ALL {
                    let kind = if chair == server.chair { ButtonKind::Primary } else { ButtonKind::Secondary };
                    row.spawn(widgets::styled_button(theme, kind, chair.label(), Val::Auto, Control::Chair(chair)));
                }
            });
            if server.chair != ChairChoice::Runner {
                deck_dropdown(section, theme, server, Side::Corp, lobby.format);
            }
            if server.chair != ChairChoice::Corp {
                deck_dropdown(section, theme, server, Side::Runner, lobby.format);
            }
            match server.seeking {
                Some(position) => {
                    section.spawn(widgets::dim(theme, format!("Looking for a game — {position} waiting in this lobby…")));
                    section.spawn(widgets::row(12.0)).with_children(|row| {
                        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Stop looking", Val::Auto, Control::CancelSeek));
                    });
                }
                None => {
                    section.spawn(widgets::row(12.0)).with_children(|row| {
                        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Find a game", px(220), Control::Seek));
                        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Leave lobby", Val::Auto, Control::LeaveLobby));
                    });
                }
            }
        });
    }
    buttons(parent, |row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Disconnect", Val::Auto, (Control::Disconnect, ButtonSound(Sfx::Back))));
    });
}

/// One lobby's line: its name, its format, who is in it and how many of
/// them are looking, and whether a game there counts.
fn lobby_line(lobby: &LobbyInfo) -> String {
    let mut line = format!("{} · {} here, {} looking", lobby_title(lobby), lobby.players, lobby.seeking);
    if lobby.password {
        line.push_str(" · password");
    }
    line.push_str(if lobby.rated { " · rated" } else { " · unrated" });
    line
}

/// The deck brought for `side`, from those legal in the lobby's format.
fn deck_dropdown(parent: &mut ChildSpawnerCommands, theme: &Theme, server: &ServerState, side: Side, format: NsgFormat) {
    let (decks, chosen) = match side {
        Side::Corp => (server.corp_decks(), server.corp_deck),
        Side::Runner => (server.runner_decks(), server.runner_deck),
    };
    deck_slot_of(parent, theme, decks, chosen, side, format, Purpose::Lobby(side));
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
                        section.spawn(widgets::row(12.0)).with_children(|row| match server.seeking {
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
    parent.spawn(widgets::heading(theme, "Make a lobby"));
    section(parent, theme, "Name", |section| field_box(section, theme, Field::LobbyName, &form.make.name, "what the others see", false));
    format_section(parent, theme, form.make.format, "The format games in this lobby are played in.");
    section(parent, theme, "Who can find it", |section| {
        section.spawn(widgets::row(10.0)).with_children(|row| {
            for (closed, label) in [(false, "Listed"), (true, "Closed")] {
                let kind = if closed == form.make.closed { ButtonKind::Primary } else { ButtonKind::Secondary };
                row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, Control::Closed(closed)));
            }
        });
        section.spawn(widgets::dim(theme, if form.make.closed { "Not listed: joined by the code the server gives it, which you pass on." } else { "Listed for anyone on the server to join." }));
    });
    section(parent, theme, "Do its games count", |section| {
        section.spawn(widgets::row(10.0)).with_children(|row| {
            for (casual, label) in [(false, "Rated"), (true, "Unrated")] {
                let kind = if casual == form.make.casual { ButtonKind::Primary } else { ButtonKind::Secondary };
                row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, Control::Casual(casual)));
            }
        });
        section.spawn(widgets::dim(theme, if form.make.casual { "Nothing played here is rated: for trying a deck, or a game nobody wants on their record." } else { "Rated as the server's own lobbies are — between two players with keys, on a server that keeps ratings." }));
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
