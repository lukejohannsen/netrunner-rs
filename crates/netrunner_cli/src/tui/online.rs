//! Play Online: the main menu's way to a game against a person.
//!
//! **Host** runs a human-vs-human `netrunner_server::Server` inside this
//! process and then joins it like any other client, over loopback — so the
//! host plays through a masked `ClientView` exactly as their opponent does,
//! and the process holding the real `GameState` is the server task, not
//! the player's screen. **Join** connects to a host (or a public
//! `netrunner_server --serve` daemon) by address, optionally into a room.
//! **Watch** lists a server's matches and spectates one.
//!
//! **A player brings their own deck** (`ClientMessage::Connect::deck`), and
//! its side is their seat; the server checks it against its format and
//! refuses an illegal one at the door. "Let the host deal" is still offered,
//! for either side or a preferred one — what `--mode remote` always did.
//!
//! **Nothing here waits on the network with the keyboard dead.** A
//! connection runs as a task (`remote::spawn_connect`) that the menu polls
//! every frame (`tick`), so the lobby wait draws a status line and Esc
//! abandons it — which closes the socket, so the daemon drops the waiter
//! instead of pairing an opponent with someone who has gone. The two
//! short blocking calls — binding the host's port and one `ListMatches`
//! round trip — are bounded and run under `block_in_place`, the pattern
//! `remote::Reconnector` set.
//!
//! Hot-seat play on one screen is deliberately absent: it would show each
//! player the other's hidden cards.

use std::future::Future;
use std::time::Duration;

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::DeckFile;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;
use netrunner_server::serve::{ServeBotKind, ServeOptions, Server};
use netrunner_server::{ClientMessage, MatchSummary};

use netrunner_client::deck_store;
use netrunner_client::hosting::{self, normalize_address, Mapping, PortMapper, Reach, Share};
use crate::remote::{self, ConnectEvent, Connecting, Joined};

pub use netrunner_client::hosting::DEFAULT_PORT;

/// How long a blocking call here may hold the screen.
const BLOCKING_TIMEOUT: Duration = Duration::from_secs(3);

/// What one key (or one tick) did, for the menu.
pub enum OnlineStep {
    Continue,
    Back,
    /// A seat or a spectator's place is ready: play it. `brought` is the
    /// id of the deck this player sent, for `play_remote`'s check.
    Play { joined: Box<Joined>, url: String, brought: Option<String> },
}

/// The deck field's choices: a deck of the player's, or the host's deal.
#[derive(Debug, Clone, PartialEq)]
enum DeckChoice {
    /// Let the server deal, preferring this side (or neither).
    Dealt(Option<Side>),
    Brought(Box<DeckFile>),
}

impl DeckChoice {
    fn label(&self) -> String {
        match self {
            DeckChoice::Dealt(None) => "Let the host deal me a deck — either side".to_string(),
            DeckChoice::Dealt(Some(side)) => format!("Let the host deal me a deck — as the {side:?}"),
            DeckChoice::Brought(deck) => format!("{:?} · {}", deck.side, deck.name),
        }
    }

    fn side(&self) -> Option<Side> {
        match self {
            DeckChoice::Dealt(side) => *side,
            DeckChoice::Brought(deck) => Some(deck.side),
        }
    }

