//! The Decks screen's state: every deck, the person's first, each with
//! its standing in the format Settings names, and the moves that make a
//! deck of the person's own — a new one, a copy, an import — or take one
//! away.
//!
//! **Saved decks are the person's; built-in decks are published lists.**
//! A built-in deck is read, copied and exported, never edited or deleted
//! (`deck_store::save` refuses its id): editing one in place would
//! silently diverge from the list it claims to be. A copy is how a
//! person builds from one.
//!
//! **Every write is `deck_store`'s**, so a deck saved here is a file the
//! terminal client lists and plays, and every failure is a notice rather
//! than a lost deck. A deck is written whatever its standing — the
//! builder's rule (`netrunner_client::deck_builder`).

use std::path::PathBuf;

use netrunner_client::deck_builder::{self, CardBook, DeckStatus};
use netrunner_client::deck_store::{self, Origin};
use netrunner_core::decks::{DeckCategory, DeckFile};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

/// One deck as the shelf shows it.
#[derive(Debug, Clone)]
pub struct ShelfRow {
    pub deck: DeckFile,
    pub saved: bool,
    pub identity: String,
    pub status: DeckStatus,
}

/// A group of rows under one heading: the person's decks, then each
/// category of built-in deck.
#[derive(Debug, Clone)]
pub struct Section {
    pub title: &'static str,
    pub blurb: &'static str,
    /// Indices into [`Shelf::rows`].
    pub rows: Vec<usize>,
}

/// What a press asked for, and what the shelf did about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Show one side's decks, or both.
    Side(Option<Side>),
    /// Copy row `n` into a new saved deck.
    Copy(usize),
    /// Ask before deleting row `n`.
    AskDelete(usize),
    /// The question's answer.
    ConfirmDelete(bool),
    /// Save a pasted or dropped list as a new deck.
    Import(String),
}

/// What happened, for the screen to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// The shelf changed: redraw.
    Changed,
    /// A deck was created (a copy, an import): open it in the editor.
    Open(String),
}

pub struct Shelf {
    dir: Option<PathBuf>,
    format: NsgFormat,
    pub rows: Vec<ShelfRow>,
    pub side: Option<Side>,
    /// The row a delete is waiting on an answer for.
    pub confirming: Option<usize>,
    /// One line for the person: a file that would not read, a save that
    /// failed, what an import skipped.
    pub notice: Option<String>,
}

impl Shelf {
    /// Reads every deck. `dir` is `None` when the OS has no data
    /// directory: the built-in decks are still listed, and anything that
    /// would write says why it cannot.
    pub fn open(dir: Option<PathBuf>, book: CardBook, format: NsgFormat) -> Self {
        let mut shelf = Shelf { dir, format, rows: Vec::new(), side: None, confirming: None, notice: None };
        shelf.reload(book);
        shelf
    }

    pub fn format(&self) -> NsgFormat {
        self.format
    }

    /// Re-reads the directory: the person's decks first, then the
    /// built-in ones by category; within each, Corp before Runner and
    /// then by name.
    pub fn reload(&mut self, book: CardBook) {
        let (stored, problems) = match &self.dir {
            Some(dir) => deck_store::list_lenient(dir),
            None => (deck_store::list_lenient(&std::env::temp_dir().join("netrunner-no-decks")).0, Vec::new()),
        };
        if !problems.is_empty() {
            self.notice = Some(format!("Some deck files could not be read: {}", problems.join("; ")));
        }
        let mut rows: Vec<ShelfRow> = stored
            .into_iter()
            .map(|stored| {
                let status = deck_builder::status(&stored.deck, book, self.format);
                ShelfRow { identity: book.title(&stored.deck.identity), saved: !matches!(stored.origin, Origin::Embedded), deck: stored.deck, status }
            })
            .collect();
        rows.sort_by_key(|row| (!row.saved, category_order(row.deck.category), row.deck.side == Side::Runner, row.deck.name.to_lowercase()));
        self.rows = rows;
    }

    /// The sections, each holding the rows the side filter leaves. The
    /// person's section is there even when empty — it is where their
    /// first deck will go — and a built-in category with nothing to show
    /// is not.
    pub fn sections(&self) -> Vec<Section> {
        let visible = |row: &ShelfRow| self.side.is_none_or(|side| row.deck.side == side);
        let mut sections = vec![Section {
            title: "Your decks",
            blurb: "Saved in your data directory; the terminal client plays them too.",
            rows: self.rows.iter().enumerate().filter(|(_, row)| row.saved && visible(row)).map(|(i, _)| i).collect(),
        }];
        for category in [DeckCategory::Sample, DeckCategory::Starter, DeckCategory::Boosted, DeckCategory::Sweep, DeckCategory::Custom] {
            let rows: Vec<usize> = self.rows.iter().enumerate().filter(|(_, row)| !row.saved && row.deck.category == category && visible(row)).map(|(i, _)| i).collect();
            if rows.is_empty() {
                continue;
            }
            let (title, blurb) = category_words(category);
            sections.push(Section { title, blurb, rows });
        }
        sections
    }

