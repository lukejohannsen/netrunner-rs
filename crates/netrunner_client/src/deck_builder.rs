//! Building a deck, without a toolkit: what a deck's standing is, the
//! edits a builder makes, the card pool it offers, and a decklist in and
//! out as text.
//!
//! **Legality comes from the validators, never from here.** A deck's
//! [`Standing`] is `DeckFile::validate` — both validators, as starting a
//! game runs them — sorted into the two answers a person acts on
//! differently: a deck the engine cannot run at all (a card it does not
//! implement, fewer cards than the identity's minimum) and a deck that
//! runs but breaks the format's rules. The one rule applied here is the
//! copy limit on an add, read off the card, for the reason the terminal
//! builder gave: a builder that let a fourth copy in only to flag it
//! would be a worse builder.
//!
//! **A deck is saved whatever its standing.** A deck under construction
//! is legitimately illegal, and an imported list may hold cards the
//! engine has not implemented; both are the person's deck, and neither is
//! a reason to lose it. What refuses an illegal deck is starting a game
//! (`decks::decks_for_match`), not saving one.
//!
//! **A card the engine does not play yet is still a card.** The catalog
//! lists every card by its NetrunnerDB v3 id, the one a card file takes
//! when the card is built. A deck may hold one — an import from a list
//! built elsewhere — and reads as [`Standing::Unplayable`] until the card
//! is implemented, when the same id plays. (Catalog-only cards were
//! `nrdb_<code>` until NSG pool Stage 0d, and a draft swapped each for the
//! playable card's id when one landed; a deck saved then still names the
//! old id, which no catalog knows.) **The pool never offers one**, though:
//! it is what a match can deal, in the format the person is building for
//! (see [`pool`]).
//!
//! Lifted out of the terminal's builder so the desktop's and the
//! terminal's are one set of rules with two faces, as `start` was for the
//! new-game form.

use netrunner_core::card::Faction;
use netrunner_core::cards::{catalog, CardRegistry};
use netrunner_core::deck::validator::MAX_COPIES_PER_CARD;
use netrunner_core::deck::DeckTally;
use netrunner_core::decks::{DeckCategory, DeckEntry, DeckError, DeckFile};
use netrunner_core::dsl::{CardDefinition, CardId, CardType};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::cards::{faction_order, legal_in, type_group, type_order};
use crate::settings::{format_label, FORMATS};

/// Longest deck name a builder accepts.
pub const MAX_NAME: usize = 48;

/// Every card a builder can name: the playable registry first, then the
/// catalog's cards for what the registry does not hold. One lookup,
/// so a title, a type and a faction read the same whether or not the
/// engine plays the card.
#[derive(Clone, Copy)]
pub struct CardBook<'a> {
    pub registry: &'a CardRegistry,
    pub catalog: &'a [CardDefinition],
}

impl<'a> CardBook<'a> {
    pub fn new(registry: &'a CardRegistry, catalog: &'a [CardDefinition]) -> Self {
        Self { registry, catalog }
    }

    pub fn get(&self, id: &CardId) -> Option<&'a CardDefinition> {
        self.registry.get(id).or_else(|| self.catalog.iter().find(|card| &card.id == id))
    }

    pub fn title(&self, id: &CardId) -> String {
        self.get(id).map_or_else(|| id.0.clone(), |card| card.title.clone())
    }

    /// Whether a match can deal this card.
    pub fn is_playable(&self, id: &CardId) -> bool {
        self.registry.get(id).is_some_and(|card| card.is_playable)
    }

    /// The card a title names, the playable one first.
    pub fn by_title(&self, title: &str) -> Option<&'a CardDefinition> {
        let wanted = fold(title);
        if wanted.is_empty() {
            return None;
        }
        self.registry
            .iter()
            .filter(|card| card.is_playable)
            .find(|card| fold(&card.title) == wanted)
            .or_else(|| self.catalog.iter().find(|card| fold(&card.title) == wanted))
    }
}

/// Where a deck stands in one format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Both validators pass: the deck can start a game.
    Legal,
    /// The engine can run it, but the format's rules refuse it.
    Illegal(String),
    /// The engine cannot run it at all: a card it does not implement, an
    /// identity missing, fewer cards than the minimum.
    Unplayable(String),
}

impl Standing {
    pub fn is_legal(&self) -> bool {
        matches!(self, Standing::Legal)
    }

    /// The reason, for a deck that is not legal.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Standing::Legal => None,
            Standing::Illegal(reason) | Standing::Unplayable(reason) => Some(reason),
        }
    }

    /// A short badge: "Startup-legal", "Not Startup-legal", "Can't be played".
    pub fn badge(&self, format: NsgFormat) -> String {
        match self {
            Standing::Legal => format!("{}-legal", format_label(format)),
            Standing::Illegal(_) => format!("Not {}-legal", format_label(format)),
            Standing::Unplayable(_) => "Can't be played".to_string(),
        }
    }
}

/// A deck's standing in the format asked about, and every format it is
/// legal in — what a list row and the editor's verdict line show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckStatus {
    pub format: NsgFormat,
    pub standing: Standing,
    pub legal_in: Vec<NsgFormat>,
}

/// Checks `deck` against both validators in `format`, and in every other
/// format for [`DeckStatus::legal_in`].
pub fn status(deck: &DeckFile, book: CardBook, format: NsgFormat) -> DeckStatus {
    let standing = standing(deck, book, format);
    let legal_in = if matches!(standing, Standing::Unplayable(_)) {
        Vec::new()
    } else {
        FORMATS.into_iter().filter(|other| if *other == format { standing.is_legal() } else { deck.validate(book.registry, *other).is_ok() }).collect()
    };
    DeckStatus { format, standing, legal_in }
}

fn standing(deck: &DeckFile, book: CardBook, format: NsgFormat) -> Standing {
    match deck.validate(book.registry, format) {
        Ok(_) => Standing::Legal,
        Err(error @ (DeckError::UnknownCard(_) | DeckError::Unplayable(_))) => {
            Standing::Unplayable(readable(&error.to_string(), book))
        }
        Err(error @ (DeckError::StarterIdentity(_) | DeckError::Illegal(_))) => Standing::Illegal(readable(&error.to_string(), book)),
    }
}

/// A validator's message with its card ids replaced by titles. The
/// validators name cards as `CardId("hedge_fund")`, which is right for a
/// log and wrong for a person; the message is theirs, and only the names
/// are swapped.
pub fn readable(message: &str, book: CardBook) -> String {
    let mut out = String::new();
    let mut rest = message;
    while let Some(at) = rest.find("CardId(") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "CardId(".len()..];
        let Some(close) = after.find(')') else {
            out.push_str(&rest[at..]);
            return out;
        };
        let inner = after[..close].trim_matches('"');
        let title = book.get(&CardId(inner.to_string())).map(|card| card.title.clone());
        out.push_str(&title.unwrap_or_else(|| inner.to_string()));
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// A deck being edited. Every edit says whether it changed anything, so
/// the caller saves only when something did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub deck: DeckFile,
}

