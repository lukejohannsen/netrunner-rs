use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardType, Cost, Effect, IceType, SubroutineBreakCount, Trigger};
use netrunner_core::rules::{
    current_actor, GamePhase, GameState, InstalledCard, InstalledRunnerCard, RunIce, RunPhase, RunState, Side,
    SubroutineStatus,
};

const WIN_SCORE: f64 = 1000.0;
const AGENDA_POINT_WEIGHT: f64 = 20.0;
const OWN_CREDIT_WEIGHT: f64 = 0.4;
const OPPONENT_CREDIT_WEIGHT: f64 = 0.2;
const BAD_PUBLICITY_WEIGHT: f64 = 3.0;
const TAG_WEIGHT: f64 = 4.0;
const BOARD_PRESENCE_WEIGHT: f64 = 1.0;
const MEMORY_WEIGHT: f64 = 0.5;

/// A rezzed piece of ICE, on top of `BOARD_PRESENCE_WEIGHT`. Set so that
/// rezzing a mid-cost ICE during an approach (−`cost` × 0.4 credits,
/// +1.0 presence, +this) comes out ahead of passing: Palisade at 3 is
/// +0.8, a 6-cost bioroid is −0.4 and stays unrezzed until the Corp is
/// richer. Before this term a one-ply Corp rezzed only ICE costing ≤ 1.
const REZZED_ICE_WEIGHT: f64 = 1.0;
/// An unrezzed install. Installing was worth exactly nothing to a one-ply
/// evaluator (the rezzed count did not move), so `GainCreditClick`'s flat
/// +0.4 always won and the heuristic Corp never installed ICE — which is
/// why heuristic-vs-random play never once produced an `IceEncountered`
/// event (ROADMAP Rules Audit §0). Set above 0.4 so a card in hand is
/// worth putting on the table, below the rezzed value so rezzing it is
/// still progress.
const UNREZZED_INSTALL_WEIGHT: f64 = 0.6;
/// One advancement token on an installed card, counted only up to the
/// card's `advancement_requirement`. Advancing costs a click and a credit
/// (−0.4) against `GainCreditClick`'s +0.4, so a token must be worth more
/// than 0.8 to be preferred; scoring at `AGENDA_POINT_WEIGHT` per point
/// still dominates once the requirement is met. Tokens past the
/// requirement are worth nothing, so the Corp scores rather than piling
/// on.
const ADVANCEMENT_WEIGHT: f64 = 1.5;
/// Each ICE subtype (Barrier, Code Gate, Sentry) the Runner has an
/// installed breaker for, counted once per subtype. A breaker's value is
/// entirely in the runs it enables, which a one-ply evaluator cannot see;
/// this is the static proxy. Cleaver (3[c], 1 MU) installs at
/// −1.2 − 0.5 + 1.0 + 2.0 = +1.3 against a click's +0.4; a second Barrier
/// breaker adds no coverage and is not installed. Before this term only a
/// 0-cost program ever cleared the bar (ROADMAP Phase 2 §5).
const BREAKER_COVERAGE_WEIGHT: f64 = 2.0;
/// The Runner is mid-run *and can afford to break every rezzed ICE still
/// ahead of it* (`run_is_breakable`). `InitiateRun` costs a click and
/// changes nothing a static evaluator can see, so with no run term at all
/// a one-ply Runner never ran — in 96 heuristic-Runner games,
/// `RunInitiated` was 0 and 86 ended by the Corp decking itself. The term
/// was first unconditional, and +0.6 against `GainCreditClick`'s +0.4 on
/// every click meant the Runner ran into rezzed ICE it could not break
/// rather than saving for a breaker: 0.7 programs installed per game and
/// 75 subroutines broken against 1,025 fired across 96
/// heuristic-vs-heuristic games (ROADMAP Phase 2 §5). Gating on
/// *affordability* rather than on owning a matching breaker is deliberate
/// — Cleaver at 0 credits breaks nothing — and only rezzed ICE counts,
/// because an unrezzed card's identity in a determinized sample is a
/// guess the real Runner cannot see and the Corp may never rez. Jacking
/// out still forfeits the term, so a breakable run is worth finishing.
const ACTIVE_RUN_WEIGHT: f64 = 0.6;
/// Each still-pending subroutine on the ICE the Runner is encountering.
/// Breaking a subroutine has no visible effect on the board, so without
/// this a Runner with a rig of breakers would never pay to use one and
/// would let every subroutine fire. At 1.0 a break costing up to two
/// credits is worth taking, and a Runner facing three unbroken
/// subroutines with no breaker prefers jacking out (+3.0 − 0.6) to walking
/// into them.
const PENDING_SUBROUTINE_WEIGHT: f64 = 1.0;
/// A decision `side` still owes — a parked card selection, choice or paid
/// choice. Toggling a card in and out of a selection changes nothing a
/// static evaluator scores, so a one-ply agent random-walked
/// `ToggleCardSelection` until `MAX_STEPS` ran out (view sweep, seed 82,
/// a Runner selection during access). With the parked state itself
/// costing something, `ConfirmCardSelection` is progress the moment it is
/// legal, and the toggles leading up to it are a short walk, not a
/// wander.
const UNRESOLVED_DECISION_WEIGHT: f64 = 0.5;
/// Each point by which the encountered ICE's strength exceeds the best
/// matching breaker's. Pumping strength changes nothing else a static
/// evaluator sees, so a Runner holding Cleaver against a strength-4
/// Barrier never paid to pump and never got to break: once the free
/// break was deleted, heuristic-vs-heuristic broke 100 subroutines in 96
/// games and let 1,151 fire. At 0.9 a 2[c] pump (−0.8) closes one point
/// of shortfall and comes out ahead; a pump past the ICE's strength is
/// worth nothing.
const STRENGTH_SHORTFALL_WEIGHT: f64 = 0.9;