    /// Every id in use, so a new deck's is free.
    pub fn taken(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.deck.id.clone()).collect()
    }

    fn names(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.deck.name.clone()).collect()
    }

    pub fn apply(&mut self, intent: Intent, book: CardBook) -> Outcome {
        self.notice = None;
        match intent {
            Intent::Side(side) => {
                if self.side == side {
                    return Outcome::Nothing;
                }
                self.side = side;
                Outcome::Changed
            }
            Intent::Copy(index) => {
                let Some(row) = self.rows.get(index) else { return Outcome::Nothing };
                let name = deck_builder::copy_name(&row.deck.name, &self.names());
                let copy = deck_builder::copy_of(&row.deck, &name, &self.taken());
                self.create(copy, book, None)
            }
            Intent::AskDelete(index) => match self.rows.get(index) {
                Some(row) if row.saved => {
                    self.confirming = Some(index);
                    Outcome::Changed
                }
                Some(_) => {
                    self.notice = Some("A built-in deck is a published list and cannot be deleted — copy it to change it".to_string());
                    Outcome::Changed
                }
                None => Outcome::Nothing,
            },
            Intent::ConfirmDelete(yes) => {
                let Some(index) = self.confirming.take() else { return Outcome::Nothing };
                if yes && let Some(row) = self.rows.get(index) {
                    let name = row.deck.name.clone();
                    let result = match &self.dir {
                        Some(dir) => deck_store::delete(dir, &row.deck.id),
                        None => Err(no_dir()),
                    };
                    self.reload(book);
                    self.notice = Some(match result {
                        Ok(()) => format!("Deleted {name}"),
                        Err(error) => format!("Not deleted: {error}"),
                    });
                }
                Outcome::Changed
            }
            Intent::Import(text) => match deck_builder::import(&text, book, &self.taken()) {
                Ok(imported) => {
                    let skipped = (!imported.skipped.is_empty()).then(|| format!("Skipped what the catalog does not know: {}", imported.skipped.join(", ")));
                    self.create(imported.deck, book, skipped)
                }
                Err(error) => {
                    self.notice = Some(format!("Nothing imported: {error}"));
                    Outcome::Changed
                }
            },
        }
    }

    /// Saves a new deck — a new one from the identity picker, a copy or
    /// an import — and says to open it. `note` is kept as the notice for
    /// when the person comes back.
    pub fn create(&mut self, deck: DeckFile, book: CardBook, note: Option<String>) -> Outcome {
        let result = match &self.dir {
            Some(dir) => deck_store::save(dir, &deck),
            None => Err(no_dir()),
        };
        match result {
            Ok(_) => {
                self.reload(book);
                self.notice = note;
                Outcome::Open(deck.id)
            }
            Err(error) => {
                self.notice = Some(format!("Not saved: {error}"));
                Outcome::Changed
            }
        }
    }
}

fn no_dir() -> String {
    "no OS data directory is available, so decks cannot be saved; set NETRUNNER_DECKS_DIR".to_string()
}

fn category_order(category: DeckCategory) -> u8 {
    match category {
        DeckCategory::Custom => 0,
        DeckCategory::Sample => 1,
        DeckCategory::Starter => 2,
        DeckCategory::Boosted => 3,
        DeckCategory::Sweep => 4,
    }
}

