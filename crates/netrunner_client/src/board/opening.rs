//! The start-of-game box: at the mulligan, both identities and the
//! opening hand turned up, over the keep and the mulligan (Phase 7 §8
//! item 16, from jinteki's start-of-game panel).
//!
//! The decision is CR 1.6.6a's — "the Corp may choose to take a
//! mulligan; then, the Runner may choose to take a mulligan", and the
//! second hand is kept — and it is a choice made by looking at five cards
//! and at who is across the table. The pop-up used to ask it with a title
//! and two buttons, over a hand drawn as the board's peek: the top of each
//! card, at the bottom of the window, the one moment in the game when the
//! whole of every card in it matters.
//!
//! **Nothing here is not already the viewer's.** The hand is the view's
//! own (`hq_cards` / `grip_cards`, `Some` only for its owner), both
//! identities are public, and what the Corp did with its hand is in the
//! Runner's log ([`Tally`]'s `first_hand`). The buttons are the engine's
//! `KeepHand` and `TakeMulligan`, as they were.

use netrunner_core::dsl::CardId;
use netrunner_core::rules::{GamePhase, Side};
use netrunner_core::view::ClientView;

use crate::tally::{FirstHand, Tally};

/// What the box shows the side deciding on its opening hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opening {
    pub side: Side,
    pub identity: Option<CardId>,
    pub opponent_identity: Option<CardId>,
    /// The hand in the order it was drawn.
    pub hand: Vec<CardId>,
    /// The line under the heading: what a mulligan does, and what the
    /// other side did with its hand when it has already decided.
    pub detail: String,
}

impl Opening {
    /// The box for `view`, when its viewer is the side deciding on a hand;
    /// `None` otherwise — the other side waiting has nothing to decide.
    pub fn of(view: &ClientView, tally: &Tally) -> Option<Opening> {
        let GamePhase::Mulligan(side) = view.phase else { return None };
        if view.viewer.side() != Some(side) {
            return None;
        }
        let (hand, identity, opponent_identity, opponent) = match side {
            Side::Corp => (view.corp.hq_cards.clone(), view.corp.identity.clone(), view.runner.identity.clone(), tally.runner.first_hand),
            Side::Runner => (view.runner.grip_cards.clone(), view.runner.identity.clone(), view.corp.identity.clone(), tally.corp.first_hand),
        };
        let mut detail = "A mulligan shuffles this hand back and draws a new one, which you must keep.".to_string();
        match opponent {
            Some(FirstHand::Kept) => detail.push_str(&format!("\nThe {:?} kept their hand.", side.other())),
            Some(FirstHand::Mulligan) => detail.push_str(&format!("\nThe {:?} took a mulligan.", side.other())),
            None => {}
        }
        Some(Opening { side, identity, opponent_identity, hand: hand.unwrap_or_default(), detail })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{GameState, PlayerAction};
    use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};

    fn awaiting(session: &mut Session) -> (Side, ClientView) {
        match session.step() {
            SessionStep::Awaiting { side, view } => (side, *view),
            other => panic!("{other:?}"),
        }
    }

    /// The Corp sees its five and both identities; once it has taken a
    /// mulligan, the Runner's box says so and shows the Runner's own five.
    #[test]
    fn each_side_sees_its_own_hand_and_the_runner_hears_what_the_corp_did() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
        let mut tally = Tally::default();

        let (side, view) = awaiting(&mut session);
        assert_eq!(side, Side::Corp, "the Corp decides first (CR 1.6.6a)");
        let corp = Opening::of(&view, &tally).expect("the Corp is deciding");
        assert_eq!(corp.hand.len(), 5);
        assert_eq!(Some(corp.hand.clone()), view.corp.hq_cards);
        assert!(corp.identity.is_some() && corp.opponent_identity.is_some());
        assert_eq!(corp.opponent_identity, view.runner.identity);
        assert!(!corp.detail.contains("Runner"), "the Runner has not decided: {}", corp.detail);
        assert_eq!(Opening::of(&session.view_for(Side::Runner), &tally), None, "the Runner is waiting, not deciding");

        session.submit(PlayerAction::TakeMulligan).unwrap();
        tally.add(&session.last_entry_for(Side::Runner).unwrap());
        let (side, view) = awaiting(&mut session);
        assert_eq!(side, Side::Runner);
        let runner = Opening::of(&view, &tally).expect("the Runner is deciding");
        assert_eq!(Some(runner.hand.clone()), view.runner.grip_cards);
        assert_eq!(runner.opponent_identity, view.corp.identity);
        assert!(runner.detail.contains("The Corp took a mulligan."), "{}", runner.detail);
    }
}
