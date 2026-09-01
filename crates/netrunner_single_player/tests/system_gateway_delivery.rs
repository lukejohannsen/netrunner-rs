//! Proof that the hand-authored System Gateway cards are actually reachable
//! from a consumer crate.
//!
//! For seven milestones these cards were correct and thoroughly tested
//! *inside* `netrunner_core`'s own `--features fs-loader` test runs, while
//! being invisible to every real consumer: nothing enabled `fs-loader`, and
//! every consumer called the hardcoded-baseline registry builder. A green
//! `netrunner_core` suite could not have caught that. These tests can — they
//! reach the cards the same way the server, CLI, and gym do, through
//! `cards::register_playable_cards` with default features.

mod common;

use common::{sg_decks, sg_registry, SG_CORP_CARDS, SG_RUNNER_CARDS};
use netrunner_bots::{HeuristicAgent, IndexedHeuristicAgent, IndexedRandomAgent, RandomAgent};
use netrunner_core::dsl::CardId;
use netrunner_bots::Agent;
use netrunner_core::rules::{validate_deck, GamePhase, GameState, Side, WindowCheckpoint};
use netrunner_session::{Coverage, Seat, Session, SessionStep, StallReason};
use netrunner_single_player::{SinglePlayerSession, MAX_STEPS};

/// The registry a consumer builds must contain System Gateway cards, and
/// they must carry real DSL rules — not the `is_playable: false`,
/// rules-empty metadata stubs the NetrunnerDB catalog conversion produces.
#[test]
fn system_gateway_cards_are_reachable_from_a_consumer_crate_with_default_features() {
    let registry = sg_registry();

    for id in SG_CORP_CARDS.into_iter().chain(SG_RUNNER_CARDS) {
        let card = registry
            .get(&CardId(id.to_string()))
            .unwrap_or_else(|| panic!("{id} should be in a consumer-built registry"));

        assert!(card.is_playable, "{id} should be playable, not a catalog-only stub");
        assert_eq!(card.set_code.as_deref(), Some("sg"), "{id} should be a System Gateway card");
        assert!(
            !card.triggers.is_empty() || !card.abilities.is_empty() || !card.subroutines.is_empty(),
            "{id} should carry real DSL rules, not just metadata"
        );
    }
}

/// The decks those cards form must be legal and must survive setup — the
/// gate `rules::deck::validate_deck` applies (`is_playable`, deck size,
/// agenda-point range) before a match can start.
#[test]
fn decks_built_from_system_gateway_cards_are_legal_and_set_up() {
    let registry = sg_registry();
    let (corp_deck, runner_deck) = sg_decks();

    validate_deck(&corp_deck, Side::Corp, &registry).expect("the System Gateway Corp deck should be legal");
    validate_deck(&runner_deck, Side::Runner, &registry).expect("the System Gateway Runner deck should be legal");

    GameState::setup(&corp_deck, &runner_deck, &registry, 7)
        .expect("setup should accept two legal System Gateway decks");
}

/// And they must actually play: a full bot-driven match to `GameOver`,
/// exercising the System Gateway cards through the same session loop the
/// single-player app uses.
#[test]
fn a_match_of_system_gateway_decks_plays_to_completion() {
    let registry = sg_registry();
    let (corp_deck, runner_deck) = sg_decks();
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 11).expect("setup should succeed");

    let corp: Box<dyn Agent> =
        Box::new(IndexedHeuristicAgent::new(HeuristicAgent::new(Side::Corp, 11), Side::Corp));
    let runner: Box<dyn Agent> = Box::new(IndexedRandomAgent::new(RandomAgent::new(11), Side::Runner));
    let (final_state, history) = SinglePlayerSession::new(state, registry, corp, runner).run();

    assert!(matches!(final_state.phase, GamePhase::GameOver(_)), "expected the match to reach GameOver");
    assert!(!history.is_empty(), "expected a non-empty action history");
}

