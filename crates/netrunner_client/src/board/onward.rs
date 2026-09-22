//! What the one Continue button leads to, read off the view.
//!
//! **Three actions move the game on without doing anything, and they were
//! three buttons that said nothing about where.** `PassPriority` closes a
//! paid-ability window once both players have passed; `ContinueRun`
//! advances a run no window is holding; `CompleteRun` declares it
//! successful and breaches. None of them says whether the ice is about to
//! be encountered, whether its subroutines are about to fire, or whether
//! the server is about to be breached — so a person pressed them to find
//! out. jinteki.net's one button names the step ("Continue to Approach
//! ice", "Breach server"), and this is that.
//!
//! **At most one of the three is ever legal for a seat, so one button can
//! stand for all three.** `PassPriority` needs an open window, the other
//! two need none; `CompleteRun` is legal only at `RunPhase::Success`, where
//! `ContinueRun` never is. `JackOut` is the one action that sits beside
//! one of them, and it keeps a button of its own because it gives
//! something up. `the_label_names_what_the_engine_does_next` holds both
//! claims to real games.
//!
//! **The step named is the one that follows if nobody does anything
//! else** — both players pass. A pass in a window the other player has
//! not passed in yet hands them priority first, and they may rez or break
//! instead; the button still says where the game goes if they let it,
//! because that is what the person pressing it is agreeing to.
//!
//! Everything here is read off the masked [`ClientView`], like the phase
//! bar ([`super::phase`]): no engine state and no rules. The table the
//! match below writes out is the engine's (`paid_ability::close_window`,
//! `run::engine::continue_run`), and the test is what keeps the two in
//! step.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{PlayerAction, RunPhase, ServerId, Side, SubroutineStatus, WindowCheckpoint};
use netrunner_core::view::ClientView;

use super::action_map::{server_name, Control};
use crate::actions::{card_title, describe_action};

/// The step the game moves to once the Continue button's action is taken
/// and nobody does anything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Onward {
    /// The ice at `position` (from the outermost, from zero), of `of`.
    ApproachIce { position: usize, of: usize },
    /// The attacked server, past its last ice.
    ApproachServer(ServerId),
    /// The approached ice, which is rezzed. `None` when the viewer may not
    /// name it.
    EncounterIce(Option<CardId>),
    /// The unbroken subroutines of the encountered ice resolve.
    FireSubroutines(usize),
    /// The movement phase: past an ice, or into a server with none.
    Movement,
    /// The Runner's decision to jack out or go on, which a Corp window
    /// can stand in front of.
    JackOutDecision,
    /// The run is declared successful and the server breached.
    Breach(ServerId),
    /// The turn formally begins (recurring credits, the Corp's draw).
    TurnBegins(Side),
    /// `side`'s action phase.
    Actions(Side),
    /// `side`'s turn ends, and the other side's begins.
    TurnEnds(Side),
    /// What is parked for prevention happens.
    NotPrevented,
}

/// Where the Continue button goes from `view`, or `None` when the view is
/// at no step one of the three actions moves on from.
pub fn onward(view: &ClientView) -> Option<Onward> {
    if let Some(window) = &view.paid_ability_window {
        return match window.checkpoint {
            WindowCheckpoint::TurnBeginning { side } => Some(Onward::TurnBegins(side)),
            WindowCheckpoint::StartOfTurn { side } | WindowCheckpoint::PostAction { side } => Some(Onward::Actions(side)),
            WindowCheckpoint::EndOfTurn { side } => Some(Onward::TurnEnds(side)),
            WindowCheckpoint::Prevention => Some(Onward::NotPrevented),
            WindowCheckpoint::Run => run_step(view, true),
        };
    }
    run_step(view, false)
}

/// The run's next step. `window` says whether a window is holding it,
/// which matters only in the movement phase: a window in front of the
/// Runner's jack-out decision closes onto that decision, and the Runner
/// going on (no window) approaches what is next.
fn run_step(view: &ClientView, window: bool) -> Option<Onward> {
    let run = view.active_run.as_ref()?;
    let next = |position: usize| {
        if position < run.ice.len() {
            Onward::ApproachIce { position, of: run.ice.len() }
        } else {
            Onward::ApproachServer(run.server)
        }
    };
    match run.phase {
        RunPhase::Initiation if run.ice.is_empty() => Some(Onward::Movement),
        RunPhase::Initiation => Some(next(0)),
        RunPhase::ApproachIce => {
            let ice = run.ice.get(run.position)?;
            Some(if ice.rezzed { Onward::EncounterIce(ice.identity.as_ref().map(|i| i.card.clone())) } else { Onward::Movement })
        }
        RunPhase::EncounterIce => {
            let pending = run
                .ice
                .get(run.position)
                .and_then(|ice| ice.identity.as_ref())
                .map_or(0, |identity| identity.subroutines.iter().filter(|sub| sub.status == SubroutineStatus::Pending).count());
            Some(if pending > 0 { Onward::FireSubroutines(pending) } else { Onward::Movement })
        }
        RunPhase::Movement if run.jack_out_permitted && window => Some(Onward::JackOutDecision),
        RunPhase::Movement => Some(next(run.position)),
        RunPhase::Success if !run.declared_successful => Some(Onward::Breach(run.server)),
        RunPhase::Success | RunPhase::AccessingCard | RunPhase::Ended => None,
    }
}

