//! The Runner's arm: the rig, the grip, what the breach can find
//! (`access_prospect`), and the Corp's board as the Runner may read it.

use super::*;

/// What the breach of `run.server` is worth to the Runner, read only off
/// what its `ClientView` shows: hidden accesses at `active_run_weight`
/// apiece, advancement tokens on the target's face-down root cards,
/// known ambushes subtracted, and the net gain of trashing what it could
/// afford to trash there. The caller gates it on `run_is_breakable`, as
/// the flat run term was.
///
/// **Nothing here reads a sampled identity.** An unrezzed card counts by
/// its position and its tokens; a face-down Archives card counts once; a
/// card is read through the registry only when it is rezzed or face-up,
/// which is when the real Runner could read it too. That is the line
/// `UNREZZED_THREAT_WEIGHT` alone crosses, on purpose and at weight zero.
///
/// **"Seen this turn" is `servers_run_this_turn`.** A second R&D run
/// finds the same top card, a second Archives run a pile already turned
/// face-up, a second remote run the same face-down card — so those count
/// nothing. HQ is the one server a repeat can pay on, because each access
/// is a random card, and it pays exactly its chance of a card not yet
/// seen: after `k` earlier HQ runs this turn, an access from `n` cards is
/// fresh with probability `((n − 1) / n)^k`. Counting every repeat at
/// full value was measured first and the Runner ran HQ 1,252 times in
/// 96 games, thirteen a game, into a hand of one or two cards it had
/// already read. At four cards the second run is 0.45 and the third 0.34,
/// so the Runner runs HQ twice and then clicks for a credit. Every
/// `run::start_run` pushes the target at initiation, so the active run
/// is already on the list and "earlier" means a second entry; one
/// card-started path (`Effect::InitiateRun` through `run/engine.rs`'s
/// deduplicating push) records a repeat run once, and that run is priced
/// as if it were the first — the cheaper direction.
pub(super) fn access_prospect(state: &GameState, run: &RunState, registry: &CardRegistry, w: &Weights, credits: u32, horizon: u32) -> f64 {
    use netrunner_core::rules::{InstallSlot, ServerId};
    let server = run.server;
    let earlier = runs_earlier_this_turn(state, server);
    let seen = earlier > 0;
    let mut hidden = 0.0_f64;
    let mut tokens = 0u32;
    let mut finishable = 0usize;
    let mut ambushes = 0usize;
    let mut damage = 0usize;
    let mut trash_gain = 0.0;
    let corp_credits = state.corp.resources.credits.0;
    for installed in state.corp.installed.iter().filter(|card| card.server == server && card.slot == InstallSlot::Root) {
        if installed.rezzed || installed.seen_by_runner {
            let Some(def) = registry.get(&installed.card) else { continue };
            if punishes_access_with_damage(def) {
                ambushes += 1;
                damage += trap_damage(state, installed, def);
            }
            if let Some(cost) = def.trash_cost
                && cost <= credits
            {
                let removed = visible_install_value(installed, registry, w, horizon) * w.opponent_board_weight;
                trash_gain += (removed - f64::from(cost) * w.own_credit_weight).max(0.0);
            }
        } else if !seen {
            hidden += 1.0;
            tokens += installed.advancement_tokens;
            // "Read the counters on it and the Corp's credits": the
            // Corp's credits are public, the card's requirement is not,
            // so a typical one stands in. See `FINISHABLE_INSTALL_WEIGHT`.
            if corp_credits + installed.advancement_tokens >= TYPICAL_ADVANCEMENT_REQUIREMENT {
                finishable += 1;
            }
        }
    }
    match server {
        ServerId::Hq => {
            let held = state.corp.hq.len();
            let accesses = (1 + run.additional_hq_access as usize).min(held);
            let fresh = if held == 0 { 0.0 } else { ((held - 1) as f64 / held as f64).powi(earlier as i32) };
            hidden += accesses as f64 * fresh;
        }
        ServerId::RnD if !seen => hidden += (1 + run.additional_rd_access as usize).min(state.corp.r_and_d.len()) as f64,
        ServerId::RnD | ServerId::Remote(_) => {}
        ServerId::Archives => {
            for archived in &state.corp.archives {
                if archived.facedown {
                    if !seen {
                        hidden += 1.0;
                    }
                } else if let Some(def) = registry.get(&archived.card) {
                    if punishes_access_with_damage(def) {
                        ambushes += 1;
                    }
                    // A face-up agenda in Archives is a steal the breach
                    // cannot miss, so it is worth the points outright
                    // rather than a hidden access's 0.6 — the one reason
                    // to run a pile the Runner has already read. Only one
                    // the Runner can pay to steal: under a rezzed
                    // Magistrate Revontulet, with no credits, it was worth
                    // four Archives runs a turn and never a credit click,
                    // until the game ran out of steps (the 256-seed view
                    // sweep, seed 120).
                    if def.card_type == CardType::Agenda && can_pay_to_steal(state, registry, def, credits) {
                        trash_gain += f64::from(def.agenda_points.unwrap_or(0)) * w.agenda_point_weight;
                    }
                }
            }
        }
    }
    // "The Runner is flatlined immediately if they suffer more damage than
    // they have cards in their grip" (CR 1.7.2b).
    let trap = if damage > state.runner.grip.len() {
        w.lethal_trap_weight
    } else {
        damage as f64 * w.known_trap_damage_weight
    };
    hidden * w.active_run_weight + f64::from(tokens) * w.advanced_card_prospect_weight
        + finishable as f64 * w.finishable_install_weight
        - ambushes as f64 * w.known_ambush_weight
        - trap
        + trash_gain
}

/// The credits the Corp would spend rezzing the unrezzed ICE still ahead
/// of the Runner in `run`, `TYPICAL_REZ_COST` a piece out of what the
/// Corp has, outermost first, until it runs out — what precept 8's run
/// makes the Corp pay. See `FORCED_REZ_WEIGHT`. Reads the Corp's
/// credits and the ICE's position, never the sampled card under it.
pub(super) fn forced_rez_credits(state: &GameState, run: &RunState) -> u32 {
    let mut budget = state.corp.resources.credits.0;
    let mut spent = 0;
    for _ in run.ice.iter().skip(run.position).filter(|ice| !ice.rezzed) {
        let rez = TYPICAL_REZ_COST.min(budget);
        budget -= rez;
        spent += rez;
    }
    spent
}

/// Whether the Runner, holding `credits`, could pay what stealing `agenda`
/// costs right now (`continuous::steal_price`). A price with anything but
/// credits and clicks in it is taken as unpayable: no such agenda is in
/// the pool, and overvaluing a run is the error that stalled a game.
pub(super) fn can_pay_to_steal(state: &GameState, registry: &CardRegistry, agenda: &netrunner_core::dsl::CardDefinition, credits: u32) -> bool {
    use netrunner_core::dsl::Cost;
    fn needs(cost: &Cost) -> Option<(u32, u32)> {
        match cost {
            Cost::Credits(n) => Some((*n, 0)),
            Cost::Clicks(n) => Some((0, *n)),
            Cost::AllOf(parts) => parts.iter().try_fold((0, 0), |(c, k), part| needs(part).map(|(pc, pk)| (c + pc, k + pk))),
            _ => None,
        }
    }
    match netrunner_core::rules::continuous::steal_price(state, registry, agenda) {
        None => true,
        Some(cost) => needs(&cost).is_some_and(|(c, k)| c <= credits && k <= state.runner.resources.clicks.0),
    }
}