/// The broad sweep: many seeds of bot-driven System Gateway play, mirroring
/// `single_player_test::no_panics_or_deadlocks_across_many_seeds` (which
/// covers the baseline Core Set matchup and is deliberately left untouched
/// as a stable regression net).
///
/// This is the only test in the suite that drives the *whole* System
/// Gateway mechanic surface — hosting, bioroid click-breaking, the
/// pending-decision primitives, facedown Archives, dynamic amounts,
/// persistent-after-trash, remove-from-game — through real agents rather
/// than scripted `apply_action` calls. Per-card tests prove each mechanic
/// works when driven correctly; this proves the engine can't be walked into
/// an unreachable state or a parked decision nothing can resolve. Both
/// seatings are swept, since which side holds the random agent changes
/// which decisions get explored.
/// How many seeds the sweep walks, default 32.
///
/// Every deadlock this test has caught was one specific RNG path, so
/// coverage scales with seed count — and the original 0..8 was
/// demonstrably too narrow: of the three deadlocks found in this area, the
/// Red Team end-of-turn run bug first appears at **seed 9** and was live on
/// `main` while the committed sweep stayed green, and a second only
/// surfaced because unrelated work reshuffled the RNG onto seed 4.
///
/// Costs roughly 1 second per seed in a debug build (both seatings). Raise
/// it for a deep run before merging engine-level work:
///
/// ```text
/// NETRUNNER_SWEEP_SEEDS=256 cargo test -p netrunner_single_player --release
/// ```
///
/// Deliberately one test body rather than a second `#[ignore]`d deep copy:
/// this repo has no CI, so `cargo test --workspace` on a developer machine
/// is the only gate, and an ignored slow test is coverage that never runs.
/// (The one `#[ignore]` precedent, `netrunner_card_sync`'s live sync, is
/// gated on network access — an environmental reason, not merely speed.)
fn sweep_seed_count() -> u64 {
    std::env::var("NETRUNNER_SWEEP_SEEDS").ok().and_then(|value| value.parse().ok()).unwrap_or(32)
}

/// Which index-based agents sit where. The same three seatings as
/// `netrunner_session`'s view-path sweep, for the same reasons — see the
/// `Seating` there: the heuristic pairings find deadlocks, and
/// random-vs-random is the only one that reaches runs and encounters at
/// all, because `HeuristicAgent` never runs and never installs ICE.
#[derive(Clone, Copy, Debug)]
enum Seating {
    HeuristicCorpRandomRunner,
    RandomCorpHeuristicRunner,
    RandomBoth,
}

impl Seating {
    const ALL: [Seating; 3] = [Seating::HeuristicCorpRandomRunner, Seating::RandomCorpHeuristicRunner, Seating::RandomBoth];

    fn drivers(self, seed: u64) -> (Box<dyn Agent>, Box<dyn Agent>) {
        let random = |side, seed| -> Box<dyn Agent> { Box::new(IndexedRandomAgent::new(RandomAgent::new(seed), side)) };
        let heuristic =
            |side, seed| -> Box<dyn Agent> { Box::new(IndexedHeuristicAgent::new(HeuristicAgent::new(side, seed), side)) };
        match self {
            Seating::HeuristicCorpRandomRunner => (heuristic(Side::Corp, seed), random(Side::Runner, seed)),
            Seating::RandomCorpHeuristicRunner => (random(Side::Corp, seed), heuristic(Side::Runner, seed)),
            Seating::RandomBoth => (random(Side::Corp, seed), random(Side::Runner, seed.wrapping_add(1))),
        }
    }

    /// Random-vs-random can legitimately outlast `MAX_STEPS` (measured: 1
    /// game in 192); that is slow play, not a deadlock. A deadlock is
    /// `StallReason::NoLegalActions`, which no seating may produce.
    fn tolerates_budget_exhaustion(self) -> bool {
        matches!(self, Seating::RandomBoth)
    }
}

