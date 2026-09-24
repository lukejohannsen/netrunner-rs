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
//! lists every printing, and one with no gameplay data has the id
//! `nrdb_<code>` (`cards::netrunnerdb`). A deck may hold one — an import
//! from a list built elsewhere, or a card added with the pool's "not
//! playable yet" cards shown — and reads as [`Standing::Unplayable`] until
//! the card is implemented, when [`Draft::resolve`] swaps the id for the
//! playable card's.
//!
//! Lifted out of the terminal's builder so the desktop's and the
//! terminal's are one set of rules with two faces, as `start` was for the
//! new-game form.

use netrunner_core::card::Faction;
use netrunner_core::cards::CardRegistry;
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
/// catalog's printings for what the registry does not hold. One lookup,
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

    /// The card a title names, the playable one first — a reprint's
    /// printing that the playable card does not stand in for is a
    /// catalog-only entry with the same title, and a list naming the
    /// title means the card, not the printing.
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
        Err(error @ (DeckError::UnknownCard(_) | DeckError::NoPrintedMetadata(_) | DeckError::Unplayable(_))) => {
            Standing::Unplayable(readable(&error.to_string(), book))
        }
        Err(error @ (DeckError::StarterIdentity(_) | DeckError::Illegal(_))) => Standing::Illegal(readable(&error.to_string(), book)),
    }
}

/// A validator's message with its card ids replaced by titles. The
/// validators name cards as `CardId("hedge_fund")` or `CardId(30002)`,
/// which is right for a log and wrong for a person; the message is
/// theirs, and only the names are swapped.
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
        let title = match inner.parse::<u32>() {
            Ok(code) => book.registry.get_by_numeric_id(netrunner_core::card::CardId(code)).map(|card| card.title.clone()).or_else(|| {
                book.catalog.iter().find(|card| card.numeric_id == Some(netrunner_core::card::CardId(code))).map(|card| card.title.clone())
            }),
            Err(_) => book.get(&CardId(inner.to_string())).map(|card| card.title.clone()),
        };
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

    /// One more copy of `card`, refused past its limit (`deck_limit`, else
    /// three), for an identity, and for the other side's card.
    pub fn add(&mut self, card: &CardDefinition) -> Result<(), String> {
        if card.card_type == CardType::Identity {
            return Err(format!("{} is an identity; change the deck's identity instead", card.title));
        }
        if card.side != self.deck.side {
            return Err(format!("{} is a {:?} card and this is a {:?} deck", card.title, card.side, self.deck.side));
        }
        let limit = card.deck_limit.unwrap_or(MAX_COPIES_PER_CARD);
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

    pub fn set_style(&mut self, style: Option<String>) -> bool {
        let changed = self.deck.style != style;
        self.deck.style = style;
        changed
    }

    /// Swaps each catalog-only id (`nrdb_<code>`) for the playable card
    /// that now implements the printing, so a deck imported before a card
    /// was implemented plays once it is. `true` if any id changed.
    pub fn resolve(&mut self, book: CardBook) -> bool {
        let mut changed = false;
        let swap = |id: &mut CardId, changed: &mut bool| {
            let Some(printing) = book.catalog.iter().find(|card| &card.id == id) else { return };
            if book.is_playable(id) {
                return;
            }
            let playable = printing
                .numeric_id
                .and_then(|code| book.registry.get_by_numeric_id(code))
                .filter(|card| card.is_playable)
                .or_else(|| book.registry.iter().find(|card| card.is_playable && card.side == printing.side && card.title == printing.title));
            if let Some(card) = playable {
                *id = card.id.clone();
                *changed = true;
            }
        };
        swap(&mut self.deck.identity, &mut changed);
        for entry in &mut self.deck.cards {
            swap(&mut entry.card, &mut changed);
        }
        if changed {
            let mut merged: Vec<DeckEntry> = Vec::new();
            for entry in std::mem::take(&mut self.deck.cards) {
                match merged.iter_mut().find(|kept| kept.card == entry.card) {
                    Some(kept) => kept.count += entry.count,
                    None => merged.push(entry),
                }
            }
            self.deck.cards = merged;
        }
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
        registry.iter().filter(|card| card.card_type == CardType::Identity && card.side == side && card.is_playable && card.numeric_id.is_some()).collect();
    identities.sort_by_key(|card| (!legal_in(card, &rules), faction_order(card.faction), card.title.clone()));
    identities
}

/// What the pool offers: one side's non-identity cards, optionally
/// narrowed to a format's pool, and with or without the printings the
/// engine does not play yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolFilter {
    pub side: Side,
    pub format: Option<NsgFormat>,
    pub faction: Option<Faction>,
    /// A `cards::type_group` name.
    pub kind: Option<&'static str>,
    pub query: String,
    /// Include catalog printings no match can deal yet.
    pub unplayable: bool,
}

