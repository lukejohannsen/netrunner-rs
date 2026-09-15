//! The printed card as a face lays it out: which number sits in which
//! corner, for a renderer that draws corners.
//!
//! A Netrunner card puts its numbers in fixed places — the cost top-left,
//! an agenda's advancement requirement where a cost would be, strength
//! bottom-left on ice and icebreakers, memory and trash cost bottom-right,
//! influence as a row of pips — and a player reads a face by where a
//! number is before reading what it is. Those positions are a fact about
//! the printed card, not about any toolkit, so they are decided here and
//! tested under plain `cargo test`; a renderer draws a [`Face`]'s slots
//! where it is told and never asks what type a card is.
//!
//! **Only what is printed.** `CardDefinition::cost` is `0` for a card
//! with no cost, which the catalog conversion cannot tell from a printed
//! 0; the face shows a cost on every card that prints one (everything
//! but agendas and identities), 0 included, because the card does.

use netrunner_core::card::{CardId, Faction};
use netrunner_core::dsl::{CardDefinition, CardType};
use netrunner_core::rules::Side;

use crate::card_text::{self, Segment};

/// One printed number, with what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Play, install or rez cost — the top-left circle.
    Cost(u32),
    Strength(i32),
    /// An agenda's advancement requirement, where a cost would sit.
    Advancement(u32),
    AgendaPoints(u32),
    TrashCost(u32),
    Memory(u32),
    /// An identity's minimum deck size.
    DeckSize(u32),
    /// An identity's influence limit; `None` prints as ∞ (unlimited).
    InfluenceLimit(Option<u32>),
    /// A Runner identity's base link.
    Link(u32),
}

impl Slot {
    /// The number as the card prints it.
    pub fn value(self) -> String {
        match self {
            Slot::Cost(n) | Slot::Advancement(n) | Slot::AgendaPoints(n) | Slot::TrashCost(n) | Slot::Memory(n) | Slot::DeckSize(n) | Slot::Link(n) => {
                n.to_string()
            }
            Slot::Strength(n) => n.to_string(),
            Slot::InfluenceLimit(Some(n)) => n.to_string(),
            Slot::InfluenceLimit(None) => "∞".to_string(),
        }
    }

    /// The word beside the number where a face has room for one.
    pub fn caption(self) -> &'static str {
        match self {
            Slot::Cost(_) => "cost",
            Slot::Strength(_) => "strength",
            Slot::Advancement(_) => "advance",
            Slot::AgendaPoints(_) => "points",
            Slot::TrashCost(_) => "trash",
            Slot::Memory(_) => "MU",
            Slot::DeckSize(_) => "deck",
            Slot::InfluenceLimit(_) => "influence",
            Slot::Link(_) => "link",
        }
    }
}

/// A card laid out for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub title: String,
    pub unique: bool,
    pub side: Side,
    pub faction: Option<Faction>,
    /// The printed type line, or the type's name when the catalog has
    /// none (a homebrew card).
    pub type_line: String,
    /// The top-left circle: the cost, an agenda's advancement
    /// requirement, nothing on an identity.
    pub cost: Option<Slot>,
    /// Strength on ice and programs that have one, an agenda's points, an
    /// identity's deck size.
    pub bottom_left: Option<Slot>,
    /// Trash cost, memory, an identity's influence limit and link — in
    /// that order, for the cards that print more than one.
    pub bottom_right: Vec<Slot>,
    /// Influence cost, drawn as pips. `None` on identities and cards
    /// that print none.
    pub influence: Option<u32>,
    pub body: Vec<Segment>,
    pub flavor: Option<String>,
    /// Whether the engine can play it (`CardDefinition::is_playable`); a
    /// browser marks the ones it cannot.
    pub implemented: bool,
    pub code: Option<CardId>,
}

impl Face {
    pub fn of(card: &CardDefinition) -> Face {
        let is = |kind: CardType| card.card_type == kind;
        let ice = matches!(card.card_type, CardType::Ice(_));
        let cost = if is(CardType::Identity) {
            None
        } else if is(CardType::Agenda) {
            card.advancement_requirement.map(Slot::Advancement)
        } else {
            Some(Slot::Cost(card.cost))
        };
        let bottom_left = if is(CardType::Identity) {
            card.min_deck_size.map(Slot::DeckSize)
        } else if is(CardType::Agenda) {
            card.agenda_points.map(Slot::AgendaPoints)
        } else if ice || is(CardType::Program) {
            card.strength.map(Slot::Strength)
        } else {
            None
        };
        let mut bottom_right = Vec::new();
        if is(CardType::Identity) {
            bottom_right.push(Slot::InfluenceLimit(if card.unlimited_influence { None } else { card.influence_limit }));
            if card.side == Side::Runner {
                bottom_right.push(Slot::Link(card.base_link.unwrap_or(0)));
            }
        } else {
            bottom_right.extend(card.trash_cost.map(Slot::TrashCost));
            bottom_right.extend(card.memory_cost.map(Slot::Memory));
        }
        Face {
            title: card.title.clone(),
            unique: card.unique,
            side: card.side,
            faction: card.faction,
            type_line: card.type_line.clone().unwrap_or_else(|| crate::cards::type_group(&card.card_type).to_string()),
            cost,
            bottom_left,
            bottom_right,
            influence: if is(CardType::Identity) { None } else { card.influence_cost },
            body: card.printed_text.as_deref().map(card_text::segments).unwrap_or_default(),
            flavor: card.flavor.clone(),
            implemented: card.is_playable,
            code: card.numeric_id,
        }
    }