/// Also the index-path **rules-coverage gate**: every `PlayerAction`
/// variant and every load-bearing `GameEvent` must be applied at least
/// once across the sweep, through the `ActionSpace` round trip. This is
/// the test that would have failed while `InstallProgram` was silently
/// unreachable, and the one that catches an `ActionSpace` cap too small
/// for a real game — a legal action with no index never reaches this path.
/// The card universe is this fixture's own two decks, not the sample pool.
#[test]
fn no_panics_or_deadlocks_across_many_seeds_system_gateway() {
    let mut coverage = Coverage::default();
    for seed in 0..sweep_seed_count() {
        for seating in Seating::ALL {
            let registry = sg_registry();
            let (corp_deck, runner_deck) = sg_decks();
            let (state, _events) =
                GameState::setup(&corp_deck, &runner_deck, &registry, seed).expect("legal decks set up cleanly");

            let (corp, runner) = seating.drivers(seed);
            let (final_state, history, outcome) =
                SinglePlayerSession::new(state, registry.clone(), corp, runner).run_with_outcome();
            assert!(
                matches!(final_state.phase, GamePhase::GameOver(_))
                    || (seating.tolerates_budget_exhaustion()
                        && matches!(outcome, SessionStep::Stalled(StallReason::BudgetExhausted))),
                "seed {seed} ({seating:?}): expected GameOver within {MAX_STEPS} steps, got {outcome:?}"
            );
            assert!(!history.is_empty(), "seed {seed} ({seating:?}): history should be non-empty");
            coverage.absorb_match(&history, &registry, &outcome);
        }
    }

    let (corp_deck, runner_deck) = sg_decks();
    let mut universe: Vec<CardId> = corp_deck.cards.iter().chain(&runner_deck.cards).map(|(id, _)| id.clone()).collect();
    universe.sort_by(|a, b| a.0.cmp(&b.0));
    let failures = coverage.gate_failures(&universe);
    assert!(
        failures.is_empty(),
        "rules never reached across {} index-path games (rerun with NETRUNNER_SWEEP_SEEDS=256 before \
         allowlisting anything):\n  {}",
        coverage.games,
        failures.join("\n  ")
    );
}

/// The position that random-vs-random seating found the day it was added
/// to the sweep, pinned by seed so a regression names itself.
///
/// Anoetic Void and Manegarm Skunkworks protecting one remote: Anoetic's
/// `OnApproachServer` parked a card selection and, once confirmed, ended
/// the run; the queued Skunkworks trigger then fired against no run and
/// parked a paid choice the Runner could neither afford nor decline
/// (declining resolves `EndTheRun` → `NoActiveRun`). See
/// `dispatcher::still_applies`.
#[test]
fn the_anoetic_void_then_skunkworks_position_plays_out() {
    let seed = 85;
    let registry = sg_registry();
    let (corp_deck, runner_deck) = sg_decks();
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed).expect("legal decks set up cleanly");
    let (corp, runner) = Seating::RandomBoth.drivers(seed);
    let (final_state, _history, outcome) = SinglePlayerSession::new(state, registry, corp, runner).run_with_outcome();
    assert!(
        matches!(final_state.phase, GamePhase::GameOver(_)),
        "seed {seed} must play to a conclusion, got {outcome:?}"
    );
}

/// The same sweep, over Null Signal Games' published System Gateway sample
/// decklists rather than the coverage-oriented deck above.
///
/// These are the decks self-play trains on and the decks a human faces in
/// local play, so a matchup that cannot finish is a matchup that produces
/// unusable training data and an unplayable game. The mechanic-coverage
/// sweep above does not catch this: it plays one fixed deck pair, while
/// these are twelve real pairings whose card interactions differ.
#[test]
fn every_sample_deck_matchup_finishes() {
    let mut coverage = Coverage::default();
    for (index, (corp_deck, runner_deck)) in netrunner_core::decks::matchups().into_iter().enumerate() {
        for seed in 0..4u64 {
            let registry = sg_registry();
            let label = format!("{}_vs_{} seed {seed}", corp_deck.id, runner_deck.id);
            let (state, _events) =
                GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed)
                    .unwrap_or_else(|e| panic!("{label}: sample decks should set up cleanly: {e:?}"));

            // Alternate which side is random so both sides' decisions get
            // explored across the sweep; the last two seeds of every
            // matchup are random-vs-random, the one seating that reaches
            // runs — and two of them because a 2-of in a 40-card deck
            // (Docklands Pass) went unseen with one.
            let seating = match (seed, index % 2 == 0) {
                (2 | 3, _) => Seating::RandomBoth,
                (_, true) => Seating::RandomCorpHeuristicRunner,
                (_, false) => Seating::HeuristicCorpRandomRunner,
            };
            let (corp, runner) = seating.drivers(seed);
            let (final_state, history, outcome) =
                SinglePlayerSession::new(state, registry.clone(), corp, runner).run_with_outcome();
            assert!(
                matches!(final_state.phase, GamePhase::GameOver(_))
                    || (seating.tolerates_budget_exhaustion()
                        && matches!(outcome, SessionStep::Stalled(StallReason::BudgetExhausted))),
                "{label} ({seating:?}): expected GameOver within {MAX_STEPS} steps, got {outcome:?}"
            );
            coverage.absorb_match(&history, &registry, &outcome);
        }
    }

    // No coverage gate here, deliberately. Forty-eight games is too small a
    // sample for "every card was seen": which cards a random Runner draws
    // and installs shifts with every engine change, and this test flagged
    // Docklands Pass and then Echelon on two consecutive branches for no
    // reason but that. The sample-pool card gate lives in the 96-game
    // view-path sweep (`netrunner_session`) and both 256-seed deep runs;
    // the index path's own gate — every action variant, through the
    // `ActionSpace` round trip — is `no_panics_or_deadlocks_across_many_seeds_system_gateway`.
    assert!(coverage.games == 48, "four seeds across twelve matchups");
}

