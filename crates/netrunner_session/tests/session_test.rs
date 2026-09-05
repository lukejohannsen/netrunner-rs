//! Pins the `Session` contracts every consumer depends on. Uses the
//! embedded sample decks rather than a hand-built fixture — the near-copies
//! of `kate_vs_hb_*` in `netrunner_server::fixtures` and
//! `netrunner_single_player::tests::common` exist only because those crates
//! predate `decks::matchups()`.

use netrunner_bots::RandomAgent;
use netrunner_core::cards::{register_playable_cards, CardRegistry};
use netrunner_core::decks;
use netrunner_core::rules::{GamePhase, GameState, MatchRules, PlayerAction, Side};
use netrunner_session::session::{Seat, SessionStep, SubmitError};
use netrunner_session::{HistoryReadError, MatchHistory, MatchRecordHeader, Session};

fn setup(seed: u64) -> (GameState, CardRegistry) {
    let mut registry = CardRegistry::new();
    register_playable_cards(&mut registry);
    let (corp, runner) = decks::matchups().into_iter().next().expect("at least one sample matchup");
    let (state, _events) = GameState::setup(&corp.to_deck(), &runner.to_deck(), &registry, seed)
        .expect("sample decks set up cleanly");
    (state, registry)
}

fn bot_vs_bot(seed: u64) -> Session {
    let (state, registry) = setup(seed);
    Session::new(
        state,
        registry,
        Seat::Agent(Box::new(RandomAgent::new(seed))),
        Seat::Agent(Box::new(RandomAgent::new(seed.wrapping_add(1)))),
    )
}

#[test]
fn two_agent_seats_play_a_whole_match_in_one_run_call() {
    let mut session = bot_vs_bot(1);
    match session.run() {
        SessionStep::Ended { .. } => {}
        other => panic!("expected Ended, got {}", describe(&other)),
    }
    assert!(matches!(session.state().phase, GamePhase::GameOver(_)));
    assert!(!session.history().is_empty(), "an agent-driven match records every action");
}

/// Uses seed 1 rather than 2 deliberately: seed 2 reaches a genuine engine
/// deadlock (`current_actor` names the Corp during a `Run`-checkpoint
/// window while the Corp has no legal action at all), which `Session`
/// reports as `Stalled(NoLegalActions)`. That is a real open bug, tracked
/// in ROADMAP.md — not something this test should paper over, and the
/// reason `StallReason::NoLegalActions` exists at all.
#[test]
fn ended_is_idempotent_so_a_polling_pump_is_safe() {
    let mut session = bot_vs_bot(1);
    let SessionStep::Ended { winner, reason } = session.run() else {
        panic!("expected Ended");
    };
    for _ in 0..3 {
        match session.step() {
            SessionStep::Ended { winner: w, reason: r } => {
                assert_eq!((w, r), (winner, reason), "Ended must report the same outcome every time");
            }
            other => panic!("expected Ended again, got {}", describe(&other)),
        }
    }
}

#[test]
fn an_external_seat_yields_awaiting_with_only_a_masked_view() {
    let (state, registry) = setup(3);
    let mut session = Session::new(state, registry, Seat::External, Seat::Agent(Box::new(RandomAgent::new(3))));

    match session.step() {
        SessionStep::Awaiting { side, view } => {
            assert_eq!(side, Side::Corp, "Corp mulligans first");
            assert!(!view.legal_actions.is_empty());
            // The whole point of the seat boundary: a Corp seat is handed
            // its own hand and never the Runner's.
            assert!(view.corp.hq_cards.is_some());
            assert!(view.runner.grip_cards.is_none());
        }
        other => panic!("expected Awaiting, got {}", describe(&other)),
    }
    assert_eq!(session.awaiting(), Some(Side::Corp));
}

#[test]
fn polling_step_while_awaiting_never_consumes_budget() {
    let (state, registry) = setup(4);
    let mut session = Session::new(state, registry, Seat::External, Seat::External);

    for _ in 0..50 {
        assert!(matches!(session.step(), SessionStep::Awaiting { .. }));
    }
    assert_eq!(session.steps(), 0, "only an applied action may consume the step budget");
}

