//! The Play Online screen's state: which page is up, what the person has
//! typed and chosen, and what a press asks the screen to do.
//!
//! **Three ways in, as the terminal offers them** (`netrunner_cli::tui::
//! online`): host a game on this machine and give the opponent an address
//! or a ticket; join one by address or ticket; or list a server's matches
//! and watch one. Hosting and joining both end at the same page, waiting
//! for a seat, and both end at the board through the same handle
//! (`MatchHandle::start_remote`).
//!
//! **No I/O here.** A press that needs the network — start hosting, dial,
//! list a server's matches — is an [`Outcome`] the screen carries out, and
//! what comes back is an [`Intent`] (`Waiting`, `Failed`, `Listed`). So
//! every rule of the form — a port that is not a number is refused before
//! anything is bound, Escape walks back one page, a failed dial reopens
//! the form it came from with the reason — is tested without a socket.

use netrunner_client::hosting::{normalize_address, Reach, DEFAULT_PORT};
use netrunner_client::online::DeckChoice;
use netrunner_core::rules::Side;
use netrunner_server::MatchSummary;

/// The page up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// The three ways in.
    Home,
    Host,
    Join,
    /// A server's matches, to watch one.
    Watch,
    /// Hosting, dialling or in a lobby, until a seat comes.
    Waiting,
}

/// A line the person types into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Join's and Watch's: a server's address or a host's ticket.
    Address,
    Room,
    Port,
}

impl Field {
    /// The longest text the field takes. A ticket is a few hundred
    /// characters of base32 and is pasted, never typed.
    pub fn max_len(self) -> usize {
        match self {
            Field::Address => 1024,
            Field::Room => 64,
            Field::Port => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Open(Page),
    /// Escape or Back: one page out, or off the screen from Home.
    Back,
    /// A field was typed into and committed.
    Typed(Field, String),
    SetReach(Reach),
    SetDeck(usize),
    /// Which side of the table a spectator sits nearer.
    SetWatchFrom(Side),
    /// Host's or Join's primary button.
    Go,
    /// List the matches at Watch's address.
    List,
    Watch(usize),
    /// The screen has started what `Go` or `Watch` asked for; this is its
    /// first line.
    Waiting(String),
    /// The waiting line changed: a place in the lobby, a reconnect.
    Status(String),
    /// What `Go`, `Watch` or `List` asked for did not work.
    Failed(String),
    Listed(Vec<MatchSummary>),
}

/// What a press asks of the screen.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Nothing,
    Redraw,
    /// Off the screen, to the main menu.
    Leave,
    Host { port: u16, reach: Reach, deck: DeckChoice },
    /// `room` is `None` for the host's public queue.
    Join { url: String, room: Option<String>, deck: DeckChoice },
    List { url: String },
    Watch { url: String, match_id: uuid::Uuid },
    /// Stop waiting: whatever is hosting or dialling is dropped.
    Stop,
}

#[derive(Debug, Clone)]
pub struct OnlineForm {
    pub page: Page,
    /// The page Waiting was entered from, which a Stop or a failure
    /// reopens.
    pub came_from: Page,
    pub address: String,
    pub room: String,
    pub port: String,
    pub reach: Reach,
    pub decks: Vec<DeckChoice>,
    pub deck: usize,
    pub watch_from: Side,
    pub matches: Vec<MatchSummary>,
    /// Whether Watch's list is an answer from the address shown, so an
    /// empty list can say "no matches" rather than nothing.
    pub listed: bool,
    /// A joiner's or a spectator's waiting line; a host's page reads the
    /// invitation instead.
    pub status: String,
    pub notice: Option<String>,
}

impl OnlineForm {
    /// `address` is where Join and Watch start: the last one used, or the
    /// terminal's default server.
    pub fn new(decks: Vec<DeckChoice>, address: String) -> Self {
        OnlineForm {
            page: Page::Home,
            came_from: Page::Home,
            address,
            room: String::new(),
            port: DEFAULT_PORT.to_string(),
            reach: Reach::Network,
            decks,
            deck: 0,
            watch_from: Side::Corp,
            matches: Vec::new(),
            listed: false,
            status: String::new(),
            notice: None,
        }
    }

    /// Back on the screen after a game or a visit elsewhere: the decks
    /// read afresh, everything typed kept, and Home up.
    pub fn reopen(&mut self, decks: Vec<DeckChoice>) {
        let chosen = self.decks.get(self.deck).cloned();
        self.deck = chosen.and_then(|chosen| decks.iter().position(|deck| *deck == chosen)).unwrap_or(0);
        self.decks = decks;
        self.page = Page::Home;
        self.notice = None;
    }