impl Draft {
    pub fn new(deck: DeckFile) -> Self {
        Self { deck }
    }

    pub fn copies(&self, id: &CardId) -> u32 {
        self.deck.cards.iter().filter(|entry| &entry.card == id).map(|entry| entry.count).sum()
    }

    /// One more copy of `card`, refused past its limit under the deck's
    /// `identity` (`CardDefinition::copy_limit_under`: its own
    /// `deck_limit`, else three, and one under Nova Initiumia or Ampère),
    /// for an identity, and for the other side's card.
    pub fn add(&mut self, card: &CardDefinition, identity: Option<&CardDefinition>) -> Result<(), String> {
        if card.card_type == CardType::Identity {
            return Err(format!("{} is an identity; change the deck's identity instead", card.title));
        }
        if card.side != self.deck.side {
            return Err(format!("{} is a {:?} card and this is a {:?} deck", card.title, card.side, self.deck.side));
        }
        let limit = card.copy_limit_under(identity, MAX_COPIES_PER_CARD);
        if self.copies(&card.id) >= limit {
            return Err(format!("{} is at its limit of {limit}", card.title));
        }
        match self.deck.cards.iter_mut().find(|entry| entry.card == card.id) {
            Some(entry) => entry.count += 1,
            None => self.deck.cards.push(DeckEntry { card: card.id.clone(), count: 1 }),
        }
        Ok(())
    }

    /// One copy fewer; `false` if the deck held none.
    pub fn remove(&mut self, id: &CardId) -> bool {
        let Some(entry) = self.deck.cards.iter_mut().find(|entry| &entry.card == id) else { return false };
        entry.count -= 1;
        self.deck.cards.retain(|entry| entry.count > 0);
        true
    }

    /// Every copy of `id` out.
    pub fn remove_all(&mut self, id: &CardId) -> bool {
        let before = self.deck.cards.len();
        self.deck.cards.retain(|entry| &entry.card != id);
        self.deck.cards.len() != before
    }

    /// A new identity of the deck's side. The cards stay: a person
    /// trying a list under another identity wants the list, and whatever
    /// the new identity makes illegal the verdict names.
    pub fn set_identity(&mut self, identity: &CardDefinition) -> Result<bool, String> {
        if identity.card_type != CardType::Identity || identity.side != self.deck.side {
            return Err(format!("{} is not a {:?} identity", identity.title, self.deck.side));
        }
        let changed = self.deck.identity != identity.id;
        self.deck.identity = identity.id.clone();
        Ok(changed)
    }

    /// A new name, trimmed and capped; the id — the file — stays, so a
    /// rename never leaves a second file behind.
    pub fn rename(&mut self, name: &str) -> bool {
        let name: String = name.trim().chars().take(MAX_NAME).collect();
        if name.is_empty() || name == self.deck.name {
            return false;
        }
        self.deck.name = name;
        true
    }

    pub fn set_style(&mut self, style: Vec<String>) -> bool {
        let changed = self.deck.style != style;
        self.deck.style = style;
        changed
    }

    /// Running totals against the identity's limits, where the list can
    /// be tallied at all (a catalog-only card has no numbers to count).
    pub fn tally(&self, book: CardBook) -> Option<DeckTally> {
        self.deck.tally(book.registry).ok()
    }
}

/// The deck's cards, one row per card, by type group then title — the
/// order a decklist is read in.
pub fn entries(deck: &DeckFile, book: CardBook) -> Vec<(CardId, u32)> {
    let mut rows: Vec<(CardId, u32)> = Vec::new();
    for entry in &deck.cards {
        match rows.iter_mut().find(|(id, _)| *id == entry.card) {
            Some((_, count)) => *count += entry.count,
            None => rows.push((entry.card.clone(), entry.count)),
        }
    }
    rows.sort_by_key(|(id, _)| {
        let card = book.get(id);
        (card.map_or(99, |card| type_order(&card.card_type)), card.map_or_else(|| id.0.clone(), |card| card.title.clone()))
    });
    rows
}

/// One type's rows in a decklist: "ICE (17)" and its cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// A `cards::type_group` name, or "Unknown" for a card no one knows.
    pub name: &'static str,
    pub total: u32,
    pub rows: Vec<(CardId, u32)>,
}

/// The deck's rows grouped under their type, for a list drawn in
/// sections.
pub fn grouped(deck: &DeckFile, book: CardBook) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (id, count) in entries(deck, book) {
        let name = book.get(&id).map_or("Unknown", |card| type_group(&card.card_type));
        match groups.iter_mut().find(|group| group.name == name) {
            Some(group) => {
                group.total += count;
                group.rows.push((id, count));
            }
            None => groups.push(Group { name, total: count, rows: vec![(id, count)] }),
        }
    }
    groups
}

/// The identities a deck of `side` may take, legal in `format` first
/// (by faction, then title) and the rest after, so a person building for
/// another format still finds theirs.
pub fn identities(registry: &CardRegistry, side: Side, format: NsgFormat) -> Vec<&CardDefinition> {
    let rules = format.rules();
    let mut identities: Vec<&CardDefinition> =
        registry.iter().filter(|card| card.card_type == CardType::Identity && card.side == side && card.is_playable).collect();
    identities.sort_by_key(|card| (!legal_in(card, rules), faction_order(card.faction), card.title.clone()));
    identities
}

/// The order the pool is listed in. Each key ends on the title, so the
/// order is total whatever the catalog's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolSort {
    /// Type, then faction — the order a decklist is read in.
    Type,
    Title,
    /// Faction, then type.
    Faction,
    /// The printed number in the top corner, lowest first: the play or
    /// install cost, and an agenda's advancement requirement.
    Cost,
    /// Influence, lowest first.
    Influence,
    /// Newest set first, then the card's number in its set — the set and
    /// number of its newest printing, whichever art a person chose for
    /// it: a sort is about where a card was printed, not how it looks.
    /// Newest first because that is how the Set filter lists them (the
    /// person's ask, 30 September 2026): the current set is where a deck
    /// is built from, the Core Set is the end of the list.
    Set,
}

impl PoolSort {
    pub const ALL: [PoolSort; 6] = [PoolSort::Type, PoolSort::Title, PoolSort::Faction, PoolSort::Cost, PoolSort::Influence, PoolSort::Set];

    pub fn label(self) -> &'static str {
        match self {
            PoolSort::Type => "Type",
            PoolSort::Title => "Title",
            PoolSort::Faction => "Faction",
            PoolSort::Cost => "Cost",
            PoolSort::Influence => "Influence",
            PoolSort::Set => "Set",
        }
    }
}

