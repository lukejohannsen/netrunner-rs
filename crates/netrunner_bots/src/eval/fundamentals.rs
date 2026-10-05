//! The terms both chairs pay before their own arm: a parked payment
//! priced as the payment made, and a parked decision priced as a lower
//! bound on what resolving it delivers. Points and credits are added in
//! `evaluate_state_with` itself, ahead of these, in the order they always
//! were.

use super::*;

/// A payment parked on its payer's answer (`netrunner_core::rules::
/// PendingPayment`), priced as the payment made: each answer is applied,
/// any further question answered the same way, and the payer's best result
/// by the payer's own reckoning is the state scored.
///
/// A parked payment is the untouched state with a question on it — the
/// action it waits on has not happened at all, by construction (the
/// engine unwinds and replays) — so without this the action that parks one
/// scores as if it did nothing. That is not a small error: when the
/// Corp's accept of Anoetic Void's offer began asking which two cards of
/// HQ to trash, the one accept the heuristic made in a 192-game pass
/// became a decline, because "accept" now looked like "nothing" beside a
/// run continuing. The same shape as `pending_decision_upside` pricing a
/// parked prompt's continuation, and exact rather than a bound: the
/// payment's own answers are finite and each is a legal action. `None`
/// when nothing is parked, or no answer goes through.
pub(super) fn through_parked_payment(state: &GameState, registry: &CardRegistry, w: &Weights) -> Option<GameState> {
    let payment = state.pending_payment.as_ref()?;
    let payer = payment.side;
    payment
        .question
        .answers()
        .into_iter()
        .filter_map(|value| netrunner_core::rules::apply_action(state, registry, payment.question.action_for(value)).ok())
        .map(|(next, _)| through_parked_payment(&next, registry, w).unwrap_or(next))
        .map(|paid| (evaluate_state_with(&paid, payer, registry, w), paid))
        .max_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, paid)| paid)
}

/// A **lower bound** on what resolving the `PendingDecision` `side`
/// currently owes will add to this same evaluation — zero whenever the
/// decision's continuation is one this evaluator cannot price, which is
/// most of them.
///
/// A bound rather than an estimate, and that is the whole design: the
/// caller credits this alongside `PENDING_DECISION_UPSIDE_WEIGHT`, and
/// over-crediting a parked prompt makes sitting in it better than
/// resolving it — the wander `UNRESOLVED_DECISION_WEIGHT` exists to
/// stop. Everything below therefore prices the *worst* resolution the
/// decision allows, never the one the agent would like, and
/// `continuation_upside` is a **whitelist**: an effect it does not
/// recognise makes the whole continuation unpriceable rather than
/// contributing zero to a sum. Measured, on the branch that added this:
/// with a catch-all instead, *Touch-ups* stalled the view sweep at seed
/// 7 for the full 10,000 actions. Its `then` advances a card by two
/// (priced, +3.0) and *then* parks a `PresentChoice` (worth zero to this
/// term but −`UNRESOLVED_DECISION_WEIGHT` to the state), so confirming
/// the selection was 1.5 worse than sitting in it and the Corp toggled
/// position 4 on and off until the budget ran out. A continuation that
/// parks a further decision therefore has to be unpriceable, and a
/// whitelist gets that right for every future `Effect` variant without
/// anyone remembering to classify it.
///
/// Only `ChooseCards` is read. `ChooseEffect` gates a set of options
/// this would have to price one by one and take the worst of;
/// `ChooseServer` and `ChooseTriggerOrder` gate a run target and an
/// ordering rather than a value. All three keep the plain penalty they
/// have always had.
pub(super) fn pending_decision_upside(state: &GameState, side: Side, registry: &CardRegistry, w: &Weights) -> f64 {
    let Some(PendingDecision::ChooseCards { side: chooser, source, filter, min, destination, then, .. }) =
        &state.pending_decision
    else {
        return 0.0;
    };
    if *chooser != side {
        return 0.0;
    }
    // Cards this selection takes out of R&D, checked where the HQ term
    // is: that term switches off entirely below `rd_draw_reserve`, so a
    // bound that moved two cards into HQ without noticing R&D had
    // crossed the reserve would over-credit exactly the draw it prices.
    let rd_spent = if matches!(source, CardZoneRef::OwnRAndD) && destination.is_some() { *min as usize } else { 0 };
    let moved = match destination {
        // No destination means no cards move — the selection names
        // targets for the `then` and leaves them where they are.
        None => 0.0,
        Some(destination) => {
            zone_size_value(state, side, w, destination, i64::from(*min), rd_spent)
                + zone_size_value(state, side, w, source, -i64::from(*min), rd_spent)
        }
    };
    match then {
        None => moved,
        Some(then) => match continuation_upside(state, side, registry, w, source, filter, then, rd_spent) {
            Some(value) => moved + value,
            None => 0.0,
        },
    }
}