#[test]
fn a_rejected_submit_leaves_the_state_and_the_awaiting_side_untouched() {
    let (state, registry) = setup(5);
    let mut session = Session::new(state, registry, Seat::External, Seat::Agent(Box::new(RandomAgent::new(5))));

    let SessionStep::Awaiting { side, .. } = session.step() else { panic!("expected Awaiting") };
    let before = session.state().clone();

    // Illegal during the Mulligan phase.
    let rejected = session.submit(PlayerAction::EndTurn);
    assert!(matches!(rejected, Err(SubmitError::Rules(_))), "got {rejected:?}");

    assert_eq!(session.state(), &before, "a rejected submit must not advance the state");
    assert_eq!(session.awaiting(), Some(side), "the same side is still on the hook");
    assert_eq!(session.steps(), 0);
    assert!(session.history().is_empty(), "a rejected action must never reach the log");

    // And the session is still usable.
    assert!(session.submit(PlayerAction::KeepHand).is_ok());
    assert_eq!(session.steps(), 1);
}

#[test]
fn submitting_after_the_match_ends_is_refused() {
    let mut session = bot_vs_bot(6);
    assert!(matches!(session.run(), SessionStep::Ended { .. }));
    assert_eq!(session.submit(PlayerAction::EndTurn), Err(SubmitError::Ended));
}

/// The JSON-Lines record carries everything a replay needs: the header
/// reproduces the opening position, the entries reproduce the final one.
#[test]
fn a_recorded_history_round_trips_through_jsonl_and_replays_to_the_same_state() {
    let seed = 9;
    let mut session = bot_vs_bot(seed);
    assert!(matches!(session.run(), SessionStep::Ended { .. }));
    let (final_state, history) = session.into_parts();

    let (corp, runner) = decks::matchups().into_iter().next().expect("at least one sample matchup");
    let header = MatchRecordHeader { seed, corp_deck: corp.to_deck(), runner_deck: runner.to_deck(), rules: MatchRules::default() };
    let mut bytes = Vec::new();
    history.write_jsonl(&header, &mut bytes).expect("writing to a Vec cannot fail");
    let text = String::from_utf8(bytes).expect("JSON is UTF-8");
    assert_eq!(text.lines().count(), history.len() + 1, "one header line, then one line per entry");

    let (read_header, read_history) = MatchHistory::read_jsonl(text.as_bytes()).expect("the record reads back");
    assert_eq!(read_header, header);
    assert_eq!(read_history, history);

    let mut registry = CardRegistry::new();
    register_playable_cards(&mut registry);
    let (mut replayed, _events) = read_header.setup(&registry).expect("the header's decks set up");
    for entry in read_history.entries() {
        replayed = netrunner_core::rules::apply_action(&replayed, &registry, entry.action.clone())
            .expect("a recorded action replays cleanly")
            .0;
    }
    assert_eq!(replayed, final_state);

    assert!(matches!(MatchHistory::read_jsonl("".as_bytes()), Err(HistoryReadError::MissingHeader)));
    assert!(matches!(MatchHistory::read_jsonl("not json\n".as_bytes()), Err(HistoryReadError::Json { line: 1, .. })));
}

#[test]
fn history_records_the_pre_action_turn_and_replays_to_the_same_state() {
    let seed = 7;
    let mut session = bot_vs_bot(seed);
    assert!(matches!(session.run(), SessionStep::Ended { .. }));
    let (final_state, history) = session.into_parts();

    assert_eq!(history.entries()[0].turn_number, 0, "mulligan actions are turn 0");
    for pair in history.entries().windows(2) {
        let (previous, next) = (pair[0].turn_number, pair[1].turn_number);
        assert!(next >= previous && next - previous <= 1, "turns advance monotonically, one at a time");
    }
    assert_eq!(history.entries().last().unwrap().turn_number, final_state.turn);

    // The invariant that makes the log a replay log: nothing may advance
    // the session's state except an action that was recorded.
    let (mut replayed, registry) = setup(seed);
    for entry in history.entries() {
        replayed = netrunner_core::rules::apply_action(&replayed, &registry, entry.action.clone())
            .expect("a recorded action replays cleanly")
            .0;
    }
    assert_eq!(replayed, final_state);
}

