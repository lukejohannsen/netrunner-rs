//! The view-based no-deadlock sweep: many seeds of real agent play across
//! every sample matchup, asserting the match always reaches `GameOver`.
//!
//! **Why this exists alongside
//! `no_panics_or_deadlocks_across_many_seeds_system_gateway`.** That sweep
//! drives *index-based* agents through `SinglePlayerSession`, so every
//! decision goes through `ActionSpace::index_of`/`action_at` over the
//! side-agnostic `get_action_mask`. This one drives `Seat::Agent`, so each
//! side chooses only from `legal_actions_for` — the per-seat `ClientView`
//! slice a real client actually receives. The two reach different code, and
//! neither substitutes for the other: the index path's round trip hid both
//! an engine panic and a deadlock that were trivially reachable here, on
//! ordinary sample decks at low seeds.
//!
//! The invariant this pins is the one every deadlock found in this repo has
//! violated: **if `current_actor` names a side, that side must have at
//! least one legal action.** `Session` reports a violation directly as
//! `StallReason::NoLegalActions { side }` rather than conflating it with
//! budget exhaustion the way each hand-rolled `else { break }` used to.

use netrunner_bots::{HeuristicAgent, RandomAgent};
use netrunner_core::cards::{register_playable_cards, CardRegistry};
use netrunner_core::decks;
use netrunner_core::rules::{GameState, Side};
use netrunner_session::{Seat, Session, SessionStep};

/// How many seeds the sweep walks, default 32 — sized for the inner loop,
/// matching `system_gateway_delivery::sweep_seed_count`. Raise it for a deep
/// run before merging engine-level work:
///
/// ```text
/// NETRUNNER_SWEEP_SEEDS=256 cargo test -p netrunner_session --release
/// ```
fn sweep_seed_count() -> u64 {
    std::env::var("NETRUNNER_SWEEP_SEEDS").ok().and_then(|value| value.parse().ok()).unwrap_or(32)
}

#[test]
fn view_based_agents_never_reach_a_state_with_no_legal_action() {
    let matchups = decks::matchups();
    assert!(!matchups.is_empty(), "the embedded sample decks should yield at least one matchup");

    for seed in 0..sweep_seed_count() {
        // Rotating by seed rather than nesting a second loop keeps the
        // default run's cost in line with the existing sweep while still
        // covering all twelve pairings as the seed count rises.
        let (corp_deck, runner_deck) = &matchups[seed as usize % matchups.len()];
        let matchup = format!("{} vs {}", corp_deck.id, runner_deck.id);

        for runner_is_random in [false, true] {
            let mut registry = CardRegistry::new();
            register_playable_cards(&mut registry);
            let (state, _events) =
                GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed)
                    .expect("sample decks are legal by construction");

            // Both seatings, since which side holds the random agent
            // changes which decisions get explored.
            let (corp, runner): (Box<dyn netrunner_bots::BotAgent>, Box<dyn netrunner_bots::BotAgent>) =
                if runner_is_random {
                    (Box::new(HeuristicAgent::new(Side::Corp, seed)), Box::new(RandomAgent::new(seed)))
                } else {
                    (Box::new(RandomAgent::new(seed)), Box::new(HeuristicAgent::new(Side::Runner, seed)))
                };

            let mut session = Session::new(state, registry, Seat::Agent(corp), Seat::Agent(runner));
            match session.run() {
                SessionStep::Ended { .. } => {}
                SessionStep::Stalled(reason) => panic!(
                    "seed {seed} ({matchup}, runner_is_random={runner_is_random}) stalled: {reason:?} \
                     after {} actions",
                    session.steps()
                ),
                other => panic!(
                    "seed {seed} ({matchup}, runner_is_random={runner_is_random}): \
                     an all-Agent session should never yield {other:?}"
                ),
            }
            assert!(!session.history().is_empty(), "seed {seed} ({matchup}): recorded no actions");
        }
    }
}

/// The specific position that motivated this sweep, pinned by seed so a
/// regression names itself rather than hiding in a 32-seed run.
///
/// A flatlining subroutine on a multi-subroutine ICE used to leave the run
/// parked at `EncounterIce` with subroutines still pending while
/// `phase` was already `GameOver`. `resolve_encounter_ice` then tried to
/// advance past the ICE anyway, `continue_run` refused with
/// `SubroutinesStillPending`, and that error propagated out through
/// `close_window` — making the Corp's own `PassPriority` illegal while
/// `current_actor` still named them. See `paid_ability::resolve_encounter_ice`.
#[test]
fn the_flatline_during_an_encounter_window_position_plays_out() {
    let (corp_deck, runner_deck) = decks::matchups().into_iter().next().expect("at least one matchup");
    let mut registry = CardRegistry::new();
    register_playable_cards(&mut registry);
    let (state, _events) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 2)
        .expect("sample decks are legal by construction");

    let mut session = Session::new(
        state,
        registry,
        Seat::Agent(Box::new(RandomAgent::new(2))),
        Seat::Agent(Box::new(RandomAgent::new(3))),
    );
    match session.run() {
        SessionStep::Ended { .. } => {}
        other => panic!(
            "seed 2 of {} vs {} must play to a conclusion, got {other:?}",
            corp_deck.id,
            runner_deck.id,
        ),
    }
}