/// How many times the Runner ran `server` this turn before the run in
/// progress — see `access_prospect` for why the active run's own entry
/// is discounted.
pub(super) fn runs_earlier_this_turn(state: &GameState, server: netrunner_core::rules::ServerId) -> usize {
    let entries = state.runner.servers_run_this_turn.iter().filter(|ran| **ran == server).count();
    let own = usize::from(state.active_run.as_ref().is_some_and(|run| run.server == server));
    entries.saturating_sub(own)
}

/// The Corp's board as the Runner's evaluation reads it, summed over
/// `visible_install_value`; subtracted at `opponent_board_weight`.
pub(super) fn visible_corp_board(state: &GameState, registry: &CardRegistry, w: &Weights, horizon: u32) -> f64 {
    state.corp.installed.iter().map(|installed| visible_install_value(installed, registry, w, horizon)).sum()
}

/// `corp_install_value` for a viewer who cannot see under a face-down
/// card. A rezzed install is public and priced exactly as the Corp
/// prices it; an unrezzed one is its install weight plus every token on
/// it at `advancement_weight`, because the requirement that would cap
/// those tokens is on a card the Runner has not seen. Two determinized
/// samples that put different cards under the same install therefore
/// score identically here, which is what keeps this term honest for the
/// search (`the_corp_board_term_reads_no_hidden_identity`).
///
/// **`UNBREAKABLE_ICE_WEIGHT` is switched off here** — the `[true; 3]` —
/// and not because the Runner may not look at its own rig. It is that the
/// Runner already pays `BREAKER_COVERAGE_WEIGHT` for covering a subtype;
/// letting the same rig fact also shrink the Corp board it subtracts would
/// count one thing twice in one score. This term prices the Corp's
/// material, and the rig is priced where the rig lives.
/// **The rig is forced to `[true; 3]` and nothing else is.** The reason
/// is double counting, not visibility: the Runner already pays
/// `BREAKER_COVERAGE_WEIGHT` for covering a subtype, so letting the same
/// rig fact also shrink the Corp board it subtracts would count one thing
/// twice in one score. This term prices the Corp's material, and the rig
/// is priced where the rig lives.
///
/// `ETR_SUBROUTINE_WEIGHT` is deliberately left alone here, and the line
/// is preference against fact. The rig flags say what a rig *can do*;
/// how many run-ending subroutines a rezzed piece of ICE has is a
/// property of the card, public the moment it is face up, and more of
/// them really is more material on the table. So the Runner's reading of
/// the Corp board moves with that term — measured on the ladder square,
/// the Runner column falls 0.000 to 0.010 a rung — which is why a Corp
/// column taken on this build is not strictly comparable with one taken
/// before it: the reference Runner is not byte-identical either.
pub(super) fn visible_install_value(installed: &InstalledCard, registry: &CardRegistry, w: &Weights, horizon: u32) -> f64 {
    if installed.rezzed {
        corp_install_value(installed, registry, w, [true; 3], horizon)
    } else {
        w.unrezzed_install_weight + f64::from(installed.advancement_tokens) * w.advancement_weight
    }
}

/// What the Runner's grip is worth, summed over `install_delta` and
/// floored at zero per card; scaled by `held_card_weight`.
pub(super) fn held_cards_value(state: &GameState, registry: &CardRegistry, w: &Weights, horizon: u32) -> f64 {
    let rig = rig_coverage(state, registry);
    state
        .runner
        .grip
        .iter()
        .filter_map(|card| registry.get(card))
        .map(|def| install_delta(def, rig, w, horizon).max(0.0))
        .sum()
}

/// The future credits the rig's economy cards declare, each read with
/// its own hosted counters as the stock (`read::future_credits`), for
/// `future_credit_weight`. Zero at the default weights.
pub(super) fn rig_income(state: &GameState, registry: &CardRegistry, horizon: u32) -> f64 {
    state
        .runner
        .rig
        .iter()
        .filter_map(|card| registry.get(&card.card).map(|def| (def, card.counters)))
        .map(|(def, counters)| future_credits(&declared_income(def), Some(counters), horizon))
        .sum()
}

/// What this evaluator would credit the Runner for installing `def` from
/// its grip, credits and memory spent included: presence, plus coverage
/// for each ICE subtype the card breaks that the rig (`rig`) cannot,
/// minus the printed cost and the memory it takes. The same arithmetic
/// the install itself scores (the grip term aside), so a card is "live"
/// in hand exactly when the Runner would install it — leaving the memory
/// out made a held Cleaver worth more than the installed one and the
/// install a net loss. Zero for anything that is not a program, hardware
/// or resource.
pub(super) fn install_delta(def: &CardDefinition, rig: [bool; 3], w: &Weights, horizon: u32) -> f64 {
    if !matches!(def.card_type, CardType::Program | CardType::Hardware | CardType::Resource) {
        return 0.0;
    }
    let new_coverage = covers(def).iter().zip(rig).filter(|(grip, rig)| **grip && !rig).count();
    // The same future credits `rig_income` will count once the card is
    // installed, so an economy card is live in hand exactly when the
    // Runner would install it — the breaker's arithmetic, for money.
    let income = if w.future_credit_weight != 0.0 {
        future_credits(&declared_income(def), None, horizon) * w.future_credit_weight
    } else {
        0.0
    };
    w.board_presence_weight + new_coverage as f64 * w.breaker_coverage_weight + income
        - f64::from(def.cost) * w.own_credit_weight
        - f64::from(def.memory_cost.unwrap_or(0)) * w.memory_weight
}

/// Credits the Runner is short of the cheapest grip breaker worth
/// installing: one that covers a subtype the rig cannot break and fits in
/// free memory. Zero with no such card, or once it is affordable. Printed
/// cost, ignoring install discounts — over-estimating the target only
/// makes the Runner save one click longer.
pub(super) fn breaker_savings_shortfall(state: &GameState, registry: &CardRegistry) -> u32 {
    let rig = rig_coverage(state, registry);
    let target = state
        .runner
        .grip
        .iter()
        .filter_map(|card| registry.get(card))
        .filter(|def| def.memory_cost.unwrap_or(0) <= state.runner.memory_units.0)
        .filter(|def| covers(def).iter().zip(rig).any(|(grip, rig)| *grip && !rig))
        .map(|def| def.cost)
        .min();
    target.map_or(0, |cost| cost.saturating_sub(state.runner.resources.credits.0))
}