/// The words for an action a seat is offered in `view`: the step for one
/// of the Continue button's three actions, `describe_action`'s label for
/// everything else. For a list of what can be done *now* — the play
/// helper, the terminal's list — and never for the log, which words an
/// action against the view it produced, where the step is already taken.
pub fn offered_label(action: &PlayerAction, registry: &CardRegistry, view: Option<&ClientView>) -> String {
    match view.filter(|_| Control::Continue.matches(action)).and_then(|view| onward(view).map(|step| step.label(registry, view.viewer.side()))) {
        Some(words) => words,
        None => describe_action(action, registry, view),
    }
}

impl Onward {
    /// The button's words, from `viewer`'s chair (`None` for a spectator,
    /// who is told the sides by name).
    pub fn label(&self, registry: &CardRegistry, viewer: Option<Side>) -> String {
        let whose = |side: Side| match (viewer == Some(side), side) {
            (true, _) => "your".to_string(),
            (false, Side::Corp) => "the Corp's".to_string(),
            (false, Side::Runner) => "the Runner's".to_string(),
        };
        match self {
            Onward::ApproachIce { position, of } => format!("Continue to Approach ice {} of {of}", position + 1),
            Onward::ApproachServer(server) => format!("Continue to Approach {}", server_name(*server)),
            Onward::EncounterIce(Some(card)) => format!("Continue to Encounter {}", card_title(card, registry)),
            Onward::EncounterIce(None) => "Continue to Encounter the ice".to_string(),
            Onward::FireSubroutines(1) => "Let 1 subroutine fire".to_string(),
            Onward::FireSubroutines(n) => format!("Let {n} subroutines fire"),
            Onward::Movement => "Continue to Movement".to_string(),
            Onward::JackOutDecision => format!("Continue to {} jack-out decision", whose(Side::Runner)),
            Onward::Breach(server) => format!("Breach {}", server_name(*server)),
            Onward::TurnBegins(side) => format!("Begin {} turn", whose(*side)),
            Onward::Actions(side) => format!("Continue to {} actions", whose(*side)),
            Onward::TurnEnds(side) if viewer == Some(*side) => "End your turn".to_string(),
            Onward::TurnEnds(side) => format!("Continue to {} turn", whose(side.other())),
            Onward::NotPrevented => "Don't prevent it".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::{BotAgent, HeuristicAgent, RandomAgent};
    use netrunner_core::rules::{GameEvent, GamePhase, GameState};
    use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};
    use std::collections::BTreeMap;

    /// One of the three actions the button stands for.
    fn moves_on(action: &PlayerAction) -> bool {
        matches!(action, PlayerAction::PassPriority { .. } | PlayerAction::ContinueRun | PlayerAction::CompleteRun)
    }

    /// Whether the state is standing on the step `onward` named, given the
    /// events that got it there.
    fn reached(onward: &Onward, state: &GameState, events: &[GameEvent]) -> bool {
        let run = state.active_run.as_ref();
        let window = state.paid_ability_window.as_ref().map(|w| w.checkpoint);
        match onward {
            Onward::ApproachIce { position, .. } => run.is_some_and(|r| r.phase == RunPhase::ApproachIce && r.position == *position),
            Onward::ApproachServer(_) => events.iter().any(|e| matches!(e, GameEvent::ServerApproached { .. })),
            Onward::EncounterIce(_) => events.iter().any(|e| matches!(e, GameEvent::IceEncountered { .. })),
            Onward::FireSubroutines(_) => events.iter().any(|e| matches!(e, GameEvent::SubroutineFired { .. })),
            Onward::Movement => run.is_some_and(|r| r.phase == RunPhase::Movement),
            Onward::JackOutDecision => run.is_some_and(|r| r.phase == RunPhase::Movement && r.jack_out_permitted) && window.is_none(),
            Onward::Breach(_) => events.iter().any(|e| matches!(e, GameEvent::RunSucceeded { .. })),
            Onward::TurnBegins(side) => window == Some(WindowCheckpoint::StartOfTurn { side: *side }) || state.phase == GamePhase::Action(*side),
            Onward::Actions(side) => window.is_none() && state.phase == GamePhase::Action(*side),
            Onward::TurnEnds(side) => window == Some(WindowCheckpoint::TurnBeginning { side: side.other() }),
            Onward::NotPrevented => state.pending_prevention.is_none() || window != Some(WindowCheckpoint::Prevention),
        }
    }

    /// Something a card's text parked, or the end of the game, came first:
    /// the step was interrupted rather than skipped.
    fn interrupted(state: &GameState) -> bool {
        matches!(state.phase, GamePhase::GameOver(_))
            || state.active_trace.is_some()
            || state.pending_prevention.is_some()
            || state.pending_paid_choice.is_some()
            || state.pending_decision.is_some()
            || state.pending_payment.is_some()
    }

    /// Takes `action` on a copy of the game, then lets every window it
    /// leaves open close with nobody acting, stopping at the first state
    /// on the named step. `None` when something parked first.
    fn let_it_happen(session: &Session, action: PlayerAction, onward: &Onward) -> Option<bool> {
        let mut copy = Session::new(session.state().clone(), session.registry().clone(), Seat::External, Seat::External);
        copy.submit(action).unwrap();
        let mut events = copy.last_entry().unwrap().events.clone();
        // A pass hands priority on once, and a Runner going on opens the
        // window of the movement phase: never more than two passes stand
        // between the press and the step.
        for _ in 0..3 {
            if reached(onward, copy.state(), &events) {
                return Some(true);
            }
            if interrupted(copy.state()) {
                return None;
            }
            let Some(window) = &copy.state().paid_ability_window else { break };
            copy.submit(PlayerAction::PassPriority { side: window.active_priority }).unwrap();
            events.extend(copy.last_entry().unwrap().events.iter().cloned());
        }
        Some(false)
    }

    /// Real games, random and heuristic seats: whenever a seat is offered
    /// one of the three actions, it is offered exactly one, the view names
    /// a step, and taking it and letting it happen lands on that step.
    #[test]
    fn the_label_names_what_the_engine_does_next() {
        let registry = crate::decks::sample_deck_registry();
        let mut checked: BTreeMap<String, usize> = BTreeMap::new();
        let mut parked = 0;
        for seed in 0..12u64 {
            let (corp_deck, runner_deck) = sweep_decks_for_seed(seed);
            let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed).unwrap();
            let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
            let mut corp: Box<dyn BotAgent> = if seed % 2 == 0 { Box::new(HeuristicAgent::new(Side::Corp, seed)) } else { Box::new(RandomAgent::new(seed)) };
            let mut runner: Box<dyn BotAgent> = if seed % 3 == 0 { Box::new(HeuristicAgent::new(Side::Runner, seed)) } else { Box::new(RandomAgent::new(seed + 11)) };
            while let SessionStep::Awaiting { side, view } = session.step() {
                let offered: Vec<&PlayerAction> = view.legal_actions.iter().filter(|a| moves_on(a)).collect();
                assert!(offered.len() <= 1, "seed {seed} {side:?}: {offered:?} offered together");
                if let Some(action) = offered.first() {
                    let named = onward(&view).unwrap_or_else(|| panic!("seed {seed} {side:?}: {action:?} offered with no step named, run {:?}", view.active_run.as_ref().map(|r| r.phase)));
                    match let_it_happen(&session, (*action).clone(), &named) {
                        Some(true) => *checked.entry(format!("{named:?}").split(['(', ' ']).next().unwrap().to_string()).or_default() += 1,
                        Some(false) => panic!("seed {seed} {side:?}: {action:?} named {named:?} ({}) and the game went elsewhere", named.label(&registry, Some(side))),
                        None => parked += 1,
                    }
                }
                let action = match side {
                    Side::Corp => corp.select_action(&view, &registry),
                    Side::Runner => runner.select_action(&view, &registry),
                };
                session.submit(action).unwrap();
            }
        }
        eprintln!("checked {checked:?}, parked {parked}");
        // Every kind of step the button can name that ordinary play
        // reaches was reached at least once.
        assert!(checked.len() >= 9, "only {} kinds of step checked: {checked:?}", checked.len());
    }