    pub fn chosen_deck(&self) -> DeckChoice {
        self.decks.get(self.deck).cloned().unwrap_or(DeckChoice::Dealt(None))
    }

    pub fn apply(&mut self, intent: Intent) -> Outcome {
        match intent {
            Intent::Open(page) => {
                self.notice = None;
                self.page = page;
                Outcome::Redraw
            }
            Intent::Back => match self.page {
                Page::Home => Outcome::Leave,
                Page::Waiting => {
                    self.page = self.came_from;
                    self.notice = None;
                    Outcome::Stop
                }
                Page::Host | Page::Join | Page::Watch => {
                    self.page = Page::Home;
                    self.notice = None;
                    Outcome::Redraw
                }
            },
            Intent::Typed(field, text) => {
                let text = text.trim().to_string();
                match field {
                    Field::Address => {
                        if text != self.address {
                            self.listed = false;
                            self.matches.clear();
                        }
                        self.address = text;
                    }
                    Field::Room => self.room = text,
                    // Digits only: a letter in a port is a typo, not a
                    // port, and is dropped as the terminal's field drops it.
                    Field::Port => self.port = text.chars().filter(char::is_ascii_digit).collect(),
                }
                Outcome::Redraw
            }
            Intent::SetReach(reach) => {
                self.reach = reach;
                Outcome::Redraw
            }
            Intent::SetDeck(index) if index < self.decks.len() => {
                self.deck = index;
                Outcome::Redraw
            }
            Intent::SetDeck(_) => Outcome::Nothing,
            Intent::SetWatchFrom(side) => {
                self.watch_from = side;
                Outcome::Redraw
            }
            Intent::Go => self.go(),
            Intent::List if self.page == Page::Watch => {
                if self.address.is_empty() {
                    self.notice = Some("Type a server's address or a host's ticket first".to_string());
                    return Outcome::Redraw;
                }
                self.notice = None;
                Outcome::List { url: normalize_address(&self.address) }
            }
            Intent::List => Outcome::Nothing,
            Intent::Watch(index) => match self.matches.get(index) {
                Some(summary) if self.page == Page::Watch => {
                    let match_id = summary.match_id;
                    Outcome::Watch { url: normalize_address(&self.address), match_id }
                }
                _ => Outcome::Nothing,
            },
            Intent::Waiting(status) => {
                if self.page != Page::Waiting {
                    self.came_from = self.page;
                }
                self.page = Page::Waiting;
                self.status = status;
                self.notice = None;
                Outcome::Redraw
            }
            Intent::Status(status) => {
                self.status = status;
                Outcome::Redraw
            }
            Intent::Failed(reason) => {
                if self.page == Page::Waiting {
                    self.page = self.came_from;
                }
                self.notice = Some(reason);
                Outcome::Redraw
            }
            Intent::Listed(matches) => {
                self.notice = matches.is_empty().then(|| format!("{} is hosting no matches right now", normalize_address(&self.address)));
                self.matches = matches;
                self.listed = true;
                Outcome::Redraw
            }
        }
    }

