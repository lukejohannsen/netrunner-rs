//! `PuctAgent` as a `Seat::Agent`: the shape the arena and the `puct` /
//! `puct-onnx` headless bots play in. Self-play drives `PuctAgent::search`
//! through its own picker; this is the *other* path, `BotAgent::select_action`,
//! and it is the one that stalled 14–23 of every 48 arena games in the
//! September 2026 volume run (ROADMAP Phase 2 §5).

use netrunner_bots::{HeuristicAgent, MctsAgent, PolicyEvaluator, PuctAgent, PuctConfig, RandomAgent, UniformPolicyEvaluator};
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

/// Plutus's alternative rez cost as the engine parks it — exactly three of a
/// five-card HQ, the Corp choosing — which is the shape that livelocked 133
/// self-play games (ROADMAP Phase 2 §5). Five slots, so a chooser that
/// deselects has a large space to wander in; a chooser that cannot has at
/// most four moves.
fn plutus_pay_pending() -> (GameState, CardRegistry) {
    let hand = ["hedge_fund", "ice_wall", "offworld_office", "regolith_mining_license", "government_subsidy"];
    let mut registry = CardRegistry::new();
    for id in hand {
        registry.insert(CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Operation,
            is_playable: true,
            ..Default::default()
        });
    }
    let mut state = GameState::new(0);
    state.phase = GamePhase::Action(Side::Corp);
    state.corp.hq = hand.iter().map(|id| CardId(id.to_string())).collect();
    state.pending_decision = Some(PendingDecision::ChooseCards {
        side: Side::Corp,
        source: CardZoneRef::OwnHq,
        filter: CardFilter::Any,
        min: 3,
        max: 3,
        reveal: true,
        shuffle_after: false,
        destination: Some(CardZoneRef::OwnArchives),
        then: None,
        selected: Vec::new(),
        source_card: Some(CardId("plutus".to_string())),
        source_install: None,
        resume: PendingChoiceResume::None,
    });
    (state, registry)
}

/// Every agent kind in the crate, seated as `side`, at a token search
/// budget. The livelock was a property of *how a bot chooses*, so every
/// chooser has to be shown to have lost it.
fn every_agent_kind(side: Side) -> Vec<(&'static str, Seat)> {
    vec![
        ("random", Seat::Agent(Box::new(RandomAgent::new(7)))),
        ("heuristic", Seat::Agent(Box::new(HeuristicAgent::new(side, 7)))),
        ("mcts", Seat::Agent(Box::new(MctsAgent::with_iterations(side, 7, 16)))),
        ("puct/uniform", puct_seat(side, 7, UniformPolicyEvaluator::new(side))),
        ("puct/toggle-lover", puct_seat(side, 7, ToggleLover)),
    ]
}

/// Drives `session` until the parked prompt clears and asserts the chooser
/// got there in at most `max_own_actions` of its own moves without ever
/// toggling a card it had already selected. The bound is `max + 1`
/// (select up to `max`, then Confirm) less whatever was pre-selected.
fn resolves_without_deselecting(label: &str, chooser: Side, mut session: Session, max_own_actions: usize) {
    let mut own = 0;
    let mut toggled: Vec<usize> = Vec::new();
    while session.state().pending_decision.is_some() {
        match session.step() {
            SessionStep::Applied { side } => {
                assert_eq!(side, chooser, "{label}: only the chooser acts during its own prompt");
                own += 1;
                assert!(own <= max_own_actions, "{label}: {own} actions in and the prompt is still open");
                let last = session.history().entries().last().expect("an applied step is recorded");
                if let PlayerAction::ToggleCardSelection { position } = last.action {
                    assert!(!toggled.contains(&position), "{label}: deselected position {position} — a bot must only ever add to a selection");
                    toggled.push(position);
                }
            }
            other => panic!("{label}: {other:?} before the prompt resolved"),
        }
    }
}

/// The fix for the livelock, pinned per agent kind: no bot deselects, so a
/// `min == max` prompt is resolved in `max + 1` moves or fewer. Before
/// `agent::progressive` the PUCT seats here toggled until the budget ran
/// out and the random seat random-walked the subset space; the one-ply
/// heuristic scored a deselect identically to the select it undid and let
/// its tie-break jitter decide.
#[test]
fn every_agent_kind_resolves_an_exact_count_prompt_without_deselecting() {
    // Tāo's swap: Runner chooses, one of two already selected → 1 select + Confirm.
    for (label, runner) in every_agent_kind(Side::Runner) {
        let (state, registry) = tao_swap_pending();
        let corp = puct_seat(Side::Corp, 99, UniformPolicyEvaluator::new(Side::Corp));
        let session = Session::new(state, registry, corp, runner);
        resolves_without_deselecting(&format!("tao/{label}"), Side::Runner, session, 2);
    }
    // Plutus's cost: Corp chooses 3 of 5 from nothing → 3 selects + Confirm.
    for (label, corp) in every_agent_kind(Side::Corp) {
        let (state, registry) = plutus_pay_pending();
        let runner = Seat::Agent(Box::new(RandomAgent::new(99)));
        let session = Session::new(state, registry, corp, runner);
        resolves_without_deselecting(&format!("plutus/{label}"), Side::Corp, session, 4);
    }
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