/// What the `then` of a parked selection is worth, or `None` when any
/// part of it is an effect this evaluator does not price — see
/// `pending_decision_upside` for why an unrecognised effect has to
/// poison the whole continuation rather than count as zero. `EndTheRun`
/// was the conspicuous absentee until the Corp got a run term at all
/// (`ACTIVE_RUN_AGAINST_WEIGHT`); it is priced here now, which is what
/// lets *Anoetic Void*'s "discard 2 from HQ to end the run" be accepted.
#[allow(clippy::too_many_arguments)]
pub(super) fn continuation_upside(
    state: &GameState,
    side: Side,
    registry: &CardRegistry,
    w: &Weights,
    source: &CardZoneRef,
    filter: &CardFilter,
    effect: &Effect,
    rd_spent: usize,
) -> Option<f64> {
    match effect {
        Effect::Sequence(effects) => effects
            .iter()
            .map(|effect| continuation_upside(state, side, registry, w, source, filter, effect, rd_spent))
            .sum(),
        Effect::GainCredits(gains, amount) if *gains == side => Some(f64::from(*amount) * w.own_credit_weight),
        Effect::DrawCards(draws, amount) if *draws == side => Some(zone_size_value(
            state,
            side,
            w,
            &own_hand_zone(side),
            i64::from(*amount),
            rd_spent + *amount as usize,
        )),
        Effect::PlaceAdvancementCounters(amount) => Some(advancement_upside(
            state,
            side,
            registry,
            w,
            source,
            filter,
            netrunner_core::rules::amount_on_table(amount, state, registry),
        )),
        // Exact rather than a bound, unusually for this function: ending
        // the run removes precisely the penalty the Corp branch applies —
        // the flat term, and the stakes when the planner prices them —
        // so the continuation is worth the terms and nothing else. Zero
        // when there is no run the Corp is paying for — an offer to end a
        // run the Runner cannot finish anyway buys nothing.
        Effect::EndTheRun if side == Side::Corp => Some(
            state
                .active_run
                .as_ref()
                .filter(|run| run_is_breakable(state, run, registry))
                .map_or(0.0, |run| w.active_run_against_weight + run_stakes(state, run, registry) * w.run_stakes_weight),
        ),
        _ => None,
    }
}

/// What playing an event or operation nets its owner, at the guide's
/// rate: its declared credits, its cards at a click each ("1 credit or
/// 1 card"), the clicks it gives or takes, less the click that plays it.
/// Hedge Fund is 4 × 0.4 − 0.4 = +1.2, Diesel 3 × 0.4 − 0.4 = +0.8,
/// Creative Commission 4 × 0.4 − 2 × 0.4 = +0.8, and Nanomanagement
/// −4 × 0.4 + 0.4 = −1.2: its clicks are worth what they do, which is
/// the planner's line to find, not this term's. Floored at zero for a
/// card in hand — a card is never worth less held than discarded.
pub(super) fn play_value(income: &Income, w: &Weights) -> f64 {
    f64::from(income.play_credits) * w.own_credit_weight
        + f64::from(income.play_cards) * w.click_weight
        + f64::from(income.play_clicks - 1) * w.click_weight
}

