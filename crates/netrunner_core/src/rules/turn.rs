use crate::cards::CardRegistry;
use crate::dsl::CardId;
use crate::rules::continuous;
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::paid_ability;
use crate::rules::turn_log;
use crate::rules::win;
use crate::rules::state::{ArchivedCard, Clicks, GamePhase, GameState, Side, WindowCheckpoint};

/// Clicks the Corp receives at the start of each turn. A base turn-structure
/// constant of the game, not a card rule — same category as the "1 click"
/// cost every basic action already hardcodes.
const CORP_CLICKS_PER_TURN: u32 = 3;
/// Clicks the Runner receives at the start of each turn.
const RUNNER_CLICKS_PER_TURN: u32 = 4;
/// Maximum hand size before a side owes a mandatory discard at end of turn.
const CORP_MAX_HAND_SIZE: usize = 5;
const RUNNER_MAX_HAND_SIZE: usize = 5;

fn clicks_for(side: Side) -> u32 {
    match side {
        Side::Corp => CORP_CLICKS_PER_TURN,
        Side::Runner => RUNNER_CLICKS_PER_TURN,
    }
}

/// Derived from what is active every time it is asked, like the memory
/// limit: the base, what the side's active cards add
/// (`continuous::hand_size`), and for the Runner the brain damage that never
/// heals. It was a stored bonus folded in at install, at a score and at
/// setup and never taken out, so a trashed T400 Memory Diamond kept its +1
/// for the rest of the game.
///
/// Signed, because a Runner whose maximum hand size is below 0 when their
/// discard step begins is flatlined (CR 1.7.2b), and a floor at 0 is what
/// hid that: Bumi 1.0's core damage took the Runner to a hand of 0 and no
/// further. [`cards_over_hand_limit`] reads it floored, which is the same
/// count for any hand.
fn max_hand_size(state: &GameState, side: Side, registry: &CardRegistry) -> i32 {
    let base = match side {
        Side::Corp => CORP_MAX_HAND_SIZE,
        Side::Runner => RUNNER_MAX_HAND_SIZE,
    };
    let granted = base as i32 + continuous::hand_size(state, registry, side);
    match side {
        Side::Corp => granted,
        Side::Runner => granted - state.runner.brain_damage as i32,
    }
}

fn hand_size(state: &GameState, side: Side) -> usize {
    match side {
        Side::Corp => state.corp.hq.len(),
        Side::Runner => state.runner.grip.len(),
    }
}

/// How many cards `side` must still discard to be within its hand limit,
/// derived fresh from live state every time.
///
/// The **single** authority on that count: both `begin_discard_step` (deciding
/// whether a discard phase is owed at all) and `discard_card` (deciding
/// whether one is finished) call this rather than tracking a countdown, so
/// the two can never disagree.
///
/// Re-deriving rather than decrementing is what makes the count survive a
/// mid-discard change to either side of the comparison — a trigger that
/// draws a card, one that raises max hand size, or brain damage lowering
/// it. No card in the current pool can do any of that during a discard
/// phase (`GameEvent::CardDiscarded` has no `dispatcher::dispatch_event`
/// arm, so nothing fires between discards at all), so this is insurance
/// for the first card that can, not a fix for a reachable bug.
fn cards_over_hand_limit(state: &GameState, side: Side, registry: &CardRegistry) -> usize {
    hand_size(state, side).saturating_sub(max_hand_size(state, side, registry).max(0) as usize)
}

/// Extracts `side` from `state.phase` if it's currently `Action(side)`, for
/// either side. `EndTurn` is symmetric — valid during whichever side's
/// Action phase happens to be active — unlike the fixed-side actions in
/// `engine.rs`, which know their expected side up front and gate on a
/// concrete `GamePhase::Action(side)` via `engine::require_phase` instead.
fn require_action_phase(state: &GameState) -> Result<Side, RulesError> {
    match state.phase {
        GamePhase::Action(side) => Ok(side),
        actual => Err(RulesError::NotInActionPhase { actual }),
    }
}

/// Extracts the owing `side` from `state.phase` if it's currently
/// `Discard { .. }`.
///
/// Deliberately does **not** return the phase's `required` count: that
/// field is a *report* of how many cards were owed when it was last
/// written, not the authority on how many still are. Every consumer
/// re-derives from live state via [`cards_over_hand_limit`] instead — see
/// its doc comment.
fn require_discard_phase(state: &GameState) -> Result<Side, RulesError> {
    match state.phase {
        GamePhase::Discard { side, .. } => Ok(side),
        actual => Err(RulesError::NotInDiscardPhase { actual }),
    }
}

/// Removes `card_id` from `side`'s hand (Corp's `hq` or Runner's `grip`).
/// Errors with `RulesError::CardNotInHand` if it isn't there.
fn take_from_hand(state: &mut GameState, side: Side, card_id: &CardId) -> Result<(), RulesError> {
    let hand = match side {
        Side::Corp => &mut state.corp.hq,
        Side::Runner => &mut state.runner.grip,
    };
    let position = hand
        .iter()
        .position(|c| c == card_id)
        .ok_or_else(|| RulesError::CardNotInHand {
            side,
            card: card_id.clone(),
        })?;
    hand.remove(position);
    Ok(())
}

/// Moves a discarded card into `side`'s discard pile (Corp's `archives` or
/// Runner's `heap`) — both fully public zones, unlike `hq`/`grip`.
fn discard_to_pile(state: &mut GameState, side: Side, card_id: CardId) {
    match side {
        // A Corp discard from HQ goes facedown — the Runner never saw it.
        Side::Corp => state.corp.archives.push(ArchivedCard::facedown(card_id)),
        Side::Runner => state.runner.heap.push(card_id),
    }
}