/// What the pool offers: one side's playable non-identity cards legal in
/// a format, narrowed by any of a set, a faction, a type group and a
/// search, in the order `sort` names.
///
/// **The format comes first, and a set is one of its sets.** A person
/// building a deck is building it for a format — the one Settings names
/// to start with — and a set outside that format has nothing they can
/// use, so "Legal in" is the leftmost filter and decides which sets the
/// Set filter offers ([`sets`]). There is no "any format": Casual lists
/// no pool, so it is already every card. The filter was "Only `<the
/// Settings format>`" or everything, then a format and a set as two
/// unrelated drop-downs, which let a Startup builder pick the Core Set
/// and see an empty pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolFilter {
    pub side: Side,
    /// Only cards this format's tables allow.
    pub format: NsgFormat,
    /// Only cards printed in this set (a v3 set id), by any printing;
    /// `None` is every set the format allows.
    pub set: Option<String>,
    pub faction: Option<Faction>,
    /// A `cards::type_group` name.
    pub kind: Option<&'static str>,
    pub query: String,
    pub sort: PoolSort,
}

impl PoolFilter {
    pub fn new(side: Side, format: NsgFormat) -> Self {
        Self { side, format, set: None, faction: None, kind: None, query: String::new(), sort: PoolSort::Type }
    }
}

/// Whether `card` is in the pool a deck of `side` is built from in a
/// format with `rules`: that side's, not an identity, one the engine
/// plays, and one the format allows. [`pool`] and [`sets`] ask this one
/// question, so the Set filter never offers a set the pool is empty for.
fn admitted(card: &CardDefinition, side: Side, rules: &netrunner_core::format::FormatRules) -> bool {
    card.side == side && card.card_type != CardType::Identity && card.is_playable && legal_in(card, rules)
}

/// The cards `filter` leaves, one entry per card, in the order
/// `filter.sort` names.
///
/// **Playable cards only.** The pool used to have a Show drop-down —
/// playable, not playable yet, every printing — and the person said
/// (30 September 2026) that playable "seems like the only reasonable
/// option to build a deck": a card the engine cannot deal is not one a
/// deck can be played with, and a deck that holds one anyway (an
/// import) still reads as unplayable and keeps the card. What the
/// engine has yet to build is the Cards screen's to say, which lists
/// every card.
///
/// A card is in a set's pool by any of its printings, so a reprint is
/// found by every set that printed it: Hedge Fund is in the Core Set's pool
/// and in System Gateway's. The catalog was one entry per printing until
/// NSG pool Stage 0d, and a catalog-only reprint was folded into the
/// playable card by title here.
pub fn pool<'a>(book: CardBook<'a>, filter: &PoolFilter) -> Vec<&'a CardDefinition> {
    let rules = filter.format.rules();
    let query = filter.query.trim().to_lowercase();
    let mut cards: Vec<&CardDefinition> = Vec::new();
    for card in book.catalog {
        if !admitted(card, filter.side, rules) {
            continue;
        }
        if filter.set.as_deref().is_some_and(|set| !catalog::printed_in(&card.id, set)) {
            continue;
        }
        if filter.faction.is_some_and(|faction| card.faction != Some(faction)) {
            continue;
        }
        if filter.kind.is_some_and(|kind| type_group(&card.card_type) != kind) {
            continue;
        }
        if !query.is_empty()
            && !card.title.to_lowercase().contains(&query)
            && !card.type_line.as_deref().is_some_and(|line| line.to_lowercase().contains(&query))
            && !card.printed_text.as_deref().is_some_and(|text| text.to_lowercase().contains(&query))
        {
            continue;
        }
        if cards.iter().any(|kept| kept.id == card.id) {
            continue;
        }
        cards.push(card);
    }
    let order = catalog::sets();
    let newest = |card: &CardDefinition| catalog::latest_printing(&card.id);
    let set_rank = |card: &CardDefinition| {
        newest(card).and_then(|printing| order.iter().position(|set| set.id == printing.set)).unwrap_or(usize::MAX)
    };
    let title = |card: &CardDefinition| card.title.to_lowercase();
    match filter.sort {
        PoolSort::Type => cards.sort_by_key(|card| (type_order(&card.card_type), faction_order(card.faction), title(card))),
        PoolSort::Title => cards.sort_by_key(|card| title(card)),
        PoolSort::Faction => cards.sort_by_key(|card| (faction_order(card.faction), type_order(&card.card_type), title(card))),
        PoolSort::Cost => cards.sort_by_key(|card| (printed_cost(card), title(card))),
        PoolSort::Influence => cards.sort_by_key(|card| (card.influence_cost.unwrap_or(0), title(card))),
        PoolSort::Set => cards.sort_by_key(|card| (set_rank(card), newest(card).map(|printing| printing.position), title(card))),
    }
    cards
}

/// The number in a card's top corner: an agenda's advancement
/// requirement, everything else's cost.
fn printed_cost(card: &CardDefinition) -> u32 {
    match card.card_type {
        CardType::Agenda => card.advancement_requirement.unwrap_or(card.cost),
        _ => card.cost,
    }
}

/// The sets the Set filter offers a deck of `side` built for `format`:
/// those with a card the pool would hold with no set chosen, newest
/// first (`cards::sets_of`). Startup's are its three; Casual's are every
/// set a playable card of the side was printed in.
pub fn sets(book: CardBook, side: Side, format: NsgFormat) -> Vec<String> {
    let rules = format.rules();
    crate::cards::sets_of(book.catalog.iter().filter(|card| admitted(card, side, rules)), Some(rules))
}

/// The factions a side's pool has cards of, in `faction_order`.
pub fn factions(book: CardBook, side: Side) -> Vec<Faction> {
    let mut factions: Vec<Faction> = book.catalog.iter().filter(|card| card.side == side && card.card_type != CardType::Identity).filter_map(|card| card.faction).collect();
    factions.sort_by_key(|faction| (faction_order(Some(*faction)), *faction as u8));
    factions.dedup();
    factions
}

/// The type groups a side's deck holds.
pub fn kinds(side: Side) -> &'static [&'static str] {
    match side {
        Side::Corp => &["Agenda", "Asset", "Upgrade", "Operation", "ICE"],
        Side::Runner => &["Event", "Hardware", "Resource", "Program"],
    }
}

/// A file-safe id from a display name — lowercase, words joined by `_` —
/// made unique against `taken` with a numeric suffix. The id is the
/// filename and what `--corp-deck` takes, so it has to be typeable; the
/// name is free text.
pub fn unique_id(name: &str, taken: &[String]) -> String {
    let mut slug = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_end_matches('_');
    let base = if slug.is_empty() { "deck".to_string() } else { slug.to_string() };
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}_{n}");
        n += 1;
    }
    id
}