    #[test]
    fn the_words_are_from_the_chair() {
        let registry = crate::decks::sample_deck_registry();
        assert_eq!(Onward::ApproachIce { position: 1, of: 3 }.label(&registry, Some(Side::Runner)), "Continue to Approach ice 2 of 3");
        assert_eq!(Onward::Breach(ServerId::Hq).label(&registry, Some(Side::Runner)), "Breach HQ");
        assert_eq!(Onward::FireSubroutines(1).label(&registry, Some(Side::Corp)), "Let 1 subroutine fire");
        assert_eq!(Onward::FireSubroutines(2).label(&registry, Some(Side::Corp)), "Let 2 subroutines fire");
        assert_eq!(Onward::JackOutDecision.label(&registry, Some(Side::Corp)), "Continue to the Runner's jack-out decision");
        assert_eq!(Onward::JackOutDecision.label(&registry, Some(Side::Runner)), "Continue to your jack-out decision");
        assert_eq!(Onward::TurnEnds(Side::Corp).label(&registry, Some(Side::Corp)), "End your turn");
        assert_eq!(Onward::TurnEnds(Side::Corp).label(&registry, Some(Side::Runner)), "Continue to your turn");
        assert_eq!(Onward::TurnEnds(Side::Corp).label(&registry, None), "Continue to the Runner's turn");
        assert_eq!(Onward::Actions(Side::Runner).label(&registry, Some(Side::Corp)), "Continue to the Runner's actions");
        assert_eq!(Onward::EncounterIce(None).label(&registry, Some(Side::Runner)), "Continue to Encounter the ice");
    }
}