/// What an event or operation in hand declares, at the guide's rate:
/// its play's net (`play_value`), floored at zero — a card is never
/// worth less held than discarded. An install's future credits are
/// priced where the install is (`runner::install_delta`, the rig, a
/// rezzed Corp card), never held: a held value is a reason to hold, not
/// to draw, and the Corp's hand is nothing here for that reason
/// (`DECLARED_VALUE_WEIGHT`).
pub(super) fn declared_value(def: &CardDefinition, w: &Weights) -> f64 {
    match def.card_type {
        CardType::Event | CardType::Operation => play_value(&declared_income(def), w).max(0.0),
        _ => 0.0,
    }
}

/// `declared_value` summed over the Runner's events in grip, for
/// `DECLARED_VALUE_WEIGHT` — see that constant for why the Corp's hand
/// is nothing here, and the Runner's install cards are left to
/// `held_cards_value`, which already prices them at `held_card_weight`
/// with the future credits inside `install_delta`; counting them here
/// too would pay the same yield twice.
pub(super) fn held_declared_value(state: &GameState, side: Side, registry: &CardRegistry, w: &Weights) -> f64 {
    if side != Side::Runner {
        return 0.0;
    }
    state
        .runner
        .grip
        .iter()
        .filter_map(|card| registry.get(card))
        .filter(|def| matches!(def.card_type, CardType::Event))
        .map(|def| declared_value(def, w))
        .sum()
}

pub(super) fn own_hand_zone(side: Side) -> CardZoneRef {
    match side {
        Side::Corp => CardZoneRef::OwnHq,
        Side::Runner => CardZoneRef::OwnGrip,
    }
}

/// What `delta` cards arriving in (or leaving) `zone` are worth. Only the
/// two hands are worth anything at all here, and only through the
/// shortfall terms that already price them — a card above the floor is
/// worth nothing to this evaluator, so a draw into a healthy hand is
/// correctly credited zero rather than optimistically.
pub(super) fn zone_size_value(
    state: &GameState,
    side: Side,
    w: &Weights,
    zone: &CardZoneRef,
    delta: i64,
    rd_spent: usize,
) -> f64 {
    let (held, floor, weight) = match (side, zone) {
        // Mirrors the live term, `rd_draw_reserve` guard included.
        (Side::Corp, CardZoneRef::OwnHq) => {
            if state.corp.r_and_d.len() < w.rd_draw_reserve + rd_spent {
                return 0.0;
            }
            (state.corp.hq.len(), w.hq_floor, w.hq_shortfall_weight)
        }
        (Side::Runner, CardZoneRef::OwnGrip) => (state.runner.grip.len(), w.grip_floor, w.grip_shortfall_weight),
        _ => return 0.0,
    };
    let after = held.saturating_add_signed(delta as isize);
    let before_shortfall = floor.saturating_sub(held) as f64;
    let after_shortfall = floor.saturating_sub(after) as f64;
    (before_shortfall - after_shortfall) * weight
}

