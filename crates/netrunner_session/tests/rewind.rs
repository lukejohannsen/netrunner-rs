//! Taking a move back (ROADMAP Phase 7 §8 item 4b and 4c): what
//! `Session::rewind` restores, when that is *free*, and that the record
//! still replays afterwards.

use netrunner_bots::RandomAgent;
use netrunner_core::cards::{register_playable_cards, CardRegistry};
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{apply_action, GamePhase, GameState, InstallId, InstalledRunnerCard, PlayerAction, ServerId, Side};
use netrunner_session::{Rewind, Seat, Session, SessionStep, UNDO_DEPTH};

fn registry() -> CardRegistry {
    let mut registry = CardRegistry::new();
    register_playable_cards(&mut registry);
    registry
}

fn card(id: &str) -> CardId {
    CardId(id.to_string())
}

/// The Runner's turn, four clicks, Red Team installed with Archives already
/// run, a stack to draw from and Jailbreak in the grip.
fn the_report() -> GameState {
    let mut state = GameState::new(1);
    state.phase = GamePhase::Action(Side::Runner);
    state.turn = 2;
    state.runner.resources.clicks.0 = 4;
    state.runner.resources.credits.0 = 5;
    state.runner.rig.push(InstalledRunnerCard { card: card("red_team"), install_id: InstallId(100), counters: 12, ..Default::default() });
    state.runner.servers_run_this_turn.push(ServerId::Archives);
    state.runner.grip = vec![card("jailbreak")];
    state.runner.stack = vec![card("sure_gamble"), card("sure_gamble"), card("sure_gamble")];
    state.corp.r_and_d = vec![card("hedge_fund"), card("hedge_fund"), card("hedge_fund")];
    state
}

fn person_as_runner(state: GameState) -> Session {
    Session::new(state, registry(), Seat::Agent(Box::new(RandomAgent::new(3))), Seat::External).with_undo(UNDO_DEPTH)
}

fn red_team(session: &mut Session) -> PlayerAction {
    let SessionStep::Awaiting { view, .. } = session.step() else { panic!("the Runner is a person") };
    view.legal_actions.iter().find(|action| matches!(action, PlayerAction::ActivateAbility { .. })).cloned().expect("Red Team's ability is legal")
}

/// The report from play: Red Team spent its click and then showed which
/// servers were left. Still on its prompt, the person puts it back and has
/// exactly what they had — and that costs nothing, because it taught them
/// nothing.
#[test]
fn a_card_still_on_its_own_prompt_goes_back_for_free() {
    let before = the_report();
    let mut session = person_as_runner(before.clone());
    assert_eq!(session.can_rewind(), None, "nothing has been done yet");

    let ability = red_team(&mut session);
    session.submit(ability).unwrap();
    assert_eq!(session.state().runner.resources.clicks.0, 3, "the click is spent before the question is asked");
    assert!(session.state().pending_decision.is_some());
    assert_eq!(session.can_rewind(), Some(Rewind::Free));

    let rewound = session.rewind().expect("there is a move to take back");
    assert_eq!((rewound.kind, rewound.removed), (Rewind::Free, 1));
    assert_eq!(session.state(), &before);
    assert!(session.history().is_empty());
    assert_eq!(session.can_rewind(), None, "one move, one take-back");
    // And the person is asked again, from the state they had.
    assert!(matches!(session.step(), SessionStep::Awaiting { side: Side::Runner, .. }));
}

/// Once the server is chosen the run has begun: the prompt is answered, so
/// what is left is an undo, not a take-back.
#[test]
fn answering_the_prompt_ends_the_free_take_back() {
    let mut session = person_as_runner(the_report());
    let ability = red_team(&mut session);
    session.submit(ability).unwrap();
    session.submit(PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }).unwrap();
    assert!(session.state().active_run.is_some());
    assert_eq!(session.can_rewind(), Some(Rewind::Undo));
}

/// jinteki's `/undo-click` rewinds a draw with no questions asked. Here the
/// state goes back just as exactly, and the session says what it was.
#[test]
fn a_draw_can_be_undone_but_not_for_free() {
    let before = the_report();
    let mut session = person_as_runner(before.clone());
    session.submit(PlayerAction::DrawCardClick { side: Side::Runner }).unwrap();
    assert_eq!(session.state().runner.grip.len(), 2);
    assert_eq!(session.can_rewind(), Some(Rewind::Undo));
    let rewound = session.rewind().unwrap();
    assert_eq!(rewound.kind, Rewind::Undo);
    assert_eq!(session.state(), &before);
}

/// Moves come back newest first, `UNDO_DEPTH` deep, and never across a turn.
#[test]
fn undo_is_a_short_stack_inside_one_turn() {
    let before = the_report();
    let mut session = person_as_runner(before.clone());
    session.submit(PlayerAction::GainCreditClick { side: Side::Runner }).unwrap();
    let after_one = session.state().clone();
    session.submit(PlayerAction::GainCreditClick { side: Side::Runner }).unwrap();
    assert_eq!(session.rewind().map(|r| r.removed), Some(1));
    assert_eq!(session.state(), &after_one);
    assert_eq!(session.rewind().map(|r| r.removed), Some(1));
    assert_eq!(session.state(), &before);
    assert_eq!(session.rewind(), None);

    // Spend the turn and end it: the next turn is not this one's to undo.
    for _ in 0..4 {
        session.submit(PlayerAction::GainCreditClick { side: Side::Runner }).unwrap();
    }
    assert!(session.can_rewind().is_some());
    session.submit(PlayerAction::EndTurn).unwrap();
    // The turn's closing window is the person's to pass; then the bot
    // plays its turn and the person is asked again in a new one.
    for _ in 0..200 {
        match session.step() {
            SessionStep::Applied { .. } => {}
            SessionStep::Awaiting { view, .. } if view.turn == 2 => {
                let pass = view.legal_actions.iter().find(|action| matches!(action, PlayerAction::PassPriority { .. })).cloned();
                session.submit(pass.expect("only a pass is left in a finished turn")).unwrap();
            }
            _ => break,
        }
    }
    assert!(session.state().turn > 2, "{:?}", session.state().phase);
    assert_eq!(session.can_rewind(), None, "the last turn is not this one's to take back");
}

/// Off unless asked for: self-play, the gym and the server pay nothing.
#[test]
fn a_session_without_undo_keeps_nothing() {
    let mut session = Session::new(the_report(), registry(), Seat::Agent(Box::new(RandomAgent::new(3))), Seat::External);
    session.submit(PlayerAction::GainCreditClick { side: Side::Runner }).unwrap();
    assert_eq!(session.can_rewind(), None);
    assert_eq!(session.rewind(), None);
}

/// The replay invariant survives: after a take-back and a different move,
/// the recorded actions still reproduce the state beside them.
#[test]
fn the_record_still_replays_after_a_take_back() {
    let start = the_report();
    let mut session = person_as_runner(start.clone());
    let ability = red_team(&mut session);
    session.submit(ability).unwrap();
    session.rewind().unwrap();
    session.submit(PlayerAction::PlayEvent { card_id: card("jailbreak") }).unwrap();
    session.submit(PlayerAction::ChooseServerForPendingDecision { server: ServerId::RnD }).unwrap();

    let registry = registry();
    let mut replayed = start;
    for entry in session.history().entries() {
        replayed = apply_action(&replayed, &registry, entry.action.clone()).expect("a recorded action replays cleanly").0;
    }
    assert_eq!(&replayed, session.state());
    assert_eq!(session.history().len(), 2, "the move taken back is not in the record");
}