/// An empty deck on `identity`, named `name`.
pub fn new_deck(name: &str, identity: &CardDefinition, taken: &[String]) -> DeckFile {
    DeckFile {
        id: unique_id(name, taken),
        name: name.trim().chars().take(MAX_NAME).collect(),
        side: identity.side,
        category: DeckCategory::Custom,
        description: None,
        how_to_play: None,
        style: Vec::new(),
        identity: identity.id.clone(),
        cards: Vec::new(),
    }
}

/// A copy of `from` under a new name. It keeps the list, its notes and
/// its style — the point of copying a published deck is to start from
/// all of it — and is `Custom`, because it is no longer the published
/// list.
pub fn copy_of(from: &DeckFile, name: &str, taken: &[String]) -> DeckFile {
    DeckFile { id: unique_id(name, taken), name: name.trim().chars().take(MAX_NAME).collect(), category: DeckCategory::Custom, ..from.clone() }
}

/// The name a copy is offered under: "Stolen Goods (copy)", and a number
/// after it when that name is already a deck's.
pub fn copy_name(from: &str, names: &[String]) -> String {
    let base = format!("{from} (copy)");
    let mut name = base.clone();
    let mut n = 2;
    while names.contains(&name) {
        name = format!("{from} (copy {n})");
        n += 1;
    }
    name
}

/// The deck as a plain-text decklist in the shape NetrunnerDB and
/// jinteki.net read: the name, the identity, then each type with its
/// count and "3x Title" lines.
pub fn export_text(deck: &DeckFile, book: CardBook) -> String {
    let mut out = format!("{}\n\n{}\n", deck.name, book.title(&deck.identity));
    for Group { name, total, rows } in grouped(deck, book) {
        out.push_str(&format!("\n{name} ({total})\n"));
        for (id, count) in rows {
            out.push_str(&format!("{count}x {}\n", book.title(&id)));
        }
    }
    out
}

/// What an import made, and what it could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub deck: DeckFile,
    /// Lines naming a card the catalog does not know, as written.
    pub skipped: Vec<String>,
}

/// Reads a decklist a person pasted or dropped: this project's deck file
/// (JSON), or a text list in any of the shapes NetrunnerDB, jinteki.net
/// and a hand-typed list use — "3x Hedge Fund", "3 Hedge Fund", "Hedge
/// Fund x3", with or without a set in brackets after the title and
/// influence dots.
///
/// **Nothing that can be read is refused.** A card the catalog does not
/// know is listed in [`Imported::skipped`]; a card it knows but the
/// engine does not play yet is kept, and the deck's standing says so. The
/// identity is the line that names one (the first, if two do); the name
/// is the first line that names nothing, else "Imported deck". Only a
/// list with no identity is an error, because a deck file cannot exist
/// without one — and even then the side is not a guess the importer
/// makes for the person.
///
/// The id is made unique against `taken`, and the category is `Custom`
/// whatever the text claimed: an imported deck is the person's.
pub fn import(text: &str, book: CardBook, taken: &[String]) -> Result<Imported, String> {
    let trimmed = text.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err("There is nothing to import: the text is empty".to_string());
    }
    if trimmed.starts_with('{') {
        let deck = DeckFile::from_json(trimmed).map_err(|e| format!("That is not a deck file: {e}"))?;
        let name = if deck.name.trim().is_empty() { "Imported deck".to_string() } else { deck.name.clone() };
        let deck = DeckFile { id: unique_id(&name, taken), name, category: DeckCategory::Custom, ..deck };
        return Ok(Imported { deck, skipped: Vec::new() });
    }

    let mut name: Option<String> = None;
    let mut identity: Option<&CardDefinition> = None;
    let mut cards: Vec<(&CardDefinition, u32)> = Vec::new();
    let mut skipped = Vec::new();
    for raw in trimmed.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Split first ("3 Hedge Fund"), then the whole line, so a title
        // that starts with a number ("15 Minutes") is still found.
        // A title is tried as written before its brackets come off,
        // because some titles carry them ("Maglectric Rapid (748 Mod)").
        let find = |title: &str| book.by_title(title).or_else(|| book.by_title(&clean_title(title)));
        let line = line.trim_end_matches(MARKS);
        let (count, card) = match split_count(line) {
            (Some(count), title) => match find(title) {
                Some(card) => (Some(count), Some(card)),
                None => (None, find(line)),
            },
            (None, title) => (None, find(title)),
        };
        let named_a_count = split_count(line).0.is_some();
        match card {
            Some(card) if card.card_type == CardType::Identity => {
                identity.get_or_insert(card);
            }
            Some(card) => {
                let count = count.unwrap_or(1);
                match cards.iter_mut().find(|(kept, _)| kept.id == card.id) {
                    Some((_, total)) => *total += count,
                    None => cards.push((card, count)),
                }
            }
            None if named_a_count => skipped.push(line.to_string()),
            // A line with no count that names no card: a heading
            // ("Event (10)", "Cards up to Elevation") or the deck's name,
            // which comes first.
            None => {
                if name.is_none() && identity.is_none() && cards.is_empty() && !is_heading(line) {
                    name = Some(line.chars().take(MAX_NAME).collect());
                }
            }
        }
    }
    let Some(identity) = identity else {
        return Err("No identity in that list: it needs a line naming the deck's identity".to_string());
    };
    for (card, _) in &cards {
        if card.side != identity.side {
            skipped.push(format!("{} (a {:?} card)", card.title, card.side));
        }
    }
    let name = name.unwrap_or_else(|| "Imported deck".to_string());
    let deck = DeckFile {
        cards: cards.into_iter().filter(|(card, _)| card.side == identity.side).map(|(card, count)| DeckEntry { card: card.id.clone(), count }).collect(),
        ..new_deck(&name, identity, taken)
    };
    Ok(Imported { deck, skipped })
}

/// A decklist as NetrunnerDB publishes one, before the catalog has seen
/// it: v3 card ids throughout, the identity among the cards as
/// `card_slots` lists it, the notes as HTML. Borrowed, so the fetcher's
/// own type (`netrunner_card_sync::Decklist`) is read without a copy and
/// this crate names no network type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Published<'a> {
    pub name: &'a str,
    /// The author's user name on NetrunnerDB; empty if unknown.
    pub author: &'a str,
    /// The decklist's page, kept with the deck so the credit travels.
    pub link: &'a str,
    pub notes: &'a str,
    /// The identity's card id.
    pub identity: &'a str,
    /// Each card's id with its copies.
    pub cards: &'a [(String, u32)],
}