/// A rough static evaluation of `state` from `side`'s perspective: positive
/// favors `side`, negative favors the opponent. Shared by `HeuristicAgent`'s
/// one-ply scoring, `MctsAgent`'s rollout/leaf evaluation, the uniform
/// PUCT evaluator's value head, and the gym's shaped reward.
///
/// Reads `PlayerResources::agenda_points` directly rather than re-deriving
/// it from `scored_agendas`/`CardRegistry`: `engine::score_agenda` and
/// `run::access::resolve_steal` already keep that field in sync on every
/// point-scoring action. The registry is used for what only a card's text
/// can say — which installs are ICE, how far an agenda is from scoring,
/// which rig cards break which ICE.
pub fn evaluate_state(state: &GameState, side: Side, registry: &CardRegistry) -> f64 {
    if let GamePhase::GameOver(winner) = state.phase {
        return if winner == side { WIN_SCORE } else { -WIN_SCORE };
    }

    let own = state.resources(side);
    let opponent = state.resources(side.other());
    let mut score = (own.agenda_points.0 as f64 - opponent.agenda_points.0 as f64) * AGENDA_POINT_WEIGHT;
    if state.is_resolution_blocked() && current_actor(state) == Some(side) {
        score -= UNRESOLVED_DECISION_WEIGHT;
    }
    score += own.credits.0 as f64 * OWN_CREDIT_WEIGHT;
    score -= opponent.credits.0 as f64 * OPPONENT_CREDIT_WEIGHT;

    match side {
        Side::Corp => {
            score -= state.corp.bad_publicity as f64 * BAD_PUBLICITY_WEIGHT;
            for installed in &state.corp.installed {
                score += corp_install_value(installed, registry);
            }
        }
        Side::Runner => {
            score -= state.runner.tags as f64 * TAG_WEIGHT;
            score += state.runner.rig.len() as f64 * BOARD_PRESENCE_WEIGHT;
            score += state.runner.memory_units.0 as f64 * MEMORY_WEIGHT;
            score += breaker_coverage(state, registry) as f64 * BREAKER_COVERAGE_WEIGHT;
            if let Some(run) = &state.active_run {
                if run_is_breakable(state, run, registry) {
                    score += ACTIVE_RUN_WEIGHT;
                }
                score -= pending_subroutines(run) as f64 * PENDING_SUBROUTINE_WEIGHT;
                score -= strength_shortfall(state, run, registry) as f64 * STRENGTH_SHORTFALL_WEIGHT;
            }
        }
    }
    score
}

