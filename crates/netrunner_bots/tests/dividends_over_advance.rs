//! The one decision `eval::AGENDA_COUNTER_WEIGHT` exists to change, and
//! the search budget it takes to find it.
//!
//! A Corp holding a fully advanced Dividends agenda with clicks to spare
//! can spend one more click before scoring and bank a counter — scoring
//! costs no click, so both lines end with the agenda scored and the
//! second is one click and one credit dearer for one counter. This test
//! pins two things the 192-game reports cannot say precisely: that the
//! term moves the *value* at all, and that it does not for an agenda that
//! pays no Dividends.
//!
//! **It also records the budget, and the budget moved.** A state evaluator
//! cannot prefer the intermediate position on its own — the over-advanced
//! agenda is worth ~2 while scoring it is worth ~40 — so the preference
//! only appears once the search reaches the position *after* the score.
//! Until September 2026 that took roughly 512 iterations. Since `EndTurn`
//! is refused while clicks remain (CR 5.6.2b) and a turn begins with a
//! window of its own (CR 5.6.1b), a turn is several plies longer, and no
//! budget up to 4096 finds it: the 1.2 the counter is worth at the end of
//! the turn is inside what the Runner's replies do to a 16-ply leaf.
//! Recorded rather than tuned away: raising the weight until a search
//! found it would mean valuing a counter above a point of agenda, which
//! is wrong. The evaluator's own preference is pinned below instead.

use netrunner_bots::{BotAgent, HeuristicAgent, PuctAgent, PuctConfig, UniformPolicyEvaluator};
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardDefinition, CardId, CardType};
use netrunner_core::rules::{
    AgendaPoints, Clicks, Credits, GamePhase, GameState, InstallId, InstalledCard, PlayerAction, PlayerResources,
    ServerId, Side,
};
use netrunner_core::view::build_client_view;

/// A 3/2 in a remote with its requirement met, three clicks and eight
/// credits: the position where "score now" and "advance once more, then
/// score" are both available and differ only by the counter.
fn position(dividends: Option<u32>) -> (GameState, CardRegistry) {
    let mut registry = CardRegistry::new();
    registry.insert(CardDefinition {
        title: "dividend_agenda".into(),
        id: CardId("dividend_agenda".into()),
        side: Side::Corp,
        card_type: CardType::Agenda,
        advancement_requirement: Some(3),
        agenda_points: Some(2),
        dividends,
        ..CardDefinition::default()
    });

    let mut state = GameState::new(0);
    state.phase = GamePhase::Action(Side::Corp);
    state.corp.resources = PlayerResources { credits: Credits(8), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
    state.corp.installed = vec![InstalledCard {
        card: CardId("dividend_agenda".into()),
        install_id: InstallId(1),
        server: ServerId::Remote(0),
        advancement_tokens: 3,
        ..Default::default()
    }];
    (state, registry)
}

fn choice_at(dividends: Option<u32>, iterations: usize) -> PlayerAction {
    let (state, registry) = position(dividends);
    let view = build_client_view(&state, &registry, Side::Corp);
    let mut agent = PuctAgent::with_config(
        Side::Corp,
        1,
        UniformPolicyEvaluator::new(Side::Corp),
        PuctConfig { iterations, ..PuctConfig::default() },
    );
    agent.select_action(&view, &registry)
}

const ADVANCE: PlayerAction = PlayerAction::AdvanceCard { target: InstallId(1) };
const SCORE: PlayerAction = PlayerAction::ScoreAgenda { target: InstallId(1) };

/// The term, where it acts: after "advance once more, then score" the
/// Corp's position is worth more than after "score now" — the counter
/// (2.0) against the click and credit it cost (0.8) — and for an agenda
/// that pays no Dividends it is worth less.
#[test]
fn the_evaluator_prefers_the_over_advanced_score_only_for_a_dividends_agenda() {
    use netrunner_core::rules::apply_action;
    for (dividends, better) in [(Some(1), true), (None, false)] {
        let (state, registry) = position(dividends);
        let value = |s: &GameState| netrunner_bots::evaluate_state(s, Side::Corp, &registry);
        let scored = apply_action(&state, &registry, SCORE).expect("score").0;
        let advanced = apply_action(&state, &registry, ADVANCE).expect("advance").0;
        let advanced_then_scored = apply_action(&advanced, &registry, SCORE).expect("then score").0;
        assert_eq!(value(&advanced_then_scored) > value(&scored), better, "dividends {dividends:?}");
    }
}

/// The budget, recorded: see the module comment for why a deep search
/// now scores at once.
#[test]
fn a_deep_search_now_scores_a_dividends_agenda_at_once() {
    assert_eq!(choice_at(Some(1), 512), SCORE, "found at 512 before a turn's clicks had to be spent");
}

/// The control, and the one that would catch a weight set too high: the
/// same position on an agenda that pays nothing for the extra token must
/// not be advanced at any budget.
#[test]
fn the_same_search_never_over_advances_an_agenda_that_pays_no_dividends() {
    for iterations in [32, 128, 512, 2048] {
        assert_ne!(choice_at(None, iterations), ADVANCE, "no Dividends, no reason to pile on (at {iterations})");
    }
}

/// The limit, stated so a later change to the weight or the search shows
/// up here rather than in a puzzling report: at the budget the benchmark
/// and the coverage reports run, neither seat finds this.
#[test]
fn a_shallow_search_and_the_one_ply_heuristic_both_score_at_once() {
    for iterations in [32, 128] {
        assert_eq!(choice_at(Some(1), iterations), SCORE, "puct@{iterations} scores rather than banking a counter");
    }

    let (state, registry) = position(Some(1));
    let view = build_client_view(&state, &registry, Side::Corp);
    let mut heuristic = HeuristicAgent::new(Side::Corp, 1);
    assert_eq!(heuristic.select_action(&view, &registry), SCORE, "one ply cannot see the score that is still there");
}
