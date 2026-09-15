//! The card browser's state: which of the catalog is showing, and which
//! card is open.
//!
//! Filters compose — a side, a faction, a type group, the format's pool,
//! a search — and every intent that changes them re-derives what is
//! visible; the selection survives a change that still shows it and
//! falls to the first visible card on one that does not, so the
//! inspector never shows a card the grid does not and is never empty
//! while the grid is not. The search is a case-insensitive substring over the
//! title, the type line and the printed text, because "net damage" is
//! how a person looks for Jinteki's ice.

use std::sync::Arc;

use netrunner_client::cards::{faction_order, legal_in, type_group, type_order};
use netrunner_core::card::{CardId, Faction};
use netrunner_core::dsl::CardDefinition;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Side(Option<Side>),
    Faction(Option<Faction>),
    /// A `cards::type_group` name, or every type.
    Kind(Option<&'static str>),
    /// Only the format's pool, or every printing.
    ToggleFormat,
    Query(String),
    Select(Option<CardId>),
    /// Every filter off, the search empty; the selection stays.
    Clear,
}

pub struct Browser {
    cards: Arc<Vec<CardDefinition>>,
    pub side: Option<Side>,
    pub faction: Option<Faction>,
    pub kind: Option<&'static str>,
    /// The format the toggle narrows to — the player's setting, Startup
    /// when none is set.
    pub format: NsgFormat,
    pub format_only: bool,
    pub query: String,
    pub selected: Option<CardId>,
}

impl Browser {
    pub fn new(cards: Arc<Vec<CardDefinition>>, format: Option<NsgFormat>) -> Self {
        let selected = cards.first().and_then(|card| card.numeric_id);
        Browser {
            cards,
            side: None,
            faction: None,
            kind: None,
            format: format.unwrap_or(NsgFormat::Startup),
            format_only: false,
            query: String::new(),
            selected,
        }
    }

    /// Applies `intent`; `true` if anything changed.
    pub fn apply(&mut self, intent: Intent) -> bool {
        let before = self.snapshot();
        match intent {
            Intent::Side(side) => {
                self.side = side;
                if let (Some(side), Some(faction)) = (side, self.faction)
                    && !is_neutral(faction)
                    && faction_side(faction) != side
                {
                    self.faction = None;
                }
            }
            Intent::Faction(faction) => {
                self.faction = faction;
                if let Some(faction) = faction
                    && !is_neutral(faction)
                {
                    self.side = Some(faction_side(faction));
                }
            }
            Intent::Kind(kind) => self.kind = kind,
            Intent::ToggleFormat => self.format_only = !self.format_only,
            Intent::Query(query) => self.query = query,
            Intent::Select(code) => self.selected = code,
            Intent::Clear => {
                self.side = None;
                self.faction = None;
                self.kind = None;
                self.format_only = false;
                self.query.clear();
            }
        }
        let visible = self.visible();
        if !self.selected.is_some_and(|code| visible.iter().any(|card| card.numeric_id == Some(code))) {
            self.selected = visible.first().and_then(|card| card.numeric_id);
        }
        self.snapshot() != before
    }

    fn snapshot(&self) -> (Option<Side>, Option<Faction>, Option<&'static str>, bool, String, Option<CardId>) {
        (self.side, self.faction, self.kind, self.format_only, self.query.clone(), self.selected)
    }

    /// The cards the filters leave, in catalog order.
    pub fn visible(&self) -> Vec<&CardDefinition> {
        let rules = self.format.rules();
        let query = self.query.trim().to_lowercase();
        self.cards
            .iter()
            .filter(|card| self.side.is_none_or(|side| card.side == side))
            .filter(|card| self.faction.is_none_or(|faction| card.faction.is_some_and(|f| f == faction || (is_neutral(f) && is_neutral(faction)))))
            .filter(|card| self.kind.is_none_or(|kind| type_group(&card.card_type) == kind))
            .filter(|card| !self.format_only || legal_in(card, &rules))
            .filter(|card| query.is_empty() || matches(card, &query))
            .collect()
    }

    pub fn selected(&self) -> Option<&CardDefinition> {
        let code = self.selected?;
        self.cards.iter().find(|card| card.numeric_id == Some(code))
    }

    pub fn total(&self) -> usize {
        self.cards.len()
    }

    /// The factions a chip is offered for: the side's, or both sides'
    /// when no side is chosen, in `faction_order`. The two neutrals are
    /// one chip — both are labelled "Neutral", and a filter on either
    /// matches both, so the side chips decide which.
    pub fn factions(&self) -> Vec<Faction> {
        let mut factions: Vec<Faction> = self.cards.iter().filter(|card| self.side.is_none_or(|side| card.side == side)).filter_map(|card| card.faction).collect();
        factions.sort_by_key(|faction| (faction_order(Some(*faction)), *faction as u8));
        factions.dedup();
        if factions.contains(&Faction::NeutralCorp) {
            factions.retain(|faction| *faction != Faction::NeutralRunner);
        }
        factions
    }

    /// The type groups present for the side, in `type_order`.
    pub fn kinds(&self) -> Vec<&'static str> {
        let mut kinds: Vec<(u8, &'static str)> =
            self.cards.iter().filter(|card| self.side.is_none_or(|side| card.side == side)).map(|card| (type_order(&card.card_type), type_group(&card.card_type))).collect();
        kinds.sort();
        kinds.dedup();
        kinds.into_iter().map(|(_, kind)| kind).collect()
    }
}