impl PoolFilter {
    pub fn new(side: Side, format: NsgFormat) -> Self {
        Self { side, format: Some(format), faction: None, kind: None, query: String::new(), unplayable: false }
    }
}

/// The cards `filter` leaves, one entry per card — the playable card
/// once, however many printings it stands in for, and a catalog-only
/// printing only where no playable card has its title — by type, then
/// faction, then title.
pub fn pool<'a>(book: CardBook<'a>, filter: &PoolFilter) -> Vec<&'a CardDefinition> {
    let rules = filter.format.map(|format| format.rules());
    let query = filter.query.trim().to_lowercase();
    let mut cards: Vec<&CardDefinition> = Vec::new();
    for card in book.catalog {
        if card.side != filter.side || card.card_type == CardType::Identity {
            continue;
        }
        // A printing no match can deal is offered only when asked for,
        // and never where a playable card has its title (a reprint).
        if !card.is_playable && (!filter.unplayable || book.registry.iter().any(|playable| playable.is_playable && playable.side == card.side && playable.title == card.title)) {
            continue;
        }
        if cards.iter().any(|kept| kept.id == card.id) {
            continue;
        }
        if rules.as_ref().is_some_and(|rules| !legal_in(card, rules)) {
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
        cards.push(card);
    }
    cards.sort_by_key(|card| (type_order(&card.card_type), faction_order(card.faction), card.title.clone()));
    cards
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
        style: None,
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
        let mut deck = DeckFile { id: unique_id(&name, taken), name, category: DeckCategory::Custom, ..deck };
        let mut draft = Draft::new(deck.clone());
        draft.resolve(book);
        deck = draft.deck;
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

    /// A short deck cannot be dealt; a Core Set card in a Startup deck
    /// can be dealt but breaks the format. Two answers a person acts on
    /// differently, and both name the card by its title.
    #[test]
    fn unplayable_and_illegal_are_told_apart_and_named_by_title() {
        let registry = registry();
        let catalog = catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        let mut short = decks::by_id("discretion_advised").unwrap();
        short.cards.truncate(3);
        assert!(matches!(standing(&short, book, NsgFormat::Startup), Standing::Unplayable(_)));

        let mut core = decks::by_id("discretion_advised").unwrap();
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
            draft.add(sure_gamble).unwrap();
        }
        assert!(draft.add(sure_gamble).unwrap_err().contains("limit of 3"));
        assert_eq!(draft.copies(&sure_gamble.id), 3);
        let hedge_fund = registry.get(&CardId("hedge_fund".into())).unwrap();
        assert!(draft.add(hedge_fund).is_err(), "a Corp card in a Runner deck");
        assert!(draft.remove(&sure_gamble.id));
        assert_eq!(draft.copies(&sure_gamble.id), 2);
        assert!(draft.remove_all(&sure_gamble.id));
        assert!(draft.deck.cards.is_empty());
        assert!(!draft.remove(&sure_gamble.id));
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
        let eternal = pool(book, &PoolFilter { format: None, ..PoolFilter::new(Side::Corp, NsgFormat::Startup) });
        assert!(eternal.iter().any(|card| card.id.0 == "ice_wall"));
        let ice = pool(book, &PoolFilter { kind: Some("ICE"), query: "wall".into(), format: None, ..PoolFilter::new(Side::Corp, NsgFormat::Startup) });
        assert!(!ice.is_empty() && ice.iter().all(|card| matches!(card.card_type, CardType::Ice(_))));
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