/// Whether the Runner can pay to break every pending subroutine on each
/// rezzed ICE it has not yet passed, out of its own credits plus the run's
/// bad-publicity pool. ICE before `run.position` is already behind the
/// Runner; unrezzed ICE is treated as passable (see `ACTIVE_RUN_WEIGHT`).
/// One rezzed ICE no rig card can break makes the whole run unbreakable —
/// a run that stops at the third ICE is worth no more than one that stops
/// at the first.
fn run_is_breakable(state: &GameState, run: &RunState, registry: &CardRegistry) -> bool {
    let mut total = 0;
    for ice in run.ice.iter().skip(run.position).filter(|ice| ice.rezzed) {
        let Some(cost) = cheapest_break_cost(state, ice, registry) else { return false };
        total += cost;
    }
    total <= state.runner.resources.credits.0 + run.bad_publicity_credits
}

/// The fewest credits any rig card needs to pump up to `ice`'s strength
/// and break all of its pending subroutines; `None` when no rig card can.
/// An ICE with nothing pending costs nothing whatever the rig holds.
fn cheapest_break_cost(state: &GameState, ice: &RunIce, registry: &CardRegistry) -> Option<u32> {
    if pending_on(ice) == 0 {
        return Some(0);
    }
    state.runner.rig.iter().filter_map(|card| break_cost(card, ice, registry)).min()
}

/// What `card` would spend to break `ice` outright: pump credits to close
/// any strength shortfall, then break credits for every pending
/// subroutine, both read off its `Paid` abilities. Only credit-costed
/// abilities are priced — Botulus's counter-costed
/// `BreakSubroutinesUnconditionally` is not a spend this term is about.
/// `None` if the card has no break matching `ice`'s subtype, or a
/// shortfall and no pump. `BoostStrengthAmount` (Unity's +X) is priced as
/// +1 per activation: X counts Unity itself so it is at least 1, and
/// over-estimating a cost only makes the Runner save one click longer.
fn break_cost(card: &InstalledRunnerCard, ice: &RunIce, registry: &CardRegistry) -> Option<u32> {
    let def = registry.get(&card.card)?;
    let pending = pending_on(ice);
    let shortfall = (ice.current_strength - card.effective_strength()).max(0) as u32;
    let mut cheapest_break: Option<u32> = None;
    let mut cheapest_pump: Option<u32> = None;
    let keep_min = |slot: &mut Option<u32>, cost: u32| *slot = Some(slot.map_or(cost, |c| c.min(cost)));
    for ability in def.abilities.iter().filter(|a| a.trigger == Trigger::Paid) {
        let credits = match ability.cost {
            None => 0,
            Some(Cost::Credits(c)) => c,
            Some(_) => continue,
        };
        ability.effect.for_each_effect(&mut |effect| match effect {
            Effect::BreakSubroutines { count, restrict_to } if restrict_to.is_none_or(|r| r == ice.ice_type) => {
                let activations = match count {
                    SubroutineBreakCount::Fixed(n) => pending.div_ceil((*n).max(1)),
                    SubroutineBreakCount::All => 1,
                };
                keep_min(&mut cheapest_break, credits * activations);
            }
            Effect::BoostStrength { amount, .. } => {
                keep_min(&mut cheapest_pump, credits * shortfall.div_ceil((*amount).max(1)));
            }
            Effect::BoostStrengthAmount { .. } => keep_min(&mut cheapest_pump, credits * shortfall),
            _ => {}
        });
    }
    let pump = if shortfall == 0 { 0 } else { cheapest_pump? };
    Some(cheapest_break? + pump)
}

/// Subroutines on `ice` still waiting to be broken or resolved.
fn pending_on(ice: &RunIce) -> u32 {
    ice.subroutines.iter().filter(|s| s.status == SubroutineStatus::Pending).count() as u32
}

