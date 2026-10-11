//! The deck picker both clients choose a deck from, and the rows it
//! lists (Phase 7 §16, 11 October 2026).
//!
//! **One list of decks, read once, told the same way everywhere.** The
//! desktop's Decks screen, its new-game form, its Server page and the
//! terminal's new-game and online screens each listed decks on their
//! own — a shelf row here, a `DeckRow` there, a `DeckFile` filtered by a
//! format on the Server page — and only the shelf said what a person
//! choosing needs: the identity's name and faction, whether the deck is
//! the person's own, and where it is legal. [`rows`] is the one read, and
//! [`Row`] carries those facts for every screen; [`Picker`] is the state
//! of choosing one of them, which the desktop draws as a pop-up over
//! whatever screen asked and the terminal as a list.
//!
//! **A deck that cannot be played at this table is listed, and cannot be
//! chosen.** A saved deck may be illegal — the builder saves whatever is
//! built — and a person who built one should find it in the list and
//! read why it is greyed out, not wonder where it went; so an illegal
//! row stays, dimmed, with its reason, and `Choose` on it is refused
//! with that reason rather than quietly ignored. A test deck
//! (`DeckCategory::Sweep`) is left out: it exists to reach rules no
//! published list prints, and is not a deck anyone picks to play.
//!
//! **Choosing a deck makes it the active deck for its side**
//! (`settings::Settings::set_active_deck`): the one the person last
//! chose, which every place a deck is asked starts from — the new-game
//! form, a lobby's seek, a tournament's entry — so a person with one
//! deck a side never chooses it twice. The picker does not write the
//! file itself; the client that owns the settings does, on the outcome.

use std::path::Path;

use netrunner_core::card::Faction;
use netrunner_core::decks::{DeckCategory, DeckFile};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::deck_builder::{self, CardBook, DeckStatus};
use crate::deck_store::{self, Origin};
use crate::settings::format_label;

/// One deck as every list shows it: the file, whether it is the
/// person's own, its identity's name and faction, and its standing in
/// the format the list was read for with every format it is legal in.
#[derive(Debug, Clone)]
pub struct Row {
    pub deck: DeckFile,
    pub saved: bool,
    pub identity: String,
    /// The identity's faction, which is the deck's.
    pub faction: Option<Faction>,
    pub status: DeckStatus,
}

impl Row {
    /// Whether the deck can start a game in the format the row was read
    /// for: both validators pass.
    pub fn playable(&self) -> bool {
        self.status.standing.is_legal()
    }

    /// Why it cannot, when it cannot.
    pub fn problem(&self) -> Option<&str> {
        self.status.standing.reason()
    }

    /// The style a bot plays it in, as a word or two.
    pub fn style(&self) -> String {
        self.deck.style_label()
    }

    /// The formats the deck is legal in, named — "Startup, Standard,
    /// Eternal" — or "no format".
    pub fn legal_words(&self) -> String {
        if self.status.legal_in.is_empty() {
            return "no format".to_string();
        }
        self.status.legal_in.iter().map(|format| format_label(*format)).collect::<Vec<_>>().join(", ")
    }

    /// One line for a list: the name, the style, the identity, whether it
    /// is saved, and why it cannot be played here if it cannot.
    pub fn label(&self) -> String {
        let saved = if self.saved { " (saved)" } else { "" };
        let problem = if self.playable() { "" } else { " — not playable here" };
        format!("{} · {} · {}{saved}{problem}", self.deck.name, self.style(), self.identity)
    }

    /// A row with nothing behind it but its words — for a test of a
    /// screen that lists rows, which needs no registry. `problem` makes
    /// it a deck that cannot be played here.
    #[doc(hidden)]
    pub fn fixture(id: &str, name: &str, side: Side, identity: &str, style: &[&str], saved: bool, problem: Option<&str>) -> Row {
        use crate::deck_builder::Standing;
        let deck = DeckFile {
            id: id.to_string(),
            name: name.to_string(),
            side,
            category: if saved { DeckCategory::Custom } else { DeckCategory::Sample },
            description: None,
            how_to_play: None,
            style: style.iter().map(|s| s.to_string()).collect(),
            identity: netrunner_core::dsl::CardId(identity.to_lowercase().replace(' ', "_")),
            cards: Vec::new(),
        };
        let format = NsgFormat::Startup;
        let standing = problem.map_or(Standing::Legal, |problem| Standing::Illegal(problem.to_string()));
        let legal_in = if problem.is_some() { Vec::new() } else { vec![format] };
        Row { deck, saved, identity: identity.to_string(), faction: None, status: DeckStatus { format, standing, legal_in } }
    }
}

/// The order built-in decks are listed in: the published samples first.
pub fn category_order(category: DeckCategory) -> u8 {
    match category {
        DeckCategory::Custom => 0,
        DeckCategory::Sample => 1,
        DeckCategory::Starter => 2,
        DeckCategory::Boosted => 3,
        DeckCategory::Sweep => 4,
    }
}