/// Ends the active side's action phase: "If the Corp has any unspent
/// [click], the Corp takes an action. Otherwise, skip to (d)" (CR 5.6.2b;
/// the Runner's is 5.7.1f), and an action window "does not give the option
/// to pass" (CR 9.2.6b). So `EndTurn` is refused while `side` has clicks
/// left (`RulesError::ClicksRemain`): it is how the active player passes
/// the paid ability window at the top of an action phase they have no
/// clicks left for, and nothing else. It used to be legal with clicks in
/// hand, and a bot, or a person pressing Enter, gave them up.
///
/// What follows is the rules' own order (CR 5.6.2d–5.6.3e, 5.7.1h–5.7.2e):
/// the action phase ends ([`GameEvent::ActionPhaseEnded`], Cacophony), the
/// side discards to its maximum hand size ([`begin_discard_step`]), a
/// paid ability window opens (`WindowCheckpoint::EndOfTurn`), unspent
/// clicks are lost, and the turn formally ends ([`finish_turn`]). The
/// engine used to zero the clicks and open the window first and discard
/// last.
///
/// Credits are untouched — they carry over turn to turn. The window keeps
/// `phase == Action(side)`, which `run::check_run_may_begin` reads.
pub fn end_turn(state: &GameState, registry: &CardRegistry) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = require_action_phase(state)?;
    if state.active_run.is_some() {
        return Err(RulesError::CannotEndTurnWhileRunActive);
    }
    // Without this, `EndTurn` would stay legal throughout the `EndOfTurn`
    // window it itself opens (that window keeps `state.phase ==
    // Action(side)`, so `require_action_phase` alone doesn't catch it),
    // letting it be resubmitted mid-window and silently reset priority back
    // to `side` regardless of who actually holds it.
    paid_ability::require_no_window(state)?;
    let clicks = state.resources(side).clicks.0;
    if clicks > 0 {
        return Err(RulesError::ClicksRemain { side, clicks });
    }

    let mut next = state.clone();
    let mut events = Vec::new();
    // "The Corp's action phase formally ends. Conditions related to the
    // action phase ending are met" (CR 5.6.2d) — Cacophony.
    let action_phase_ended = GameEvent::ActionPhaseEnded { side };
    dispatcher::emit(&mut next, registry, &mut events, action_phase_ended)?;
    if next.is_over() {
        return Ok((next, events));
    }
    events.extend(begin_discard_step(&mut next, side, registry)?);
    Ok((next, events))
}

/// The discard step (CR 5.6.3a, 5.7.2a): `side` discards down to its
/// maximum hand size. Parks in `GamePhase::Discard { side, required }`
/// while cards are owed — [`discard_card`] clears it — and goes straight
/// on to the end-of-turn window when none are.
fn begin_discard_step(state: &mut GameState, side: Side, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = Vec::new();
    // A new Runner discard phase starts with an empty record whether or
    // not anything will be discarded in it — see
    // `RunnerState::discarded_this_discard_phase`.
    if side == Side::Runner {
        state.runner.discarded_this_discard_phase.clear();
    }
    // "The Runner is also flatlined if, at the beginning of their discard
    // step, their maximum hand size is less than 0" (CR 1.7.2b). A
    // failed state rather than a standing condition — a hand size may dip
    // below 0 mid-turn and recover — so it is asked here, at the one
    // moment the rule names, and not in `checkpoint`.
    if side == Side::Runner && max_hand_size(state, side, registry) < 0 {
        events.push(GameEvent::RunnerFlatlined);
        events.extend(win::end_game(state, Side::Corp));
        return Ok(events);
    }
    let over_by = cards_over_hand_limit(state, side, registry);
    if over_by > 0 {
        state.phase = GamePhase::Discard { side, required: over_by };
        events.push(GameEvent::DiscardPending { side, required: over_by });
    } else {
        events.push(open_end_of_turn_window(state, side));
    }
    Ok(events)
}

/// The discard phase's paid ability window (CR 5.6.3b, 5.7.2b), after the
/// discard. It runs under `Action(side)`, the phase the turn's guards
/// already read as "this side's turn, no run may start"; closing it is
/// [`finish_turn`].
fn open_end_of_turn_window(state: &mut GameState, side: Side) -> GameEvent {
    state.phase = GamePhase::Action(side);
    paid_ability::open_window_for(state, side, WindowCheckpoint::EndOfTurn { side })
}

/// Emits `GameEvent::DiscardPhaseEnded` for `side` and dispatches whatever
/// reacts to it — once per turn, at the turn's formal end, whether or not
/// anything was discarded: in rules terms the phase still happened, so
/// anything keyed on its end (Jinteki: Restoring Humanity) still fires.
fn dispatch_discard_phase_end(
    state: &mut GameState,
    side: Side,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let event = GameEvent::DiscardPhaseEnded { side };
    let mut events = vec![event.clone()];
    events.extend(dispatcher::dispatch_event(state, registry, &event)?);
    Ok(events)
}