    /// The body as one string, glyphs or fallbacks.
    pub fn body_text(&self, glyphs: bool) -> String {
        card_text::render(&self.body, glyphs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardId as DslId, IceType};

    fn card(card_type: CardType, side: Side) -> CardDefinition {
        CardDefinition { id: DslId("x".to_string()), title: "X".to_string(), side, card_type, ..CardDefinition::default() }
    }

    #[test]
    fn ice_puts_rez_cost_top_left_and_strength_bottom_left() {
        let mut ice = card(CardType::Ice(IceType::Barrier), Side::Corp);
        ice.cost = 4;
        ice.strength = Some(3);
        ice.influence_cost = Some(2);
        let face = Face::of(&ice);
        assert_eq!(face.cost, Some(Slot::Cost(4)));
        assert_eq!(face.bottom_left, Some(Slot::Strength(3)));
        assert!(face.bottom_right.is_empty());
        assert_eq!(face.influence, Some(2));
    }

    #[test]
    fn an_agenda_prints_advancement_where_a_cost_would_be_and_points_below() {
        let mut agenda = card(CardType::Agenda, Side::Corp);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        let face = Face::of(&agenda);
        assert_eq!(face.cost, Some(Slot::Advancement(3)));
        assert_eq!(face.bottom_left, Some(Slot::AgendaPoints(2)));
    }

    #[test]
    fn an_icebreaker_prints_strength_and_memory() {
        let mut program = card(CardType::Program, Side::Runner);
        program.cost = 3;
        program.strength = Some(1);
        program.memory_cost = Some(1);
        let face = Face::of(&program);
        assert_eq!(face.cost, Some(Slot::Cost(3)));
        assert_eq!(face.bottom_left, Some(Slot::Strength(1)));
        assert_eq!(face.bottom_right, vec![Slot::Memory(1)]);
    }

    #[test]
    fn an_asset_prints_its_trash_cost_bottom_right() {
        let mut asset = card(CardType::Asset, Side::Corp);
        asset.trash_cost = Some(2);
        let face = Face::of(&asset);
        assert_eq!(face.cost, Some(Slot::Cost(0)), "a printed 0 is still printed");
        assert_eq!(face.bottom_left, None);
        assert_eq!(face.bottom_right, vec![Slot::TrashCost(2)]);
    }

    #[test]
    fn an_identity_prints_deck_size_influence_and_link_and_no_cost() {
        let mut id = card(CardType::Identity, Side::Runner);
        id.min_deck_size = Some(45);
        id.influence_limit = Some(15);
        id.base_link = Some(1);
        id.influence_cost = Some(0);
        let face = Face::of(&id);
        assert_eq!(face.cost, None);
        assert_eq!(face.bottom_left, Some(Slot::DeckSize(45)));
        assert_eq!(face.bottom_right, vec![Slot::InfluenceLimit(Some(15)), Slot::Link(1)]);
        assert_eq!(face.influence, None);
        let mut corp = card(CardType::Identity, Side::Corp);
        corp.unlimited_influence = true;
        assert_eq!(Face::of(&corp).bottom_right, vec![Slot::InfluenceLimit(None)]);
        assert_eq!(Slot::InfluenceLimit(None).value(), "∞");
    }

    #[test]
    fn a_card_with_no_numbers_and_no_text_has_empty_slots() {
        let face = Face::of(&card(CardType::Event, Side::Runner));
        assert_eq!(face.bottom_left, None);
        assert!(face.bottom_right.is_empty());
        assert!(face.body.is_empty());
        assert_eq!(face.type_line, "Event", "no type line on record, so the type's name");
    }

    /// Every printing lays out: ice has a strength, agendas have points,
    /// programs have memory, and every non-identity has a cost circle.
    #[test]
    fn every_catalog_card_lays_out() {
        let registry = crate::decks::sample_deck_registry();
        for card in crate::cards::catalog(&registry) {
            let face = Face::of(&card);
            match card.card_type {
                CardType::Identity => assert_eq!(face.cost, None, "{}", card.title),
                CardType::Agenda => assert!(matches!(face.cost, Some(Slot::Advancement(_))), "{}", card.title),
                _ => assert!(matches!(face.cost, Some(Slot::Cost(_))), "{}", card.title),
            }
            if matches!(card.card_type, CardType::Ice(_)) {
                assert!(matches!(face.bottom_left, Some(Slot::Strength(_))), "{} has no strength", card.title);
            }
            if card.card_type == CardType::Program {
                assert!(face.bottom_right.iter().any(|slot| matches!(slot, Slot::Memory(_))), "{} has no MU", card.title);
            }
        }
    }
}