/// Reads a decklist NetrunnerDB published (Phase 7 §10 Stage 6). The
/// ids are v3's and so is the catalog's, so a card is matched by id and
/// nothing else: a card the catalog does not know is listed in
/// [`Imported::skipped`] by its id, as the text import lists a line it
/// could not read, and a card of the other side is skipped by title. The
/// identity is the one refusal, as in [`import`]: a deck file cannot
/// exist without one, and NetrunnerDB names it apart from the cards.
///
/// The author and the link go in the one-line `description`, and the
/// notes, HTML made plain ([`plain_text`]), in `how_to_play` — the field
/// for prose about how the deck wants to be played, which is what a
/// published list's notes are. Credit for the list stays with the deck
/// that way; the file is still the person's own copy.
pub fn from_published(list: Published, book: CardBook, taken: &[String]) -> Result<Imported, String> {
    let identity_id = CardId(list.identity.to_string());
    let identity = match book.get(&identity_id) {
        Some(card) if card.card_type == CardType::Identity => card,
        Some(card) => return Err(format!("NetrunnerDB names {} as the identity, which is not one", card.title)),
        None => return Err(format!("NetrunnerDB names an identity this catalog does not know: {}", list.identity)),
    };
    let mut cards: Vec<DeckEntry> = Vec::new();
    let mut skipped = Vec::new();
    for (id, count) in list.cards {
        if *id == list.identity {
            continue;
        }
        match book.get(&CardId(id.clone())) {
            None => skipped.push(id.clone()),
            Some(card) if card.card_type == CardType::Identity => skipped.push(format!("{} (an identity)", card.title)),
            Some(card) if card.side != identity.side => skipped.push(format!("{} (a {:?} card)", card.title, card.side)),
            Some(card) => match cards.iter_mut().find(|entry| entry.card == card.id) {
                Some(entry) => entry.count += count,
                None => cards.push(DeckEntry { card: card.id.clone(), count: *count }),
            },
        }
    }
    let name = if list.name.trim().is_empty() { "NetrunnerDB deck" } else { list.name };
    let description = match (list.author.trim(), list.link.trim()) {
        ("", "") => None,
        ("", link) => Some(format!("From NetrunnerDB: {link}")),
        (author, "") => Some(format!("By {author} on NetrunnerDB")),
        (author, link) => Some(format!("By {author} on NetrunnerDB: {link}")),
    };
    let notes = plain_text(list.notes);
    let deck = DeckFile { cards, description, how_to_play: (!notes.is_empty()).then_some(notes), ..new_deck(name, identity, taken) };
    Ok(Imported { deck, skipped })
}