/// The Runner's terms, added to `score` in the order `evaluate_state_with`
/// always added them — see `corp::score`.
pub(super) fn score(state: &GameState, registry: &CardRegistry, w: &Weights, horizon: u32, score: &mut f64) {
    *score -= state.runner.tags as f64 * w.tag_weight;
    *score += state.runner.rig.len() as f64 * w.board_presence_weight;
    *score += state.runner.memory_units.0 as f64 * w.memory_weight;
    *score += breaker_coverage(state, registry) as f64 * w.breaker_coverage_weight;
    *score -= breaker_savings_shortfall(state, registry) as f64 * w.savings_shortfall_weight;
    *score -= w.grip_floor.saturating_sub(state.runner.grip.len()) as f64 * w.grip_shortfall_weight;
    *score += held_cards_value(state, registry, w, horizon) * w.held_card_weight;
    *score -= visible_corp_board(state, registry, w, horizon) * w.opponent_board_weight;
    if state.this_turn.times(Trigger::OnSuccessfulRun) > 0 {
        *score += w.successful_run_weight;
    }
    if let Some(run) = &state.active_run {
        let pool = state.runner.resources.credits.0 + run.bad_publicity_credits;
        if let Some(due) = remaining_break_cost(state, run, registry).filter(|due| *due <= pool) {
            *score += access_prospect(state, run, registry, w, pool - due, horizon);
        }
        *score -= pending_subroutines(run) as f64 * w.pending_subroutine_weight;
        *score -= strength_shortfall(state, run, registry) as f64 * w.strength_shortfall_weight;
        if w.unrezzed_threat_weight != 0.0 {
            *score -= unbreakable_unrezzed_ice(state, run, registry) as f64 * w.unrezzed_threat_weight;
        }
        if w.forced_rez_weight != 0.0 {
            *score += f64::from(forced_rez_credits(state, run)) * w.forced_rez_weight;
        }
    }
    // The guide's-rate terms, after everything above (see
    // `evaluate_state_with`).
    if w.future_credit_weight != 0.0 {
        *score += rig_income(state, registry, horizon) * w.future_credit_weight;
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::test_support::*;
    use netrunner_core::dsl::{CardDefinition, CardId};
    use netrunner_core::rules::{Credits, GameState, InstallId, InstalledRunnerCard};

    #[test]
    fn a_first_breaker_beats_a_credit_click_and_a_duplicate_does_not() {
        let registry = CardRegistry::from_cards(vec![breaker("cleaver", Some(IceType::Barrier))]);
        let mut clicked = GameState::new(0);
        clicked.runner.resources.credits = Credits(6);
        let mut installed = GameState::new(0);
        installed.runner.resources.credits = Credits(2); // paid 3 for Cleaver, no click credit
        installed.runner.rig = vec![rig_card("cleaver")];
        assert!(evaluate_state(&installed, Side::Runner, &registry) > evaluate_state(&clicked, Side::Runner, &registry));

        let mut second = installed.clone();
        second.runner.resources.credits = Credits(0);
        second.runner.rig.push(rig_card("cleaver"));
        let mut clicked_instead = installed.clone();
        clicked_instead.runner.resources.credits = Credits(3);
        assert!(evaluate_state(&clicked_instead, Side::Runner, &registry) > evaluate_state(&second, Side::Runner, &registry));
    }

    /// Starting a run must beat a credit, jacking out must lose the run
    /// bonus, and unbroken subroutines in an encounter must count against
    /// the Runner — that is what makes a heuristic Runner run at all, and
    /// use the breakers it installs.
    #[test]
    fn a_run_in_progress_beats_a_credit_and_pending_subroutines_count_against_it() {
        use netrunner_core::rules::{EncounteredSubroutine, RunIce, ServerId};
        let registry = CardRegistry::new();
        let mut clicked = GameState::new(0);
        clicked.runner.resources.credits = Credits(1);
        let mut running = GameState::new(0);
        running.corp.hq = corp_cards("hq", 1);
        clicked.corp.hq = corp_cards("hq", 1);
        running.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
        assert!(evaluate_state(&running, Side::Runner, &registry) > evaluate_state(&clicked, Side::Runner, &registry));

        let sub = |id| EncounteredSubroutine {
            id,
            definition: netrunner_core::dsl::SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None },
            status: SubroutineStatus::Pending,
            gained: false,
        };
        let mut encountering = running.clone();
        encountering.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            position: 0,
            ice: vec![RunIce {
                install_id: netrunner_core::rules::InstallId::PLACEHOLDER,
                card_id: CardId("ice_wall".to_string()),
                ice_type: IceType::Barrier,
                subroutines: vec![sub(0), sub(1), sub(2)],
                rezzed: true,
            }],
            ..Default::default()
        });
        let mut jacked_out = GameState::new(0);
        jacked_out.active_run = None;
        assert!(
            evaluate_state(&jacked_out, Side::Runner, &registry) > evaluate_state(&encountering, Side::Runner, &registry),
            "three unbroken subroutines are worse than no run"
        );
        let mut broke_one = encountering.clone();
        broke_one.runner.resources.credits = Credits(0); // paid 1 for it
        encountering.runner.resources.credits = Credits(1);
        broke_one.active_run.as_mut().unwrap().ice[0].subroutines[0].status = SubroutineStatus::Broken;
        assert!(
            evaluate_state(&broke_one, Side::Runner, &registry) > evaluate_state(&encountering, Side::Runner, &registry),
            "paying a credit to break a subroutine is worth it"
        );
    }

    /// Pumping a matching breaker up to the ICE's strength is worth its
    /// credits; a breaker for the wrong subtype leaves nothing to pump.
    #[test]
    fn a_strength_shortfall_against_a_matching_breaker_is_worth_pumping() {
        use netrunner_core::rules::{RunIce, ServerId};
        // The ice prints 4: a run's ice stores no strength of its own.
        let wall = CardDefinition {
            id: CardId("palisade".to_string()),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::Barrier),
            strength: Some(4),
            ..CardDefinition::default()
        };
        let registry = CardRegistry::from_cards(vec![breaker("cleaver", Some(IceType::Barrier)), wall]);
        let encountering = |strength: i32, credits: u32| {
            let mut state = GameState::new(0);
            state.runner.resources.credits = Credits(credits);
            state.runner.rig = vec![InstalledRunnerCard {
                card: CardId("cleaver".to_string()),
                base_strength: strength,
                ..Default::default()
            }];
            state.active_run = Some(RunState {
                server: ServerId::Hq,
                phase: RunPhase::EncounterIce,
                position: 0,
                ice: vec![RunIce {
                    install_id: netrunner_core::rules::InstallId::PLACEHOLDER,
                    card_id: CardId("palisade".to_string()),
                    ice_type: IceType::Barrier,
                    subroutines: Vec::new(),
                    rezzed: true,
                }],
                ..Default::default()
            });
            state
        };
        let short = encountering(3, 2);
        let pumped = encountering(4, 0); // paid 2 to pump one point
        assert!(evaluate_state(&pumped, Side::Runner, &registry) > evaluate_state(&short, Side::Runner, &registry));
        let over = encountering(5, 0);
        assert_eq!(
            evaluate_state(&over, Side::Runner, &registry),
            evaluate_state(&pumped, Side::Runner, &registry),
            "strength past the ICE's is worth nothing more"
        );
    }

    /// The whole point of the conditional run term: with no breaker for a
    /// rezzed ICE, a run is worth nothing (so a credit click wins); with a
    /// breaker and the credits to use it, the run is worth taking. What
    /// breaking costs is not subtracted from the run — `remaining_break_cost`
    /// records the measurement that says so — but it is gone before the
    /// Runner can trash anything at the end of it.
    #[test]
    fn a_run_into_rezzed_ice_with_no_matching_breaker_is_not_worth_a_click() {
        let registry = CardRegistry::from_cards(vec![priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        let ice = || vec![run_ice(1, IceType::Barrier, 1, true)];
        assert_eq!(run_term(vec![], 5, ice(), 0, &registry), 0.0, "nothing in the rig breaks a Barrier");
        let wrong_subtype = priced_breaker("carmen", Some(IceType::Sentry), (1, 1), (2, 3));
        let registry = CardRegistry::from_cards(vec![wrong_subtype, priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        assert_eq!(run_term(vec![rig_card("carmen")], 5, ice(), 0, &registry), 0.0, "a Sentry breaker does not break a Barrier");
        let cleaver = InstalledRunnerCard { base_strength: 3, ..rig_card("cleaver") };
        assert_eq!(run_term(vec![cleaver], 5, ice(), 0, &registry), ACTIVE_RUN_WEIGHT);
    }

    /// Owning the breaker is not enough: the run is credited only when the
    /// pump and break credits are actually in hand (bad-publicity credits
    /// count — they are spendable on exactly this), and what it is
    /// credited is the breach, whole.
    #[test]
    fn a_run_is_credited_only_when_the_breaks_are_affordable() {
        use netrunner_core::rules::ServerId;
        let registry = CardRegistry::from_cards(vec![priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        let cleaver = || vec![InstalledRunnerCard { base_strength: 3, ..rig_card("cleaver") }];
        // Strength 4 against Cleaver's 3: one 2[c] pump, then one 1[c] break covers both subroutines.
        let ice = || vec![run_ice(4, IceType::Barrier, 2, true)];
        assert_eq!(run_term(cleaver(), 2, ice(), 0, &registry), 0.0, "3 credits needed, 2 held");
        assert_eq!(run_term(cleaver(), 3, ice(), 0, &registry), ACTIVE_RUN_WEIGHT);
        // Two rezzed ICE are paid for together.
        let two = || vec![run_ice(4, IceType::Barrier, 2, true), run_ice(1, IceType::Barrier, 3, true)];
        assert_eq!(run_term(cleaver(), 4, two(), 0, &registry), 0.0, "3 + 2 credits needed, 4 held");
        assert_eq!(run_term(cleaver(), 5, two(), 0, &registry), ACTIVE_RUN_WEIGHT);

        let registry = with_printed_ice(&registry, &ice());
        let mut idle = GameState::new(0);
        idle.runner.resources.credits = Credits(2);
        idle.runner.rig = cleaver();
        idle.corp.hq = corp_cards("hq", 1);
        let mut running = idle.clone();
        running.active_run =
            Some(RunState { server: ServerId::Hq, ice: ice(), bad_publicity_credits: 1, ..Default::default() });
        let term = evaluate_state(&running, Side::Runner, &registry) - evaluate_state(&idle, Side::Runner, &registry);
        assert_eq!(round3(term), ACTIVE_RUN_WEIGHT, "a bad-publicity credit closes the gap");
    }

    /// An unrezzed ICE's identity in a determinized sample is a guess the
    /// real Runner cannot see, so it never blocks the run term; nor does
    /// ICE the run has already passed. `unbreakable_unrezzed_ice` is the
    /// term that may price it instead, and is weighted to zero here.
    #[test]
    fn unrezzed_and_already_passed_ice_never_block_the_run_term() {
        let registry = CardRegistry::new();
        let unrezzed = vec![run_ice(9, IceType::Barrier, 3, false)];
        assert_eq!(run_term(vec![], 0, unrezzed, 0, &registry), ACTIVE_RUN_WEIGHT);
        let passed = vec![run_ice(9, IceType::Barrier, 3, true)];
        assert_eq!(run_term(vec![], 0, passed, 1, &registry), ACTIVE_RUN_WEIGHT);
        let no_subroutines = vec![run_ice(9, IceType::Barrier, 0, true)];
        assert_eq!(run_term(vec![], 0, no_subroutines, 0, &registry), ACTIVE_RUN_WEIGHT, "nothing to break costs nothing");
    }

    /// The whole point of the savings term: with an unaffordable breaker
    /// in grip, a credit click beats a run on an open server; once the
    /// breaker is affordable the penalty is gone and credits are worth
    /// only their usual weight.
    #[test]
    fn a_credit_click_beats_an_open_run_while_a_grip_breaker_is_unaffordable() {
        use netrunner_core::rules::{MemoryUnits, ServerId};
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3)]);
        let saving = |credits: u32| {
            let mut state = GameState::new(0);
            state.runner.resources.credits = Credits(credits);
            state.runner.memory_units = MemoryUnits(4);
            state.runner.grip = vec![CardId("cleaver".to_string())];
            state.corp.hq = corp_cards("hq", 1);
            state
        };
        let clicked = saving(2);
        let mut ran = saving(1);
        ran.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
        assert!(evaluate_state(&clicked, Side::Runner, &registry) > evaluate_state(&ran, Side::Runner, &registry));

        let affordable = evaluate_state(&saving(3), Side::Runner, &registry);
        let one_more = evaluate_state(&saving(4), Side::Runner, &registry);
        assert!(((one_more - affordable) - OWN_CREDIT_WEIGHT).abs() < 1e-9, "no penalty left to close");
        let short_by_one = evaluate_state(&saving(2), Side::Runner, &registry);
        assert!(
            ((affordable - short_by_one) - (OWN_CREDIT_WEIGHT + SAVINGS_SHORTFALL_WEIGHT)).abs() < 1e-9,
            "the last credit of the gap is worth its weight plus the shortfall"
        );
    }

    /// Nothing to save for: a grip breaker for a subtype the rig already
    /// covers, or one that does not fit in free memory.
    #[test]
    fn a_covered_subtype_or_a_breaker_that_does_not_fit_in_memory_is_not_saved_for() {
        use netrunner_core::rules::MemoryUnits;
        let registry = CardRegistry::from_cards(vec![
            costed_breaker("cleaver", Some(IceType::Barrier), 3),
            costed_breaker("corroder", Some(IceType::Barrier), 2),
        ]);
        let mut state = GameState::new(0);
        state.runner.resources.credits = Credits(0);
        state.runner.memory_units = MemoryUnits(4);
        state.runner.grip = vec![CardId("corroder".to_string())];
        assert_eq!(breaker_savings_shortfall(&state, &registry), 2);
        state.runner.rig = vec![rig_card("cleaver")];
        assert_eq!(breaker_savings_shortfall(&state, &registry), 0, "Barrier is already covered");
        state.runner.rig.clear();
        state.runner.memory_units = MemoryUnits(0);
        assert_eq!(breaker_savings_shortfall(&state, &registry), 0, "no memory to install it into");
    }

    /// The reason this is a penalty on the shortfall and not a bonus on
    /// credits held: installing the breaker saved for must still beat
    /// clicking once the credits are there.
    #[test]
    fn installing_the_breaker_saved_for_still_beats_clicking() {
        use netrunner_core::rules::MemoryUnits;
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3)]);
        let mut clicked = GameState::new(0);
        clicked.runner.resources.credits = Credits(4);
        clicked.runner.memory_units = MemoryUnits(4);
        clicked.runner.grip = vec![CardId("cleaver".to_string())];
        let mut installed = GameState::new(0);
        installed.runner.resources.credits = Credits(0);
        installed.runner.memory_units = MemoryUnits(3);
        installed.runner.rig = vec![rig_card("cleaver")];
        assert!(evaluate_state(&installed, Side::Runner, &registry) > evaluate_state(&clicked, Side::Runner, &registry));
    }

    /// The grip term is a shortfall below a floor: each card up to the
    /// floor is worth `GRIP_SHORTFALL_WEIGHT`, cards past it nothing.
    #[test]
    fn a_grip_below_the_floor_costs_something_and_a_full_one_does_not() {
        let registry = CardRegistry::new();
        let with_grip = |cards: usize| {
            let mut state = GameState::new(0);
            state.runner.grip = (0..cards).map(|i| CardId(format!("card_{i}"))).collect();
            evaluate_state(&state, Side::Runner, &registry)
        };
        assert!((with_grip(1) - with_grip(0) - GRIP_SHORTFALL_WEIGHT).abs() < 1e-9);
        assert!((with_grip(GRIP_FLOOR) - with_grip(GRIP_FLOOR - 1) - GRIP_SHORTFALL_WEIGHT).abs() < 1e-9);
        assert_eq!(with_grip(GRIP_FLOOR + 1), with_grip(GRIP_FLOOR), "cards past the floor are worth nothing");
    }

    /// The door: a run that reached its server this turn is worth the run
    /// term for the rest of the turn, once, and only to the Runner.
    #[test]
    fn a_successful_run_this_turn_keeps_the_run_term_once() {
        let registry = CardRegistry::new();
        let mut breached = GameState::new(0);
        // Counted the way the engine counts it: there is no flag to set.
        netrunner_core::rules::dispatch_event(&mut breached, &registry, &netrunner_core::rules::GameEvent::RunSucceeded { server: netrunner_core::rules::ServerId::Archives }).expect("a run succeeds");
        let fresh = GameState::new(0);
        let delta = evaluate_state(&breached, Side::Runner, &registry) - evaluate_state(&fresh, Side::Runner, &registry);
        assert!((delta - SUCCESSFUL_RUN_WEIGHT).abs() < 1e-9);
        assert_eq!(
            evaluate_state(&breached, Side::Corp, &registry),
            evaluate_state(&fresh, Side::Corp, &registry),
            "Runner-only"
        );
    }

    /// Why the weight sits above the run term: with a thin grip a draw
    /// beats an open-server run; with the floor met, the run wins again.
    #[test]
    fn drawing_beats_an_open_run_below_the_grip_floor_and_not_above_it() {
        use netrunner_core::rules::ServerId;
        let registry = CardRegistry::new();
        let holding = |cards: usize, running: bool| {
            let mut state = GameState::new(0);
            state.runner.grip = (0..cards).map(|i| CardId(format!("card_{i}"))).collect();
            state.corp.hq = corp_cards("hq", 1);
            if running {
                state.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
            }
            evaluate_state(&state, Side::Runner, &registry)
        };
        assert!(holding(GRIP_FLOOR, false) > holding(GRIP_FLOOR - 1, true), "draw up to the floor first");
        assert!(holding(GRIP_FLOOR, true) > holding(GRIP_FLOOR + 1, false), "then run rather than keep drawing");
    }

    /// The Carmen case behind `BREAKER_COVERAGE_WEIGHT`'s size: a 5-cost
    /// breaker is worth installing over an open run. **From a grip above
    /// the floor**, since `HELD_CARD_WEIGHT`: from an at-floor grip the
    /// install now loses the card's held value *and* the grip shortfall,
    /// and the open run wins — the Runner draws first (a live top card is
    /// worth more than the run) or runs, and installs Carmen the click
    /// after. That trade was measured rather than avoided: with the held
    /// term the Runner draws more than twice as often and Carmen is
    /// installed as often as before, so the old at-floor claim is not
    /// worth a lower held weight (one that would keep it, 0.13, leaves
    /// no draw worth a credit).
    #[test]
    fn a_five_cost_breaker_beats_an_open_run_from_a_grip_above_the_floor() {
        use netrunner_core::rules::{MemoryUnits, ServerId};
        let registry = CardRegistry::from_cards(vec![costed_breaker("carmen", Some(IceType::Sentry), 5)]);
        let position = |grip: usize| {
            let mut ran = GameState::new(0);
            ran.runner.resources.credits = Credits(5);
            ran.runner.memory_units = MemoryUnits(4);
            ran.corp.hq = corp_cards("hq", 1);
            ran.runner.grip = corp_cards("grip", grip);
            ran.runner.grip.push(CardId("carmen".to_string()));
            let mut installed = ran.clone();
            installed.runner.grip.pop();
            installed.runner.resources.credits = Credits(0);
            installed.runner.memory_units = MemoryUnits(3);
            installed.runner.rig = vec![rig_card("carmen")];
            ran.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
            (evaluate_state(&installed, Side::Runner, &registry), evaluate_state(&ran, Side::Runner, &registry))
        };
        let (installed, ran) = position(GRIP_FLOOR);
        assert!(installed > ran, "above the floor Carmen beats the run: {installed} vs {ran}");
        let (installed, ran) = position(GRIP_FLOOR - 1);
        assert!(installed < ran, "at the floor the run comes first: {installed} vs {ran}");
    }

    /// The person's complaint, as a test: a pile the Runner has already
    /// turned face-up is not worth a click, a face-down card in it is, and
    /// a face-up agenda in it is the steal the breach cannot miss.
    #[test]
    fn an_archives_run_is_worth_its_face_down_cards_and_face_up_agendas_and_nothing_else() {
        use netrunner_core::rules::{ArchivedCard, ServerId};
        let registry = CardRegistry::from_cards(vec![asset("nico_campaign", 2), advanceable("offworld_office", 3)]);
        let mut state = GameState::new(0);
        state.corp.archives = vec![ArchivedCard::faceup(CardId("nico_campaign".to_string())); 3];
        assert_eq!(prospect(&state, ServerId::Archives, &registry), 0.0, "three known assets: nothing to find");
        state.corp.archives.push(ArchivedCard { card: CardId("nico_campaign".to_string()), facedown: true });
        assert_eq!(prospect(&state, ServerId::Archives, &registry), ACTIVE_RUN_WEIGHT, "one card the Runner has not seen");
        state.corp.archives.push(ArchivedCard::faceup(CardId("offworld_office".to_string())));
        assert_eq!(
            prospect(&state, ServerId::Archives, &registry),
            ACTIVE_RUN_WEIGHT + 2.0 * AGENDA_POINT_WEIGHT,
            "a face-up agenda is worth its points"
        );
    }

    /// The position the person watched the bot lose from: Urtica Cipher
    /// face-up in Archives beside one face-down card. The face-down card
    /// alone would be worth a run; the known ambush makes the run worth
    /// less than the credit click it displaces.
    #[test]
    fn a_known_ambush_in_archives_makes_the_run_worth_less_than_a_credit() {
        use netrunner_core::rules::{ArchivedCard, ServerId};
        let registry = CardRegistry::from_cards(vec![ambush("urtica_cipher"), asset("nico_campaign", 2)]);
        let mut state = GameState::new(0);
        state.corp.archives = vec![
            ArchivedCard::faceup(CardId("urtica_cipher".to_string())),
            ArchivedCard { card: CardId("nico_campaign".to_string()), facedown: true },
        ];
        let term = prospect(&state, ServerId::Archives, &registry);
        assert!((term - (ACTIVE_RUN_WEIGHT - KNOWN_AMBUSH_WEIGHT)).abs() < 1e-9, "{term}");
        assert!(term < OWN_CREDIT_WEIGHT, "a credit click wins");
        // The same ambush, still face down, is not read — its identity in
        // a determinized sample is a guess.
        state.corp.archives[0].facedown = true;
        assert_eq!(prospect(&state, ServerId::Archives, &registry), 2.0 * ACTIVE_RUN_WEIGHT);
    }

    /// The same top card, the same pile, the same face-down remote: a
    /// second run this turn finds nothing new anywhere but HQ.
    #[test]
    fn a_second_run_on_the_same_server_this_turn_is_worth_nothing_except_on_hq() {
        use netrunner_core::rules::{ArchivedCard, ServerId};
        let registry = CardRegistry::from_cards(vec![asset("nico_campaign", 2)]);
        let mut state = GameState::new(0);
        state.corp.hq = corp_cards("hq", 4);
        state.corp.r_and_d = corp_cards("rd", 10);
        state.corp.archives = vec![ArchivedCard { card: CardId("nico_campaign".to_string()), facedown: true }];
        state.corp.installed = vec![InstalledCard {
            card: CardId("nico_campaign".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            ..Default::default()
        }];
        for server in [ServerId::Hq, ServerId::RnD, ServerId::Archives, ServerId::Remote(0)] {
            assert_eq!(prospect(&state, server, &registry), ACTIVE_RUN_WEIGHT, "first run on {server:?}");
        }
        // `start_run` records the target at initiation, so a run in
        // progress is already listed once; a repeat is a second entry.
        let mut again = state.clone();
        for server in [ServerId::Hq, ServerId::RnD, ServerId::Archives, ServerId::Remote(0)] {
            again.runner.servers_run_this_turn = vec![server, server];
            // HQ holds four cards, so the second access is fresh three
            // times in four; everywhere else a repeat finds what the first
            // run found.
            let expected = if server == ServerId::Hq { 0.75 * ACTIVE_RUN_WEIGHT } else { 0.0 };
            assert!((prospect(&again, server, &registry) - expected).abs() < 1e-9, "second run on {server:?}");
            again.runner.servers_run_this_turn = vec![server];
            assert_eq!(prospect(&again, server, &registry), ACTIVE_RUN_WEIGHT, "the run's own entry is not a repeat");
        }
        // The repeat's value falls with each run and with a thinner HQ:
        // a third run at four cards is under a credit click, and a second
        // run on a one-card HQ finds the card it already saw.
        again.runner.servers_run_this_turn = vec![ServerId::Hq; 3];
        let third = prospect(&again, ServerId::Hq, &registry);
        assert!(third < OWN_CREDIT_WEIGHT && third > 0.0, "{third}");
        again.corp.hq = corp_cards("hq", 1);
        again.runner.servers_run_this_turn = vec![ServerId::Hq; 2];
        assert_eq!(prospect(&again, ServerId::Hq, &registry), 0.0);
    }

    /// Where the Corp is scoring is where the Runner goes first: each
    /// token on a face-down root card adds to the run, and past one token
    /// the remote is worth more than any central.
    #[test]
    fn an_advanced_face_down_remote_card_is_worth_more_than_a_central() {
        use netrunner_core::rules::ServerId;
        let registry = CardRegistry::from_cards(vec![advanceable("offworld_office", 3)]);
        let mut state = GameState::new(0);
        state.corp.hq = corp_cards("hq", 4);
        state.corp.installed = vec![InstalledCard {
            card: CardId("offworld_office".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            advancement_tokens: 2,
            ..Default::default()
        }];
        let remote = prospect(&state, ServerId::Remote(0), &registry);
        assert!((remote - (ACTIVE_RUN_WEIGHT + 2.0 * ADVANCED_CARD_PROSPECT_WEIGHT)).abs() < 1e-9, "{remote}");
        assert!(remote > prospect(&state, ServerId::Hq, &registry));
        // The tokens are read off the install, not the card: an ambush
        // advanced twice is worth exactly the same run, because the Runner
        // cannot tell the two apart.
        let trap = CardRegistry::from_cards(vec![ambush("offworld_office")]);
        assert_eq!(prospect(&state, ServerId::Remote(0), &trap), remote);
    }

    /// What the Runner can afford at the end of a run is what it has left
    /// after breaking in. Before this it read the credits it started the
    /// run with, so it could break in and arrive unable to trash the
    /// asset it came for. The credits themselves are not charged to the
    /// run — see `remaining_break_cost`.
    #[test]
    fn a_trash_is_priced_from_what_is_left_after_breaking_in() {
        use netrunner_core::rules::{RunIce, ServerId};
        let mut nico = asset("nico_campaign", 2);
        nico.trash_cost = Some(2);
        let registry = CardRegistry::from_cards(vec![nico, priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        let ice = |strength| vec![RunIce { rezzed: true, ..run_ice(strength, IceType::Barrier, 1, true) }];
        let at = |credits: u32, strength: i32| {
            let mut state = GameState::new(0);
            state.runner.resources.credits = Credits(credits);
            state.runner.rig = vec![InstalledRunnerCard { base_strength: 3, ..rig_card("cleaver") }];
            state.corp.installed = vec![InstalledCard {
                card: CardId("nico_campaign".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                rezzed: true,
                ..Default::default()
            }];
            let registry = with_printed_ice(&registry, &ice(strength));
            let mut running = state.clone();
            running.active_run = Some(RunState { server: ServerId::Remote(0), ice: ice(strength), ..Default::default() });
            let term = evaluate_state(&running, Side::Runner, &registry) - evaluate_state(&state, Side::Runner, &registry);
            (term * 1000.0).round() / 1000.0
        };
        let removed = (BOARD_PRESENCE_WEIGHT + REZZED_ASSET_WEIGHT) * OPPONENT_BOARD_WEIGHT;
        let worth_trashing = round3((removed - 2.0 * OWN_CREDIT_WEIGHT).max(0.0));
        // Strength 5 against Cleaver's 3: two 2[c] pumps and a 1[c] break,
        // so 5 of the Runner's credits are gone before the breach.
        assert_eq!(at(6, 5), 0.0, "6 credits, 5 to break in: 1 left does not pay a 2[c] trash");
        assert_eq!(at(7, 5), worth_trashing, "7 leaves exactly the 2 the trash costs");
        // The same breach with a strength-3 Barrier costs 1[c] to break,
        // so 3 credits are enough for both.
        assert_eq!(at(3, 3), worth_trashing, "a cheap toll leaves the trash affordable");
        assert_eq!(at(2, 3), 0.0, "and 2 credits do not cover both");
    }

    /// The trash lever, both halves: a rezzed asset the Runner can afford
    /// to trash makes the run worth starting, and once accessed, trashing
    /// it beats leaving it — at 2[c], and not at 4[c].
    #[test]
    fn a_trashable_rezzed_asset_is_worth_the_run_and_the_trash() {
        use netrunner_core::rules::{ArchivedCard, ServerId};
        let priced = |trash_cost: u32| {
            let mut nico = asset("nico_campaign", 2);
            nico.trash_cost = Some(trash_cost);
            CardRegistry::from_cards(vec![nico])
        };
        let mut state = GameState::new(0);
        state.runner.resources.credits = Credits(5);
        state.corp.installed = vec![InstalledCard {
            card: CardId("nico_campaign".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];
        let removed = (BOARD_PRESENCE_WEIGHT + REZZED_ASSET_WEIGHT) * OPPONENT_BOARD_WEIGHT;
        let cheap = prospect(&state, ServerId::Remote(0), &priced(2));
        assert!((cheap - (removed - 2.0 * OWN_CREDIT_WEIGHT)).abs() < 1e-9, "{cheap}");
        assert!(cheap > 0.0, "the run that ends the loop is worth starting");
        assert_eq!(prospect(&state, ServerId::Remote(0), &priced(4)), 0.0, "too dear to trash: a known card, nothing to find");
        assert_eq!(prospect(&state, ServerId::Remote(0), &priced(2)), cheap, "the prospect does not depend on having run before");

        let mut trashed = state.clone();
        trashed.corp.installed.clear();
        trashed.corp.archives = vec![ArchivedCard::faceup(CardId("nico_campaign".to_string()))];
        trashed.runner.resources.credits = Credits(3);
        assert!(evaluate_state(&trashed, Side::Runner, &priced(2)) > evaluate_state(&state, Side::Runner, &priced(2)));
        assert!(
            (evaluate_state(&trashed, Side::Corp, &priced(2)) - evaluate_state(&state, Side::Corp, &priced(2))
                - (-(BOARD_PRESENCE_WEIGHT + REZZED_ASSET_WEIGHT) + 2.0 * OPPONENT_CREDIT_WEIGHT))
                .abs()
                < 1e-9,
            "the Corp's own reading of the same trash is unchanged"
        );
        let mut dear = trashed.clone();
        dear.runner.resources.credits = Credits(1);
        assert!(evaluate_state(&dear, Side::Runner, &priced(4)) < evaluate_state(&state, Side::Runner, &priced(4)));
    }

    /// The property that keeps the board term usable by a search over
    /// determinized samples: two samples that put different cards under
    /// the same face-down install score the same for the Runner.
    #[test]
    fn the_corp_board_term_reads_no_hidden_identity() {
        use netrunner_core::rules::ServerId;
        let registry = CardRegistry::from_cards(vec![advanceable("offworld_office", 3), ambush("urtica_cipher"), ice("palisade", 3)]);
        let under = |card: &str, rezzed: bool| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId(card.to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                advancement_tokens: 2,
                rezzed,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Runner, &registry)
        };
        assert_eq!(under("offworld_office", false), under("urtica_cipher", false));
        assert_eq!(under("offworld_office", false), under("palisade", false));
        assert!(under("offworld_office", false) < evaluate_state(&GameState::new(0), Side::Runner, &registry), "and it counts");
        assert_ne!(under("urtica_cipher", true), under("palisade", true), "rezzed, the card is public and priced as itself");
    }

    /// The draw the person never saw: at the floor, a draw that brings a
    /// breaker for an uncovered subtype beats a credit, and a draw that
    /// brings a card the Runner would never install does not.
    #[test]
    fn drawing_a_breaker_for_an_uncovered_subtype_beats_a_credit_at_the_floor() {
        let mut dead = costed_breaker("dead", Some(IceType::Barrier), 3);
        dead.card_type = CardType::Resource;
        dead.abilities.clear();
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3), dead]);
        let holding = |drawn: Option<&str>| {
            let mut state = GameState::new(0);
            state.runner.resources.credits = Credits(if drawn.is_some() { 5 } else { 6 });
            state.runner.grip = corp_cards("filler", GRIP_FLOOR);
            if let Some(card) = drawn {
                state.runner.grip.push(CardId(card.to_string()));
            }
            evaluate_state(&state, Side::Runner, &registry)
        };
        let delta = BOARD_PRESENCE_WEIGHT + BREAKER_COVERAGE_WEIGHT - 3.0 * OWN_CREDIT_WEIGHT - MEMORY_WEIGHT;
        assert!((holding(Some("cleaver")) - holding(None) - (delta * HELD_CARD_WEIGHT - OWN_CREDIT_WEIGHT)).abs() < 1e-9);
        assert!(holding(Some("cleaver")) > holding(None), "the draw beats the credit");
        assert!(holding(Some("dead")) < holding(None), "a 3-cost card with no coverage is not worth drawing for");
        assert!(
            (holding(Some("dead")) - holding(None) + OWN_CREDIT_WEIGHT).abs() < 1e-9,
            "and is worth exactly nothing held — the term never goes negative"
        );
    }

    /// The reason the term is a fraction of the install rather than a
    /// value per card: the live card still goes on the table, and a
    /// second breaker for a covered subtype is dead in hand.
    #[test]
    fn a_live_card_is_still_installed_and_a_covered_breaker_is_dead_in_hand() {
        use netrunner_core::rules::MemoryUnits;
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3)]);
        let mut held = GameState::new(0);
        held.runner.resources.credits = Credits(3);
        held.runner.memory_units = MemoryUnits(4);
        held.runner.grip = corp_cards("filler", GRIP_FLOOR);
        held.runner.grip.push(CardId("cleaver".to_string()));
        let mut installed = held.clone();
        installed.runner.grip.pop();
        installed.runner.resources.credits = Credits(0);
        installed.runner.memory_units = MemoryUnits(3);
        installed.runner.rig = vec![rig_card("cleaver")];
        assert!(evaluate_state(&installed, Side::Runner, &registry) > evaluate_state(&held, Side::Runner, &registry));

        let mut duplicate = installed.clone();
        duplicate.runner.grip.push(CardId("cleaver".to_string()));
        let mut without = installed.clone();
        without.runner.grip.push(CardId("filler_9".to_string()));
        assert_eq!(
            evaluate_state(&duplicate, Side::Runner, &registry),
            evaluate_state(&without, Side::Runner, &registry),
            "Barrier is covered, so a second Cleaver is worth what an unknown card is"
        );
    }

    /// Precept 11's owed lever: an economy resource is live in hand at
    /// the guide's rate — worth drawing toward and installing — and dead
    /// at the reference, which priced it at its presence less its cost.
    #[test]
    fn an_economy_resource_is_live_in_hand_at_the_guides_rate_and_installed() {
        use netrunner_core::rules::MemoryUnits;
        let pool = pool();
        let w = guide();
        let rig = [false; 3];
        let telework = printed(&pool, "telework_contract");
        assert!(install_delta(&telework, rig, &w, horizon(Stage::Early)) > install_delta(&telework, rig, &Weights::default(), 9) + 1.0);
        let mut held = GameState::new(0);
        held.runner.resources.credits = Credits(5);
        held.runner.memory_units = MemoryUnits(4);
        held.runner.grip = corp_cards("filler", GRIP_FLOOR);
        held.runner.grip.push(CardId("telework_contract".to_string()));
        let mut installed = held.clone();
        installed.runner.grip.pop();
        installed.runner.resources.credits = Credits(4);
        installed.runner.rig = vec![InstalledRunnerCard { card: CardId("telework_contract".to_string()), counters: 9, ..Default::default() }];
        assert!(evaluate_state_with(&installed, Side::Runner, &pool, &w) > evaluate_state_with(&held, Side::Runner, &pool, &w), "installing it beats holding it");
        let mut clicked = held.clone();
        clicked.runner.resources.credits = Credits(6);
        assert!(evaluate_state_with(&installed, Side::Runner, &pool, &w) > evaluate_state_with(&clicked, Side::Runner, &pool, &w), "and beats a credit");
        // Taking the credits off it beats the credit click, as for the Corp.
        let mut took = installed.clone();
        took.runner.resources.credits = Credits(7);
        took.runner.rig[0].counters = 6;
        let mut clicked_instead = installed.clone();
        clicked_instead.runner.resources.credits = Credits(5);
        assert!(evaluate_state_with(&took, Side::Runner, &pool, &w) > evaluate_state_with(&clicked_instead, Side::Runner, &pool, &w));
    }

    /// Precept 12: an economy asset the Corp has rezzed is worth trashing
    /// early, when its turns pay the Corp more than the trash costs the
    /// Runner, and not late — the same yield the Corp reads, at the
    /// Runner's half weight.
    #[test]
    fn a_rezzed_economy_asset_is_worth_trashing_early_and_not_late() {
        use netrunner_core::rules::ServerId;
        let pool = pool();
        let w = guide();
        let pad = |stage_points: i32| {
            let mut state = GameState::new(0);
            state.runner.resources.credits = Credits(5);
            state.runner.resources.agenda_points = netrunner_core::rules::AgendaPoints(stage_points);
            state.corp.installed = vec![InstalledCard { card: CardId("pad_campaign".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), rezzed: true, ..Default::default() }];
            let run = RunState { server: ServerId::Remote(0), ..Default::default() };
            access_prospect(&state, &run, &pool, &w, 5, horizon(stage(&state)))
        };
        assert!(pad(0) > 0.0, "early: the run that trashes it is worth starting, {}", pad(0));
        assert_eq!(pad(6), 0.0, "late: 4[c] to trash two turns of income is not");
        let reference = Weights::default();
        let mut state = GameState::new(0);
        state.runner.resources.credits = Credits(5);
        state.corp.installed = vec![InstalledCard { card: CardId("pad_campaign".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), rezzed: true, ..Default::default() }];
        assert_eq!(access_prospect(&state, &RunState { server: ServerId::Remote(0), ..Default::default() }, &pool, &reference, 5, 9), 0.0, "the reference never paid 4[c] for it");
    }

    /// "An unadvanced card is only a threat if the Corp can afford to
    /// finish it next turn": a fresh remote install in front of a rich
    /// Corp is worth a run over a central, and in front of a broke one
    /// it is one hidden card like any other.
    #[test]
    fn a_face_down_remote_card_is_a_threat_only_while_the_corp_can_afford_to_finish_it() {
        use netrunner_core::rules::ServerId;
        let registry = CardRegistry::from_cards(vec![advanceable("plan", 3)]);
        let w = guide();
        let remote = |corp_credits: u32| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(corp_credits);
            state.corp.hq = corp_cards("hq", 4);
            state.corp.installed = vec![InstalledCard { card: CardId("plan".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), ..Default::default() }];
            let prospect = |server| access_prospect(&state, &RunState { server, ..Default::default() }, &registry, &w, 0, 9);
            (prospect(ServerId::Remote(0)), prospect(ServerId::Hq))
        };
        let (rich_remote, hq) = remote(3);
        assert!((rich_remote - (w.active_run_weight + w.finishable_install_weight)).abs() < 1e-9, "{rich_remote}");
        assert!(rich_remote > hq);
        let (broke_remote, _) = remote(2);
        assert!((broke_remote - w.active_run_weight).abs() < 1e-9, "{broke_remote}");
    }

    /// "Make the Corp rez": a run into unrezzed ICE is worth the credits
    /// the rez would cost the Corp, and nothing when the Corp has none to
    /// spend.
    #[test]
    fn a_run_into_unrezzed_ice_is_worth_the_rez_it_forces() {
        use netrunner_core::rules::ServerId;
        let w = guide();
        let run = |ice: Vec<RunIce>, corp_credits: u32| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(corp_credits);
            let run = RunState { server: ServerId::Hq, ice, position: 0, ..Default::default() };
            forced_rez_credits(&state, &run)
        };
        let unrezzed = || vec![run_ice(3, IceType::Barrier, 1, false), run_ice(3, IceType::Barrier, 1, false)];
        assert_eq!(run(unrezzed(), 10), 2 * TYPICAL_REZ_COST);
        assert_eq!(run(unrezzed(), 5), 5, "the second rez is what is left");
        assert_eq!(run(unrezzed(), 0), 0);
        assert_eq!(run(vec![run_ice(3, IceType::Barrier, 1, true)], 10), 0, "rezzed already");
        assert!(w.forced_rez_weight * f64::from(TYPICAL_REZ_COST) < w.active_run_weight, "a forced rez is worth less than the access itself");
    }
}