/// Resumes after the end-of-turn window closes (`paid_ability::
/// close_window`'s `EndOfTurn` arm): unspent clicks are lost (CR 5.6.3c),
/// the turn formally ends — "conditions related to the turn or discard
/// phase ending are met" (CR 5.6.3d) — and play proceeds to the other
/// side's turn.
pub(crate) fn finish_turn(
    state: &mut GameState,
    side: Side,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    // After the window, not before it: the window belongs to the discard
    // phase of a turn whose clicks are not yet lost. No card in the pool
    // gains a click there, and `EndTurn` is refused with clicks left, so
    // this is almost always 0 already.
    state.resources_mut(side).clicks = Clicks(0);
    let mut events = vec![GameEvent::TurnEnded { side }];
    events.extend(dispatch_discard_phase_end(state, side, registry)?);
    enter_start_of_turn(state, &mut events, side.other())?;
    Ok(events)
}

/// Discard `card_id` from hand to satisfy a pending mandatory discard (see
/// [`begin_discard_step`]). Errors with `RulesError::NotInDiscardPhase`
/// outside `GamePhase::Discard`, or `RulesError::CardNotInHand` if the card
/// isn't in the owing side's hand. Once the phase's `required` count
/// reaches zero, opens the end-of-turn window — the same step
/// `begin_discard_step` takes directly when no discard was owed at all.
pub fn discard_card(
    state: &GameState,
    card_id: CardId,
    registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = require_discard_phase(state)?;
    let mut next = state.clone();
    take_from_hand(&mut next, side, &card_id)?;
    discard_to_pile(&mut next, side, card_id.clone());
    if side == Side::Runner {
        next.runner.discarded_this_discard_phase.push(card_id.clone());
    }
    let mut events = vec![GameEvent::CardDiscarded { side, card: card_id }];

    // Re-derived from the post-discard state, not decremented from the
    // phase's stored count — see `cards_over_hand_limit`'s doc comment.
    let remaining = cards_over_hand_limit(&next, side, registry);
    if remaining == 0 {
        events.push(open_end_of_turn_window(&mut next, side));
    } else {
        next.phase = GamePhase::Discard { side, required: remaining };
    }

    Ok((next, events))
}

/// The first steps of `next_side`'s turn, up to its first window: "The
/// Corp gains their allotted clicks", then "a paid ability window occurs"
/// (CR 5.6.1a–b; the Runner's are 5.7.1a–b). The turn's own bookkeeping —
/// the turn counter, the turn log, the once-per-turn uses — moves here,
/// because the window already belongs to the new turn. Closing the window
/// is [`begin_turn`].
///
/// The one entry to a turn: from [`finish_turn`], and from setup for the
/// Corp's first.
pub(crate) fn enter_start_of_turn(
    next: &mut GameState,
    events: &mut Vec<GameEvent>,
    next_side: Side,
) -> Result<(), RulesError> {
    // The discard-phase-end dispatch just before this can end the game
    // (a flatlining `OnDiscardPhaseEnd`); writing `StartOfTurn` over a
    // `GameOver` would revert the win.
    if next.is_over() {
        return Ok(());
    }
    next.phase = GamePhase::StartOfTurn(next_side);
    next.turn += 1;

    // Aggressive Trendsetting's "+1 allotted [click] for your next turn",
    // banked during the Runner's turn and spent here — part of the
    // allotment, so it is inside the `TurnStarted` event's `clicks` count
    // and any `OnTurnStart` ability that adds clicks (Otto Campaign) stacks
    // on top of it.
    let banked = match next_side {
        Side::Corp => std::mem::take(&mut next.corp.extra_clicks_next_turn),
        Side::Runner => 0,
    };
    next.resources_mut(next_side).clicks = Clicks(clicks_for(next_side) + banked);

    // Before the turn's first moment, so the new turn counts from here.
    turn_log::rotate(next);
    // Both sides, at every turn start — "once per turn" means once per
    // *turn*, and a Corp ability used on the Corp's own turn must be
    // usable again during the Runner's. Clearing only the starting side's
    // set made a Corp gate span its own turn and the Runner's as one
    // window, which Phật Gioan Baotixita ("the first time each turn an
    // agenda is scored or stolen" — either player's turn) reads from both
    // sides of.
    next.corp.once_per_turn_used.clear();
    next.runner.once_per_turn_used.clear();
    match next_side {
        // Everything still installed was necessarily installed on an
        // earlier turn — Seamless Launch's "did not install this turn"
        // eligibility.
        Side::Corp => next.corp.installed.iter_mut().for_each(|installed| installed.installed_this_turn = false),
        Side::Runner => next.runner.servers_run_this_turn.clear(),
    }

    events.push(paid_ability::open_window_for(next, next_side, WindowCheckpoint::TurnBeginning { side: next_side }));
    Ok(())
}

