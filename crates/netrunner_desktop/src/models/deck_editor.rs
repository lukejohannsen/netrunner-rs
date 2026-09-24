//! The deck editor's state: the deck being built, the pool it is built
//! from, and the verdict on it, re-read after every edit.
//!
//! **Every edit is saved as it is made**, the terminal builder's rule: a
//! deck under construction is legitimately illegal, so there is no "valid
//! enough to save" point to wait for and nothing to lose on a quit. An
//! intent that changed the deck says so ([`Outcome::Save`]) and the
//! screen writes it; a failed write is a notice, and the edit stays in
//! the editor.
//!
//! **A built-in deck opens read-only.** It is a published list; the
//! editor shows it — the cards, the verdict, how to play it — with Copy
//! to edit, and refuses every intent that would change it.

use netrunner_client::deck_builder::{self, CardBook, DeckStatus, Draft, PoolFilter};
use netrunner_client::start::Personality;
use netrunner_core::card::Faction;
use netrunner_core::decks::DeckFile;
use netrunner_core::dsl::{CardDefinition, CardId};
use netrunner_core::format::NsgFormat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    Add(CardId),
    Remove(CardId),
    RemoveAll(CardId),
    Identity(CardId),
    Rename(String),
    /// The bot style a game plays the deck in; `None` is balanced.
    Style(Option<String>),
    Faction(Option<Faction>),
    /// A `cards::type_group` name.
    Kind(Option<&'static str>),
    Query(String),
    /// Only the format's pool, or every printing.
    FormatOnly(bool),
    /// Show the printings the engine does not play yet.
    Unplayable(bool),
    ClearFilters,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// The pool or a filter changed: redraw, nothing to write.
    View,
    /// The deck changed: redraw and write it.
    Save,
}

pub struct Editor {
    pub draft: Draft,
    pub read_only: bool,
    pub format: NsgFormat,
    pub filter: PoolFilter,
    pub status: DeckStatus,
    /// A refusal that is not an error — the copy limit, a read-only
    /// deck — or a write that failed.
    pub note: Option<String>,
}

impl Editor {
    /// Opens `deck`; a saved deck's catalog-only ids are swapped for the
    /// playable card where one now exists (`Draft::resolve`), which is an
    /// edit, so the first redraw saves it.
    pub fn open(deck: DeckFile, read_only: bool, book: CardBook, format: NsgFormat) -> (Self, bool) {
        let mut draft = Draft::new(deck);
        let resolved = !read_only && draft.resolve(book);
        let filter = PoolFilter::new(draft.deck.side, format);
        let status = deck_builder::status(&draft.deck, book, format);
        (Editor { draft, read_only, format, filter, status, note: None }, resolved)
    }

    pub fn deck(&self) -> &DeckFile {
        &self.draft.deck
    }