fn category_words(category: DeckCategory) -> (&'static str, &'static str) {
    match category {
        DeckCategory::Sample => ("Sample decks", "Null Signal Games' published sample decklists. Copy one to build from it."),
        DeckCategory::Starter => ("Learn to Play decks", "The starter decks, played to six points."),
        DeckCategory::Boosted => ("Boosted starter decks", "A starter deck with its booster pack."),
        DeckCategory::Sweep => ("Test decks", "Built to reach rules no published list prints; Eternal-legal only."),
        DeckCategory::Custom => ("Other decks", "Built-in decks with no category."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_client::deck_builder::Standing;
    use netrunner_core::cards::CardRegistry;
    use netrunner_core::dsl::CardDefinition;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Scratch(std::env::temp_dir().join(format!("netrunner_shelf_{name}_{}_{n}", std::process::id())))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn cards() -> (CardRegistry, Vec<CardDefinition>) {
        let registry = netrunner_client::decks::sample_deck_registry();
        let catalog = netrunner_client::cards::catalog(&registry);
        (registry, catalog)
    }

    fn index_of(shelf: &Shelf, id: &str) -> usize {
        shelf.rows.iter().position(|row| row.deck.id == id).unwrap()
    }

    #[test]
    fn every_built_in_deck_is_listed_with_its_standing_and_the_persons_section_is_first() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let dir = Scratch::new("list");
        let shelf = Shelf::open(Some(dir.0.clone()), book, NsgFormat::Startup);
        assert_eq!(shelf.rows.len(), netrunner_core::decks::embedded_decks().len());
        let sections = shelf.sections();
        assert_eq!(sections[0].title, "Your decks");
        assert!(sections[0].rows.is_empty());
        assert!(sections.iter().any(|section| section.title == "Sample decks"));
        let sweep = shelf.rows.iter().find(|row| row.deck.category == DeckCategory::Sweep).unwrap();
        assert!(!sweep.status.standing.is_legal(), "a test deck is Eternal-only");
        assert!(sweep.status.legal_in.contains(&NsgFormat::Eternal));
        let stolen = &shelf.rows[index_of(&shelf, "stolen_goods")];
        assert_eq!(stolen.status.standing, Standing::Legal);
    }

    #[test]
    fn a_copy_is_saved_as_the_persons_and_opened() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let dir = Scratch::new("copy");
        let mut shelf = Shelf::open(Some(dir.0.clone()), book, NsgFormat::Startup);
        let from = index_of(&shelf, "stolen_goods");
        let Outcome::Open(id) = shelf.apply(Intent::Copy(from), book) else { panic!("a copy opens") };
        let saved = deck_store::read_file(&dir.0.join(format!("{id}.json"))).unwrap();
        assert_eq!(saved.category, DeckCategory::Custom);
        assert!(saved.name.ends_with("(copy)"));
        assert_eq!(shelf.sections()[0].rows.len(), 1, "the copy is in the person's section");
        let Outcome::Open(second) = shelf.apply(Intent::Copy(from), book) else { panic!() };
        assert_ne!(id, second, "a second copy does not overwrite the first");
    }

    #[test]
    fn only_a_saved_deck_deletes_and_only_after_the_question() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let dir = Scratch::new("delete");
        let mut shelf = Shelf::open(Some(dir.0.clone()), book, NsgFormat::Startup);
        let built_in = index_of(&shelf, "stolen_goods");
        shelf.apply(Intent::AskDelete(built_in), book);
        assert_eq!(shelf.confirming, None);
        assert!(shelf.notice.as_deref().unwrap().contains("cannot be deleted"));

        let Outcome::Open(id) = shelf.apply(Intent::Copy(built_in), book) else { panic!() };
        let mine = index_of(&shelf, &id);
        shelf.apply(Intent::AskDelete(mine), book);
        shelf.apply(Intent::ConfirmDelete(false), book);
        assert!(shelf.rows.iter().any(|row| row.deck.id == id), "kept on a no");
        shelf.apply(Intent::AskDelete(index_of(&shelf, &id)), book);
        shelf.apply(Intent::ConfirmDelete(true), book);
        assert!(!shelf.rows.iter().any(|row| row.deck.id == id));
        assert!(!dir.0.join(format!("{id}.json")).exists());
    }

    /// An illegal import is still saved: the builder's rule.
    #[test]
    fn an_import_is_saved_whatever_its_standing_and_says_what_it_skipped() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let dir = Scratch::new("import");
        let mut shelf = Shelf::open(Some(dir.0.clone()), book, NsgFormat::Startup);
        let identity = book.title(&netrunner_core::decks::by_id("stolen_goods").unwrap().identity);
        let Outcome::Open(id) = shelf.apply(Intent::Import(format!("Tiny\n{identity}\n3x Sure Gamble\n2x Nothing Real\n")), book) else { panic!("imports") };
        let row = &shelf.rows[index_of(&shelf, &id)];
        assert!(row.saved);
        assert!(matches!(row.status.standing, Standing::Unplayable(_)), "three cards is not a deck, and is saved anyway");
        assert!(shelf.notice.as_deref().unwrap().contains("Nothing Real"));
        assert_eq!(shelf.apply(Intent::Import("just words".into()), book), Outcome::Changed);
        assert!(shelf.notice.as_deref().unwrap().starts_with("Nothing imported"));
    }

    #[test]
    fn the_side_filter_narrows_every_section() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let dir = Scratch::new("side");
        let mut shelf = Shelf::open(Some(dir.0.clone()), book, NsgFormat::Startup);
        shelf.apply(Intent::Side(Some(Side::Corp)), book);
        assert!(shelf.sections().iter().flat_map(|section| section.rows.iter()).all(|i| shelf.rows[*i].deck.side == Side::Corp));
    }

    #[test]
    fn with_no_data_directory_the_built_in_decks_still_list_and_a_copy_says_why_it_failed() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let mut shelf = Shelf::open(None, book, NsgFormat::Startup);
        assert!(!shelf.rows.is_empty());
        assert_eq!(shelf.apply(Intent::Copy(0), book), Outcome::Changed);
        assert!(shelf.notice.as_deref().unwrap().contains("data directory"));
    }
}