/// How far the strongest rig breaker able to break the encountered ICE's
/// subtype falls short of its strength — zero when a breaker matches or
/// exceeds it, when no breaker matches at all (there is nothing to pump),
/// or outside an encounter.
fn strength_shortfall(state: &GameState, run: &RunState, registry: &CardRegistry) -> i32 {
    if run.phase != RunPhase::EncounterIce {
        return 0;
    }
    let Some(ice) = run.ice.get(run.position) else { return 0 };
    let best = state
        .runner
        .rig
        .iter()
        .filter(|card| breaks_subtype(card, ice.ice_type, registry))
        .map(|card| card.effective_strength())
        .max();
    best.map_or(0, |strength| (ice.current_strength - strength).max(0))
}

/// Whether `card`'s abilities include a `BreakSubroutines` that applies to
/// `subtype` — restricted to it, or unrestricted.
fn breaks_subtype(card: &netrunner_core::rules::InstalledRunnerCard, subtype: IceType, registry: &CardRegistry) -> bool {
    let Some(def) = registry.get(&card.card) else { return false };
    let mut found = false;
    for ability in &def.abilities {
        ability.effect.for_each_effect(&mut |effect| {
            if let Effect::BreakSubroutines { restrict_to, .. } = effect
                && restrict_to.is_none_or(|r| r == subtype)
            {
                found = true;
            }
        });
    }
    found
}

/// Subroutines on the currently encountered ICE that have neither been
/// broken nor resolved. Zero outside an encounter.
fn pending_subroutines(run: &RunState) -> usize {
    if run.phase != RunPhase::EncounterIce {
        return 0;
    }
    run.ice.get(run.position).map_or(0, |ice| pending_on(ice) as usize)
}

fn corp_install_value(installed: &InstalledCard, registry: &CardRegistry) -> f64 {
    let def = registry.get(&installed.card);
    let is_ice = def.is_some_and(|d| matches!(d.card_type, CardType::Ice(_)));
    let mut value = if installed.rezzed {
        BOARD_PRESENCE_WEIGHT + if is_ice { REZZED_ICE_WEIGHT } else { 0.0 }
    } else {
        UNREZZED_INSTALL_WEIGHT
    };
    if let Some(required) = def.and_then(|d| d.advancement_requirement) {
        value += installed.advancement_tokens.min(required) as f64 * ADVANCEMENT_WEIGHT;
    }
    value
}