/// Resumes after the turn's first window closes (`paid_ability::
/// close_window`'s `TurnBeginning` arm), in the rules' order (CR
/// 5.6.1c–f, 5.7.1c–e): recurring credits refill, the turn formally begins
/// (`GameEvent::TurnStarted`, which `Trigger::OnTurnStart` hears), the Corp
/// makes its mandatory draw, and a window opens ahead of the first action
/// (`WindowCheckpoint::StartOfTurn`, whose closing sets `Action(side)`).
///
/// The Corp used to draw before its turn began, so Au Co.'s look at the
/// top 3 of R&D saw the three after the drawn card, and a Corp with an
/// empty R&D lost before its turn started. Now the draw is the step it
/// is, and a failed one loses there (CR 1.7.2c) — after the turn-begins
/// abilities, which may have drawn or looked, have resolved.
pub(crate) fn begin_turn(state: &mut GameState, side: Side, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    // Comprehensive Rules 1.10.5a/c: recurring credits refill "before
    // abilities meet their trigger conditions for your turn beginning".
    let mut events = crate::rules::payment::refill(state, registry, side)?;

    // "The Corp's turn formally begins. Conditions related to the turn
    // beginning are met" (CR 5.6.1d) — PAD Campaign's "gain 1 credit".
    let clicks = state.resources(side).clicks.0;
    let turn_started = GameEvent::TurnStarted { side, clicks };
    dispatcher::emit(state, registry, &mut events, turn_started)?;
    if state.is_over() {
        return Ok(events);
    }

    if side == Side::Corp {
        // "The Corp performs their mandatory draw" (CR 5.6.1e). Top of R&D
        // is the end of the Vec, as `RunnerState::stack`'s is.
        match state.corp.r_and_d.pop() {
            Some(card) => {
                state.corp.hq.push(card);
                events.push(GameEvent::CardDrawn { side: Side::Corp });
            }
            None => {
                events.extend(win::end_game(state, Side::Runner));
                return Ok(events);
            }
        }
    }

    events.push(paid_ability::open_window_for(state, side, WindowCheckpoint::StartOfTurn { side }));
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::InstallId;
    use crate::dsl::CardId;
    use crate::rules::action::PlayerAction;
    use crate::rules::run::{RunPhase, RunState};
    use crate::rules::test_support::install_of;
    use crate::rules::state::{
        AgendaPoints, CorpState, Credits, MemoryUnits, PlayerResources, RunnerState,
    };

    /// Resolves every `PaidAbilityWindow` open on `state` by having whichever
    /// side currently holds priority submit `PlayerAction::PassPriority`,
    /// repeatedly, until none remains — e.g. an `EndOfTurn` window closing
    /// into a fresh `StartOfTurn` window, which itself needs closing before
    /// `state.phase` actually reaches `Action(_)`. Every test in this module
    /// that used to assert an immediate post-`end_turn`/`discard_card` phase
    /// now needs this, since both functions pause at a window rather than
    /// completing the transition inline. Goes through the public
    /// `apply_action` entry point (rather than calling `paid_ability::
    /// pass_priority` directly) so this same helper is copyable verbatim
    /// into any other module's test suite.
    pub(crate) fn close_all_windows(mut state: GameState, registry: &CardRegistry) -> (GameState, Vec<GameEvent>) {
        let mut events = Vec::new();
        while let Some(window) = &state.paid_ability_window {
            let side = window.active_priority;
            let (next, ev) = crate::rules::apply_action(&state, registry, PlayerAction::PassPriority { side })
                .expect("pass priority should succeed");
            state = next;
            events.extend(ev);
        }
        (state, events)
    }

    fn game_state(
        active_turn: Side,
        corp_clicks: u32,
        corp_credits: u32,
        runner_clicks: u32,
        runner_credits: u32,
    ) -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources {
                    credits: Credits(corp_credits),
                    clicks: Clicks(corp_clicks),
                    agenda_points: AgendaPoints(0),
                },
                ..Default::default()
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(runner_credits),
                    clicks: Clicks(runner_clicks),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                ..Default::default()
            },
            phase: GamePhase::Action(active_turn),
            ..Default::default()
        }
    }

    /// `enter_start_of_turn` runs right after the discard-phase-end dispatch
    /// and used to write `StartOfTurn` first thing — a flatlining
    /// `OnDiscardPhaseEnd` (Jinteki: Restoring Humanity's shape, with
    /// damage instead of a credit) would have had its win overwritten one
    /// statement later.
    #[test]
    fn a_discard_phase_end_flatline_is_not_overwritten_by_start_of_turn() {
        let identity = crate::dsl::CardDefinition {
            id: CardId("lethal_identity".to_string()),
            title: "Lethal".to_string(),
            side: Side::Corp,
            card_type: crate::dsl::CardType::Identity,
            triggers: vec![crate::dsl::TriggeredEffect {
                subject: None, when: None, acts_on_subject: false, first_each_turn: false,
                text: None,
                trigger: crate::dsl::Trigger::OnDiscardPhaseEnd,
                effects: vec![crate::dsl::Effect::DealDamage(crate::dsl::DamageType::Net, 1)],
                requirement: None,
            }],
            ..Default::default()
        };
        let registry = CardRegistry::from_cards(vec![identity]);
        let mut state = game_state(Side::Corp, 0, 5, 0, 5);
        state.corp.identity = Some(CardId("lethal_identity".to_string()));
        state.corp.r_and_d = vec![CardId("filler".to_string())];
        assert!(state.runner.grip.is_empty(), "any damage flatlines");
        let turn_before = state.turn;

        let (state, _) = end_turn(&state, &registry).expect("corp ends turn");
        let (state, events) = close_all_windows(state, &registry);

        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp), "{events:?}");
        assert_eq!(state.turn, turn_before, "the Runner's turn never started");
        assert!(!events.iter().any(|e| matches!(e, GameEvent::TurnStarted { side: Side::Runner, .. })));
        assert!(state.paid_ability_window.is_none(), "no start-of-turn window over a finished game");
        assert_eq!(events.iter().filter(|e| matches!(e, GameEvent::GameOver { .. })).count(), 1);
    }

    #[test]
    fn corp_ending_turn_hands_control_to_runner_with_four_clicks() {
        let state = game_state(Side::Corp, 0, 5, 0, 2);
        let registry = CardRegistry::new();
        let (next, mut events) = end_turn(&state, &registry).expect("should succeed");
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        assert_eq!(next.phase, GamePhase::Action(Side::Runner));
        assert_eq!(next.runner.resources.clicks, Clicks(4));
        assert!(events.contains(&GameEvent::TurnEnded { side: Side::Corp }));
        assert!(events.contains(&GameEvent::TurnStarted { side: Side::Runner, clicks: 4 }));
    }

    #[test]
    fn runner_ending_turn_hands_control_to_corp_with_three_clicks() {
        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        // A non-empty R&D so the Corp's mandatory draw succeeds rather than
        // decking out — this test is about the click handoff, not deck-out.
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        let registry = CardRegistry::new();
        let (next, mut events) = end_turn(&state, &registry).expect("should succeed");
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        assert_eq!(next.phase, GamePhase::Action(Side::Corp));
        assert_eq!(next.corp.resources.clicks, Clicks(3));
        assert!(events.contains(&GameEvent::TurnEnded { side: Side::Runner }));
        assert!(events.contains(&GameEvent::TurnStarted { side: Side::Corp, clicks: 3 }));
        assert!(events.contains(&GameEvent::CardDrawn { side: Side::Corp }));
    }

    /// `turn` advances once per side's turn, not once per round — so a
    /// Corp→Runner handoff increments it just like a Runner→Corp one.
    #[test]
    fn each_sides_turn_advances_the_turn_counter_by_one() {
        let state = game_state(Side::Corp, 0, 5, 0, 2);
        let registry = CardRegistry::new();
        assert_eq!(state.turn, 0);

        let (next, _) = end_turn(&state, &registry).expect("should succeed");
        let (next, _) = close_all_windows(next, &registry);
        assert_eq!(next.turn, 1, "Runner's turn began");

        let mut next = crate::rules::test_support::clicks_spent(&next);
        next.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        let (next, _) = end_turn(&next, &registry).expect("should succeed");
        let (next, _) = close_all_windows(next, &registry);
        assert_eq!(next.turn, 2, "Corp's turn began");
    }

    /// A Corp that cannot make its mandatory draw loses at the draw (CR
    /// 5.6.1e, 1.7.2c), which comes after its turn has formally begun
    /// (5.6.1d) — so the turn is counted, `TurnStarted` is in the record
    /// ahead of the `GameOver`, and the two never disagree. The Corp used to
    /// lose before its turn started.
    #[test]
    fn a_corp_deck_out_comes_after_its_turn_begins() {
        let state = game_state(Side::Runner, 0, 5, 0, 2);
        let registry = CardRegistry::new();
        let turn_before = state.turn;
        assert!(state.corp.r_and_d.is_empty(), "fixture must deck the Corp out");

        let (next, mut events) = end_turn(&state, &registry).expect("should succeed");
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        assert_eq!(next.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(next.turn, turn_before + 1, "the turn began, then the draw failed");
        let began = events.iter().position(|e| matches!(e, GameEvent::TurnStarted { side: Side::Corp, .. })).expect("the turn began");
        let over = events.iter().position(|e| matches!(e, GameEvent::GameOver { winner: Side::Runner })).expect("and was lost");
        assert!(began < over, "{events:?}");
    }

    #[test]
    fn ending_turn_does_not_change_either_sides_credits() {
        let state = game_state(Side::Corp, 0, 5, 0, 2);
        let (next, _events) = end_turn(&state, &CardRegistry::new()).expect("should succeed");

        assert_eq!(next.corp.resources.credits, Credits(5));
        assert_eq!(next.runner.resources.credits, Credits(2));
    }

    /// CR 5.6.2b: "If the Corp has any unspent [click], the Corp takes an
    /// action", and an action window "does not give the option to pass"
    /// (CR 9.2.6b). Ending the turn holding clicks used to be legal, and
    /// gave them up.
    #[test]
    fn ending_a_turn_with_clicks_left_is_refused() {
        let state = game_state(Side::Corp, 2, 5, 0, 2);

        assert_eq!(end_turn(&state, &CardRegistry::new()), Err(RulesError::ClicksRemain { side: Side::Corp, clicks: 2 }));
        let legal = crate::rules::legal_actions(&state, &CardRegistry::new());
        assert!(!legal.contains(&PlayerAction::EndTurn), "{legal:?}");
    }

    /// Unspent clicks are lost at CR 5.6.3c, after the discard phase's
    /// window and before the turn formally ends. `EndTurn` needs none left,
    /// so this is a click gained after it: the loss still happens.
    #[test]
    fn a_click_gained_in_the_end_of_turn_window_is_lost_with_the_turn() {
        let state = game_state(Side::Corp, 0, 5, 0, 2);
        let registry = CardRegistry::new();
        let (mut next, _) = end_turn(&state, &registry).expect("should succeed");
        next.corp.resources.clicks = Clicks(1);
        let (next, events) = close_all_windows(next, &registry);

        assert_eq!(next.corp.resources.clicks, Clicks(0), "unspent clicks are lost at end of turn");
        assert_eq!(next.corp.resources.credits, Credits(5), "credits, unlike clicks, carry over");
        assert!(events.contains(&GameEvent::TurnEnded { side: Side::Corp }));
    }

    /// The regression this exists for. `activate_ability` resolves the
    /// acting side from card ownership whenever a window is open,
    /// deliberately bypassing phase — so leftover clicks were spendable on
    /// the *opponent's* turn. Reachable with Regolith Mining License's
    /// `[click]: take 3[c]` at any window, including the post-action one.
    #[test]
    fn clicks_left_over_from_a_turn_cannot_pay_for_an_off_turn_paid_ability() {
        use crate::dsl::{AbilityDef, CardDefinition, CardType, Cost, Effect, Trigger};
        use crate::rules::engine::apply_action;
        use crate::rules::state::InstalledCard;

        let mut registry = CardRegistry::new();
        registry.insert(CardDefinition {
            id: CardId("regolith_mining_license".to_string()),
            title: "Regolith Mining License".to_string(),
            side: Side::Corp,
            card_type: CardType::Asset,
            abilities: vec![AbilityDef {
                text: None,
                trigger: Trigger::Paid,
                cost: Some(Cost::Clicks(1)),
                requirement: None,
                effect: Effect::GainCredits(Side::Corp, 3),
                cost_discount_if: None, used_by: None }],
            is_playable: true,
            ..Default::default()
        });

        // Corp ends its turn, with the asset rezzed. It cannot end it
        // holding clicks (CR 5.6.2b), so none are left to spend.
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.corp.installed = vec![InstalledCard {
            install_id: InstallId(1068),
            card: CardId("regolith_mining_license".to_string()),
            rezzed: true,
            ..Default::default()
        }];

        let (state, _) = end_turn(&state, &registry).expect("ending the turn should succeed");
        let activate = PlayerAction::ActivateAbility {
            target: install_of(&state, "regolith_mining_license"),
            ability_index: 0,
        };

        // Still inside the Corp's own EndOfTurn window: already too late.
        assert_eq!(
            apply_action(&state, &registry, activate.clone()),
            Err(RulesError::NotEnoughClicks { side: Side::Corp, available: 0, requested: 1 })
        );

        // And still refused mid-Runner-turn, in an open window — the
        // scenario that made this reachable at all, since `activate_ability`
        // lets the non-active side act whenever a window is open. Any
        // window will do; a post-action one is the newest way to get here.
        let (mut state, _) = close_all_windows(state, &registry);
        assert_eq!(state.phase, GamePhase::Action(Side::Runner), "control passed to the Runner");
        state.paid_ability_window = Some(crate::rules::state::PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::PostAction { side: Side::Runner },
            return_phase: Box::new(state.phase),
        });

        assert_eq!(
            apply_action(&state, &registry, activate),
            Err(RulesError::NotEnoughClicks { side: Side::Corp, available: 0, requested: 1 }),
            "clicks from a finished turn must never fund an off-turn ability"
        );
    }

    #[test]
    fn end_turn_opens_an_end_of_turn_window_giving_the_ending_side_priority_first() {
        let state = game_state(Side::Corp, 0, 5, 0, 2);
        let (next, events) = end_turn(&state, &CardRegistry::new()).expect("should succeed");

        let window = next.paid_ability_window.expect("an EndOfTurn window should be open");
        assert_eq!(window.checkpoint, WindowCheckpoint::EndOfTurn { side: Side::Corp });
        assert_eq!(window.active_priority, Side::Corp);
        assert_eq!(window.consecutive_passes, 0);
        assert_eq!(
            events,
            vec![GameEvent::ActionPhaseEnded { side: Side::Corp }, GameEvent::PaidAbilityWindowOpened { side: Side::Corp }],
            "the discard step came first, with nothing to discard; the turn ends when the window closes"
        );
        // Control hasn't actually passed yet — this is still the ending side's turn.
        assert_eq!(next.phase, GamePhase::Action(Side::Corp));
    }

    /// CR 5.6.1: the Corp gains its clicks, a window opens, recurring
    /// credits refill, the turn formally begins, and only then does the
    /// Corp draw — followed by the window ahead of its first action. The
    /// engine used to draw first and have one window, after everything.
    #[test]
    fn a_corp_turn_begins_with_clicks_and_a_window_then_begins_formally_then_draws() {
        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        let registry = CardRegistry::new();
        let pass = |state: GameState, events: &mut Vec<GameEvent>| {
            let side = state.paid_ability_window.as_ref().expect("a window is open").active_priority;
            let (state, ev) = crate::rules::apply_action(&state, &registry, PlayerAction::PassPriority { side }).expect("pass");
            events.extend(ev);
            state
        };

        let (state, mut events) = end_turn(&state, &registry).expect("should succeed");
        let state = pass(pass(state, &mut events), &mut events);

        // 5.6.1a–b: clicks, then the window — nothing has begun or been drawn.
        assert_eq!(state.phase, GamePhase::StartOfTurn(Side::Corp));
        assert_eq!(state.corp.resources.clicks, Clicks(3));
        let window = state.paid_ability_window.as_ref().expect("the turn's first window");
        assert_eq!(window.checkpoint, WindowCheckpoint::TurnBeginning { side: Side::Corp });
        assert_eq!(window.active_priority, Side::Corp);
        assert!(!events.iter().any(|e| matches!(e, GameEvent::TurnStarted { .. })));
        assert!(!state.corp.hq.contains(&CardId("hedge_fund".to_string())));

        let mut events = Vec::new();
        let state = pass(pass(state, &mut events), &mut events);

        // 5.6.1d–e, then 5.6.2a: the turn begins, the Corp draws, a window.
        let began = events.iter().position(|e| *e == GameEvent::TurnStarted { side: Side::Corp, clicks: 3 }).expect("the turn began");
        let drew = events.iter().position(|e| *e == GameEvent::CardDrawn { side: Side::Corp }).expect("the Corp drew");
        assert!(began < drew, "{events:?}");
        assert!(state.corp.hq.contains(&CardId("hedge_fund".to_string())));
        let window = state.paid_ability_window.expect("a StartOfTurn window should be open");
        assert_eq!(window.checkpoint, WindowCheckpoint::StartOfTurn { side: Side::Corp });
    }

    #[test]
    fn runner_ending_turn_gives_corp_a_mandatory_draw_into_hq() {
        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())];
        let registry = CardRegistry::new();
        let (next, mut events) = end_turn(&state, &registry).expect("should succeed");
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        // Draws from the top of R&D, i.e. the end of the Vec (mirrors
        // RunnerState::stack's convention).
        assert_eq!(next.corp.r_and_d, vec![CardId("hedge_fund".to_string())]);
        assert_eq!(next.corp.hq, vec![CardId("ice_wall".to_string())]);
        assert!(events.contains(&GameEvent::TurnEnded { side: Side::Runner }));
        assert!(events.contains(&GameEvent::TurnStarted { side: Side::Corp, clicks: 3 }));
        assert!(events.contains(&GameEvent::CardDrawn { side: Side::Corp }));
    }

    #[test]
    fn corp_ending_turn_gives_no_draw_since_only_the_corp_draws_automatically() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.runner.stack = vec![CardId("sure_gamble".to_string())];
        let registry = CardRegistry::new();
        let (next, mut events) = end_turn(&state, &registry).expect("should succeed");
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        // Control passed to the Runner, so no automatic draw happens here —
        // only the Corp draws automatically at the start of their turn.
        assert_eq!(next.runner.stack, vec![CardId("sure_gamble".to_string())]);
        assert!(next.runner.grip.is_empty());
        assert!(events.contains(&GameEvent::TurnEnded { side: Side::Corp }));
        assert!(events.contains(&GameEvent::TurnStarted { side: Side::Runner, clicks: 4 }));
        assert!(!events.contains(&GameEvent::CardDrawn { side: Side::Runner }));
    }

    #[test]
    fn mandatory_draw_with_empty_rd_ends_game_with_runner_win() {
        let state = game_state(Side::Runner, 0, 5, 0, 2);
        let registry = CardRegistry::new();
        let (next, mut events) = end_turn(&state, &registry).expect("should succeed");
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        // Deck-out: the Corp can't make their mandatory draw, so the game
        // ends at the draw (CR 1.7.2c) — no underflow/panic, and no
        // further window opens once `GameOver` is reached.
        assert!(next.corp.hq.is_empty());
        assert_eq!(next.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(next.paid_ability_window, None);
        assert!(events.contains(&GameEvent::TurnEnded { side: Side::Runner }));
        assert!(events.contains(&GameEvent::GameOver { winner: Side::Runner }));
    }

    #[test]
    fn ending_turn_while_a_run_is_active_errors() {
        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            jack_out_permitted: true,
            ..Default::default()
        });

        assert_eq!(end_turn(&state, &CardRegistry::new()), Err(RulesError::CannotEndTurnWhileRunActive));
    }

    #[test]
    fn ending_turn_outside_action_phase_returns_not_in_action_phase() {
        let mut state = game_state(Side::Corp, 3, 5, 0, 2);
        state.phase = GamePhase::Discard { side: Side::Corp, required: 1 };

        assert_eq!(
            end_turn(&state, &CardRegistry::new()),
            Err(RulesError::NotInActionPhase {
                actual: GamePhase::Discard { side: Side::Corp, required: 1 }
            })
        );
    }

    #[test]
    fn ending_turn_over_hand_size_transitions_to_discard_instead_of_next_start_of_turn() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.corp.hq = (0..6).map(|i| CardId(format!("card_{i}"))).collect();
        let registry = CardRegistry::new();
        let (next, mut events) = end_turn(&state, &registry).expect("should succeed");
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        assert_eq!(next.phase, GamePhase::Discard { side: Side::Corp, required: 1 });
        // Control has NOT passed to the Runner yet — clicks are untouched.
        assert_eq!(next.runner.resources.clicks, Clicks(0));
        // The discard comes before the window and the turn's end (CR 5.6.3).
        assert!(next.paid_ability_window.is_none());
        assert!(!events.contains(&GameEvent::TurnEnded { side: Side::Corp }));
        assert!(events.contains(&GameEvent::DiscardPending { side: Side::Corp, required: 1 }));
    }

    /// CR 1.7.2b: "The Runner is also flatlined if, at the beginning of
    /// their discard step, their maximum hand size is less than 0." The
    /// hand size was floored at 0, so six core damage only ever cost the
    /// Runner their grip; at exactly 0 they play on.
    #[test]
    fn a_runner_whose_maximum_hand_size_is_below_zero_is_flatlined_at_the_discard_step() {
        let registry = CardRegistry::new();
        for (core_damage, flatlined) in [(5, false), (6, true)] {
            let mut state = game_state(Side::Runner, 0, 5, 0, 2);
            state.corp.r_and_d = vec![CardId("filler".to_string())];
            state.runner.brain_damage = core_damage;
            let (next, mut events) = end_turn(&state, &registry).expect("the Runner ends their turn");
            let (next, close_events) = close_all_windows(next, &registry);
            events.extend(close_events);

            if flatlined {
                assert_eq!(next.phase, GamePhase::GameOver(Side::Corp), "{events:?}");
                assert!(events.contains(&GameEvent::RunnerFlatlined));
            } else {
                assert_eq!(next.phase, GamePhase::Action(Side::Corp), "a hand size of 0 is not below 0");
            }
        }
    }

    #[test]
    fn ending_turn_within_hand_size_skips_discard_entirely() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.corp.hq = (0..5).map(|i| CardId(format!("card_{i}"))).collect();
        let registry = CardRegistry::new();
        let (next, _events) = end_turn(&state, &registry).expect("should succeed");
        let (next, _close_events) = close_all_windows(next, &registry);

        assert_eq!(next.phase, GamePhase::Action(Side::Runner));
    }

    #[test]
    fn discard_card_moves_card_from_hq_to_archives() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.phase = GamePhase::Discard { side: Side::Corp, required: 1 };
        state.corp.hq = vec![CardId("hedge_fund".to_string())];

        let registry = CardRegistry::new();
        let (next, mut events) =
            discard_card(&state, CardId("hedge_fund".to_string()), &registry).expect("should succeed");
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        assert!(next.corp.hq.is_empty());
        // A Corp discard from HQ goes facedown.
        assert_eq!(next.corp.archives, vec![ArchivedCard::facedown(CardId("hedge_fund".to_string()))]);
        // Last mandatory discard cleared: control passes to the Runner.
        assert_eq!(next.phase, GamePhase::Action(Side::Runner));
        assert_eq!(next.runner.resources.clicks, Clicks(4));
        assert!(events.contains(&GameEvent::CardDiscarded { side: Side::Corp, card: CardId("hedge_fund".to_string()) }));
        assert!(events.contains(&GameEvent::TurnStarted { side: Side::Runner, clicks: 4 }));
    }

    #[test]
    fn discard_card_moves_card_from_grip_to_heap() {
        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        state.phase = GamePhase::Discard { side: Side::Runner, required: 1 };
        state.runner.grip = vec![CardId("sure_gamble".to_string())];
        // A non-empty R&D so the Corp's mandatory draw succeeds rather than
        // decking out — this test is about the heap mechanic, not deck-out.
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];

        let registry = CardRegistry::new();
        let (next, _events) =
            discard_card(&state, CardId("sure_gamble".to_string()), &registry).expect("should succeed");
        let (next, _close_events) = close_all_windows(next, &registry);

        assert!(next.runner.grip.is_empty());
        assert_eq!(next.runner.heap, vec![CardId("sure_gamble".to_string())]);
        assert_eq!(next.phase, GamePhase::Action(Side::Corp));
    }

    /// `CORP_MAX_HAND_SIZE` is 5, so a 7-card HQ owes 2 discards; after one,
    /// 6 cards still owes 1 and the phase persists.
    ///
    /// The hand is deliberately *genuinely* over the limit rather than
    /// carrying a fabricated `required`: since `cards_over_hand_limit`
    /// re-derives the count from live state, a stored count that live state
    /// doesn't support is no longer meaningful (and could never be produced
    /// by `begin_discard_step` in the first place).
    #[test]
    fn discard_card_with_more_than_one_owed_stays_in_discard_phase() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.phase = GamePhase::Discard { side: Side::Corp, required: 2 };
        state.corp.hq = (0..7).map(|i| CardId(format!("hq_card_{i}"))).collect();

        let (next, events) =
            discard_card(&state, CardId("hq_card_0".to_string()), &CardRegistry::new()).expect("should succeed");

        assert_eq!(next.corp.hq.len(), 6);
        assert_eq!(next.phase, GamePhase::Discard { side: Side::Corp, required: 1 });
        assert_eq!(
            events,
            vec![GameEvent::CardDiscarded {
                side: Side::Corp,
                card: CardId("hq_card_0".to_string())
            }]
        );
    }

    /// The stored `required` is a report, not the authority: a phase
    /// claiming more discards than live state actually owes resolves on the
    /// live figure. Unreachable via `begin_discard_step` today — this pins the
    /// re-derivation itself, which exists so a future mid-discard trigger
    /// (drawing a card, raising max hand size, dealing brain damage) can't
    /// desynchronize the count. See `cards_over_hand_limit`.
    #[test]
    fn discard_count_is_rederived_from_live_state_not_counted_down() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        // Claims 3 owed, but a 6-card HQ against CORP_MAX_HAND_SIZE 5 owes
        // exactly 1 — so a single discard must finish the phase.
        state.phase = GamePhase::Discard { side: Side::Corp, required: 3 };
        state.corp.hq = (0..6).map(|i| CardId(format!("hq_card_{i}"))).collect();

        let (next, _events) =
            discard_card(&state, CardId("hq_card_0".to_string()), &CardRegistry::new()).expect("should succeed");

        assert_ne!(
            next.phase,
            GamePhase::Discard { side: Side::Corp, required: 2 },
            "must not blindly decrement the stored count"
        );
        assert!(
            !matches!(next.phase, GamePhase::Discard { .. }),
            "the Corp is within hand size after one discard, so the phase is over (got {:?})",
            next.phase
        );
    }

    #[test]
    fn discard_card_outside_discard_phase_returns_not_in_discard_phase() {
        let state = game_state(Side::Corp, 3, 5, 0, 2);

        assert_eq!(
            discard_card(&state, CardId("hedge_fund".to_string()), &CardRegistry::new()),
            Err(RulesError::NotInDiscardPhase { actual: GamePhase::Action(Side::Corp) })
        );
    }

    #[test]
    fn discard_card_not_in_hand_returns_card_not_in_hand() {
        let mut state = game_state(Side::Corp, 0, 5, 0, 2);
        state.phase = GamePhase::Discard { side: Side::Corp, required: 1 };

        assert_eq!(
            discard_card(&state, CardId("hedge_fund".to_string()), &CardRegistry::new()),
            Err(RulesError::CardNotInHand {
                side: Side::Corp,
                card: CardId("hedge_fund".to_string())
            })
        );
    }
}