    fn go(&mut self) -> Outcome {
        let deck = self.chosen_deck();
        match self.page {
            Page::Host => match self.port.parse::<u16>() {
                Ok(port) => Outcome::Host { port, reach: self.reach, deck },
                Err(_) => {
                    self.notice = Some(format!("{:?} is not a port number", self.port));
                    Outcome::Redraw
                }
            },
            Page::Join if self.address.is_empty() => {
                self.notice = Some("Type the host's address, or paste its ticket".to_string());
                Outcome::Redraw
            }
            Page::Join => {
                let room = Some(self.room.trim().to_string()).filter(|room| !room.is_empty());
                Outcome::Join { url: normalize_address(&self.address), room, deck }
            }
            Page::Home | Page::Watch | Page::Waiting => Outcome::Nothing,
        }
    }
}

/// Who a hosted game is for, in a pill's few words; the whole sentence
/// (`Reach::label`) goes under the pills.
pub fn reach_pill(reach: Reach) -> &'static str {
    match reach {
        Reach::ThisMachine => "This machine",
        Reach::Network => "My network",
        Reach::Internet => "The internet",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> OnlineForm {
        OnlineForm::new(vec![DeckChoice::Dealt(None), DeckChoice::Dealt(Some(Side::Runner))], "ws://127.0.0.1:8080".to_string())
    }

    #[test]
    fn escape_walks_back_one_page_and_then_off_the_screen() {
        let mut form = form();
        form.apply(Intent::Open(Page::Join));
        assert_eq!(form.apply(Intent::Back), Outcome::Redraw);
        assert_eq!(form.page, Page::Home);
        assert_eq!(form.apply(Intent::Back), Outcome::Leave);
    }

    /// A port is digits, and one that is not a port is refused before
    /// anything is bound.
    #[test]
    fn a_port_is_digits_and_a_bad_one_is_refused_at_the_form() {
        let mut form = form();
        form.apply(Intent::Open(Page::Host));
        form.apply(Intent::Typed(Field::Port, "9x9".to_string()));
        assert_eq!(form.port, "99");
        form.apply(Intent::Typed(Field::Port, "99999".to_string()));
        assert_eq!(form.apply(Intent::Go), Outcome::Redraw);
        assert!(form.notice.as_deref().is_some_and(|notice| notice.contains("not a port")));
        form.apply(Intent::Typed(Field::Port, "0".to_string()));
        form.apply(Intent::SetReach(Reach::Internet));
        form.apply(Intent::SetDeck(1));
        assert_eq!(form.apply(Intent::Go), Outcome::Host { port: 0, reach: Reach::Internet, deck: DeckChoice::Dealt(Some(Side::Runner)) });
    }

    /// Join dials what was typed, an address given its scheme and port
    /// (a ticket is passed as it is: `hosting::normalize_address`, tested
    /// there), and a room only when one was named.
    #[test]
    fn join_dials_the_address_and_names_a_room_only_if_given() {
        let mut form = form();
        form.apply(Intent::Open(Page::Join));
        form.apply(Intent::Typed(Field::Address, " 192.168.1.5 ".to_string()));
        assert_eq!(form.apply(Intent::Go), Outcome::Join { url: "ws://192.168.1.5:8080".to_string(), room: None, deck: DeckChoice::Dealt(None) });
        form.apply(Intent::Typed(Field::Room, "friday".to_string()));
        form.apply(Intent::Typed(Field::Address, "host.example:9000".to_string()));
        let Outcome::Join { url, room, .. } = form.apply(Intent::Go) else { panic!() };
        assert_eq!((url.as_str(), room.as_deref()), ("ws://host.example:9000", Some("friday")));
        form.apply(Intent::Typed(Field::Address, String::new()));
        assert_eq!(form.apply(Intent::Go), Outcome::Redraw, "nothing to dial");
    }

    /// Waiting remembers the form it came from: Stop and a failure both
    /// reopen it, the failure with its reason.
    #[test]
    fn waiting_goes_back_to_the_form_it_came_from() {
        let mut form = form();
        form.apply(Intent::Open(Page::Join));
        form.apply(Intent::Waiting("Connecting…".to_string()));
        assert_eq!(form.page, Page::Waiting);
        form.apply(Intent::Status("In the lobby".to_string()));
        assert_eq!(form.apply(Intent::Back), Outcome::Stop);
        assert_eq!(form.page, Page::Join);
        form.apply(Intent::Waiting("Connecting…".to_string()));
        form.apply(Intent::Failed("refused".to_string()));
        assert_eq!((form.page, form.notice.as_deref()), (Page::Join, Some("refused")));
    }

    /// Watching lists the address's matches, says so when there are none,
    /// and forgets a list when the address changes.
    #[test]
    fn a_list_belongs_to_its_address() {
        let mut form = form();
        form.apply(Intent::Open(Page::Watch));
        assert_eq!(form.apply(Intent::List), Outcome::List { url: "ws://127.0.0.1:8080".to_string() });
        form.apply(Intent::Listed(Vec::new()));
        assert!(form.listed && form.notice.as_deref().is_some_and(|notice| notice.contains("no matches")));
        assert_eq!(form.apply(Intent::Watch(0)), Outcome::Nothing, "nothing to watch");
        form.apply(Intent::Typed(Field::Address, "elsewhere".to_string()));
        assert!(!form.listed);
    }

    #[test]
    fn a_deck_chosen_before_is_chosen_again_when_the_list_is_read_afresh() {
        let mut form = form();
        form.apply(Intent::SetDeck(1));
        form.page = Page::Join;
        form.reopen(vec![DeckChoice::Dealt(Some(Side::Corp)), DeckChoice::Dealt(None), DeckChoice::Dealt(Some(Side::Runner))]);
        assert_eq!((form.deck, form.page), (2, Page::Home));
    }
}