/// Every deck, the person's first, then the built-in ones by category,
/// Corp before Runner and then by name — each with its standing in
/// `format`. `dir` is the saved-deck directory, `None` when the OS has
/// none, in which case the built-in decks are the list. A deck file that
/// cannot be read is a line in the problems, never a reason to list
/// nothing (`deck_store::list_lenient`).
pub fn rows(dir: Option<&Path>, book: CardBook, format: NsgFormat) -> (Vec<Row>, Vec<String>) {
    let (stored, problems) = match dir {
        Some(dir) => deck_store::list_lenient(dir),
        None => (deck_store::list_lenient(&std::env::temp_dir().join("netrunner-no-decks")).0, Vec::new()),
    };
    let mut rows: Vec<Row> = stored
        .into_iter()
        .map(|stored| {
            let status = deck_builder::status(&stored.deck, book, format);
            let faction = book.get(&stored.deck.identity).and_then(|identity| identity.faction);
            Row { identity: book.title(&stored.deck.identity), faction, saved: !matches!(stored.origin, Origin::Embedded), deck: stored.deck, status }
        })
        .collect();
    rows.sort_by_key(|row| (!row.saved, category_order(row.deck.category), row.deck.side == Side::Runner, row.deck.name.to_lowercase()));
    (rows, problems)
}

/// The rows a picker for `side` offers: that side's, test decks left out.
pub fn rows_for(side: Side, rows: &[Row]) -> Vec<Row> {
    rows.iter().filter(|row| row.deck.side == side && row.deck.category != DeckCategory::Sweep).cloned().collect()
}

/// What a client can do to the picker. The terminal maps keys onto
/// these; the desktop's buttons and its search field write them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// The search box's text: a deck is kept when its name or its
    /// identity's holds the text, case ignored.
    Query(String),
    /// List only the decks legal in a format, or every deck.
    Legal(Option<NsgFormat>),
    /// Up or down the visible rows, wrapping.
    Move(i32),
    /// The row under the cursor.
    ChooseCursor,
    /// A row by its index into [`Picker::rows`].
    Choose(usize),
}

/// What the picker did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// The list changed: redraw.
    Changed,
    /// A deck was chosen, by id.
    Chosen(String),
    /// The deck cannot be chosen here, and this is why.
    Refused(String),
}

/// Choosing one of a side's decks for a table in a format.
#[derive(Debug, Clone)]
pub struct Picker {
    pub side: Side,
    /// The format the table is in: what a row's `playable` answers.
    pub format: NsgFormat,
    pub rows: Vec<Row>,
    /// A format to list only the decks legal in, or none.
    pub legal: Option<NsgFormat>,
    pub query: String,
    /// The row the keys are on, an index into [`Picker::rows`].
    pub cursor: usize,
}

impl Picker {
    /// A picker over `rows`, which are `side`'s already ([`rows_for`]),
    /// with the cursor on `chosen` when it names one of them — the active
    /// deck, or the one the form holds — else on the first playable row.
    pub fn open(side: Side, format: NsgFormat, rows: Vec<Row>, chosen: Option<&str>) -> Self {
        let cursor = chosen
            .and_then(|id| rows.iter().position(|row| row.deck.id == id))
            .or_else(|| rows.iter().position(Row::playable))
            .unwrap_or(0);
        Picker { side, format, rows, legal: None, query: String::new(), cursor }
    }

    /// The rows the query and the format filter leave, as indices into
    /// [`Picker::rows`], in the list's order.
    pub fn visible(&self) -> Vec<usize> {
        let query = self.query.trim().to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| self.legal.is_none_or(|format| row.status.legal_in.contains(&format)))
            .filter(|(_, row)| query.is_empty() || row.deck.name.to_lowercase().contains(&query) || row.identity.to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect()
    }

    /// The row under the cursor, when the filters leave it visible.
    pub fn under_cursor(&self) -> Option<&Row> {
        self.visible().contains(&self.cursor).then(|| &self.rows[self.cursor])
    }

    /// The row with this id, if the picker lists it.
    pub fn row(&self, id: &str) -> Option<&Row> {
        self.rows.iter().find(|row| row.deck.id == id)
    }

