use crate::cards::CardRegistry;
use crate::dsl::CardId;
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::paid_ability;
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

/// `state` is needed for the Runner arm — `RUNNER_MAX_HAND_SIZE` is reduced
/// by `state.runner.brain_damage`, which never heals.
fn max_hand_size(state: &GameState, side: Side) -> usize {
    match side {
        Side::Corp => CORP_MAX_HAND_SIZE + state.corp.max_hand_size_bonus as usize,
        Side::Runner => (RUNNER_MAX_HAND_SIZE + state.runner.max_hand_size_bonus as usize)
            .saturating_sub(state.runner.brain_damage),
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
/// The **single** authority on that count: both `finish_end_turn` (deciding
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
fn cards_over_hand_limit(state: &GameState, side: Side) -> usize {
    hand_size(state, side).saturating_sub(max_hand_size(state, side))
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

/// End the active side's turn. Opens a `WindowCheckpoint::EndOfTurn { side }`
/// paid ability window; closing it hands control to the other side via
/// [`finish_end_turn`] (see that function's doc comment for what happens
/// next — the hand-size/`Discard`/[`enter_start_of_turn`] logic this
/// function used to run inline).
///
/// Deliberately NOT modeled: individual `Trigger::OnTurnStart`-style card
/// reactions to the end-of-turn window itself (only the window/priority
/// machinery is generic — no card currently has an end-of-turn trigger).
///
/// Credits are untouched — they carry over turn to turn. **Clicks are
/// not**: unspent clicks are lost the moment a turn ends, so this zeroes
/// them for `side`.
///
/// This used to leave them in place, on the reasoning that "every
/// click-spending action is already gated by `engine::require_phase`, so
/// leftover clicks are inert." That holds for *actions* and fails for
/// *paid abilities*: `engine::activate_ability` resolves the acting side
/// from card ownership whenever a `PaidAbilityWindow` is open, explicitly
/// bypassing phase. A `Cost::Clicks` ability (Regolith Mining License's
/// `[click]: take 3[c]`) could therefore be paid for off-turn, out of
/// clicks that should no longer exist — at any run checkpoint, either turn
/// boundary, or a `WindowCheckpoint::PostAction`.
///
/// Zeroed here rather than in [`enter_start_of_turn`] because clicks are
/// lost when *this* turn ends, which is strictly earlier than when the
/// opponent's begins — and the gap between the two is exactly the
/// `EndOfTurn` window where they were spendable.
pub fn end_turn(state: &GameState, _registry: &CardRegistry) -> Result<(GameState, Vec<GameEvent>), RulesError> {
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

    let mut next = state.clone();
    let mut events = vec![GameEvent::TurnEnded { side }];

    // Before the window opens, so the window itself can't spend them —
    // it is part of the turn ending, not more action phase. See this
    // function's doc comment for why leaving them was unsafe.
    next.resources_mut(side).clicks = Clicks(0);

    if side == Side::Runner {
        // `BoostDuration::Turn` strength buffs last until end of turn, not
        // until the Runner's next turn — `end_turn` already guards
        // `CannotEndTurnWhileRunActive`, so there's no run-boundary
        // ambiguity to worry about here.
        next.runner.reset_turn_strength_buffs();
        // Snapshot before `enter_start_of_turn` (reached via this same
        // `EndOfTurn` window, or via `discard_card` if a mandatory discard
        // intervenes first) resets `made_successful_run_this_turn` for the
        // Runner's new turn — see `EffectRequirement::
        // RunnerMadeSuccessfulRunLastTurn`'s doc comment.
        next.runner.made_successful_run_last_turn = next.runner.made_successful_run_this_turn;
    }

    events.push(paid_ability::open_window_for(&mut next, side, WindowCheckpoint::EndOfTurn { side }));

    Ok((next, events))
}

/// Resumes what [`end_turn`]'s `WindowCheckpoint::EndOfTurn` window was
/// pausing: the hand-size check `end_turn` used to run inline. Hands control
/// to the other side via [`enter_start_of_turn`] if `side`'s hand is within
/// its max hand size (`CORP_MAX_HAND_SIZE`/`RUNNER_MAX_HAND_SIZE`); otherwise
/// transitions to `GamePhase::Discard { side, required }` first — control
/// only passes once `PlayerAction::DiscardCard` (via [`discard_card`])
/// clears it. Called only from `paid_ability::close_window`'s `EndOfTurn`
/// arm.
/// Emits `GameEvent::DiscardPhaseEnded` for `side` and dispatches whatever
/// reacts to it. Called from both places a discard phase can end — cleared
/// by `discard_card`, or skipped outright by `finish_end_turn` when the side
/// was already within hand size — so `Trigger::OnDiscardPhaseEnd` fires
/// exactly once per turn either way.
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

pub(crate) fn finish_end_turn(
    state: &mut GameState,
    side: Side,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = Vec::new();
    let over_by = cards_over_hand_limit(state, side);
    if over_by > 0 {
        state.phase = GamePhase::Discard { side, required: over_by };
        events.push(GameEvent::DiscardPending { side, required: over_by });
    } else {
        // The discard phase ends here even though it was skipped outright —
        // in rules terms the phase still happened, so anything keyed on its
        // end (Jinteki: Restoring Humanity) must still fire.
        events.extend(dispatch_discard_phase_end(state, side, registry)?);
        enter_start_of_turn(state, &mut events, side.other(), registry)?;
    }
    Ok(events)
}

/// Discard `card_id` from hand to satisfy a pending mandatory discard (see
/// [`end_turn`]). Errors with `RulesError::NotInDiscardPhase` outside
/// `GamePhase::Discard`, or `RulesError::CardNotInHand` if the card isn't in
/// the owing side's hand. Once the phase's `required` count reaches zero,
/// hands control to the other side via [`enter_start_of_turn`] — the same
/// handoff `end_turn` performs directly when no discard was owed at all.
pub fn discard_card(
    state: &GameState,
    card_id: CardId,
    registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = require_discard_phase(state)?;
    let mut next = state.clone();
    take_from_hand(&mut next, side, &card_id)?;
    discard_to_pile(&mut next, side, card_id.clone());
    let mut events = vec![GameEvent::CardDiscarded { side, card: card_id }];

    // Re-derived from the post-discard state, not decremented from the
    // phase's stored count — see `cards_over_hand_limit`'s doc comment.
    let remaining = cards_over_hand_limit(&next, side);
    if remaining == 0 {
        events.extend(dispatch_discard_phase_end(&mut next, side, registry)?);
        enter_start_of_turn(&mut next, &mut events, side.other(), registry)?;
    } else {
        next.phase = GamePhase::Discard { side, required: remaining };
    }

    Ok((next, events))
}

/// Flips control, refills clicks, and resolves `StartOfTurn(next_side)`'s
/// mandatory triggers before auto-advancing to `Action(next_side)`.
/// Centralizing entry here (rather than a bare side check inline in
/// `end_turn`, as before `GamePhase` existed) is what lets a future
/// `StartOfTurn(Runner)` trigger reuse this hook instead of `end_turn`/
/// `discard_card` growing another special case. Called from both `end_turn`
/// (hand size already within limits) and `discard_card` (last mandatory
/// discard just cleared).
///
/// If control is passing to the Corp and their R&D is empty, the Corp is
/// unable to make their mandatory draw and loses immediately (deck-out) —
/// the turn never actually starts: no clicks are refilled and no
/// `TurnStarted` is emitted, only `GameEvent::GameOver`. This check has to
/// live here rather than in `win::check_win_conditions`, since it's this
/// exact draw attempt that fails, not a standing condition safely
/// re-derivable from `GameState` alone elsewhere — see
/// `check_win_conditions`'s doc comment.
pub(crate) fn enter_start_of_turn(
    next: &mut GameState,
    events: &mut Vec<GameEvent>,
    next_side: Side,
    registry: &CardRegistry,
) -> Result<(), RulesError> {
    // The discard-phase-end dispatch just before this can end the game
    // (a flatlining `OnDiscardPhaseEnd`); writing `StartOfTurn` over a
    // `GameOver` would revert the win. Same guard again below, after the
    // start-of-turn triggers, before a window is opened over the corpse.
    if next.is_over() {
        return Ok(());
    }
    next.phase = GamePhase::StartOfTurn(next_side);

    if next_side == Side::Corp && next.corp.r_and_d.is_empty() {
        events.extend(win::end_game(next, Side::Runner));
        return Ok(());
    }

    // Below the deck-out return above, so a Corp that cannot make its
    // mandatory draw never counts the turn it failed to start — matching
    // this function's own "the turn never actually starts" rule, and
    // keeping `turn` in lockstep with `TurnStarted`, which is emitted
    // nowhere else.
    next.turn += 1;

    let clicks = clicks_for(next_side);
    next.resources_mut(next_side).clicks = Clicks(clicks);
    let turn_started_event = GameEvent::TurnStarted { side: next_side, clicks };
    events.push(turn_started_event.clone());

    if next_side == Side::Corp {
        // Top of R&D mirrors `RunnerState::stack`'s convention — drawing
        // pops the end of the Vec (see `engine.rs::draw_card_click`).
        if let Some(card) = next.corp.r_and_d.pop() {
            next.corp.hq.push(card);
            events.push(GameEvent::CardDrawn { side: Side::Corp });
        }

        next.corp.first_install_used_this_turn = false;
        next.corp.recurring_credits = next.corp.recurring_credits_max;
        next.corp.once_per_turn_used.clear();
        next.corp.agenda_points_scored_this_turn = 0;
        next.corp.cannot_score_agendas_this_turn = false;
        // Everything still installed was necessarily installed on an earlier
        // turn — Seamless Launch's "did not install this turn" eligibility.
        for installed in &mut next.corp.installed {
            installed.installed_this_turn = false;
        }
    } else {
        next.runner.first_hq_run_used_this_turn = false;
        next.runner.first_install_discount_used_this_turn = false;
        next.runner.once_per_turn_used.clear();
        next.runner.made_successful_run_this_turn = false;
    }

    // `Trigger::OnTurnStart` — e.g. PAD Campaign's "gain 1 credit". Only
    // rezzed Corp installs / any Runner rig card (always face-up) get their
    // start-of-turn ability; an unrezzed asset stays silent, same as every
    // other rez-gated ability in this engine — `dispatch_event` applies this
    // same scoping from `GameEvent::TurnStarted::side`.
    events.extend(dispatcher::dispatch_event(next, registry, &turn_started_event)?);
    if next.is_over() {
        return Ok(());
    }

    // Open a paid ability window before handing control over, giving both
    // sides a chance to fire a `Trigger::Paid` ability at the top of the new
    // turn. `next.phase` stays `StartOfTurn(next_side)` while it's open;
    // closing it (`paid_ability::close_window`'s `StartOfTurn` arm) sets
    // `phase = Action(next_side)`.
    events.push(paid_ability::open_window_for(next, next_side, WindowCheckpoint::StartOfTurn { side: next_side }));
    Ok(())
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

        let mut next = next;
        next.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        let (next, _) = end_turn(&next, &registry).expect("should succeed");
        let (next, _) = close_all_windows(next, &registry);
        assert_eq!(next.turn, 2, "Corp's turn began");
    }

    /// A Corp that cannot make its mandatory draw loses before the turn
    /// starts — no clicks, no `TurnStarted`, and so no increment either.
    /// `turn` and `TurnStarted` must never disagree.
    #[test]
    fn a_corp_deck_out_does_not_advance_the_turn_counter() {
        let state = game_state(Side::Runner, 0, 5, 0, 2);
        let registry = CardRegistry::new();
        let turn_before = state.turn;
        assert!(state.corp.r_and_d.is_empty(), "fixture must deck the Corp out");

        let (next, mut events) = end_turn(&state, &registry).expect("should succeed");
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        assert_eq!(next.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(next.turn, turn_before, "the turn never started, so it is not counted");
        assert!(!events.iter().any(|e| matches!(e, GameEvent::TurnStarted { .. })));
    }

    #[test]
    fn ending_turn_does_not_change_either_sides_credits() {
        let state = game_state(Side::Corp, 0, 5, 0, 2);
        let (next, _events) = end_turn(&state, &CardRegistry::new()).expect("should succeed");

        assert_eq!(next.corp.resources.credits, Credits(5));
        assert_eq!(next.runner.resources.credits, Credits(2));
    }

    #[test]
    fn ending_a_turn_with_clicks_left_loses_them() {
        // Ends the turn early, holding 2 of 3 clicks.
        let state = game_state(Side::Corp, 2, 5, 0, 2);

        let (next, _events) = end_turn(&state, &CardRegistry::new()).expect("should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(0), "unspent clicks are lost at end of turn");
        assert_eq!(next.corp.resources.credits, Credits(5), "credits, unlike clicks, carry over");
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
                trigger: Trigger::Paid,
                cost: Some(Cost::Clicks(1)),
                requirement: None,
                effect: Effect::GainCredits(Side::Corp, 3),
                cost_discount_if: None,
            }],
            is_playable: true,
            ..Default::default()
        });

        // Corp ends its turn holding 2 clicks, with the asset rezzed.
        let mut state = game_state(Side::Corp, 2, 5, 0, 2);
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
        assert_eq!(events, vec![GameEvent::TurnEnded { side: Side::Corp }, GameEvent::PaidAbilityWindowOpened { side: Side::Corp }]);
        // Control hasn't actually passed yet — this is still the ending side's turn.
        assert_eq!(next.phase, GamePhase::Action(Side::Corp));
    }

    #[test]
    fn enter_start_of_turn_opens_a_start_of_turn_window_after_mandatory_draw_and_ontrunstart_triggers() {
        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        let registry = CardRegistry::new();

        let (state, mut events) = end_turn(&state, &registry).expect("should succeed");
        let side = state.paid_ability_window.as_ref().expect("EndOfTurn window should be open").active_priority;
        let (state, ev) = crate::rules::apply_action(&state, &registry, PlayerAction::PassPriority { side }).expect("first pass should succeed");
        events.extend(ev);
        let side = state.paid_ability_window.as_ref().expect("still open after one pass").active_priority;
        let (state, ev) = crate::rules::apply_action(&state, &registry, PlayerAction::PassPriority { side })
            .expect("second pass should close the EndOfTurn window and open a StartOfTurn one");
        events.extend(ev);

        // Corp already drew (mandatory draw is part of entering their turn).
        assert!(state.corp.hq.contains(&CardId("hedge_fund".to_string())));
        assert_eq!(state.phase, GamePhase::StartOfTurn(Side::Corp));
        let window = state.paid_ability_window.expect("a StartOfTurn window should be open");
        assert_eq!(window.checkpoint, WindowCheckpoint::StartOfTurn { side: Side::Corp });
        assert_eq!(window.active_priority, Side::Corp);
        assert!(events.contains(&GameEvent::CardDrawn { side: Side::Corp }));
        assert!(events.contains(&GameEvent::TurnStarted { side: Side::Corp, clicks: 3 }));
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
    fn end_turn_for_runner_resets_turn_strength_buff() {
        use crate::rules::state::InstalledRunnerCard;

        let mut state = game_state(Side::Runner, 0, 5, 0, 2);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        state.runner.rig = vec![InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            encounter_strength_buff: 1,
            turn_strength_buff: 3,
            ..Default::default()
        }];

        let (next, _events) = end_turn(&state, &CardRegistry::new()).expect("should succeed");

        assert_eq!(next.runner.rig[0].turn_strength_buff, 0);
        // Encounter-duration buffs are a separate cleanup hook
        // (`run::engine::continue_run`), untouched here.
        assert_eq!(next.runner.rig[0].encounter_strength_buff, 1);
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
        // ends immediately — no underflow/panic, but also no turn starts
        // (no clicks refilled, no `TurnStarted`), and no further window
        // opens once `GameOver` is reached.
        assert!(next.corp.hq.is_empty());
        assert_eq!(next.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(next.corp.resources.clicks, Clicks(0));
        assert_eq!(next.paid_ability_window, None);
        assert!(events.contains(&GameEvent::TurnEnded { side: Side::Runner }));
        assert!(events.contains(&GameEvent::GameOver { winner: Side::Runner }));
        assert!(!events.contains(&GameEvent::TurnStarted { side: Side::Corp, clicks: 3 }));
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
        assert!(events.contains(&GameEvent::TurnEnded { side: Side::Corp }));
        assert!(events.contains(&GameEvent::DiscardPending { side: Side::Corp, required: 1 }));
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
    /// by `finish_end_turn` in the first place).
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
    /// live figure. Unreachable via `finish_end_turn` today — this pins the
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
