//! Real boards for the tests: a `Session` of two random agents, pumped
//! until a side is at a decision the test wants. Through the one match
//! loop (the Session Rule), never a hand-rolled `apply_action` loop.

use netrunner_bots::RandomAgent;
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{Deck, DeckOrder, GameState, MatchRules, Side};
use netrunner_core::view::ClientView;
use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};

/// A board where `side` is at a decision `pred` admits, with the
/// registry it was dealt from and `side`'s own deck. `None` when the
/// game ends first.
pub fn view_where(seed: u64, side: Side, mut pred: impl FnMut(&ClientView) -> bool) -> Option<(CardRegistry, ClientView, Deck)> {
    let (corp, runner) = sweep_decks_for_seed(seed);
    let registry = crate::decks::sample_deck_registry();
    let own = if side == Side::Corp { corp.to_deck() } else { runner.to_deck() };
    let (state, _) = GameState::setup_with(&corp.to_deck(), &runner.to_deck(), &registry, seed, MatchRules::default(), DeckOrder::Shuffled).expect("setup");
    let corp_seat = Seat::Agent(Box::new(RandomAgent::new(seed)));
    let runner_seat = Seat::Agent(Box::new(RandomAgent::new(seed.wrapping_add(1))));
    let mut session = Session::new(state, registry.clone(), corp_seat, runner_seat);
    loop {
        let view = session.view_for(side);
        if !view.legal_actions.is_empty() && pred(&view) {
            return Some((registry, view, own));
        }
        match session.step() {
            SessionStep::Applied { .. } => {}
            SessionStep::Awaiting { .. } => unreachable!("both seats are agents"),
            SessionStep::Ended { .. } | SessionStep::Stalled(_) => return None,
        }
    }
}

/// The first decision of `side` after `steps` applied actions.
pub fn view_after(seed: u64, steps: usize, side: Side) -> (CardRegistry, ClientView, Deck) {
    let mut seen = 0usize;
    view_where(seed, side, |_| {
        seen += 1;
        seen > steps
    })
    .expect("the game outlasts the steps asked for")
}