    fn deck(&self) -> Option<DeckFile> {
        match self {
            DeckChoice::Dealt(_) => None,
            DeckChoice::Brought(deck) => Some((**deck).clone()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormKind {
    Host,
    Join,
}

/// The fields of the Host and Join forms, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Address,
    Room,
    Port,
    Reach,
    Deck,
    Go,
}

impl FormKind {
    fn fields(self) -> &'static [Field] {
        match self {
            FormKind::Join => &[Field::Address, Field::Room, Field::Deck, Field::Go],
            FormKind::Host => &[Field::Port, Field::Reach, Field::Deck, Field::Go],
        }
    }
}

#[derive(Debug, Clone)]
struct Form {
    kind: FormKind,
    cursor: usize,
    address: String,
    room: String,
    port: String,
    /// Who the hosted game is for: this machine, the network, or the
    /// internet by way of the router (`hosting::Reach`).
    reach: Reach,
    deck: usize,
    /// The text field being typed into.
    editing: Option<Field>,
}

impl Form {
    fn field(&self) -> Field {
        self.kind.fields()[self.cursor]
    }

    fn text_mut(&mut self, field: Field) -> Option<&mut String> {
        match field {
            Field::Address => Some(&mut self.address),
            Field::Room => Some(&mut self.room),
            Field::Port => Some(&mut self.port),
            _ => None,
        }
    }
}

/// The in-process server while this player hosts. Dropping it stops the
/// server — the match, if one is running, ends with it — and releases the
/// router's mapping, if one was asked for.
struct Hosting {
    task: tokio::task::JoinHandle<std::io::Result<()>>,
    port: u16,
    /// The addresses to give the opponent, nearest first.
    shares: Vec<Share>,
    /// The request to the router, for `Reach::Internet`; `mapped` is its
    /// last answer, polled on the menu's tick.
    mapping: Option<Box<dyn PortMapper>>,
    mapped: Mapping,
}

impl Hosting {
    fn poll(&mut self) {
        if let Some(mapping) = &mut self.mapping {
            self.mapped = mapping.poll();
        }
    }
}

impl Drop for Hosting {
    fn drop(&mut self) {
        self.task.abort();
    }
}

enum Mode {
    Home { cursor: usize },
    Form(Form),
    PickDeck { form: Form, cursor: usize },
    Waiting { connecting: Connecting, status: String, url: String, brought: Option<String>, back: Box<Mode> },
    Watch { address: String, editing: bool, matches: Vec<MatchSummary>, cursor: usize },
}

const HOME: [(&str, &str); 3] = [
    ("Host a game", "Run a game on this machine and give your opponent the address"),
    ("Join a game", "Connect to a host or a public server by address"),
    ("Watch a game", "List a server's matches and spectate one"),
];

pub struct OnlineScreen {
    mode: Mode,
    decks: Vec<DeckChoice>,
    player: String,
    format: NsgFormat,
    /// The address Join and Watch start from: `--server`, as the flag path
    /// uses it.
    default_address: String,
    hosting: Option<Hosting>,
    notice: Option<String>,
}

impl OnlineScreen {
    /// Offers every deck legal in `format` — the server has the last word
    /// on its own format, but a deck illegal here is not worth offering.
    pub fn open(
        decks_dir: &std::path::Path,
        registry: &CardRegistry,
        format: NsgFormat,
        player: String,
        default_address: String,
    ) -> Result<Self, String> {
        let mut decks = vec![DeckChoice::Dealt(None), DeckChoice::Dealt(Some(Side::Corp)), DeckChoice::Dealt(Some(Side::Runner))];
        let mut owned: Vec<DeckFile> = deck_store::list(decks_dir)?
            .into_iter()
            .map(|stored| stored.deck)
            .filter(|deck| deck.validate(registry, format).is_ok())
            .collect();
        owned.sort_by_key(|deck| (deck.side == Side::Runner, deck.name.to_lowercase()));
        decks.extend(owned.into_iter().map(|deck| DeckChoice::Brought(Box::new(deck))));
        Ok(OnlineScreen { mode: Mode::Home { cursor: 0 }, decks, player, format, default_address, hosting: None, notice: None })
    }

    fn form(&self, kind: FormKind) -> Form {
        Form {
            kind,
            cursor: 0,
            address: self.default_address.clone(),
            room: String::new(),
            port: DEFAULT_PORT.to_string(),
            reach: Reach::Network,
            deck: 0,
            editing: None,
        }
    }

    /// Back from a game: the server this player hosted stops with the
    /// match, and the screen is Home again.
    pub fn returned(&mut self) {
        self.hosting = None;
        self.mode = Mode::Home { cursor: 0 };
    }

    /// Polls a connection in progress. Called every frame, key or no key.
    pub fn tick(&mut self) -> OnlineStep {
        let Mode::Waiting { connecting, status, .. } = &mut self.mode else { return OnlineStep::Continue };
        // The router answers seconds after the server is up, so the host's
        // line follows it rather than being written once.
        if let Some(hosting) = &mut self.hosting {
            hosting.poll();
            *status = hosting_status(hosting);
        }
        let mut outcome = None;
        while let Ok(event) = connecting.events.try_recv() {
            match event {
                ConnectEvent::Queued(position) => {
                    // The host's line keeps the address: waiting is exactly
                    // when they need it to hand to their opponent.
                    *status = match &self.hosting {
                        Some(hosting) => hosting_status(hosting),
                        None => format!("In the lobby, waiting for an opponent ({position} waiting)…"),
                    };
                }
                other => {
                    outcome = Some(other);
                    break;
                }
            }
        }
        let Some(outcome) = outcome else { return OnlineStep::Continue };
        let Mode::Waiting { url, brought, back, .. } = std::mem::replace(&mut self.mode, Mode::Home { cursor: 0 }) else {
            unreachable!("checked above")
        };
        match outcome {
            ConnectEvent::Joined(joined) => OnlineStep::Play { joined, url, brought },
            ConnectEvent::Failed(error) => {
                self.hosting = None;
                self.notice = Some(error.to_string());
                self.mode = *back;
                OnlineStep::Continue
            }
            ConnectEvent::Queued(_) => unreachable!("handled in the loop"),
        }
    }

    pub fn key(&mut self, key: KeyCode) -> OnlineStep {
        self.notice = None;
        match std::mem::replace(&mut self.mode, Mode::Home { cursor: 0 }) {
            Mode::Home { mut cursor } => match key {
                KeyCode::Up | KeyCode::Char('k') => self.mode = Mode::Home { cursor: (cursor + HOME.len() - 1) % HOME.len() },
                KeyCode::Down | KeyCode::Char('j') => self.mode = Mode::Home { cursor: (cursor + 1) % HOME.len() },
                KeyCode::Enter => {
                    self.mode = match cursor {
                        0 => Mode::Form(self.form(FormKind::Host)),
                        1 => Mode::Form(self.form(FormKind::Join)),
                        _ => Mode::Watch { address: self.default_address.clone(), editing: true, matches: Vec::new(), cursor: 0 },
                    };
                }
                KeyCode::Esc | KeyCode::Char('q') => return OnlineStep::Back,
                _ => {
                    cursor %= HOME.len();
                    self.mode = Mode::Home { cursor };
                }
            },
            Mode::Form(form) => return self.form_key(form, key),
            Mode::PickDeck { mut form, mut cursor } => match key {
                KeyCode::Up | KeyCode::Char('k') => {
                    cursor = (cursor + self.decks.len() - 1) % self.decks.len();
                    self.mode = Mode::PickDeck { form, cursor };
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    cursor = (cursor + 1) % self.decks.len();
                    self.mode = Mode::PickDeck { form, cursor };
                }
                KeyCode::Enter => {
                    form.deck = cursor;
                    self.mode = Mode::Form(form);
                }
                _ => self.mode = Mode::Form(form),
            },
            Mode::Waiting { connecting, status, url, brought, back } => {
                if matches!(key, KeyCode::Esc | KeyCode::Char('q')) {
                    connecting.cancel();
                    self.hosting = None;
                    self.mode = *back;
                } else {
                    self.mode = Mode::Waiting { connecting, status, url, brought, back };
                }
            }
            Mode::Watch { mut address, mut editing, matches, mut cursor } => {
                if editing {
                    match key {
                        KeyCode::Char(c) if !c.is_control() => address.push(c),
                        KeyCode::Backspace => {
                            address.pop();
                        }
                        KeyCode::Esc => return OnlineStep::Back,
                        KeyCode::Enter => return self.fetch_matches(address),
                        _ => {}
                    }
                    self.mode = Mode::Watch { address, editing, matches, cursor };
                    return OnlineStep::Continue;
                }
                match key {
                    KeyCode::Up | KeyCode::Char('k') if !matches.is_empty() => cursor = (cursor + matches.len() - 1) % matches.len(),
                    KeyCode::Down | KeyCode::Char('j') if !matches.is_empty() => cursor = (cursor + 1) % matches.len(),
                    KeyCode::Char('r') => return self.fetch_matches(address),
                    KeyCode::Char('a') => editing = true,
                    KeyCode::Enter if !matches.is_empty() => {
                        let url = normalize_address(&address);
                        let hello = ClientMessage::Spectate { match_id: matches[cursor].match_id };
                        let back = Box::new(Mode::Watch { address, editing, matches, cursor });
                        self.mode = Mode::Waiting {
                            connecting: remote::spawn_connect(url.clone(), hello),
                            status: format!("Connecting to {url}…"),
                            url,
                            brought: None,
                            back,
                        };
                        return OnlineStep::Continue;
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.mode = Mode::Home { cursor: 2 };
                        return OnlineStep::Continue;
                    }
                    _ => {}
                }
                self.mode = Mode::Watch { address, editing, matches, cursor };
            }
        }
        OnlineStep::Continue
    }

    fn form_key(&mut self, mut form: Form, key: KeyCode) -> OnlineStep {
        if let Some(field) = form.editing {
            match key {
                KeyCode::Char(c) if !c.is_control() && (field != Field::Port || c.is_ascii_digit()) => {
                    form.text_mut(field).expect("an editable field").push(c);
                }
                KeyCode::Backspace => {
                    form.text_mut(field).expect("an editable field").pop();
                }
                KeyCode::Enter | KeyCode::Esc | KeyCode::Tab | KeyCode::Down => {
                    form.editing = None;
                    if matches!(key, KeyCode::Tab | KeyCode::Down) {
                        form.cursor = (form.cursor + 1) % form.kind.fields().len();
                    }
                }
                _ => {}
            }
            self.mode = Mode::Form(form);
            return OnlineStep::Continue;
        }
        let len = form.kind.fields().len();
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Home { cursor: if form.kind == FormKind::Host { 0 } else { 1 } };
                return OnlineStep::Continue;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => form.cursor = (form.cursor + len - 1) % len,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => form.cursor = (form.cursor + 1) % len,
            KeyCode::Left if form.field() == Field::Reach => form.reach = form.reach.step(true),
            KeyCode::Right | KeyCode::Char(' ') if form.field() == Field::Reach => form.reach = form.reach.step(false),
            KeyCode::Enter => match form.field() {
                Field::Address | Field::Room | Field::Port => form.editing = Some(form.field()),
                Field::Reach => form.reach = form.reach.step(false),
                Field::Deck => {
                    let cursor = form.deck;
                    self.mode = Mode::PickDeck { form, cursor };
                    return OnlineStep::Continue;
                }
                Field::Go => return self.go(form),
            },
            _ => {}
        }
        self.mode = Mode::Form(form);
        OnlineStep::Continue
    }

    /// Submits a form: joins the address, or starts the server and joins it.
    fn go(&mut self, form: Form) -> OnlineStep {
        let choice = self.decks[form.deck].clone();
        let brought = choice.deck().map(|deck| deck.id.clone());
        let (url, status) = match form.kind {
            FormKind::Join => {
                let url = normalize_address(&form.address);
                (url.clone(), format!("Connecting to {url}…"))
            }
            FormKind::Host => {
                let Ok(port) = form.port.parse::<u16>() else {
                    self.notice = Some(format!("{:?} is not a port number", form.port));
                    self.mode = Mode::Form(form);
                    return OnlineStep::Continue;
                };
                match start_hosting(port, form.reach, self.format) {
                    Ok((hosting, local_url)) => {
                        let status = hosting_status(&hosting);
                        self.hosting = Some(hosting);
                        (local_url, status)
                    }
                    Err(error) => {
                        self.notice = Some(format!("Could not host on port {port}: {error}"));
                        self.mode = Mode::Form(form);
                        return OnlineStep::Continue;
                    }
                }
            }
        };
        let room = (form.kind == FormKind::Join && !form.room.trim().is_empty()).then(|| form.room.trim().to_string());
        let hello = remote::connect_message(&self.player, choice.side(), room, choice.deck());
        self.mode = Mode::Waiting {
            connecting: remote::spawn_connect(url.clone(), hello),
            status,
            url,
            brought,
            back: Box::new(Mode::Form(form)),
        };
        OnlineStep::Continue
    }

    fn fetch_matches(&mut self, address: String) -> OnlineStep {
        let url = normalize_address(&address);
        let matches = match block_on_bounded(remote::list_matches(&url)) {
            Some(Ok((matches, _, _))) => {
                if matches.is_empty() {
                    self.notice = Some(format!("{url} is hosting no matches right now (r to refresh)"));
                }
                matches
            }
            Some(Err(error)) => {
                self.notice = Some(error.to_string());
                self.mode = Mode::Watch { address, editing: true, matches: Vec::new(), cursor: 0 };
                return OnlineStep::Continue;
            }
            None => {
                self.notice = Some(format!("{url} did not answer within {}s", BLOCKING_TIMEOUT.as_secs()));
                self.mode = Mode::Watch { address, editing: true, matches: Vec::new(), cursor: 0 };
                return OnlineStep::Continue;
            }
        };
        self.mode = Mode::Watch { address, editing: false, matches, cursor: 0 };
        OnlineStep::Continue
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let [body, footer] =
            Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        let help = match &self.mode {
            Mode::Home { cursor } => {
                let items: Vec<ListItem> = HOME
                    .iter()
                    .map(|(label, blurb)| {
                        ListItem::new(vec![
                            Line::from(Span::styled(*label, Style::default().add_modifier(Modifier::BOLD))),
                            Line::from(Span::styled(format!("  {blurb}"), Style::default().fg(Color::Gray))),
                        ])
                    })
                    .collect();
                draw_list(frame, body, &format!("Play Online — as {}", self.player), items, Some(*cursor));
                "Up/Down choose · Enter selects · Esc back"
            }
            Mode::Form(form) => {
                self.draw_form(frame, body, form);
                if form.editing.is_some() { "Type · Enter or Tab finishes" } else { "Up/Down choose · Enter edits or picks · Esc back" }
            }
            Mode::PickDeck { cursor, .. } => {
                let items = self.decks.iter().map(|choice| ListItem::new(choice.label())).collect();
                draw_list(frame, body, "Your deck — its side is your seat", items, Some(*cursor));
                "Up/Down choose · Enter picks · Esc keeps the old choice"
            }
            Mode::Waiting { status, .. } => {
                let mut text: Vec<Line> = status.lines().map(|line| Line::from(line.to_string())).collect();
                text.extend([Line::from(""), Line::from("Esc stops waiting.")]);
                frame.render_widget(
                    Paragraph::new(text).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title("Play Online")),
                    body,
                );
                "Esc stops waiting"
            }
            Mode::Watch { address, editing, matches, cursor } => {
                let [top, list] =
                    Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).areas(body);
                let address = if *editing { format!("{address}▏") } else { address.clone() };
                frame.render_widget(Paragraph::new(address).block(Block::default().borders(Borders::ALL).title("Server")), top);
                let items = matches
                    .iter()
                    .map(|m| {
                        let decks = if m.corp_deck.is_empty() { String::new() } else { format!("  [{} vs {}]", m.corp_deck, m.runner_deck) };
                        ListItem::new(format!("{} (Corp) vs {} (Runner){decks} — {}s", m.corp, m.runner, m.started_secs_ago))
                    })
                    .collect();
                draw_list(frame, list, "Matches", items, (!matches.is_empty()).then_some(*cursor));
                if *editing { "Type the address · Enter lists its matches · Esc back" } else { "Enter watches · r refreshes · a edits the address · Esc back" }
            }
        };
        let line = match &self.notice {
            Some(notice) => Line::from(Span::styled(notice.clone(), Style::default().fg(Color::Red))),
            None => Line::from(Span::styled(help, Style::default().fg(Color::DarkGray))),
        };
        frame.render_widget(Paragraph::new(line), footer);
    }

    fn draw_form(&self, frame: &mut Frame, area: Rect, form: &Form) {
        let text = |field: Field, value: &str| if form.editing == Some(field) { format!("{value}▏") } else { value.to_string() };
        let rows: Vec<String> = form
            .kind
            .fields()
            .iter()
            .map(|field| match field {
                Field::Address => format!("Server address   {}", text(Field::Address, &form.address)),
                Field::Room => format!(
                    "Room             {}",
                    if form.room.is_empty() && form.editing != Some(Field::Room) { "(none — the public queue)".to_string() } else { text(Field::Room, &form.room) }
                ),
                Field::Port => format!("Port             {}", text(Field::Port, &form.port)),
                Field::Reach => format!("Who can join     ‹ {} ›", form.reach.label()),
                Field::Deck => format!("Your deck        {}", self.decks[form.deck].label()),
                Field::Go => match form.kind {
                    FormKind::Join => "[ Connect ]".to_string(),
                    FormKind::Host => "[ Start hosting ]".to_string(),
                },
            })
            .collect();
        let items = rows.into_iter().map(ListItem::new).collect();
        let title = match form.kind {
            FormKind::Join => "Join a game",
            FormKind::Host => "Host a game — a human-vs-human server on this machine, which you join too",
        };
        draw_list(frame, area, title, items, Some(form.cursor));
    }
}