#[test]
fn without_history_records_nothing_but_still_plays_and_classifies() {
    let (state, registry) = setup(8);
    let mut session = Session::new(
        state,
        registry,
        Seat::Agent(Box::new(RandomAgent::new(8))),
        Seat::Agent(Box::new(RandomAgent::new(9))),
    )
    .without_history();

    // `Ended`'s reason is computed at apply time precisely so it survives
    // history being off.
    assert!(matches!(session.run(), SessionStep::Ended { .. }));
    assert!(session.history().is_empty());
    assert!(session.last_entry().is_none());
}

#[test]
fn an_exhausted_budget_is_reported_as_such_and_not_as_a_stall() {
    let (state, registry) = setup(10);
    let mut session = Session::new(
        state,
        registry,
        Seat::Agent(Box::new(RandomAgent::new(10))),
        Seat::Agent(Box::new(RandomAgent::new(11))),
    )
    .with_max_steps(5);

    match session.run() {
        SessionStep::Stalled(reason) => {
            assert_eq!(reason, netrunner_session::StallReason::BudgetExhausted);
        }
        other => panic!("expected a budget stall, got {}", describe(&other)),
    }
    assert_eq!(session.steps(), 5);
}

/// The livelock the volume runs were burning 10,000 actions on, reduced to
/// its essentials: a chooser that only ever toggles inside a `min == max`
/// card selection. The session must stop it at `DECISION_BUDGET` and say
/// *which card's* prompt it was — not run to `MAX_STEPS` and report a
/// budget stall indistinguishable from a slow game.
#[test]
fn a_decision_that_never_resolves_is_reported_as_a_livelock_naming_the_card() {
    use netrunner_bots::BotAgent;
    use netrunner_core::dsl::{CardDefinition, CardFilter, CardId, CardType, CardZoneRef};
    use netrunner_core::rules::{PendingChoiceResume, PendingDecision};
    use netrunner_core::view::ClientView;
    use netrunner_session::{StallReason, DECISION_BUDGET, MAX_STEPS};

    /// Always picks a toggle when one is offered — the greedy chooser that,
    /// indifferent between equal-valued toggles, never lands on Confirm.
    struct AlwaysToggles;
    impl BotAgent for AlwaysToggles {
        fn select_action(&mut self, view: &ClientView, _registry: &CardRegistry) -> PlayerAction {
            view.legal_actions
                .iter()
                .find(|a| matches!(a, PlayerAction::ToggleCardSelection { .. }))
                .unwrap_or(&view.legal_actions[0])
                .clone()
        }
    }

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
    // Plutus's alternative rez cost, as the engine parks it: exactly three
    // of the five, with the card that asked recorded on the decision.
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

    let mut session = Session::new(
        state,
        registry,
        Seat::Agent(Box::new(AlwaysToggles)),
        Seat::Agent(Box::new(RandomAgent::new(1))),
    );
    let outcome = session.run();

    assert_eq!(
        outcome_reason(&outcome),
        Some(StallReason::DecisionLivelock {
            side: Side::Corp,
            source_card: Some(CardId("plutus".to_string())),
            actions: DECISION_BUDGET,
        }),
        "got {}",
        describe(&outcome)
    );
    assert_eq!(session.steps(), DECISION_BUDGET, "stopped at the decision budget, not the step budget");
    // The whole point of a separate budget: this stopped in 256 actions where
    // `BudgetExhausted` would have cost `MAX_STEPS`. An order of magnitude
    // is the bar — if the two ever drift close, one of them is wrong.
    assert!(session.steps() * 8 <= MAX_STEPS, "the livelock budget ({}) must be far cheaper than the step budget ({MAX_STEPS})", session.steps());
}

fn outcome_reason(step: &SessionStep) -> Option<netrunner_session::StallReason> {
    match step {
        SessionStep::Stalled(reason) => Some(reason.clone()),
        _ => None,
    }
}

fn describe(step: &SessionStep) -> String {
    match step {
        SessionStep::Applied { side } => format!("Applied({side:?})"),
        SessionStep::Awaiting { side, .. } => format!("Awaiting({side:?})"),
        SessionStep::Ended { winner, reason } => format!("Ended({winner:?}, {reason:?})"),
        SessionStep::Stalled(reason) => format!("Stalled({reason:?})"),
    }
}