    pub fn apply(&mut self, intent: Intent) -> Outcome {
        match intent {
            Intent::Query(text) => {
                if self.query == text {
                    return Outcome::Nothing;
                }
                self.query = text;
                self.settle_cursor();
                Outcome::Changed
            }
            Intent::Legal(format) => {
                if self.legal == format {
                    return Outcome::Nothing;
                }
                self.legal = format;
                self.settle_cursor();
                Outcome::Changed
            }
            Intent::Move(delta) => {
                let visible = self.visible();
                if visible.is_empty() {
                    return Outcome::Nothing;
                }
                let at = visible.iter().position(|index| *index == self.cursor).unwrap_or(0) as i32;
                let next = visible[(at + delta).rem_euclid(visible.len() as i32) as usize];
                if next == self.cursor {
                    return Outcome::Nothing;
                }
                self.cursor = next;
                Outcome::Changed
            }
            Intent::ChooseCursor => match self.under_cursor() {
                Some(_) => self.apply(Intent::Choose(self.cursor)),
                None => Outcome::Nothing,
            },
            Intent::Choose(index) => match self.rows.get(index) {
                Some(row) if row.playable() => Outcome::Chosen(row.deck.id.clone()),
                Some(row) => Outcome::Refused(format!(
                    "{} can't be played in {}: {}",
                    row.deck.name,
                    format_label(self.format),
                    row.problem().unwrap_or("it is not legal here")
                )),
                None => Outcome::Nothing,
            },
        }
    }

    /// After a filter changes, the cursor stays where it was if that row
    /// is still shown, else goes to the first row shown.
    fn settle_cursor(&mut self) {
        let visible = self.visible();
        if !visible.contains(&self.cursor) {
            self.cursor = visible.first().copied().unwrap_or(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Row> {
        vec![
            Row::fixture("brick_stack", "Brick Stack", Side::Corp, "Weyland Consortium: Built to Last", &["glacier"], false, None),
            Row::fixture("my_deck", "My Deck", Side::Corp, "Haas-Bioroid: Precision Design", &[], true, None),
            Row::fixture("banned", "Old Favourite", Side::Corp, "NBN: Reality Plus", &[], true, Some("Cleaver is banned in Startup")),
            Row::fixture("stolen_goods", "Stolen Goods", Side::Runner, "Ken Tenma", &["pressure"], false, None),
        ]
    }

    /// Only the side's decks are offered, and the cursor starts on the
    /// deck named — the active one — or the first playable row.
    #[test]
    fn a_picker_offers_a_sides_decks_and_starts_on_the_one_named() {
        let corp = rows_for(Side::Corp, &rows());
        assert_eq!(corp.len(), 3);
        assert_eq!(Picker::open(Side::Corp, NsgFormat::Startup, corp.clone(), Some("my_deck")).cursor, 1);
        assert_eq!(Picker::open(Side::Corp, NsgFormat::Startup, corp.clone(), Some("nope")).cursor, 0, "an unknown id: the first playable");
        assert_eq!(Picker::open(Side::Corp, NsgFormat::Startup, corp, None).under_cursor().map(|row| row.deck.id.as_str()), Some("brick_stack"));
    }

    /// The search keeps a deck by its name or its identity's, case
    /// ignored; an illegal deck is listed and refused by name with its
    /// reason; the keys wrap over what is shown.
    #[test]
    fn a_search_narrows_the_list_and_an_illegal_deck_is_refused_with_its_reason() {
        let mut picker = Picker::open(Side::Corp, NsgFormat::Startup, rows_for(Side::Corp, &rows()), None);
        assert_eq!(picker.apply(Intent::Query("precision".to_string())), Outcome::Changed);
        assert_eq!(picker.visible(), vec![1], "found by the identity's name");
        assert_eq!(picker.cursor, 1, "the cursor moved onto what is shown");
        assert_eq!(picker.apply(Intent::Query(String::new())), Outcome::Changed);
        assert_eq!(picker.visible(), vec![0, 1, 2]);
        assert_eq!(picker.apply(Intent::Move(-1)), Outcome::Changed);
        assert_eq!(picker.cursor, 0, "up from the second row");
        assert_eq!(picker.apply(Intent::Move(-1)), Outcome::Changed);
        assert_eq!(picker.cursor, 2, "wraps to the last");
        let refused = picker.apply(Intent::ChooseCursor);
        assert!(matches!(&refused, Outcome::Refused(reason) if reason.contains("Old Favourite") && reason.contains("Cleaver")), "{refused:?}");
        assert_eq!(picker.apply(Intent::Choose(0)), Outcome::Chosen("brick_stack".to_string()));
        assert_eq!(picker.apply(Intent::Legal(Some(NsgFormat::Startup))), Outcome::Changed);
        assert_eq!(picker.visible(), vec![0, 1], "the illegal deck is legal nowhere");
        assert_eq!(picker.apply(Intent::Legal(Some(NsgFormat::Startup))), Outcome::Nothing, "already so");
    }

    /// The label a terminal list prints, and the legal formats in words.
    #[test]
    fn a_row_reads_as_one_line() {
        let list = rows();
        assert_eq!(list[0].label(), "Brick Stack · glacier · Weyland Consortium: Built to Last");
        assert_eq!(list[2].label(), "Old Favourite · balanced · NBN: Reality Plus (saved) — not playable here");
        assert_eq!(list[0].legal_words(), "Startup");
        assert_eq!(list[2].legal_words(), "no format");
    }
}