fn draw_list(frame: &mut Frame, area: Rect, title: &str, items: Vec<ListItem>, cursor: Option<usize>) {
    let mut state = ListState::default();
    state.select(cursor);
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title.to_string()))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

/// The host's waiting line: every address to give out, each with who can
/// use it, and — hosting for the internet — where the router's answer
/// stands. One address per line, because the line is what they read out.
fn hosting_status(hosting: &Hosting) -> String {
    let mut lines = vec!["Hosting. Give your opponent an address — waiting for them to join…".to_string()];
    if hosting.mapping.is_some() {
        lines.push(match &hosting.mapped {
            Mapping::Asking => format!("  asking your router to open port {}…", hosting.port),
            Mapping::Open(addr) => format!("  {} — from anywhere (your router opened the port)", hosting::ws_url((*addr).into())),
            Mapping::Failed(failure) => format!("  {}", failure.advice(hosting.port, hosting::lan_ipv4())),
        });
    }
    lines.extend(hosting.shares.iter().map(|share| format!("  {} — {}", share.url, share.who)));
    lines.join("\n")
}

/// Runs a future to completion from the menu's synchronous loop, giving up
/// after `BLOCKING_TIMEOUT`. `block_in_place` parks this worker so the
/// runtime's others carry on — the binary's multi-threaded runtime, which
/// is what `Reconnector::try_resume` already relies on.
fn block_on_bounded<F: Future>(future: F) -> Option<F::Output> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async { tokio::time::timeout(BLOCKING_TIMEOUT, future).await.ok() })
    })
}

