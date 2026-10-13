//! The deck a person brings to a game online, shared by both clients'
//! Play Online screens.
//!
//! **A player brings their own deck** (`Chair`, on the seek), and its
//! side is their seat; the host checks it against its format and
//! refuses an illegal one at the door. **There is no "let the host deal"**
//! (Phase 4 §7 stage 3, 26 September 2026): the game is about decks built
//! to surprise, and a server deals nobody a deck unless its operator says
//! so (`ServeOptions::deals`). Everyone has the built-in decks to bring,
//! which are listed first.

use std::path::Path;

use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::DeckFile;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;
use netrunner_protocol::{Level, LobbyInfo, QueueMode};

use crate::deck_builder::{self, CardBook, Standing};
use crate::deck_picker::Row;
use crate::deck_store;
use crate::settings::{format_label, FORMATS};

/// How a deck is offered: its side, which is the seat it takes, and its
/// name.
pub fn label(deck: &DeckFile) -> String {
    format!("{:?} · {}", deck.side, deck.name)
}

/// Every deck legal in `format` — the built-in ones and the player's own —
/// Corp decks first and each side by name. The host has the last word on
/// its own format, but a deck illegal here is not worth offering. A saved
/// deck that cannot be read is left out rather than hiding the rest
/// (`deck_store::list_lenient`), so the built-in decks are always offered.
pub fn deck_choices(decks_dir: &Path, registry: &CardRegistry, format: NsgFormat) -> Vec<DeckFile> {
    let (stored, _problems) = deck_store::list_lenient(decks_dir);
    let mut decks: Vec<DeckFile> = stored.into_iter().map(|stored| stored.deck).filter(|deck| deck.validate(registry, format).is_ok()).collect();
    decks.sort_by_key(|deck| (deck.side == Side::Runner, deck.name.to_lowercase()));
    decks
}

/// An active deck as the Server Dashboard draws it and a queue checks it
/// (Phase 7 §14): the file, its identity's name and faction, and its
/// standing in every format, so a format pill is greyed with the reason
/// before anything is sent.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveDeck {
    pub deck: DeckFile,
    pub identity: String,
    pub faction: Option<netrunner_core::card::Faction>,
    /// The standing per format, in `FORMATS` order.
    pub standings: Vec<(NsgFormat, Standing)>,
}

impl ActiveDeck {
    /// Read off a picker row, which already knows the identity.
    pub fn of(row: &Row, book: CardBook) -> ActiveDeck {
        let standings = FORMATS.into_iter().map(|format| (format, deck_builder::standing(&row.deck, book, format))).collect();
        ActiveDeck { deck: row.deck.clone(), identity: row.identity.clone(), faction: row.faction, standings }
    }

    /// A deck with nothing behind it but its words — for a test of a
    /// screen with no registry. `illegal_in` names the formats it is not
    /// legal in, each with the same reason.
    #[doc(hidden)]
    pub fn fixture(deck: DeckFile, identity: &str, illegal_in: &[(NsgFormat, &str)]) -> ActiveDeck {
        let standings = FORMATS.into_iter().map(|format| (format, illegal_in.iter().find(|(other, _)| *other == format).map_or(Standing::Legal, |(_, reason)| Standing::Illegal(reason.to_string())))).collect();
        ActiveDeck { deck, identity: identity.to_string(), faction: None, standings }
    }

    pub fn legal_in(&self, format: NsgFormat) -> bool {
        self.standings.iter().any(|(other, standing)| *other == format && standing.is_legal())
    }

    /// Why it cannot be played in `format`, when it cannot.
    pub fn problem_in(&self, format: NsgFormat) -> Option<&str> {
        self.standings.iter().find(|(other, _)| *other == format).and_then(|(_, standing)| standing.reason())
    }

    /// The formats it is legal in, for the badges.
    pub fn legal_formats(&self) -> Vec<NsgFormat> {
        self.standings.iter().filter(|(_, standing)| standing.is_legal()).map(|(format, _)| *format).collect()
    }
}

/// Why a queue in `format` cannot be joined with these decks, or `None`
/// when both are legal there: the side and its deck's reason, or that a
/// side has no deck at all. The dashboard's greyed pill says this.
pub fn queue_problem(format: NsgFormat, corp: Option<&ActiveDeck>, runner: Option<&ActiveDeck>) -> Option<String> {
    for (side, deck) in [(Side::Corp, corp), (Side::Runner, runner)] {
        match deck {
            None => return Some(format!("No {side:?} deck is chosen")),
            Some(deck) => {
                if let Some(problem) = deck.problem_in(format) {
                    return Some(format!("{side:?} deck: {problem}"));
                }
            }
        }
    }
    None
}

/// The queue state's line: "Format: Standard | Role preference: Any".
pub fn searching_line(format: NsgFormat, prefer: Option<Side>) -> String {
    format!("Format: {} | Role preference: {}", format_label(format), preference_label(prefer))
}

/// A lobby as a line names it (Phase 4 §9): the server's queues by their
/// mode and format — "Ranked · Startup", "Casual · Standard" — and a
/// friend queue by the name its maker gave it and its format.
pub fn lobby_title(lobby: &LobbyInfo) -> String {
    let format = crate::settings::format_label(lobby.format);
    match lobby.mode {
        QueueMode::Ranked | QueueMode::Casual => format!("{} · {format}", lobby.mode.name()),
        QueueMode::Friends => format!("{} · {format}", lobby.name),
    }
}