fn matches(card: &CardDefinition, query: &str) -> bool {
    card.title.to_lowercase().contains(query)
        || card.type_line.as_deref().is_some_and(|line| line.to_lowercase().contains(query))
        || card.printed_text.as_deref().is_some_and(|text| text.to_lowercase().contains(query))
}

fn is_neutral(faction: Faction) -> bool {
    matches!(faction, Faction::NeutralCorp | Faction::NeutralRunner)
}

pub fn faction_side(faction: Faction) -> Side {
    match faction {
        Faction::Anarch | Faction::Criminal | Faction::Shaper | Faction::NeutralRunner => Side::Runner,
        Faction::HaasBioroid | Faction::Jinteki | Faction::Nbn | Faction::WeylandConsortium | Faction::NeutralCorp => Side::Corp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser() -> Browser {
        let registry = netrunner_client::decks::sample_deck_registry();
        Browser::new(Arc::new(netrunner_client::cards::catalog(&registry)), None)
    }

    fn code(browser: &Browser, title: &str) -> CardId {
        browser.visible().iter().find(|card| card.title == title).and_then(|card| card.numeric_id).unwrap()
    }

    #[test]
    fn no_filter_shows_the_whole_catalog_and_each_filter_narrows_it() {
        let mut b = browser();
        let all = b.visible().len();
        assert_eq!(all, b.total());
        assert!(b.apply(Intent::Side(Some(Side::Corp))));
        let corp = b.visible().len();
        assert!(corp < all && b.visible().iter().all(|card| card.side == Side::Corp));
        assert!(b.apply(Intent::Kind(Some("ICE"))));
        assert!(b.visible().iter().all(|card| matches!(card.card_type, netrunner_core::dsl::CardType::Ice(_))));
        assert!(b.apply(Intent::Faction(Some(Faction::Jinteki))));
        assert!(b.visible().iter().all(|card| card.faction == Some(Faction::Jinteki)));
        assert!(!b.apply(Intent::Faction(Some(Faction::Jinteki))), "the same filter again changes nothing");
        assert!(b.apply(Intent::Clear));
        assert_eq!(b.visible().len(), all);
    }

    #[test]
    fn a_faction_implies_its_side_and_a_side_drops_a_foreign_faction() {
        let mut b = browser();
        b.apply(Intent::Faction(Some(Faction::Anarch)));
        assert_eq!(b.side, Some(Side::Runner));
        b.apply(Intent::Side(Some(Side::Corp)));
        assert_eq!(b.faction, None);
        assert!(b.factions().iter().all(|faction| faction_side(*faction) == Side::Corp));
        assert!(b.kinds().contains(&"Agenda") && !b.kinds().contains(&"Program"));
    }

    /// Both neutrals are one chip, and it matches both sides' neutral
    /// cards until a side is chosen.
    #[test]
    fn neutral_is_one_chip_that_matches_both_sides() {
        let mut b = browser();
        let neutrals: Vec<_> = b.factions().into_iter().filter(|f| is_neutral(*f)).collect();
        assert_eq!(neutrals, vec![Faction::NeutralCorp]);
        b.apply(Intent::Faction(Some(Faction::NeutralCorp)));
        assert_eq!(b.side, None, "a neutral chip does not pick a side");
        let visible = b.visible();
        assert!(visible.iter().any(|card| card.side == Side::Corp) && visible.iter().any(|card| card.side == Side::Runner), "neutral cards of both sides");
        b.apply(Intent::Side(Some(Side::Runner)));
        assert_eq!(b.faction, Some(Faction::NeutralCorp), "the neutral filter survives a side");
        assert!(b.visible().iter().all(|card| card.faction == Some(Faction::NeutralRunner)));
    }

    #[test]
    fn the_search_reads_the_printed_text_not_only_the_title() {
        let mut b = browser();
        b.apply(Intent::Query("NET DAMAGE".to_string()));
        let hits = b.visible();
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|card| !card.title.to_lowercase().contains("net damage")), "a hit by text alone");
        assert!(hits.iter().all(|card| card.printed_text.as_deref().unwrap_or("").to_lowercase().contains("net damage")
            || card.title.to_lowercase().contains("net damage")
            || card.type_line.as_deref().unwrap_or("").to_lowercase().contains("net damage")));
    }

    #[test]
    fn the_selection_survives_a_filter_that_shows_it_and_falls_to_the_first_card_on_one_that_does_not() {
        let mut b = browser();
        assert!(b.selected().is_some(), "the first card is open from the start");
        let tithe = code(&b, "Tithe");
        assert!(b.apply(Intent::Select(Some(tithe))));
        assert_eq!(b.selected().map(|card| card.title.as_str()), Some("Tithe"));
        b.apply(Intent::Side(Some(Side::Corp)));
        assert_eq!(b.selected, Some(tithe), "Tithe is Corp");
        b.apply(Intent::Side(Some(Side::Runner)));
        assert_ne!(b.selected, Some(tithe), "no Runner card is Tithe");
        assert_eq!(b.selected().map(|card| card.side), Some(Side::Runner), "the first Runner card is open instead");
        b.apply(Intent::Query("zzzz no such card".to_string()));
        assert_eq!(b.selected, None, "nothing visible, nothing open");
    }

    #[test]
    fn the_format_toggle_keeps_only_the_pool() {
        let mut b = browser();
        let all = b.visible().len();
        b.apply(Intent::ToggleFormat);
        let startup = b.visible().len();
        assert!(startup < all, "the Core Set is outside Startup");
        assert!(b.visible().iter().all(|card| matches!(card.set_code.as_deref(), Some("sg" | "elev"))));
    }
}