/// How many of the three ICE subtypes the rig can break: a rig card whose
/// abilities contain `Effect::BreakSubroutines` covers its `restrict_to`
/// subtype, or all three when unrestricted (an AI breaker).
fn breaker_coverage(state: &GameState, registry: &CardRegistry) -> usize {
    // Indexed Barrier, Code Gate, Sentry — `IceType` is not `Hash`, and
    // three flags say it more plainly than a set would anyway.
    let mut covered = [false; 3];
    let slot = |subtype: IceType| match subtype {
        IceType::Barrier => 0,
        IceType::CodeGate => 1,
        IceType::Sentry => 2,
    };
    for card in &state.runner.rig {
        let Some(def) = registry.get(&card.card) else { continue };
        for ability in &def.abilities {
            ability.effect.for_each_effect(&mut |effect| {
                if let Effect::BreakSubroutines { restrict_to, .. } = effect {
                    match restrict_to {
                        Some(subtype) => covered[slot(*subtype)] = true,
                        None => covered = [true; 3],
                    }
                }
            });
        }
    }
    covered.iter().filter(|c| **c).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{AbilityDef, CardDefinition, CardId, SubroutineBreakCount, Trigger};
    use netrunner_core::rules::{AgendaPoints, Credits, GameState, InstallId, InstalledRunnerCard};

    fn empty() -> CardRegistry {
        CardRegistry::new()
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

    fn ice(id: &str, cost: u32) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::Barrier),
            cost,
            is_playable: true,
            ..Default::default()
        }
    }

    fn breaker(id: &str, restrict_to: Option<IceType>) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Runner,
            card_type: CardType::Program,
            abilities: vec![AbilityDef {
                trigger: Trigger::Paid,
                cost: None,
                requirement: None,
                effect: Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to },
                cost_discount_if: None,
            }],
            is_playable: true,
            ..Default::default()
        }
    }

    fn rig_card(id: &str) -> InstalledRunnerCard {
        InstalledRunnerCard { card: CardId(id.to_string()), ..Default::default() }
    }

    /// The whole point of the breaker term: a first breaker for a subtype
    /// is worth installing over a credit; a second for the same subtype
    /// adds nothing.
    #[test]
    fn breaker_coverage_counts_each_ice_subtype_once() {
        let registry = CardRegistry::from_cards(vec![
            breaker("cleaver", Some(IceType::Barrier)),
            breaker("corroder", Some(IceType::Barrier)),
            breaker("carmen", Some(IceType::Sentry)),
            breaker("mayfly", None),
        ]);
        let mut state = GameState::new(0);
        assert_eq!(breaker_coverage(&state, &registry), 0);
        state.runner.rig = vec![rig_card("cleaver")];
        assert_eq!(breaker_coverage(&state, &registry), 1);
        state.runner.rig.push(rig_card("corroder"));
        assert_eq!(breaker_coverage(&state, &registry), 1, "a second Barrier breaker covers nothing new");
        state.runner.rig.push(rig_card("carmen"));
        assert_eq!(breaker_coverage(&state, &registry), 2);
        state.runner.rig.push(rig_card("mayfly"));
        assert_eq!(breaker_coverage(&state, &registry), 3, "an AI breaker covers everything");
    }

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

    #[test]
    fn advancement_is_valued_up_to_the_requirement_and_no_further() {
        let mut agenda = ice("offworld_office", 0);
        agenda.card_type = CardType::Agenda;
        agenda.advancement_requirement = Some(3);
        let registry = CardRegistry::from_cards(vec![agenda]);
        let at = |tokens| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId("offworld_office".to_string()),
                install_id: InstallId(1),
                advancement_tokens: tokens,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        assert!(at(1) > at(0));
        assert!(at(3) > at(2));
        assert_eq!(at(4), at(3), "tokens past the requirement are worth nothing — score instead");
    }

    /// Rezzing a mid-cost ICE at approach must beat passing, and installing
    /// must beat clicking for a credit; that is what makes heuristic play
    /// reach an encounter at all.
    #[test]
    fn rezzing_a_three_cost_ice_beats_passing_and_installing_beats_a_credit() {
        let registry = CardRegistry::from_cards(vec![ice("palisade", 3)]);
        let installed = |rezzed, credits| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state.corp.installed = vec![InstalledCard {
                card: CardId("palisade".to_string()),
                install_id: InstallId(1),
                rezzed,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        assert!(installed(true, 2) > installed(false, 5), "paying 3 to rez Palisade is progress");
        let mut in_hand = GameState::new(0);
        in_hand.corp.resources.credits = Credits(6);
        in_hand.corp.hq = vec![CardId("palisade".to_string())];
        assert!(installed(false, 5) > evaluate_state(&in_hand, Side::Corp, &registry), "installing beats a credit");
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
        running.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
        assert!(evaluate_state(&running, Side::Runner, &registry) > evaluate_state(&clicked, Side::Runner, &registry));

        let sub = |id| EncounteredSubroutine {
            id,
            definition: netrunner_core::dsl::SubroutineDef { text: String::new(), effect: Effect::EndTheRun },
            status: SubroutineStatus::Pending,
        };
        let mut encountering = running.clone();
        encountering.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            position: 0,
            ice: vec![RunIce {
                install_id: netrunner_core::rules::InstallId::PLACEHOLDER,
                card_id: CardId("ice_wall".to_string()),
                current_strength: 1,
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

    /// Pumping a matching breaker up to the ICE's strength is worth its
    /// credits; a breaker for the wrong subtype leaves nothing to pump.
    #[test]
    fn a_strength_shortfall_against_a_matching_breaker_is_worth_pumping() {
        use netrunner_core::rules::{RunIce, ServerId};
        let registry = CardRegistry::from_cards(vec![breaker("cleaver", Some(IceType::Barrier))]);
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
                    current_strength: 4,
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

    /// A breaker priced like a real one: `break_cost` per activation
    /// breaking `break_count` subroutines, `pump_cost` per `pump_amount`
    /// strength — Cleaver is `(1, 2), (2, 1)`.
    fn priced_breaker(
        id: &str,
        restrict_to: Option<IceType>,
        (break_cost, break_count): (u32, u32),
        (pump_cost, pump_amount): (u32, u32),
    ) -> CardDefinition {
        use netrunner_core::dsl::BoostDuration;
        let mut def = breaker(id, restrict_to);
        def.abilities[0].cost = Some(Cost::Credits(break_cost));
        def.abilities[0].effect = Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(break_count), restrict_to };
        def.abilities.push(AbilityDef {
            trigger: Trigger::Paid,
            cost: Some(Cost::Credits(pump_cost)),
            requirement: None,
            effect: Effect::BoostStrength { amount: pump_amount, duration: BoostDuration::Encounter },
            cost_discount_if: None,
        });
        def
    }

    /// One piece of ICE on a run, all subroutines pending.
    fn run_ice(strength: i32, ice_type: IceType, subroutines: usize, rezzed: bool) -> RunIce {
        use netrunner_core::rules::{EncounteredSubroutine, InstallId};
        RunIce {
            install_id: InstallId::PLACEHOLDER,
            card_id: CardId("ice".to_string()),
            current_strength: strength,
            ice_type,
            subroutines: (0..subroutines)
                .map(|id| EncounteredSubroutine {
                    id,
                    definition: netrunner_core::dsl::SubroutineDef { text: String::new(), effect: Effect::EndTheRun },
                    status: SubroutineStatus::Pending,
                })
                .collect(),
            rezzed,
        }
    }

    /// `evaluate_state` for the Runner, `credits` in hand, approaching the
    /// outermost ICE of a run over `ice` — minus the same board with no run,
    /// so the result is exactly what the run term contributed.
    fn run_term(rig: Vec<InstalledRunnerCard>, credits: u32, ice: Vec<RunIce>, position: usize, registry: &CardRegistry) -> f64 {
        use netrunner_core::rules::ServerId;
        let mut idle = GameState::new(0);
        idle.runner.resources.credits = Credits(credits);
        idle.runner.rig = rig;
        let mut running = idle.clone();
        running.active_run = Some(RunState { server: ServerId::Hq, ice, position, ..Default::default() });
        let term = evaluate_state(&running, Side::Runner, registry) - evaluate_state(&idle, Side::Runner, registry);
        (term * 1000.0).round() / 1000.0 // the other terms cancel, up to float noise
    }

    /// The whole point of the conditional run term: with no breaker for a
    /// rezzed ICE, a run is worth nothing (so a credit click wins); with a
    /// breaker and the credits to use it, the run is worth taking.
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
    /// count — they are spendable on exactly this).
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

        let mut idle = GameState::new(0);
        idle.runner.resources.credits = Credits(2);
        idle.runner.rig = cleaver();
        let mut running = idle.clone();
        running.active_run =
            Some(RunState { server: ServerId::Hq, ice: ice(), bad_publicity_credits: 1, ..Default::default() });
        let term = evaluate_state(&running, Side::Runner, &registry) - evaluate_state(&idle, Side::Runner, &registry);
        assert_eq!((term * 1000.0).round() / 1000.0, ACTIVE_RUN_WEIGHT, "a bad-publicity credit closes the gap");
    }

    /// An unrezzed ICE's identity in a determinized sample is a guess the
    /// real Runner cannot see, so it never blocks the run term; nor does
    /// ICE the run has already passed.
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

    /// An unrestricted (AI) breaker prices a run over any subtype; a
    /// breaker short on strength with no pump cannot break at any price.
    #[test]
    fn an_ai_breaker_covers_any_subtype_and_a_pumpless_shortfall_is_unbreakable() {
        let registry = CardRegistry::from_cards(vec![breaker("mayfly", None)]);
        let sentry = || vec![run_ice(1, IceType::Sentry, 2, true)];
        let mayfly = |strength| vec![InstalledRunnerCard { base_strength: strength, ..rig_card("mayfly") }];
        assert_eq!(run_term(mayfly(1), 0, sentry(), 0, &registry), ACTIVE_RUN_WEIGHT, "a free AI break costs nothing");
        assert_eq!(run_term(mayfly(0), 9, sentry(), 0, &registry), 0.0, "one point short and no pump ability");
    }
}