/// Whom a seek is against, in a pill's words: a person, or a bot at a
/// rung of the ladder, named as the new-game form names it.
pub fn opponent_label(bot: Option<Level>) -> String {
    match bot {
        None => "A person".to_string(),
        Some(level) => format!("{} bot", rung_label(level)),
    }
}

/// A rung of the ladder by its name, capitalised: a pill beside "A person"
/// in a row that says it is choosing an opponent.
pub fn rung_label(level: Level) -> String {
    let mut letters = level.name().chars();
    letters.next().map_or(String::new(), |first| first.to_uppercase().chain(letters).collect())
}

/// A side preferred by a seek that brought both decks, in a pill's words.
pub fn preference_label(prefer: Option<Side>) -> &'static str {
    match prefer {
        None => "Any / Balanced",
        Some(Side::Corp) => "Prefer Corp",
        Some(Side::Runner) => "Prefer Runner",
    }
}

/// What the Join page's one field holds (Phase 7 §15): a ticket, an
/// address, nothing yet, or something that is neither — read off the
/// text as it is typed, so the line under the field says which way in
/// the connection will take before Connect is pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Empty,
    /// A host's ticket: an encrypted tunnel to their key, from anywhere.
    Ticket,
    /// A direct connection to this host and port, as
    /// `hosting::normalize_address` reads it.
    Address { host: String, port: u16 },
    Neither,
}

impl Target {
    pub fn detect(input: &str) -> Target {
        let input = input.trim();
        if input.is_empty() {
            return Target::Empty;
        }
        if crate::peer::Ticket::parse(input).is_some() {
            return Target::Ticket;
        }
        let url = crate::hosting::normalize_address(input);
        // `ws://host:port`, the host bracketed for IPv6.
        let rest = url.trim_start_matches("wss://").trim_start_matches("ws://");
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) if !host.is_empty() => (host, port),
            _ => return Target::Neither,
        };
        match port.parse::<u16>() {
            Ok(port) if host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '[' | ']' | ':' | '_')) => Target::Address { host: host.to_string(), port },
            _ => Target::Neither,
        }
    }

    /// The line under the field.
    pub fn line(&self) -> String {
        match self {
            Target::Empty => "Nothing yet. Paste the ticket, or type the address, your opponent gave you.".to_string(),
            Target::Ticket => "Detected: a ticket — an encrypted tunnel to their key, which works from anywhere.".to_string(),
            Target::Address { host, port } => format!("Detected: an address — a direct connection to {host}, port {port}."),
            Target::Neither => "That is not an address or a ticket.".to_string(),
        }
    }

    pub fn is_usable(&self) -> bool {
        matches!(self, Target::Ticket | Target::Address { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_join_field_says_what_it_holds() {
        assert_eq!(Target::detect("  "), Target::Empty);
        assert_eq!(Target::detect("192.168.1.50"), Target::Address { host: "192.168.1.50".into(), port: 8080 });
        assert_eq!(Target::detect("ws://[::1]:9000"), Target::Address { host: "[::1]".into(), port: 9000 });
        assert_eq!(Target::detect("not an address at all!"), Target::Neither);
        assert!(Target::detect("192.168.1.50").line().contains("port 8080"));
    }

    /// Every choice is a deck — there is no deal to ask for — Corp first,
    /// and the built-in decks are there with nothing saved.
    #[test]
    fn the_choices_are_every_legal_deck_corp_first() {
        let dir = std::env::temp_dir().join(format!("netrunner_online_choices_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let decks = deck_choices(&dir, &crate::decks::sample_deck_registry(), NsgFormat::Startup);
        let brick = decks.iter().find(|deck| deck.id == "brick_stack").expect("the built-in decks are listed");
        assert_eq!(brick.side, Side::Corp, "a deck's side is the seat");
        let first_runner = decks.iter().position(|deck| deck.side == Side::Runner).expect("Runner decks too");
        assert!(decks[..first_runner].iter().all(|deck| deck.side == Side::Corp) && decks[first_runner..].iter().all(|deck| deck.side == Side::Runner));
        assert_eq!(label(brick), format!("Corp · {}", brick.name));
    }

    #[test]
    fn a_lobby_is_named_by_its_mode_or_its_maker() {
        let lobby = |id: &str, mode| LobbyInfo { id: id.into(), name: "Friday".into(), format: NsgFormat::Startup, mode, password: false, rated: false, bots: Vec::new(), people: Vec::new() };
        assert_eq!(lobby_title(&lobby("ranked-startup", QueueMode::Ranked)), "Ranked · Startup");
        assert_eq!(lobby_title(&lobby("casual-startup", QueueMode::Casual)), "Casual · Startup");
        assert_eq!(lobby_title(&lobby("K7M2QX", QueueMode::Friends)), "Friday · Startup");
        assert_eq!(opponent_label(None), "A person");
        assert_eq!(opponent_label(Some(Level::Veteran)), "Veteran bot");
    }
}