/// HTML as plain text: tags dropped, a block's end (or a `<br>`) a line
/// break, the common entities decoded, runs of blank lines collapsed to
/// one — so paragraphs stay apart and list items do not. NetrunnerDB
/// serves a decklist's notes as the HTML its editor saved, and a deck
/// file is read by people, in both clients, with no HTML renderer
/// anywhere.
pub fn plain_text(html: &str) -> String {
    const BLOCKS: [&str; 13] = ["p", "div", "li", "ul", "ol", "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "tr"];
    let mut text = String::new();
    let mut tag: Option<String> = None;
    for c in html.chars() {
        match (&mut tag, c) {
            (None, '<') => tag = Some(String::new()),
            (Some(inside), '>') => {
                let closing = inside.starts_with('/');
                let name = inside.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_lowercase();
                if name == "br" || (closing && BLOCKS.contains(&name.as_str())) {
                    text.push('\n');
                }
                tag = None;
            }
            (Some(inside), c) => inside.push(c),
            (None, c) => text.push(c),
        }
    }
    let decoded = decode_entities(&text);
    let mut lines: Vec<&str> = Vec::new();
    for line in decoded.lines().map(str::trim) {
        if line.is_empty() && lines.last().is_none_or(|last| last.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

/// `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&#39;`, `&apos;`, `&nbsp;` and the
/// numeric forms; anything else is left as written.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix('#')
                .and_then(|number| match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => number.parse().ok(),
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A list line's count and the rest: "3x Title", "3 Title", "Title x3",
/// "Title ×3". `None` for a line with no count.
fn split_count(line: &str) -> (Option<u32>, &str) {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &line[digits..];
        let rest = rest.strip_prefix(['x', 'X', '×']).unwrap_or(rest);
        if rest.starts_with(char::is_whitespace)
            && let Ok(count) = line[..digits].parse()
        {
            return (Some(count), rest.trim());
        }
    }
    if let Some((title, count)) = line.rsplit_once(char::is_whitespace) {
        let count = count.strip_prefix(['x', 'X', '×']).unwrap_or("");
        if let Ok(count) = count.parse() {
            return (Some(count), title.trim());
        }
    }
    (None, line)
}

/// The title alone: no set in brackets after it, no influence dots, no
/// trailing markers.
fn clean_title(title: &str) -> String {
    let mut title = title.trim().to_string();
    loop {
        let trimmed = title.trim_end_matches(MARKS).to_string();
        let trimmed = match trimmed.rfind(" (").or_else(|| trimmed.rfind(" [")) {
            Some(at) if trimmed.ends_with(')') || trimmed.ends_with(']') => trimmed[..at].trim_end().to_string(),
            _ => trimmed,
        };
        if trimmed == title {
            return title;
        }
        title = trimmed;
    }
}

/// What a list writes after a title that is not part of it: influence
/// dots, a star for a favourite, spaces.
const MARKS: [char; 7] = ['●', '•', '○', '◦', '★', '*', ' '];

/// "Event (10)", "ICE (17)", "Cards up to …", "Total …": the parts of an
/// exported list that name a section rather than a card or the deck.
fn is_heading(line: &str) -> bool {
    let lower = line.to_lowercase();
    let group = ["agenda", "asset", "upgrade", "operation", "ice", "event", "hardware", "resource", "program", "identity", "barrier", "code gate", "sentry"];
    let word = lower.split(" (").next().unwrap_or("").trim();
    group.iter().any(|g| word == *g || word == format!("{g}s")) || lower.starts_with("cards up to") || lower.starts_with("total") || lower.contains("influence")
}

/// A title as compared: lowercase, accents off, every apostrophe the
/// same and nothing but letters and digits. NetrunnerDB spells
/// *Tomorrowʼs Headline* with U+02BC and *Karunā* with a macron; a
/// person types neither.
fn fold(title: &str) -> String {
    title
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
            'è' | 'é' | 'ê' | 'ë' | 'ē' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ø' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            // Every apostrophe, U+02BC included, which is a letter to
            // `is_alphanumeric`.
            'ʼ' | '’' | '‘' => '\'',
            other => other,
        })
        .filter(|c| c.is_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::decks;

    fn registry() -> CardRegistry {
        crate::decks::sample_deck_registry()
    }

    fn catalog(registry: &CardRegistry) -> Vec<CardDefinition> {
        crate::cards::catalog(registry)
    }

    #[test]
    fn ids_are_typeable_and_unique() {
        assert_eq!(unique_id("Brick Stack 2!", &[]), "brick_stack_2");
        assert_eq!(unique_id("  Rush — HB  ", &[]), "rush_hb");
        assert_eq!(unique_id("!!!", &[]), "deck");
        assert_eq!(unique_id("Brick Stack", &["brick_stack".to_string(), "brick_stack_2".to_string()]), "brick_stack_3");
    }

    #[test]
    fn a_published_deck_is_legal_and_names_the_formats_it_is_legal_in() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        let deck = decks::by_id("stolen_goods").unwrap();
        let status = status(&deck, book, NsgFormat::Startup);
        assert_eq!(status.standing, Standing::Legal);
        assert!(status.legal_in.contains(&NsgFormat::Startup) && status.legal_in.contains(&NsgFormat::Eternal), "{:?}", status.legal_in);
        assert_eq!(status.standing.badge(NsgFormat::Startup), "Startup-legal");
    }

    /// Startup is whole: Vantage Point was the last of its three packs to
    /// be built (NSG pool, VP Stage 8), so every card of the catalog that
    /// Startup allows is one the engine plays, and every identity Startup
    /// allows is one a deck can take. A card added to Startup's pool by a
    /// re-sync fails here until it is built. Asked of the catalog, since
    /// the pool itself never shows an unplayable card.
    #[test]
    fn every_startup_card_is_playable() {
        let registry = registry();
        let catalog = catalog(&registry);
        let rules = NsgFormat::Startup.rules();
        for side in [Side::Corp, Side::Runner] {
            let waiting: Vec<&str> = catalog.iter().filter(|card| card.side == side && legal_in(card, rules) && !card.is_playable).map(|card| card.title.as_str()).collect();
            assert!(waiting.is_empty(), "{side:?} Startup cards not playable yet: {waiting:?}");
            let offered = identities(&registry, side, NsgFormat::Startup);
            let unoffered: Vec<&str> = catalog
                .iter()
                .filter(|card| card.card_type == CardType::Identity && card.side == side && legal_in(card, rules))
                .filter(|card| !offered.iter().any(|identity| identity.id == card.id))
                .map(|card| card.title.as_str())
                .collect();
            assert!(unoffered.is_empty(), "{side:?} Startup identities not offered: {unoffered:?}");
        }
    }

    /// A reprint is the card the engine plays: System Update 2021 prints
    /// The Maker’s Eye with a curly apostrophe, and its pool holds the Core
    /// Set's playable The Maker's Eye — no catalog-only copy beside it to
    /// fold by title, as there was while the catalog was one entry per
    /// printing.
    #[test]
    fn a_reprint_is_the_playable_card() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        let every = PoolFilter::new(Side::Runner, NsgFormat::Eternal);
        let su21 = pool(book, &PoolFilter { set: Some("system_update_2021".into()), ..every });
        let makers_eye: Vec<_> = su21.iter().filter(|card| card.title.contains("Maker")).collect();
        assert_eq!(makers_eye.len(), 1, "{makers_eye:?}");
        assert_eq!(makers_eye[0].id.0, "the_makers_eye");
        assert!(makers_eye[0].is_playable);
    }

    /// A short deck cannot be dealt; a Core Set card in a Startup deck
    /// can be dealt but breaks the format. Two answers a person acts on
    /// differently, and both name the card by its title.
    #[test]
    fn unplayable_and_illegal_are_told_apart_and_named_by_title() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        let mut short = decks::by_id("glyph_of_warding").unwrap();
        short.cards.truncate(3);
        assert!(matches!(standing(&short, book, NsgFormat::Startup), Standing::Unplayable(_)));

        // A Startup-legal list (Glyph of Warding), so the Ice Wall is the
        // one thing Startup refuses.
        let mut core = decks::by_id("glyph_of_warding").unwrap();
        let ice = core.cards.iter().position(|entry| matches!(registry.get(&entry.card).map(|card| &card.card_type), Some(CardType::Ice(_)))).unwrap();
        core.cards[ice] = DeckEntry { card: CardId("ice_wall".to_string()), count: core.cards[ice].count.min(3) };
        let Standing::Illegal(reason) = standing(&core, book, NsgFormat::Startup) else { panic!("a Core Set card is illegal in Startup") };
        assert!(reason.contains("Ice Wall") && !reason.contains("CardId"), "{reason}");
    }

    #[test]
    fn adding_stops_at_the_copy_limit_and_refuses_identities_and_the_other_side() {
        let registry = registry();
        let mut draft = Draft::new(new_deck("Mine", identities(&registry, Side::Runner, NsgFormat::Startup)[0], &[]));
        let sure_gamble = registry.get(&CardId("sure_gamble".into())).unwrap();
        for _ in 0..3 {
            draft.add(sure_gamble, None).unwrap();
        }
        assert!(draft.add(sure_gamble, None).unwrap_err().contains("limit of 3"));
        assert_eq!(draft.copies(&sure_gamble.id), 3);
        let hedge_fund = registry.get(&CardId("hedge_fund".into())).unwrap();
        assert!(draft.add(hedge_fund, None).is_err(), "a Corp card in a Runner deck");
        assert!(draft.remove(&sure_gamble.id));
        assert_eq!(draft.copies(&sure_gamble.id), 2);
        assert!(draft.remove_all(&sure_gamble.id));
        assert!(draft.deck.cards.is_empty());
        assert!(!draft.remove(&sure_gamble.id));
    }

    /// Nova Initiumia prints "Your deck cannot include more than 1 copy of
    /// any card", so the builder stops at one under it.
    #[test]
    fn adding_stops_at_one_copy_under_an_identity_that_says_so() {
        let registry = registry();
        let nova = registry.get(&CardId("nova_initiumia_catalyst_impetus".into())).unwrap();
        let mut draft = Draft::new(new_deck("Mine", nova, &[]));
        let sure_gamble = registry.get(&CardId("sure_gamble".into())).unwrap();
        draft.add(sure_gamble, Some(nova)).unwrap();
        assert!(draft.add(sure_gamble, Some(nova)).unwrap_err().contains("limit of 1"));
    }

    #[test]
    fn a_rename_keeps_the_id_and_ignores_a_blank() {
        let mut draft = Draft::new(decks::by_id("stolen_goods").unwrap());
        let id = draft.deck.id.clone();
        assert!(!draft.rename("   "));
        assert!(draft.rename("  My Goods "));
        assert_eq!((draft.deck.name.as_str(), draft.deck.id.as_str()), ("My Goods", id.as_str()));
    }

    #[test]
    fn the_pool_is_one_side_in_the_format_and_each_card_once() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        let startup = pool(book, &PoolFilter::new(Side::Corp, NsgFormat::Startup));
        assert!(startup.iter().all(|card| card.side == Side::Corp && card.card_type != CardType::Identity && card.is_playable));
        assert_eq!(startup.iter().filter(|card| card.title == "Hedge Fund").count(), 1, "a reprint is one card");
        assert!(!startup.iter().any(|card| card.id.0 == "ice_wall"), "Ice Wall is Core Set");
        let eternal = pool(book, &PoolFilter::new(Side::Corp, NsgFormat::Eternal));
        assert!(eternal.iter().any(|card| card.id.0 == "ice_wall"));
        let ice = pool(book, &PoolFilter { kind: Some("ICE"), query: "wall".into(), ..PoolFilter::new(Side::Corp, NsgFormat::Eternal) });
        assert!(!ice.is_empty() && ice.iter().all(|card| matches!(card.card_type, CardType::Ice(_))));
    }

    /// The pool is what a match can deal: never a card the engine does not
    /// play, in any format, on either side. Every card of the embedded
    /// catalog is built since System Update 2021 closed (tranche 8 Stage
    /// 11w), so today this holds trivially; it stops doing so the day a
    /// catalog sync brings in sets still to build, which is when it matters.
    #[test]
    fn the_pool_never_offers_a_card_the_engine_does_not_play() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        for format in NsgFormat::ALL {
            for side in [Side::Corp, Side::Runner] {
                let offered = pool(book, &PoolFilter::new(side, format));
                assert!(offered.iter().all(|card| card.is_playable), "{format:?} {side:?}");
                assert!(!offered.is_empty());
            }
        }
    }

    /// The Set filter offers the format's sets, newest first, and only
    /// those with a card the pool holds: Startup is its three packs, so a
    /// Startup builder is never offered the Core Set; Eternal is every set
    /// a playable card of the side was printed in, the Core Set last.
    #[test]
    fn the_sets_offered_are_the_formats_newest_first() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        for side in [Side::Corp, Side::Runner] {
            assert_eq!(sets(book, side, NsgFormat::Startup), vec!["vantage_point", "elevation", "system_gateway"], "{side:?}");
            let eternal = sets(book, side, NsgFormat::Eternal);
            assert_eq!(eternal.first().map(String::as_str), Some("vantage_point"), "{eternal:?}");
            assert_eq!(eternal.last().map(String::as_str), Some("core_set"), "{eternal:?}");
            assert!(eternal.contains(&"system_update_2021".to_string()), "a reprint puts its set on the list: {eternal:?}");
            // Downfall Stage 1 built Runner cards and Stage 2 Corp cards,
            // so both sides are offered the set.
            assert!(eternal.contains(&"downfall".to_string()), "{side:?}: {eternal:?}");
            // Midnight Sun is complete, and Uprising Stage 1 built cards of
            // both sides, so both are offered both sets.
            assert!(eternal.contains(&"midnight_sun".to_string()), "{side:?}: {eternal:?}");
            assert!(eternal.contains(&"uprising".to_string()), "{side:?}: {eternal:?}");
            let order = catalog::sets();
            let rank = |set: &String| order.iter().position(|known| known.id == *set).unwrap();
            assert!(eternal.windows(2).all(|pair| rank(&pair[0]) < rank(&pair[1])), "newest first: {eternal:?}");
            for set in &eternal {
                assert!(!pool(book, &PoolFilter { set: Some(set.clone()), ..PoolFilter::new(side, NsgFormat::Eternal) }).is_empty(), "{set} is offered with nothing behind it");
            }
            // Casual lists no pool, so it is every card, and offers one set
            // Eternal does not: Terminal Directive Cards, which no format's
            // set list names, though its cards are in both pools by their
            // other printings (FFG plan, Stage 0a-i).
            let casual = sets(book, side, NsgFormat::Casual);
            let beyond: Vec<&String> = casual.iter().filter(|set| !eternal.contains(set)).collect();
            assert_eq!(beyond, [&"terminal_directive_cards".to_string()], "{side:?}: Casual is every card");
            assert!(eternal.iter().all(|set| casual.contains(set)), "{side:?}: Casual is every card");
        }
    }

    /// A set narrows the pool to the cards it printed, reprints included,
    /// and every sort keeps the same cards; the set sort is newest first.
    #[test]
    fn the_pool_filters_by_set_and_sorts_every_way() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        let every = PoolFilter::new(Side::Corp, NsgFormat::Eternal);
        let core = pool(book, &PoolFilter { set: Some("core_set".into()), ..every.clone() });
        assert!(!core.is_empty() && core.iter().any(|card| card.id.0 == "ice_wall"));
        assert!(core.iter().any(|card| card.title == "Hedge Fund"), "a reprint is found by the set it was printed in");
        assert!(!pool(book, &PoolFilter { set: Some("elevation".into()), ..every.clone() }).iter().any(|card| card.id.0 == "ice_wall"));

        let all = pool(book, &every);
        let ids = |cards: &[&CardDefinition]| {
            let mut ids: Vec<String> = cards.iter().map(|card| card.id.0.clone()).collect();
            ids.sort();
            ids
        };
        for sort in PoolSort::ALL {
            let sorted = pool(book, &PoolFilter { sort, ..every.clone() });
            assert_eq!(ids(&sorted), ids(&all), "{sort:?} keeps the same cards");
        }
        let by_cost = pool(book, &PoolFilter { sort: PoolSort::Cost, ..every.clone() });
        assert!(by_cost.windows(2).all(|pair| printed_cost(pair[0]) <= printed_cost(pair[1])));
        let by_title = pool(book, &PoolFilter { sort: PoolSort::Title, ..every.clone() });
        assert!(by_title.windows(2).all(|pair| pair[0].title.to_lowercase() <= pair[1].title.to_lowercase()));
        let by_set = pool(book, &PoolFilter { sort: PoolSort::Set, ..every });
        let order = catalog::sets();
        let rank = |card: &CardDefinition| {
            catalog::latest_printing(&card.id).and_then(|printing| order.iter().position(|set| set.id == printing.set)).unwrap_or(usize::MAX)
        };
        assert!(by_set.windows(2).all(|pair| rank(pair[0]) <= rank(pair[1])));
        assert!(catalog::printed_in(&by_set[0].id, "vantage_point"), "the newest set leads: {}", by_set[0].title);
        assert!(catalog::latest_printing(&by_set[by_set.len() - 1].id).is_some_and(|printing| printing.set == "core_set"), "the Core Set ends it: {}", by_set[by_set.len() - 1].title);
    }

    #[test]
    fn an_exported_list_imports_back_to_the_same_deck() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        for deck in decks::embedded_decks().into_iter().filter(|deck| deck.category == DeckCategory::Sample) {
            let text = export_text(&deck, book);
            let imported = import(&text, book, &[]).unwrap_or_else(|e| panic!("{}: {e}\n{text}", deck.id));
            assert!(imported.skipped.is_empty(), "{}: {:?}", deck.id, imported.skipped);
            assert_eq!(imported.deck.name, deck.name);
            assert_eq!(imported.deck.identity, deck.identity);
            assert_eq!(entries(&imported.deck, book), entries(&deck, book), "{}", deck.id);
            assert_eq!(imported.deck.category, DeckCategory::Custom);
        }
    }

    /// The shapes lists arrive in from elsewhere: NetrunnerDB's text
    /// export with sets and influence dots, jinteki.net's bare counts, a
    /// trailing count, a card typed without its accent, and a card the
    /// catalog has never heard of.
    #[test]
    fn a_pasted_list_reads_every_common_shape_and_skips_only_what_it_cannot_name() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        let identity = book.get(&decks::by_id("stolen_goods").unwrap().identity).unwrap().title.clone();
        let text = format!(
            "Borrowed Time\n\n{identity} (sg)\n\nEvent (6)\n3x Sure Gamble (sg)\n2 Sure Gamble\nOverclock x2 ●●\n\nResource (1)\n1x Not A Real Card\nCards up to Elevation\n"
        );
        let imported = import(&text, book, &["borrowed_time".to_string()]).unwrap();
        assert_eq!(imported.deck.name, "Borrowed Time");
        assert_eq!(imported.deck.id, "borrowed_time_2");
        assert_eq!(imported.deck.side, Side::Runner);
        let draft = Draft::new(imported.deck.clone());
        assert_eq!(draft.copies(&CardId("sure_gamble".into())), 5, "two lines of one card add up; the validator judges the total");
        assert_eq!(draft.copies(&CardId("overclock".into())), 2);
        assert_eq!(imported.skipped, vec!["1x Not A Real Card".to_string()]);
    }

    #[test]
    fn a_list_without_an_identity_is_the_one_refusal() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        assert!(import("3x Sure Gamble", book, &[]).unwrap_err().contains("identity"));
        assert!(import("   ", book, &[]).is_err());
    }

    #[test]
    fn a_deck_file_imports_as_the_persons_own_under_a_free_id() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        let published = decks::by_id("stolen_goods").unwrap();
        let imported = import(&published.to_json().unwrap(), book, &["stolen_goods".to_string()]).unwrap();
        assert_eq!(imported.deck.id, "stolen_goods_2");
        assert_eq!(imported.deck.category, DeckCategory::Custom);
        assert_eq!(imported.deck.cards, published.cards);
    }

    /// A published list is matched by id: the identity named apart from
    /// the cards is the deck's and its slot is not a card, an id the
    /// catalog does not know is skipped by id, a card of the other side
    /// by title, and the author, the link and the notes travel with the
    /// deck as its description and its notes.
    #[test]
    fn a_published_list_is_read_by_id_and_keeps_its_credit() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        let published = decks::by_id("stolen_goods").unwrap();
        let identity = published.identity.0.clone();
        let cards = vec![(identity.clone(), 1), ("sure_gamble".to_string(), 3), ("hedge_fund".to_string(), 2), ("no_such_card".to_string(), 1), ("overclock".to_string(), 1)];
        let list = Published {
            name: "Seb's Zahya",
            author: "seb",
            link: "https://netrunnerdb.com/en/decklist/1b98e609-0000-0000-0000-000000000000",
            notes: "<p>Run <em>early</em> &amp; often.</p>\n<p>Then money.</p>",
            identity: &identity,
            cards: &cards,
        };
        let imported = from_published(list, book, &["seb_s_zahya".to_string()]).unwrap();
        assert_eq!(imported.deck.identity, published.identity);
        assert_eq!(imported.deck.side, Side::Runner);
        assert_eq!(imported.deck.id, "seb_s_zahya_2");
        assert_eq!(imported.deck.category, DeckCategory::Custom);
        let draft = Draft::new(imported.deck.clone());
        assert_eq!(draft.copies(&CardId("sure_gamble".into())), 3);
        assert_eq!(draft.copies(&CardId("overclock".into())), 1);
        assert_eq!(draft.copies(&published.identity), 0, "the identity's slot is not a card");
        assert_eq!(imported.skipped, vec!["Hedge Fund (a Corp card)".to_string(), "no_such_card".to_string()]);
        assert_eq!(imported.deck.description.as_deref(), Some("By seb on NetrunnerDB: https://netrunnerdb.com/en/decklist/1b98e609-0000-0000-0000-000000000000"));
        assert_eq!(imported.deck.how_to_play.as_deref(), Some("Run early & often.\n\nThen money."));

        let unnamed = from_published(Published { name: " ", author: "", link: "", notes: "", ..list }, book, &[]).unwrap();
        assert_eq!(unnamed.deck.name, "NetrunnerDB deck");
        assert_eq!((unnamed.deck.description, unnamed.deck.how_to_play), (None, None));
        let unknown = from_published(Published { identity: "nobody", ..list }, book, &[]).unwrap_err();
        assert!(unknown.contains("nobody"), "{unknown}");
        let not_one = from_published(Published { identity: "sure_gamble", ..list }, book, &[]).unwrap_err();
        assert!(not_one.contains("Sure Gamble"), "{not_one}");
    }

    #[test]
    fn html_notes_read_as_plain_text() {
        assert_eq!(plain_text("<p>One</p>\n<p>Two<br>three</p><ul><li>a</li><li>b</li></ul>"), "One\n\nTwo\nthree\na\nb", "paragraphs apart, list items not");
        assert_eq!(plain_text("<p>One</p><p>Two</p>"), "One\nTwo", "a blank line only where the source had one");
        assert_eq!(plain_text("A &lt;b&gt; &quot;c&quot; &#39;d&#39; &#x41;&#66; &nbsp;e &unknown; & f"), "A <b> \"c\" 'd' AB  e &unknown; & f");
        assert_eq!(plain_text("  \n\n <div></div> \n"), "");
        assert_eq!(plain_text("plain words"), "plain words");
    }

    #[test]
    fn a_copy_is_custom_and_its_name_does_not_repeat() {
        let published = decks::by_id("stolen_goods").unwrap();
        let names = vec![format!("{} (copy)", published.name)];
        let name = copy_name(&published.name, &names);
        assert_eq!(name, format!("{} (copy 2)", published.name));
        let copy = copy_of(&published, &name, std::slice::from_ref(&published.id));
        assert_eq!(copy.category, DeckCategory::Custom);
        assert_eq!(copy.cards, published.cards);
        assert_ne!(copy.id, published.id);
    }

    #[test]
    fn titles_fold_accents_and_apostrophes() {
        assert_eq!(fold("Tomorrowʼs Headline"), fold("tomorrow's headline"));
        assert_eq!(fold("Karunā"), "karuna");
        assert_eq!(clean_title("Hedge Fund (sg) ●●"), "Hedge Fund");
        assert_eq!(split_count("3x Hedge Fund"), (Some(3), "Hedge Fund"));
        assert_eq!(split_count("Hedge Fund x2"), (Some(2), "Hedge Fund"));
        assert_eq!(split_count("Hedge Fund"), (None, "Hedge Fund"));
        assert_eq!(split_count("15 Minutes"), (Some(15), "Minutes"), "a count first is a count; `import` then tries the whole line");
    }
}