    /// The pool as the filters leave it.
    pub fn pool<'a>(&self, book: CardBook<'a>) -> Vec<&'a CardDefinition> {
        deck_builder::pool(book, &self.filter)
    }

    /// The styles a bot may play this deck in, the deck's own first:
    /// balanced (`None`), then each profile written for its side.
    pub fn styles(&self) -> Vec<Option<Personality>> {
        let mut styles = vec![None];
        styles.extend(Personality::ALL.iter().copied().filter(|p| p.side() == Some(self.draft.deck.side)).map(Some));
        styles
    }

    pub fn apply(&mut self, intent: Intent, book: CardBook) -> Outcome {
        self.note = None;
        let edits = matches!(intent, Intent::Add(_) | Intent::Remove(_) | Intent::RemoveAll(_) | Intent::Identity(_) | Intent::Rename(_) | Intent::Style(_));
        if edits && self.read_only {
            self.note = Some("A built-in deck is a published list — copy it to change it".to_string());
            return Outcome::View;
        }
        let changed = match intent {
            Intent::Add(id) => match book.get(&id) {
                Some(card) => match self.draft.add(card) {
                    Ok(()) => true,
                    Err(refusal) => {
                        self.note = Some(refusal);
                        return Outcome::View;
                    }
                },
                None => false,
            },
            Intent::Remove(id) => self.draft.remove(&id),
            Intent::RemoveAll(id) => self.draft.remove_all(&id),
            Intent::Identity(id) => match book.get(&id).map(|card| self.draft.set_identity(card)) {
                Some(Ok(changed)) => changed,
                Some(Err(refusal)) => {
                    self.note = Some(refusal);
                    return Outcome::View;
                }
                None => false,
            },
            Intent::Rename(name) => self.draft.rename(&name),
            Intent::Style(style) => self.draft.set_style(style),
            Intent::Faction(faction) => return self.view(|filter| filter.faction = faction),
            Intent::Kind(kind) => return self.view(|filter| filter.kind = kind),
            Intent::Query(query) => return self.view(|filter| filter.query = query),
            Intent::FormatOnly(only) => {
                let format = self.format;
                return self.view(|filter| filter.format = only.then_some(format));
            }
            Intent::Unplayable(show) => return self.view(|filter| filter.unplayable = show),
            Intent::ClearFilters => {
                let fresh = PoolFilter { format: self.filter.format, unplayable: self.filter.unplayable, ..PoolFilter::new(self.draft.deck.side, self.format) };
                return self.view(|filter| *filter = fresh);
            }
        };
        if !changed {
            return Outcome::Nothing;
        }
        self.status = deck_builder::status(&self.draft.deck, book, self.format);
        Outcome::Save
    }

    fn view(&mut self, change: impl FnOnce(&mut PoolFilter)) -> Outcome {
        let before = self.filter.clone();
        change(&mut self.filter);
        if self.filter == before { Outcome::Nothing } else { Outcome::View }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_client::deck_builder::Standing;
    use netrunner_core::cards::CardRegistry;
    use netrunner_core::decks;
    use netrunner_core::dsl::CardType;
    use netrunner_core::rules::Side;

    fn cards() -> (CardRegistry, Vec<CardDefinition>) {
        let registry = netrunner_client::decks::sample_deck_registry();
        let catalog = netrunner_client::cards::catalog(&registry);
        (registry, catalog)
    }

    #[test]
    fn an_edit_is_saved_and_re_judged_and_a_filter_only_redraws() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let (mut editor, _) = Editor::open(decks::by_id("stolen_goods").unwrap(), false, book, NsgFormat::Startup);
        assert_eq!(editor.status.standing, Standing::Legal);
        let first = editor.deck().cards[0].card.clone();
        for _ in 0..3 {
            editor.apply(Intent::Remove(first.clone()), book);
        }
        assert!(editor.draft.copies(&first) == 0);
        assert!(!editor.status.standing.is_legal(), "the verdict is re-read after the edit: {:?}", editor.status.standing);
        assert_eq!(editor.apply(Intent::Kind(Some("Event")), book), Outcome::View);
        assert!(editor.pool(book).iter().all(|card| card.card_type == CardType::Event && card.side == Side::Runner));
        assert_eq!(editor.apply(Intent::Kind(Some("Event")), book), Outcome::Nothing);
        assert_eq!(editor.apply(Intent::Add(first.clone()), book), Outcome::Save);
    }

    #[test]
    fn a_built_in_deck_refuses_every_edit_and_still_filters() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let (mut editor, resolved) = Editor::open(decks::by_id("stolen_goods").unwrap(), true, book, NsgFormat::Startup);
        assert!(!resolved);
        let first = editor.deck().cards[0].card.clone();
        assert_eq!(editor.apply(Intent::Remove(first.clone()), book), Outcome::View);
        assert!(editor.note.as_deref().unwrap().contains("copy it"));
        assert_eq!(editor.draft.deck, decks::by_id("stolen_goods").unwrap());
        assert_eq!(editor.apply(Intent::Rename("Mine".into()), book), Outcome::View);
        assert_eq!(editor.apply(Intent::Query("gamble".into()), book), Outcome::View);
    }

    #[test]
    fn the_copy_limit_is_a_note_not_an_edit() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let (mut editor, _) = Editor::open(decks::by_id("stolen_goods").unwrap(), false, book, NsgFormat::Startup);
        let full = editor.deck().cards.iter().find(|entry| entry.count == 3).unwrap().card.clone();
        assert_eq!(editor.apply(Intent::Add(full), book), Outcome::View);
        assert!(editor.note.as_deref().unwrap().contains("limit"));
    }

    #[test]
    fn the_styles_are_balanced_then_the_sides_profiles() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let (mut editor, _) = Editor::open(decks::by_id("stolen_goods").unwrap(), false, book, NsgFormat::Startup);
        let styles = editor.styles();
        assert_eq!(styles[0], None);
        assert!(styles[1..].iter().all(|style| style.unwrap().side() == Some(Side::Runner)));
        let current = editor.deck().style.clone();
        let name = styles[1..].iter().map(|style| style.unwrap().name().to_string()).find(|name| Some(name) != current.as_ref()).unwrap();
        assert_eq!(editor.apply(Intent::Style(Some(name.clone())), book), Outcome::Save);
        assert_eq!(editor.deck().style.as_deref(), Some(name.as_str()));
    }

    #[test]
    fn an_identity_of_the_other_side_is_refused() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let (mut editor, _) = Editor::open(decks::by_id("stolen_goods").unwrap(), false, book, NsgFormat::Startup);
        let corp = decks::by_id("discretion_advised").unwrap().identity;
        assert_eq!(editor.apply(Intent::Identity(corp), book), Outcome::View);
        assert!(editor.note.is_some());
    }
}