/// The post-action paid-ability window must not become *constant* —
/// costing both players a `PassPriority` after every single action, which
/// is what a too-loose `paid_ability::has_usable_paid_ability` would do.
/// That is what happened before icebreaker abilities were gated to
/// encounters, when a rig full of breakers always answered "yes".
///
/// An upper bound only, deliberately. This test originally asserted a
/// lower bound too, and it passed — 63 openings across ~9,000 steps. That
/// turned out to be an artifact: `end_turn` was leaving unspent clicks in
/// place, so the Corp kept spending them off-turn on *Regolith Mining
/// License*'s `[click]` ability. With clicks correctly cleared, the only
/// System Gateway card either side can use on the opponent's turn is
/// *Spin Doctor* (cost `RemoveSelfFromGame`, no requirement) — one copy,
/// which must be drawn, installed **and** rezzed before it qualifies. So
/// whether a window opens at all in any given sample of games is luck,
/// and asserting on it would be flaky.
///
/// "Does it fire when someone qualifies" is covered deterministically by
/// `rules::engine::tests::
/// a_click_action_opens_a_post_action_window_when_the_opponent_can_respond`.
/// What only a real game can check is that it does *not* fire constantly.
#[test]
fn post_action_windows_stay_rare() {
    let mut opened = 0usize;
    let mut steps = 0usize;

    for seed in 0..16u64 {
        let registry = sg_registry();
        let (corp_deck, runner_deck) = sg_decks();
        let (state, _events) =
            GameState::setup(&corp_deck, &runner_deck, &registry, seed).expect("legal decks set up cleanly");
        // The fifth hand-rolled copy of the match loop used to live here,
        // because it has to inspect `paid_ability_window` *between* steps
        // and `SinglePlayerSession::run` blocked until `GameOver`. A pull-
        // shaped `Session` gives it that for free: peek at `state()`, then
        // `step()`. The agents are the same ones the `Indexed*` aliases
        // wrapped — `BotAgentIndexAdapter` is `build_client_view` plus an
        // `index_of` round trip over exactly these — so same seed, same
        // view, same decisions.
        let mut session = Session::new(
            state,
            registry,
            Seat::Agent(Box::new(HeuristicAgent::new(Side::Corp, seed))),
            Seat::Agent(Box::new(RandomAgent::new(seed))),
        );
        let mut was_open = false;

        loop {
            let now_open = matches!(
                session.state().paid_ability_window.as_ref().map(|window| window.checkpoint),
                Some(WindowCheckpoint::PostAction { .. })
            );
            // Count openings, not the steps spent inside one.
            if now_open && !was_open {
                opened += 1;
            }
            was_open = now_open;

            match session.step() {
                SessionStep::Applied { .. } => steps += 1,
                _ => break,
            }
        }
    }

    assert!(
        opened * 10 < steps,
        "post-action windows opened {opened} times in {steps} steps — the gate is too loose, \
         and every action is costing both players a PassPriority"
    );
}
