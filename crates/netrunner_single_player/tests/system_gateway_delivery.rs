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
use netrunner_core::rules::{validate_deck, GamePhase, GameState, Side};
use netrunner_single_player::{PlayerDriver, SinglePlayerSession, MAX_STEPS};

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

    let corp: Box<dyn PlayerDriver> =
        Box::new(IndexedHeuristicAgent::new(HeuristicAgent::new(Side::Corp, 11), Side::Corp));
    let runner: Box<dyn PlayerDriver> = Box::new(IndexedRandomAgent::new(RandomAgent::new(11), Side::Runner));
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
#[test]
fn no_panics_or_deadlocks_across_many_seeds_system_gateway() {
    for seed in 0..8 {
        for runner_is_random in [false, true] {
            let registry = sg_registry();
            let (corp_deck, runner_deck) = sg_decks();
            let (state, _events) =
                GameState::setup(&corp_deck, &runner_deck, &registry, seed).expect("legal decks set up cleanly");

            let (corp, runner): (Box<dyn PlayerDriver>, Box<dyn PlayerDriver>) = if runner_is_random {
                (
                    Box::new(IndexedHeuristicAgent::new(HeuristicAgent::new(Side::Corp, seed), Side::Corp)),
                    Box::new(IndexedRandomAgent::new(RandomAgent::new(seed), Side::Runner)),
                )
            } else {
                (
                    Box::new(IndexedRandomAgent::new(RandomAgent::new(seed), Side::Corp)),
                    Box::new(IndexedHeuristicAgent::new(HeuristicAgent::new(Side::Runner, seed), Side::Runner)),
                )
            };

            let (final_state, history) = SinglePlayerSession::new(state, registry, corp, runner).run();
            assert!(
                matches!(final_state.phase, GamePhase::GameOver(_)),
                "seed {seed} (runner_is_random={runner_is_random}): expected GameOver within {MAX_STEPS} steps"
            );
            assert!(
                !history.is_empty(),
                "seed {seed} (runner_is_random={runner_is_random}): history should be non-empty"
            );
        }
    }
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
    for (index, (corp_deck, runner_deck)) in netrunner_core::decks::matchups().into_iter().enumerate() {
        for seed in 0..3u64 {
            let registry = sg_registry();
            let label = format!("{}_vs_{} seed {seed}", corp_deck.id, runner_deck.id);
            let (state, _events) =
                GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed)
                    .unwrap_or_else(|e| panic!("{label}: sample decks should set up cleanly: {e:?}"));

            // Alternate which side is random so both sides' decisions get
            // explored across the sweep.
            let corp_is_random = index % 2 == 0;
            let (corp, runner): (Box<dyn PlayerDriver>, Box<dyn PlayerDriver>) = if corp_is_random {
                (
                    Box::new(IndexedRandomAgent::new(RandomAgent::new(seed), Side::Corp)),
                    Box::new(IndexedHeuristicAgent::new(HeuristicAgent::new(Side::Runner, seed), Side::Runner)),
                )
            } else {
                (
                    Box::new(IndexedHeuristicAgent::new(HeuristicAgent::new(Side::Corp, seed), Side::Corp)),
                    Box::new(IndexedRandomAgent::new(RandomAgent::new(seed), Side::Runner)),
                )
            };

            let (final_state, _history) = SinglePlayerSession::new(state, registry, corp, runner).run();
            assert!(
                matches!(final_state.phase, GamePhase::GameOver(_)),
                "{label}: expected GameOver within {MAX_STEPS} steps"
            );
        }
    }
}