/// What another advancement token is worth on the *worst* card the parked
/// selection could put it on. `card_matches_filter` is the
/// definition-level half of the filter — its instance-level clauses
/// (`Unrezzed`, `TopOfZone`) pass everything — so the cards walked here
/// are a **superset** of the eligible ones, which is exactly what keeps
/// the minimum over them a lower bound.
///
/// The evaluator cannot see which card the selection will land on: a
/// toggle changes `selected`, which nothing here scores. So the worst
/// eligible target is not pessimism, it is the honest reading — an
/// agenda already at its requirement or an ambush at
/// `ambush_advancement_cap` gains nothing from another token, and where
/// one of those is eligible this term is correctly zero.
pub(super) fn advancement_upside(
    state: &GameState,
    side: Side,
    registry: &CardRegistry,
    w: &Weights,
    source: &CardZoneRef,
    filter: &CardFilter,
    amount: u32,
) -> f64 {
    if side != Side::Corp || !matches!(source, CardZoneRef::OwnInstalled) {
        return 0.0;
    }
    let mut worst: Option<f64> = None;
    // Advancing does not change what the rig covers, so the unbreakable
    // term is the same on both sides of the delta and cancels; the real
    // coverage is passed anyway rather than a stand-in that only happens
    // to cancel today. The horizon likewise: a token changes no card's
    // income.
    let rig = rig_coverage(state, registry);
    let horizon = horizon(stage(state, registry));
    for installed in &state.corp.installed {
        let Some(def) = registry.get(&installed.card) else { return 0.0 };
        if !card_matches_filter(def, filter) {
            continue;
        }
        let mut advanced = installed.clone();
        advanced.advancement_tokens += amount;
        let delta = corp_install_value(state, &advanced, registry, w, rig, horizon) - corp_install_value(state, installed, registry, w, rig, horizon);
        worst = Some(worst.map_or(delta, |worst: f64| worst.min(delta)));
    }
    worst.unwrap_or(0.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::test_support::*;
    use netrunner_core::dsl::{AbilityDef, CardDefinition, CardId, Trigger};
    use netrunner_core::rules::{AgendaPoints, GameState, InstallId, InstalledRunnerCard};

    /// A payment parked on a card question is scored as the payment made,
    /// with the pick its payer likes best — here the Runner keeps the
    /// program (held, it is worth something) and trashes the event — not
    /// as the untouched board the parked state is.
    #[test]
    fn a_parked_card_payment_is_scored_as_the_payers_best_answer() {
        use netrunner_core::dsl::{Cost, CardFilter, CardZoneRef, Effect};
        use netrunner_core::rules::{apply_action, Clicks, GamePhase, PlayerAction};
        let card = |id: &str, card_type: CardType| CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Runner,
            card_type,
            is_playable: true,
            ..Default::default()
        };
        let mut trasher = card("trasher", CardType::Resource);
        trasher.abilities = vec![AbilityDef {
            trigger: Trigger::Paid,
            cost: Some(Cost::Trash { from: CardZoneRef::OwnGrip, filter: CardFilter::Any, count: 1, reveal: false }),
            text: None,
            requirement: None,
            effect: Effect::GainCredits(Side::Runner, 5),
            cost_discount_if: None,
            used_by: None,
            access: false,
            from_hand: false,
            part_of: None,
        }];
        let registry = CardRegistry::from_cards(vec![trasher, card("an_event", CardType::Event), card("a_program", CardType::Program)]);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = vec![CardId("an_event".to_string()), CardId("a_program".to_string())];
        state.runner.rig = vec![InstalledRunnerCard { card: CardId("trasher".to_string()), install_id: InstallId(2001), ..Default::default() }];

        let (parked, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: InstallId(2001), ability_index: 0 }).expect("asks");
        assert!(parked.pending_payment.is_some());
        let w = Weights::default();
        let scores: Vec<f64> = (0..2)
            .map(|position| {
                let (paid, _) = apply_action(&parked, &registry, PlayerAction::ToggleCardSelection { position }).expect("pays");
                evaluate_state_with(&paid, Side::Runner, &registry, &w)
            })
            .collect();
        let best = scores.iter().copied().fold(f64::MIN, f64::max);
        assert_eq!(evaluate_state_with(&parked, Side::Runner, &registry, &w), best);
        assert!(evaluate_state_with(&parked, Side::Runner, &registry, &w) > evaluate_state_with(&state, Side::Runner, &registry, &w), "5 credits, paid for");
    }

    #[test]
    fn game_over_returns_win_or_loss_constant_regardless_of_other_fields() {
        let mut state = GameState::new(0);
        state.phase = GamePhase::GameOver(Side::Corp);
        state.runner.tags = 10;
        state.corp.bad_publicity = 10;

        assert_eq!(evaluate_state(&state, Side::Corp, &empty()), WIN_SCORE);
        assert_eq!(evaluate_state(&state, Side::Runner, &empty()), -WIN_SCORE);
    }

    #[test]
    fn agenda_point_lead_favors_the_leading_side() {
        let mut state = GameState::new(0);
        state.corp.resources.agenda_points = AgendaPoints(4);

        assert!(evaluate_state(&state, Side::Corp, &empty()) > 0.0);
        assert!(evaluate_state(&state, Side::Runner, &empty()) < 0.0);
    }

    #[test]
    fn corp_bad_publicity_lowers_the_corp_score() {
        let clean = GameState::new(0);
        let mut dirty = GameState::new(0);
        dirty.corp.bad_publicity = 3;

        assert!(evaluate_state(&clean, Side::Corp, &empty()) > evaluate_state(&dirty, Side::Corp, &empty()));
    }

    #[test]
    fn runner_tags_lower_the_runner_score() {
        let clean = GameState::new(0);
        let mut tagged = GameState::new(0);
        tagged.runner.tags = 2;

        assert!(evaluate_state(&clean, Side::Runner, &empty()) > evaluate_state(&tagged, Side::Runner, &empty()));
    }

    /// A parked decision the side must resolve scores below the same
    /// board with nothing parked, so confirming a selection is preferred
    /// to toggling it back and forth.
    #[test]
    fn an_unresolved_decision_of_ones_own_costs_something() {
        use netrunner_core::dsl::{CardFilter, CardZoneRef};
        use netrunner_core::rules::{PendingChoiceResume, PendingDecision};
        let registry = CardRegistry::new();
        let mut clear = GameState::new(0);
        clear.phase = GamePhase::Action(Side::Runner);
        let mut parked = clear.clone();
        parked.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Runner,
            source: CardZoneRef::OwnGrip,
            filter: CardFilter::Any,
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected: Vec::new(),
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        assert!(evaluate_state(&parked, Side::Runner, &registry) < evaluate_state(&clear, Side::Runner, &registry));
        assert_eq!(
            evaluate_state(&parked, Side::Corp, &registry),
            evaluate_state(&clear, Side::Corp, &registry),
            "the opponent's parked decision is not the Corp's problem"
        );
    }

    /// The structural guarantee behind `PENDING_DECISION_UPSIDE_WEIGHT`,
    /// asserted on the constants rather than inferred from a position:
    /// while the allowance stays under the penalty it is credited
    /// alongside, and the upside it accompanies is a lower bound on what
    /// resolving delivers, resolving a prompt always beats sitting in it.
    /// Raise this above `UNRESOLVED_DECISION_WEIGHT` and the toggle-walk
    /// that weight exists to stop comes back.
    #[test]
    fn the_prompt_allowance_stays_under_the_penalty_it_offsets() {
        let w = Weights::default();
        assert!(w.pending_decision_upside_weight < w.unresolved_decision_weight);
    }

    #[test]
    fn a_parked_prompt_that_will_advance_a_card_is_worth_entering_and_still_worth_resolving() {
        let registry = CardRegistry::from_cards(vec![advanceable("under", 3)]);
        let mut clear = GameState::new(0);
        clear.phase = GamePhase::Action(Side::Corp);
        clear.corp.installed = vec![under_requirement()];
        let parked = parked_advancement_prompt(vec![under_requirement()]);
        let mut resolved = clear.clone();
        resolved.corp.installed[0].advancement_tokens = 1;

        let score = |state: &GameState| evaluate_state(state, Side::Corp, &registry);
        assert!(
            score(&parked) > score(&clear),
            "a prompt that will land an advancement token is worth being handed"
        );
        assert!(
            score(&resolved) > score(&parked),
            "and resolving it still beats sitting in it — the toggle-walk guarantee"
        );
    }

    /// The bound is over the *worst* target the selection allows, because
    /// the evaluator cannot see which one a toggle will pick. One agenda
    /// already at its requirement among the eligible cards is enough to
    /// price the whole prompt at nothing.
    #[test]
    fn a_prompt_whose_worst_target_gains_nothing_keeps_the_full_penalty() {
        let registry = CardRegistry::from_cards(vec![advanceable("under", 3), advanceable("done", 1)]);
        let finished = InstalledCard {
            card: CardId("done".to_string()),
            install_id: InstallId(2),
            advancement_tokens: 1,
            ..Default::default()
        };
        let mut clear = GameState::new(0);
        clear.phase = GamePhase::Action(Side::Corp);
        clear.corp.installed = vec![under_requirement(), finished.clone()];
        let parked = parked_advancement_prompt(vec![under_requirement(), finished]);

        let score = |state: &GameState| evaluate_state(state, Side::Corp, &registry);
        assert!(score(&parked) < score(&clear), "the worst eligible target gains nothing, so neither does the prompt");
    }

    /// *Touch-ups*' shape, and the regression this whole design is
    /// built around: a continuation that advances a card (priced) and
    /// *then* parks a `PresentChoice` (not priced, but charged the
    /// penalty the moment it lands). Crediting the priced half alone
    /// made confirming the selection worse than sitting in it, and the
    /// heuristic Corp toggled one position on and off for the whole
    /// 10,000-action budget (view sweep, seed 7, gimbatul vs
    /// professional_opportunities). An unrecognised effect anywhere in
    /// the continuation has to make the whole thing unpriceable.
    #[test]
    fn a_continuation_that_parks_a_further_decision_is_not_priced_at_all() {
        let registry = CardRegistry::from_cards(vec![advanceable("under", 3)]);
        let mut clear = GameState::new(0);
        clear.phase = GamePhase::Action(Side::Corp);
        clear.corp.installed = vec![under_requirement()];

        let mut parked = parked_advancement_prompt(vec![under_requirement()]);
        let priced = evaluate_state(&parked, Side::Corp, &registry);
        let Some(PendingDecision::ChooseCards { then, .. }) = &mut parked.pending_decision else { unreachable!() };
        *then = Some(Box::new(Effect::Sequence(vec![
            Effect::PlaceAdvancementCounters(Amount::Fixed(1)),
            Effect::PresentChoice {
                chooser: Side::Corp,
                options: vec![Effect::Sequence(Vec::new()), Effect::Sequence(Vec::new())],
                texts: Vec::new(),
            },
        ])));

        assert!(evaluate_state(&parked, Side::Corp, &registry) < priced);
        assert_eq!(
            evaluate_state(&clear, Side::Corp, &registry) - evaluate_state(&parked, Side::Corp, &registry),
            UNRESOLVED_DECISION_WEIGHT,
            "a continuation that parks another decision keeps the plain penalty"
        );
    }

    /// A prompt whose continuation this evaluator cannot price — most of
    /// them, `EndTheRun` included — is charged the plain penalty exactly
    /// as it was before the term existed.
    #[test]
    fn an_unpriceable_prompt_is_charged_exactly_the_old_penalty() {
        use netrunner_core::rules::PendingChoiceResume;
        let registry = CardRegistry::new();
        let mut clear = GameState::new(0);
        clear.phase = GamePhase::Action(Side::Corp);
        let mut parked = clear.clone();
        parked.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OwnHq,
            filter: CardFilter::Any,
            min: 2,
            max: 2,
            reveal: false,
            shuffle_after: false,
            destination: Some(CardZoneRef::OwnArchives),
            then: Some(Box::new(Effect::EndTheRun)),
            selected: Vec::new(),
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        assert_eq!(
            evaluate_state(&clear, Side::Corp, &registry) - evaluate_state(&parked, Side::Corp, &registry),
            UNRESOLVED_DECISION_WEIGHT
        );
    }

    /// The stall that set `UNRESOLVED_DECISION_WEIGHT`'s size: resolving
    /// a parked selection must beat keeping it parked even when confirming
    /// costs a card from an at-floor hand, on either side.
    #[test]
    fn resolving_a_decision_outweighs_the_card_it_costs_from_an_at_floor_hand() {
        use netrunner_core::dsl::{CardFilter, CardZoneRef};
        use netrunner_core::rules::{PendingChoiceResume, PendingDecision};
        let registry = CardRegistry::new();
        let parked = |side: Side, source: CardZoneRef| PendingDecision::ChooseCards {
            side,
            source,
            filter: CardFilter::Any,
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected: Vec::new(),
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        };
        let mut corp_parked = GameState::new(0);
        corp_parked.phase = GamePhase::Action(Side::Corp);
        corp_parked.corp.r_and_d = corp_cards("rd", RD_DRAW_RESERVE + 10);
        corp_parked.corp.hq = corp_cards("hq", HQ_FLOOR);
        let mut corp_resolved = corp_parked.clone();
        corp_resolved.corp.hq.pop();
        corp_parked.pending_decision = Some(parked(Side::Corp, CardZoneRef::OwnHq));
        assert!(evaluate_state(&corp_resolved, Side::Corp, &registry) > evaluate_state(&corp_parked, Side::Corp, &registry));

        let mut runner_parked = GameState::new(0);
        runner_parked.phase = GamePhase::Action(Side::Runner);
        runner_parked.runner.grip = corp_cards("grip", GRIP_FLOOR);
        let mut runner_resolved = runner_parked.clone();
        runner_resolved.runner.grip.pop();
        runner_parked.pending_decision = Some(parked(Side::Runner, CardZoneRef::OwnGrip));
        assert!(evaluate_state(&runner_resolved, Side::Runner, &registry) > evaluate_state(&runner_parked, Side::Runner, &registry));
    }

    /// A play is worth what its text declares over the click that plays
    /// it: Hedge Fund four credits for a click, Diesel three cards, and
    /// Nanomanagement nothing on its own — its clicks are worth what the
    /// line does with them.
    #[test]
    fn a_play_is_worth_its_declared_credits_cards_and_clicks_over_the_click_it_costs() {
        let pool = pool();
        let w = guide();
        let value = |id: &str| play_value(&declared_income(&printed(&pool, id)), &w);
        assert!((value("hedge_fund") - 1.2).abs() < 1e-9, "{}", value("hedge_fund"));
        assert!((value("diesel") - 0.8).abs() < 1e-9);
        assert!((value("creative_commission") - 0.8).abs() < 1e-9, "four credits for two clicks");
        assert!(value("nanomanagement") < 0.0);
        assert_eq!(declared_value(&printed(&pool, "nanomanagement"), &w), 0.0, "floored in hand");
        assert!(value("hedge_fund") > value("diesel"), "the guide's best economy play");
    }

    /// The Runner holds a Sure Gamble for something and a card with no
    /// economy for nothing; the Corp's hand is priced by the line that
    /// plays it and not held at all (see `DECLARED_VALUE_WEIGHT`).
    #[test]
    fn an_event_in_grip_is_worth_its_declared_economy_and_the_corps_hand_is_not_held() {
        let pool = pool();
        let w = guide();
        let runner_holding = |id: &str, w: &Weights| {
            let mut state = GameState::new(0);
            state.runner.grip = corp_cards("filler", GRIP_FLOOR);
            state.runner.grip.push(CardId(id.to_string()));
            evaluate_state_with(&state, Side::Runner, &pool, w)
        };
        assert!((runner_holding("sure_gamble", &w) - runner_holding("palisade", &w) - 1.2 * w.declared_value_weight).abs() < 1e-9);
        assert_eq!(runner_holding("sure_gamble", &Weights::default()), runner_holding("palisade", &Weights::default()), "the reference is unmoved");
        let corp_holding = |id: &str| {
            let mut state = GameState::new(0);
            state.corp.r_and_d = corp_cards("rd", RD_DRAW_RESERVE + 10);
            state.corp.hq = corp_cards("filler", HQ_FLOOR);
            state.corp.hq.push(CardId(id.to_string()));
            evaluate_state_with(&state, Side::Corp, &pool, &w)
        };
        assert_eq!(corp_holding("hedge_fund"), corp_holding("palisade"), "a Hedge Fund in HQ is worth what the line that plays it is worth");
        assert_eq!(declared_value(&printed(&pool, "pad_campaign"), &w), 0.0, "an install's future is priced where the install is");
    }
}
