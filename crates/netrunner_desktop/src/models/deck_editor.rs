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

use netrunner_client::deck_builder::{self, CardBook, DeckStatus, Draft, PoolFilter, PoolSort};
use netrunner_client::start::Plan;
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
    /// The format the pool is legal in. It decides which sets the Set
    /// filter offers, so a chosen set the new format has no card of is
    /// let go, rather than leaving the pool empty under a filter the
    /// drop-down no longer lists.
    Format(NsgFormat),
    /// Only the cards one set printed (a v3 set id), or every set.
    Set(Option<String>),
    Sort(PoolSort),
    /// Every filter back to where the editor opened it; the sort stays,
    /// because it is how the person reads the pool, not what they asked
    /// it for.
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
    /// Opens `deck`. A card the engine does not play yet keeps its id,
    /// which is the id it plays under once it is built — there is nothing
    /// to swap on opening, as there was while catalog-only cards were
    /// `nrdb_<code>` (until NSG pool Stage 0d).
    pub fn open(deck: DeckFile, read_only: bool, book: CardBook, format: NsgFormat) -> Self {
        let draft = Draft::new(deck);
        let filter = PoolFilter::new(draft.deck.side, format);
        let status = deck_builder::status(&draft.deck, book, format);
        Editor { draft, read_only, format, filter, status, note: None }
    }

    pub fn deck(&self) -> &DeckFile {
        &self.draft.deck
    }

    /// The pool as the filters leave it.
    pub fn pool<'a>(&self, book: CardBook<'a>) -> Vec<&'a CardDefinition> {
        deck_builder::pool(book, &self.filter)
    }

    /// The styles a bot may play this deck in: balanced (`None`), then
    /// each plan written for its side on its own. A stack is written into
    /// the file by hand; the editor picks one word.
    pub fn styles(&self) -> Vec<Option<Plan>> {
        let mut styles = vec![None];
        styles.extend(Plan::for_side(self.draft.deck.side).map(Some));
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
                Some(card) => match self.draft.add(card, book.get(&self.draft.deck.identity)) {
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
            Intent::Style(style) => self.draft.set_style(style.into_iter().collect()),
            Intent::Faction(faction) => return self.view(|filter| filter.faction = faction),
            Intent::Kind(kind) => return self.view(|filter| filter.kind = kind),
            Intent::Query(query) => return self.view(|filter| filter.query = query),
            Intent::Format(format) => {
                let offered = deck_builder::sets(book, self.draft.deck.side, format);
                return self.view(|filter| {
                    filter.format = format;
                    if filter.set.as_ref().is_some_and(|set| !offered.contains(set)) {
                        filter.set = None;
                    }
                });
            }
            Intent::Set(set) => return self.view(|filter| filter.set = set),
            Intent::Sort(sort) => return self.view(|filter| filter.sort = sort),
            Intent::ClearFilters => {
                let fresh = PoolFilter { sort: self.filter.sort, ..PoolFilter::new(self.draft.deck.side, self.format) };
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
        let mut editor = Editor::open(decks::by_id("stolen_goods").unwrap(), false, book, NsgFormat::Startup);
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

    /// The set, format and sort each redraw; Clear puts the filters back
    /// and keeps the sort.
    #[test]
    fn the_pool_narrows_by_set_and_format_and_clear_keeps_the_sort() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let mut editor = Editor::open(decks::by_id("stolen_goods").unwrap(), false, book, NsgFormat::Startup);
        assert_eq!(editor.apply(Intent::Format(NsgFormat::Eternal), book), Outcome::View);
        assert_eq!(editor.apply(Intent::Set(Some("core_set".into())), book), Outcome::View);
        assert!(!editor.pool(book).is_empty());
        assert_eq!(editor.apply(Intent::Sort(PoolSort::Cost), book), Outcome::View);
        assert_eq!(editor.apply(Intent::ClearFilters, book), Outcome::View);
        assert_eq!(editor.filter, PoolFilter { sort: PoolSort::Cost, ..PoolFilter::new(Side::Runner, NsgFormat::Startup) });
    }

    /// The format decides the sets: a set chosen under Eternal that
    /// Startup has no card of is let go when Startup is chosen, and one
    /// both formats hold stays.
    #[test]
    fn a_format_lets_go_of_a_set_it_does_not_offer() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let mut editor = Editor::open(decks::by_id("stolen_goods").unwrap(), false, book, NsgFormat::Startup);
        assert_eq!(editor.apply(Intent::Format(NsgFormat::Eternal), book), Outcome::View);
        assert_eq!(editor.apply(Intent::Set(Some("core_set".into())), book), Outcome::View);
        assert_eq!(editor.apply(Intent::Format(NsgFormat::Startup), book), Outcome::View);
        assert_eq!(editor.filter.set, None, "Startup has no Core Set card");
        assert_eq!(editor.filter.format, NsgFormat::Startup);
        assert!(!editor.pool(book).is_empty());
        assert_eq!(editor.apply(Intent::Set(Some("elevation".into())), book), Outcome::View);
        assert_eq!(editor.apply(Intent::Format(NsgFormat::Eternal), book), Outcome::View);
        assert_eq!(editor.filter.set.as_deref(), Some("elevation"), "Eternal holds Elevation too");
        assert_eq!(editor.apply(Intent::Format(NsgFormat::Eternal), book), Outcome::Nothing);
    }

    #[test]
    fn a_built_in_deck_refuses_every_edit_and_still_filters() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let mut editor = Editor::open(decks::by_id("stolen_goods").unwrap(), true, book, NsgFormat::Startup);
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
        let mut editor = Editor::open(decks::by_id("stolen_goods").unwrap(), false, book, NsgFormat::Startup);
        let full = editor.deck().cards.iter().find(|entry| entry.count == 3).unwrap().card.clone();
        assert_eq!(editor.apply(Intent::Add(full), book), Outcome::View);
        assert!(editor.note.as_deref().unwrap().contains("limit"));
    }

    #[test]
    fn the_styles_are_balanced_then_the_sides_profiles() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let mut editor = Editor::open(decks::by_id("stolen_goods").unwrap(), false, book, NsgFormat::Startup);
        let styles = editor.styles();
        assert_eq!(styles[0], None);
        assert!(styles[1..].iter().all(|style| style.unwrap().side() == Side::Runner));
        let current = editor.deck().style.clone();
        let name = styles[1..].iter().map(|style| style.unwrap().name().to_string()).find(|name| current != vec![name.clone()]).unwrap();
        assert_eq!(editor.apply(Intent::Style(Some(name.clone())), book), Outcome::Save);
        assert_eq!(editor.deck().style, vec![name]);
    }

    #[test]
    fn an_identity_of_the_other_side_is_refused() {
        let (registry, catalog) = cards();
        let book = CardBook::new(&registry, &catalog);
        let mut editor = Editor::open(decks::by_id("stolen_goods").unwrap(), false, book, NsgFormat::Startup);
        let corp = decks::by_id("discretion_advised").unwrap().identity;
        assert_eq!(editor.apply(Intent::Identity(corp), book), Outcome::View);
        assert!(editor.note.is_some());
    }
}
