//! `PuctAgent` as a `Seat::Agent`: the shape the arena and the `puct` /
//! `puct-onnx` headless bots play in. Self-play drives `PuctAgent::search`
//! through its own picker; this is the *other* path, `BotAgent::select_action`,
//! and it is the one that stalled 14–23 of every 48 arena games in the
//! September 2026 volume run (ROADMAP Phase 2 §5).

use netrunner_bots::{PolicyEvaluator, PuctAgent, PuctConfig, UniformPolicyEvaluator};
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardDefinition, CardFilter, CardId, CardType, CardZoneRef, IceType};
use netrunner_core::rules::{
    get_action_mask, ActionSpace, GamePhase, GameState, InstallId, InstallSlot, InstalledCard, PendingChoiceResume,
    PendingDecision, PlayerAction, ServerId, Side,
};
use netrunner_session::{Seat, Session, SessionStep, StallReason};

fn barrier(id: &str) -> CardDefinition {
    CardDefinition {
        id: CardId(id.to_string()),
        title: id.to_string(),
        side: Side::Corp,
        card_type: CardType::Ice(IceType::Barrier),
        strength: Some(2),
        is_playable: true,
        ..Default::default()
    }
}

/// *Tāo Salonga*'s "swap two installed Barriers", parked over two Corp ICE
/// with one already selected — the card-selection shape whose toggle
/// two-cycle burned whole step budgets in self-play before its driver grew
/// a cycle break. `select_action` had none, so the same state under a
/// `Seat::Agent(PuctAgent)` could toggle one position on and off until
/// `MAX_STEPS`.
fn tao_swap_pending() -> (GameState, CardRegistry) {
    let mut registry = CardRegistry::new();
    for id in ["palisade", "ballista"] {
        registry.insert(barrier(id));
    }
    let mut state = GameState::new(0);
    state.phase = GamePhase::Action(Side::Corp);
    state.corp.installed = vec![
        InstalledCard {
            card: CardId("palisade".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(1),
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        },
        InstalledCard {
            card: CardId("ballista".to_string()),
            install_id: InstallId(2),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: false,
            ..Default::default()
        },
    ];
    state.pending_decision = Some(PendingDecision::ChooseCards {
        side: Side::Runner,
        source: CardZoneRef::OpponentInstalled,
        filter: CardFilter::CardType(CardType::Ice(IceType::Barrier)),
        min: 2,
        max: 2,
        reveal: false,
        shuffle_after: false,
        destination: None,
        then: None,
        selected: vec![0],
        source_card: None,
        source_install: None,
        resume: PendingChoiceResume::None,
    });
    (state, registry)
}

/// Stands in for an overfit network: whenever toggling position 1 is
/// legal, every bit of prior mass goes there, and the value is flat. The
/// search then spends its visits on that toggle in both the selected and
/// the deselected state, and a greedy pick plays it forever. The uniform
/// evaluator does *not* cycle on this fixture — its visits land nearly flat
/// and the tie-break wanders — which is why the arena stalls were on the
/// trained side and why this test needs a biased evaluator to reproduce
/// them.
struct ToggleLover;

impl PolicyEvaluator for ToggleLover {
    fn evaluate(&self, state: &GameState, registry: &CardRegistry) -> (Vec<f32>, f32) {
        let mask = get_action_mask(state, registry);
        let mut priors = vec![0.0f32; ActionSpace::SIZE];
        match ActionSpace::index_of(state, &PlayerAction::ToggleCardSelection { position: 1 }) {
            Some(index) if mask[index] => priors[index] = 1.0,
            _ => {
                let legal = mask.iter().filter(|&&l| l).count().max(1) as f32;
                for (prior, &legal_here) in priors.iter_mut().zip(&mask) {
                    if legal_here {
                        *prior = 1.0 / legal;
                    }
                }
            }
        }
        (priors, 0.0)
    }
}

fn puct_seat(side: Side, seed: u64, evaluator: impl PolicyEvaluator + 'static) -> Seat {
    Seat::Agent(Box::new(PuctAgent::with_config(
        side,
        seed,
        evaluator,
        PuctConfig { c_puct: 1.5, iterations: 16, max_depth: 4 },
    )))
}

/// Once the selection is confirmed the swap resolves and the game runs off
/// the empty decks within a handful of steps, so a session that is still
/// going after a couple of hundred is cycling on the toggle. Fails on the
/// `select_action` that had no cycle break, at every seed.
#[test]
fn a_puct_seat_does_not_toggle_a_card_selection_until_the_budget_runs_out() {
    for seed in 0..12u64 {
        let (state, registry) = tao_swap_pending();
        let mut session = Session::new(
            state,
            registry,
            puct_seat(Side::Corp, seed, UniformPolicyEvaluator::new(Side::Corp)),
            puct_seat(Side::Runner, seed + 100, ToggleLover),
        )
        .with_max_steps(200);
        let outcome = session.run();
        assert!(
            !matches!(outcome, SessionStep::Stalled(StallReason::BudgetExhausted)),
            "seed {seed}: the PUCT seat toggled the selection for 200 steps without confirming it"
        );
    }
}

