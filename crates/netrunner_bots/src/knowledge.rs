//! What a bot knows that its `ClientView` does not say: the format the
//! game is played in, the deck it brought, and the cards it has seen.
//!
//! **Every hidden card a bot imagines is drawn from here** (Phase 5 §25
//! Stage 2, 29 September 2026). Before this existed, `determinize`
//! guessed the opponent's deck by matching its identity to an embedded
//! published list — a guess presented as knowledge, which happened to be
//! right for every sample deck and wrong for every deck a person builds
//! — and, failing a match, sampled the whole registry: every set of every
//! format, so a Startup game imagined Core Set cards behind the ICE. What
//! replaces the match is the three things a player at the table actually
//! has:
//!
//! - **The format.** A card outside the format's pool, or on its ban
//!   list, is never in the opponent's deck (`NsgFormat::rules`). `Casual`
//!   is every playable card, which is what the registry fallback was.
//! - **The opponent's identity**, which is public: its faction says which
//!   cards cost nothing to include, and its influence budget bounds how
//!   many out-of-faction cards there can be. A card seen spends its
//!   influence, so a deck that has shown three Anarch cards has less room
//!   for a fourth (`determinize`'s prior, `SEEN_COPY_WEIGHT`).
//! - **What it has seen.** A card accessed in HQ, looked at on top of R&D
//!   or revealed leaves the view again, and the view carries no memory.
//!   `observe` keeps one: for every card, the most copies the seat has
//!   ever had evidence of at once. A deck that holds a card usually holds
//!   two or three, so the other copies of a seen card are likelier than
//!   an unseen card of the same faction, and a copy in the heap or the
//!   score area is one fewer to draw.
//!
//! **The seat's own deck is exact**, not a prior: `own_deck` minus what
//! the view shows of its own cards is the rest of its stack, up to order.
//! A seat built without one still finds a published list by its identity
//! — the person's decision, 29 September 2026: a bot's own deck is not a
//! guess, so the match stays where it is knowledge and goes where it was
//! not.
//!
//! **The engine and the view are unchanged.** Nothing here is a rule: a
//! format never reaches `GameState`, and a masked view carries no memory
//! by design (the record is the harness's). The knowledge rides with the
//! agent, is shown every view the seat acts on (`BotAgent::observe`,
//! called by `netrunner_session::Session` before `select_action`), and is
//! read by `determinize` alone.

use std::collections::BTreeMap;

use netrunner_core::dsl::CardId;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Deck;
use netrunner_core::view::ClientView;

use crate::determinize::visible_cards;

/// The format, the seat's own deck and what it has seen — see the module
/// docs. `Default` is a seat that knows nothing: `Casual`, no deck, no
/// memory, which is a test fixture or a client sampling for a preview
/// (`netrunner_client::board::preview`, whose sample reads public
/// information only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Knowledge {
    /// The format the game is played in: the pool the opponent's hidden
    /// cards are drawn from.
    pub format: NsgFormat,
    /// The deck this seat brought, when the builder knows it. `None`
    /// falls back to the published list the seat's identity names, then
    /// to the format prior (`determinize::remaining_decklist`).
    pub own_deck: Option<Deck>,
    /// Per card, the most copies this seat has ever seen at once —
    /// visible cards and cards shown in an access alike, so a card that
    /// went back into HQ is still known to be somewhere.
    seen: BTreeMap<CardId, usize>,
}

impl Default for Knowledge {
    fn default() -> Self {
        Self::new(NsgFormat::Casual, None)
    }
}

impl Knowledge {
    pub fn new(format: NsgFormat, own_deck: Option<Deck>) -> Self {
        Self { format, own_deck, seen: BTreeMap::new() }
    }

    /// Records everything `view` shows the identity of. Monotone: a
    /// count only rises, and it rises to the number of copies visible at
    /// once rather than by one per sighting, because two sightings may
    /// be the same copy — an HQ card accessed on turn three and rezzed
    /// on turn eight is one card, and the memory must never say two.
    pub fn observe(&mut self, view: &ClientView) {
        let mut now: BTreeMap<CardId, usize> = BTreeMap::new();
        for card in visible_cards(view) {
            *now.entry(card).or_insert(0) += 1;
        }
        for (card, count) in now {
            let known = self.seen.entry(card).or_insert(0);
            *known = (*known).max(count);
        }
    }

    /// The most copies of `card` this seat has ever seen at once.
    pub fn seen(&self, card: &CardId) -> usize {
        self.seen.get(card).copied().unwrap_or(0)
    }

    /// How many distinct cards the seat remembers having seen.
    pub fn cards_seen(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::cards::CardRegistry;
    use netrunner_core::dsl::{CardDefinition, CardType};
    use netrunner_core::rules::{ArchivedCard, GamePhase, GameState, Side};
    use netrunner_core::view::build_client_view;

    fn card(id: &str, side: Side, card_type: CardType) -> CardDefinition {
        CardDefinition { id: CardId(id.to_string()), title: id.to_string(), side, card_type, is_playable: true, ..Default::default() }
    }

    /// A faceup card in Archives is seen; putting it back into R&D takes
    /// it out of the view and not out of the memory; and a second copy
    /// beside the first raises the count to two, where a second sighting
    /// of one copy does not.
    #[test]
    fn memory_keeps_a_card_the_view_has_stopped_showing_and_counts_copies_seen_at_once() {
        let mut registry = CardRegistry::new();
        registry.insert(card("hedge_fund", Side::Corp, CardType::Operation));
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.archives = vec![ArchivedCard::faceup(CardId("hedge_fund".to_string()))];

        let mut knowledge = Knowledge::default();
        assert_eq!(knowledge.seen(&CardId("hedge_fund".to_string())), 0);
        knowledge.observe(&build_client_view(&state, &registry, Side::Runner));
        assert_eq!(knowledge.seen(&CardId("hedge_fund".to_string())), 1);

        // Shuffled back: gone from the view, kept in the memory.
        state.corp.r_and_d.push(state.corp.archives.remove(0).card);
        knowledge.observe(&build_client_view(&state, &registry, Side::Runner));
        assert_eq!(knowledge.seen(&CardId("hedge_fund".to_string())), 1, "seen once, not twice");

        // Two faceup at once is two copies.
        state.corp.archives = vec![ArchivedCard::faceup(CardId("hedge_fund".to_string())); 2];
        knowledge.observe(&build_client_view(&state, &registry, Side::Runner));
        assert_eq!(knowledge.seen(&CardId("hedge_fund".to_string())), 2);
        assert_eq!(knowledge.cards_seen(), 1);
    }

    #[test]
    fn a_seat_that_knows_nothing_is_casual_with_no_deck() {
        let knowledge = Knowledge::default();
        assert_eq!(knowledge.format, NsgFormat::Casual);
        assert_eq!(knowledge.own_deck, None);
        assert_eq!(knowledge.cards_seen(), 0);
    }
}