/// Binds a human-vs-human server on `port` and starts it; returns it and
/// the loopback URL the host joins by. Port 0 takes any free port (tests).
/// `Reach::Internet` also starts asking the router to forward the port,
/// which the tick polls.
///
/// Unrated, by design: a rating is a claim by a server somebody else
/// runs, and this process holds the seed and the unmasked state of the
/// game its own host is playing in (`docs/identity-and-rating.md`).
fn start_hosting(port: u16, reach: Reach, format: NsgFormat) -> Result<(Hosting, String), String> {
    let options = ServeOptions { bot_runner: ServeBotKind::None, format, ..ServeOptions::default() };
    let listener = hosting::bind_listener(reach, port).map_err(|error| error.to_string())?;
    let server = Server::from_listener(listener, options).map_err(|error| error.to_string())?;
    let port = server.local_addr().map_err(|error| error.to_string())?.port();
    let task = tokio::spawn(server.run());
    let hosting = Hosting {
        task,
        port,
        shares: hosting::share_addresses(reach, port),
        mapping: hosting::map_port(reach, port),
        mapped: Mapping::Asking,
    };
    Ok((hosting, format!("ws://127.0.0.1:{port}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use netrunner_core::rules::Viewer;

    fn screen(name: &str) -> (OnlineScreen, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("netrunner_online_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let screen = OnlineScreen::open(&dir, &netrunner_client::decks::sample_deck_registry(), NsgFormat::Startup, name.to_string(), "ws://127.0.0.1:8080".into())
            .unwrap();
        (screen, dir)
    }

    fn press(screen: &mut OnlineScreen, keys: &[KeyCode]) {
        for key in keys {
            let _ = screen.key(*key);
        }
    }

    /// Picks the deck with this id in an open deck picker.
    fn pick_deck(screen: &mut OnlineScreen, id: &str) {
        let index = screen.decks.iter().position(|choice| matches!(choice, DeckChoice::Brought(deck) if deck.id == id)).unwrap();
        let Mode::PickDeck { cursor, .. } = &mut screen.mode else { panic!("the picker is open") };
        *cursor = index;
        screen.key(KeyCode::Enter);
    }

    /// Ticks until the connection resolves.
    async fn until_play(screen: &mut OnlineScreen) -> (Box<Joined>, String, Option<String>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let OnlineStep::Play { joined, url, brought } = screen.tick() {
                return (joined, url, brought);
            }
            assert!(Instant::now() < deadline, "no seat within 10s; notice: {:?}", screen.notice);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[test]
    fn the_deck_choices_are_the_hosts_deal_then_every_legal_deck() {
        let (screen, _) = screen("choices");
        assert_eq!(screen.decks[0], DeckChoice::Dealt(None));
        assert!(screen.decks.iter().any(|choice| matches!(choice, DeckChoice::Brought(deck) if deck.id == "brick_stack")));
        assert_eq!(DeckChoice::Dealt(Some(Side::Runner)).side(), Some(Side::Runner));
        let brick = screen.decks.iter().find(|choice| choice.deck().is_some_and(|deck| deck.id == "brick_stack")).unwrap();
        assert_eq!(brick.side(), Some(Side::Corp), "a deck's side is the seat");
    }

    #[test]
    fn the_port_field_takes_digits_only_and_a_bad_port_is_refused() {
        let (mut screen, _) = screen("port");
        press(&mut screen, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace]);
        press(&mut screen, &[KeyCode::Char('9'), KeyCode::Char('x'), KeyCode::Char('9'), KeyCode::Enter]);
        let Mode::Form(form) = &screen.mode else { panic!() };
        assert_eq!(form.port, "99", "letters are not typed into a port");
        let Mode::Form(form) = std::mem::replace(&mut screen.mode, Mode::Home { cursor: 0 }) else { panic!() };
        let mut form = form;
        form.port = "99999".into();
        screen.go(form);
        assert!(screen.notice.as_deref().is_some_and(|notice| notice.contains("not a port")), "{:?}", screen.notice);
        assert!(screen.hosting.is_none());
    }

    #[test]
    fn escape_walks_back_out() {
        let (mut screen, _) = screen("escape");
        press(&mut screen, &[KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(&screen.mode, Mode::Form(form) if form.kind == FormKind::Join));
        press(&mut screen, &[KeyCode::Esc]);
        assert!(matches!(screen.mode, Mode::Home { cursor: 1 }));
        assert!(matches!(screen.key(KeyCode::Esc), OnlineStep::Back));
    }

    /// The whole flow over real sockets: one screen hosts with a Runner deck
    /// it brought, another joins by the host's address with a Corp deck,
    /// and both come out with seats — on the sides their decks name, with
    /// the decks they brought. Then the watcher lists that match.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_host_and_a_joiner_are_seated_with_the_decks_they_brought() {
        let (mut host, _) = screen("host");
        // Host → port field: clear it to 0 (any free port), this machine only.
        press(&mut host, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace]);
        press(&mut host, &[KeyCode::Char('0'), KeyCode::Enter, KeyCode::Down, KeyCode::Left, KeyCode::Down, KeyCode::Enter]);
        pick_deck(&mut host, "stolen_goods");
        press(&mut host, &[KeyCode::Down, KeyCode::Enter]);
        let Mode::Waiting { url, .. } = &host.mode else { panic!("hosting: {:?}", host.notice) };
        let address = url.clone();
        assert!(host.hosting.as_ref().unwrap().shares[0].url.starts_with("ws://127.0.0.1:"), "this machine only");

        let (mut joiner, _) = screen("joiner");
        press(&mut joiner, &[KeyCode::Down, KeyCode::Enter, KeyCode::Enter]);
        press(&mut joiner, &vec![KeyCode::Backspace; 40]);
        for c in address.trim_start_matches("ws://").chars() {
            joiner.key(KeyCode::Char(c));
        }
        press(&mut joiner, &[KeyCode::Enter, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        pick_deck(&mut joiner, "brick_stack");
        press(&mut joiner, &[KeyCode::Down, KeyCode::Enter]);

        host.tick();
        let Mode::Waiting { status, .. } = &host.mode else { panic!() };
        assert!(status.contains(&address), "the host's waiting line keeps the address to share: {status}");

        let (joined, _, brought) = until_play(&mut joiner).await;
        assert_eq!(joined.viewer, Viewer::Player(Side::Corp));
        assert_eq!(brought.as_deref(), Some("brick_stack"));
        assert_eq!(joined.decks, ("brick_stack".to_string(), "stolen_goods".to_string()));
        let (hosted, host_url, _) = until_play(&mut host).await;
        assert_eq!(hosted.viewer, Viewer::Player(Side::Runner));
        assert_eq!(host_url, address);

        let (mut watcher, _) = screen("watcher");
        press(&mut watcher, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        press(&mut watcher, &vec![KeyCode::Backspace; 40]);
        for c in address.chars() {
            watcher.key(KeyCode::Char(c));
        }
        watcher.key(KeyCode::Enter);
        let Mode::Watch { matches, .. } = &watcher.mode else { panic!("{:?}", watcher.notice) };
        assert_eq!(matches.len(), 1);
        assert_eq!((matches[0].corp.as_str(), matches[0].runner.as_str()), ("joiner", "host"), "players go by their own names");
        watcher.key(KeyCode::Enter);
        let (watching, _, _) = until_play(&mut watcher).await;
        assert_eq!(watching.viewer, Viewer::Spectator);

        host.returned();
        assert!(host.hosting.is_none(), "the hosted server stops with the game");
    }

    /// A router request whose answer the test sets, and which says when it
    /// has been let go.
    struct FakeRouter {
        answer: std::sync::Arc<std::sync::Mutex<Mapping>>,
        released: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl PortMapper for FakeRouter {
        fn poll(&mut self) -> Mapping {
            self.answer.lock().unwrap().clone()
        }
    }

    impl Drop for FakeRouter {
        fn drop(&mut self) {
            self.released.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Hosting for the internet, the host's line follows the router: asking,
    /// then the public address to give out — or, behind the provider's NAT,
    /// why no address will do and what to do instead. Leaving lets the
    /// mapping go.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_hosts_line_follows_the_routers_answer() {
        use hosting::MappingFailure;
        use std::net::{Ipv4Addr, SocketAddrV4};

        let (mut host, _) = screen("router");
        press(&mut host, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace]);
        press(&mut host, &[KeyCode::Char('0'), KeyCode::Enter, KeyCode::Down, KeyCode::Left, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        assert!(matches!(host.mode, Mode::Waiting { .. }), "{:?}", host.notice);
        let answer = std::sync::Arc::new(std::sync::Mutex::new(Mapping::Asking));
        let released = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        host.hosting.as_mut().unwrap().mapping = Some(Box::new(FakeRouter { answer: answer.clone(), released: released.clone() }));
        let status = |host: &mut OnlineScreen| {
            host.tick();
            let Mode::Waiting { status, .. } = &host.mode else { panic!("{:?}", host.notice) };
            status.clone()
        };

        assert!(status(&mut host).contains("asking your router to open port"));
        *answer.lock().unwrap() = Mapping::Open(SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 40123));
        assert!(status(&mut host).contains("ws://203.0.113.7:40123 — from anywhere"), "the router's port, which may not be ours");
        *answer.lock().unwrap() = Mapping::Failed(MappingFailure::SharedAddress(Ipv4Addr::new(100, 70, 1, 1)));
        let shared = status(&mut host);
        assert!(shared.contains("100.70.1.1") && shared.contains("Tailscale"), "{shared}");
        assert!(shared.contains("ws://127.0.0.1:"), "the addresses that do work are still listed: {shared}");

        host.key(KeyCode::Esc);
        host.returned();
        assert!(released.load(std::sync::atomic::Ordering::SeqCst), "leaving releases the mapping");
    }

    /// A player who stops waiting leaves the lobby: the next to arrive
    /// waits too, instead of being paired with someone who has gone.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn abandoning_the_wait_leaves_the_lobby() {
        let (mut host, _) = screen("abandon");
        press(&mut host, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace]);
        press(&mut host, &[KeyCode::Char('0'), KeyCode::Enter, KeyCode::Down, KeyCode::Left, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
        let Mode::Waiting { url, .. } = &host.mode else { panic!("{:?}", host.notice) };
        let url = url.clone();
        // Keep the server alive past the host's own departure, to see the
        // lobby from outside.
        let hosting = host.hosting.take();
        let deadline = Instant::now() + Duration::from_secs(10);
        while remote::list_matches(&url).await.unwrap().1 != 1 {
            assert!(Instant::now() < deadline, "the host never reached the lobby");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        host.key(KeyCode::Esc);
        while remote::list_matches(&url).await.unwrap().1 != 0 {
            assert!(Instant::now() < deadline, "the abandoned waiter is still in the lobby");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        drop(hosting);
    }
}
