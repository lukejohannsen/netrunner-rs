use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardDefinition, CardType, Cost, Effect, IceType, SubroutineBreakCount, Trigger};
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
/// +1.0 presence, +this, −`UNREZZED_INSTALL_WEIGHT`) comes out ahead of
/// passing: the rez delta is 1.4 − 0.4 × cost, so Palisade at 3 is +0.2
/// and a 6-cost bioroid is −1.0 and stays unrezzed until the Corp is
/// richer. Before this term a one-ply Corp rezzed only ICE costing ≤ 1.
/// Moved 1.0 → 1.4 in step with `UNREZZED_INSTALL_WEIGHT` 0.6 → 1.0 so
/// that delta did not change.
const REZZED_ICE_WEIGHT: f64 = 1.4;
/// A rezzed asset or upgrade, on top of `BOARD_PRESENCE_WEIGHT`. Without
/// it a rezzed non-ICE card was worth `BOARD_PRESENCE_WEIGHT −
/// UNREZZED_INSTALL_WEIGHT` = +0.4 minus its rez cost × 0.4, so nothing
/// costing 2 or more was ever rezzed: Nico Campaign and Manegarm Skunkworks
/// were installed 33 and 34 times in 96 heuristic-vs-heuristic games and
/// rezzed never. Rezzing is now 1.0 − 0.4 × cost — +0.2 for a 2-cost
/// asset — and anything above zero gets rezzed eventually, because a rez
/// costs no click and competes with `EndTurn` at 0 once the clicks are
/// spent.
const REZZED_ASSET_WEIGHT: f64 = 1.0;
/// An unrezzed install. Installing was worth exactly nothing to a one-ply
/// evaluator (the rezzed count did not move), so `GainCreditClick`'s flat
/// +0.4 always won and the heuristic Corp never installed ICE — which is
/// why heuristic-vs-random play never once produced an `IceEncountered`
/// event (ROADMAP Rules Audit §0). Raised 0.6 → 1.0 for `HQ_FLOOR`: a
/// draw below the floor has to beat a credit (`HQ_SHORTFALL_WEIGHT` >
/// 0.4), and installing *from* the floor then loses that same weight, so
/// at 0.6 the Corp would have stalled at the floor clicking for credits —
/// the rhythm it already had. At 1.0 an install from the floor is +0.5
/// and still wins. A welcome side effect: a second ICE on a server (1[c]
/// to install) is now +0.6 and happens, where it was +0.2 and never did;
/// a third at +0.2 still does not.
const UNREZZED_INSTALL_WEIGHT: f64 = 1.0;
/// One advancement token on an installed card, counted only up to the
/// card's `advancement_requirement`. Advancing costs a click and a credit
/// (−0.4) against `GainCreditClick`'s +0.4, so a token must be worth more
/// than 0.8 to be preferred; scoring at `AGENDA_POINT_WEIGHT` per point
/// still dominates once the requirement is met. Tokens past the
/// requirement are worth nothing, so the Corp scores rather than piling
/// on.
const ADVANCEMENT_WEIGHT: f64 = 1.5;
/// Each piece of ICE, up to two, protecting the server an installed
/// agenda sits in. Every install was worth the same flat
/// `UNREZZED_INSTALL_WEIGHT` wherever it went, so the one-ply Corp put an
/// agenda in a fresh naked remote as readily as behind ICE, and never
/// preferred to ICE the remote it was scoring out of. Read on the agenda,
/// not the ICE, so it does both jobs: installing an agenda into a
/// one-ICE remote is +0.5 over a naked one, and installing ICE in front of
/// an installed agenda is +0.5 on top of the install — enough for a
/// second ICE there (1[c]) to beat one elsewhere. Capped at two so the
/// Corp does not stack a fourth ICE on one remote instead of scoring.
const AGENDA_PROTECTION_WEIGHT: f64 = 0.5;
/// ICE beyond this many on an agenda's server earns nothing more.
const AGENDA_PROTECTION_CAP: usize = 2;
/// Each ICE subtype (Barrier, Code Gate, Sentry) the Runner has an
/// installed breaker for, counted once per subtype. A breaker's value is
/// entirely in the runs it enables, which a one-ply evaluator cannot see;
/// this is the static proxy. Cleaver (3[c], 1 MU) installs at
/// −1.2 − 0.5 + 1.0 + 3.0 = +2.3 against a click's +0.4; a second Barrier
/// breaker adds no coverage and is not installed. Before this term only a
/// 0-cost program ever cleared the bar (ROADMAP Phase 2 §5). Raised 2.0 →
/// 3.0 for Carmen (5[c]): at 2.0 her install was +0.5, under an open
/// run's +0.6, and from an at-floor grip (−`GRIP_SHORTFALL_WEIGHT`) it was
/// −0.2 — she was never installed in any heuristic seating. At 3.0 she is
/// +1.5, or +0.8 from the floor, and every breaker in the pool clears a
/// run from anywhere.
const BREAKER_COVERAGE_WEIGHT: f64 = 3.0;
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
/// wander. **It must outweigh whatever confirming gives up**: at 0.5 it
/// exactly cancelled the `HQ_SHORTFALL_WEIGHT` of the card a Corp
/// selection from an at-floor HQ hands over, confirm tied with a toggle,
/// and the tie-break jitter wandered for 10,000 steps (heuristic Corp vs
/// random Runner, seed 77, Discretion Advised vs Planning Ahead). Set
/// well above the largest per-card term on either side (`GRIP_SHORTFALL_
/// WEIGHT` 0.7, `HQ_SHORTFALL_WEIGHT` 0.5, a rig card's 1.0 presence).
const UNRESOLVED_DECISION_WEIGHT: f64 = 2.0;
/// Each point by which the encountered ICE's strength exceeds the best
/// matching breaker's. Pumping strength changes nothing else a static
/// evaluator sees, so a Runner holding Cleaver against a strength-4
/// Barrier never paid to pump and never got to break: once the free
/// break was deleted, heuristic-vs-heuristic broke 100 subroutines in 96
/// games and let 1,151 fire. At 0.9 a 2[c] pump (−0.8) closes one point
/// of shortfall and comes out ahead; a pump past the ICE's strength is
/// worth nothing.
const STRENGTH_SHORTFALL_WEIGHT: f64 = 0.9;
/// Each credit the Runner is short of the cheapest breaker in grip that
/// would cover an ICE subtype the rig does not (`breaker_savings_shortfall`).
/// A one-ply evaluator cannot see the install it is saving for, and once
/// the run term stopped paying for runs into unbreakable ICE the Runner
/// simply ran open servers instead (+0.6 beats a credit's +0.4 every
/// click): `ProgramInstalled` stayed at 66 across 96 heuristic-vs-heuristic
/// games. This is a penalty on the *shortfall*, not a bonus on credits
/// held, and the difference is the whole design: a bonus would vanish the
/// moment the breaker is installed and fight the install it exists to
/// enable (Cleaver's +1.3 would lose 3 × the weight). The shortfall is zero
/// once the card is affordable, so installing keeps its full margin, while
/// every click that closes the gap is worth 0.4 + 0.3 = +0.7 — ahead of an
/// open-server run — and every credit spent elsewhere while short costs
/// 0.3 more. (Carmen's install is +0.5, already below a run at +0.6; a
/// pre-existing weight interaction this term does not touch.)
const SAVINGS_SHORTFALL_WEIGHT: f64 = 0.3;
/// Each card the Runner's grip is below `GRIP_FLOOR`. A card in hand was
/// worth nothing to the evaluator, so `DrawCardClick` never beat anything
/// and the heuristic Runner clicked it once in 96 games: its grip was the
/// opening hand plus event draws, which is what capped program installs
/// at 66 per 96 games after the run and savings terms — a breaker in the
/// opening hand was installed on turn one, and no other breaker was ever
/// drawn (ROADMAP Phase 2 §5). Shaped as a shortfall below a floor rather
/// than a value per card so it does not tax installs from a healthy hand:
/// below the floor a draw is +0.7, ahead of an open-server run's +0.6 and
/// a credit's +0.4, and installing from a 3-card grip loses 0.7 of
/// Cleaver's +1.3 — so the Runner draws one card first, then installs at
/// full margin; at or above the floor the term is flat and runs resume.
/// The floor is below the hand limit so a draw never has to be discarded.
/// Runner-only: the Corp's mandatory draw hides the same blindness, and
/// what a heuristic Corp should hold is a different question.
const GRIP_SHORTFALL_WEIGHT: f64 = 0.7;
/// Cards in grip below which `GRIP_SHORTFALL_WEIGHT` applies.
const GRIP_FLOOR: usize = 3;
/// The Corp's `GRIP_SHORTFALL_WEIGHT`: each card HQ is below `HQ_FLOOR`.
/// The heuristic Corp never clicked to draw (0 in every heuristic
/// seating) and deployed about one card a turn — the mandatory draw —
/// spending the rest of its clicks on credits. Below the floor a draw is
/// +0.5 against a credit's +0.4; an install from the floor is
/// `UNREZZED_INSTALL_WEIGHT` − 0.5 = +0.5 (see that constant for why it
/// had to rise first), so the Corp alternates draw and install rather
/// than stalling; at the floor a draw is worth 0 and the clicks go to
/// installs and advancement.
const HQ_SHORTFALL_WEIGHT: f64 = 0.5;
/// Cards in HQ below which `HQ_SHORTFALL_WEIGHT` applies. Two, not the
/// Runner's three: the mandatory draw already feeds HQ every turn, and
/// every card held there is one more an HQ run can steal.
const HQ_FLOOR: usize = 3;
/// R&D size below which the HQ term switches off. A Corp that draws its
/// deck out loses at the next mandatory draw, and a one-ply evaluator
/// sees that only one action too late. R&D's size is public, so the brake
/// reads nothing the Corp's `ClientView` does not show.
const RD_DRAW_RESERVE: usize = 5;

/// Every tunable term of `evaluate_state`, as one value. `Default` is the
/// constants above, so `evaluate_state` is `evaluate_state_with(..,
/// &Weights::default())` and every existing caller scores exactly as it
/// did; a `Personality` is a `Weights` that differs from the default in a
/// few documented places. The constants keep the reasoning — each one's
/// doc comment records the measurement that set it — and the struct
/// carries the numbers, so a profile can move one without restating the
/// rest. Two terms exist only for profiles and default to zero, so the
/// balanced evaluator is unchanged by their existence:
/// `opponent_grip_weight` (a Corp that wants the Runner's hand thin) and
/// `installed_agenda_weight` (a Corp that wants the agenda on the table
/// before the ICE in front of it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    pub agenda_point_weight: f64,
    pub own_credit_weight: f64,
    pub opponent_credit_weight: f64,
    pub bad_publicity_weight: f64,
    pub tag_weight: f64,
    pub board_presence_weight: f64,
    pub memory_weight: f64,
    pub rezzed_ice_weight: f64,
    pub rezzed_asset_weight: f64,
    pub unrezzed_install_weight: f64,
    pub advancement_weight: f64,
    pub agenda_protection_weight: f64,
    pub agenda_protection_cap: usize,
    pub breaker_coverage_weight: f64,
    pub active_run_weight: f64,
    pub pending_subroutine_weight: f64,
    pub unresolved_decision_weight: f64,
    pub strength_shortfall_weight: f64,
    pub savings_shortfall_weight: f64,
    pub grip_shortfall_weight: f64,
    pub grip_floor: usize,
    pub hq_shortfall_weight: f64,
    pub hq_floor: usize,
    pub rd_draw_reserve: usize,
    /// Corp only: each card in the Runner's grip, subtracted. Zero by
    /// default — the balanced Corp does not play for the flatline — and
    /// positive for a `Personality::Trap`, for whom every point of net
    /// damage dealt is worth this much and an empty grip is a kill.
    pub opponent_grip_weight: f64,
    /// Corp only: each installed, unscored agenda, added. Zero by default
    /// — every unrezzed install is worth the same flat
    /// `unrezzed_install_weight`, which is why the balanced Corp has no
    /// preference between putting down the agenda and the ICE — and
    /// positive for a `Personality::Rush`, for whom the agenda comes
    /// first and the ICE if there is time. Measured before it existed:
    /// a rush profile built from the other terms alone advanced *less*
    /// than balanced (1,045 → 1,032 `CardAdvanced` in 96 games), because
    /// the one-ply Corp already advances every agenda it has installed
    /// and the profile had no way to say "install it".
    pub installed_agenda_weight: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            agenda_point_weight: AGENDA_POINT_WEIGHT,
            own_credit_weight: OWN_CREDIT_WEIGHT,
            opponent_credit_weight: OPPONENT_CREDIT_WEIGHT,
            bad_publicity_weight: BAD_PUBLICITY_WEIGHT,
            tag_weight: TAG_WEIGHT,
            board_presence_weight: BOARD_PRESENCE_WEIGHT,
            memory_weight: MEMORY_WEIGHT,
            rezzed_ice_weight: REZZED_ICE_WEIGHT,
            rezzed_asset_weight: REZZED_ASSET_WEIGHT,
            unrezzed_install_weight: UNREZZED_INSTALL_WEIGHT,
            advancement_weight: ADVANCEMENT_WEIGHT,
            agenda_protection_weight: AGENDA_PROTECTION_WEIGHT,
            agenda_protection_cap: AGENDA_PROTECTION_CAP,
            breaker_coverage_weight: BREAKER_COVERAGE_WEIGHT,
            active_run_weight: ACTIVE_RUN_WEIGHT,
            pending_subroutine_weight: PENDING_SUBROUTINE_WEIGHT,
            unresolved_decision_weight: UNRESOLVED_DECISION_WEIGHT,
            strength_shortfall_weight: STRENGTH_SHORTFALL_WEIGHT,
            savings_shortfall_weight: SAVINGS_SHORTFALL_WEIGHT,
            grip_shortfall_weight: GRIP_SHORTFALL_WEIGHT,
            grip_floor: GRIP_FLOOR,
            hq_shortfall_weight: HQ_SHORTFALL_WEIGHT,
            hq_floor: HQ_FLOOR,
            rd_draw_reserve: RD_DRAW_RESERVE,
            opponent_grip_weight: 0.0,
            installed_agenda_weight: 0.0,
        }
    }
}

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
    evaluate_state_with(state, side, registry, &Weights::default())
}

/// `evaluate_state` under a particular `Weights` — what a `Personality`
/// gives the heuristic, MCTS and the uniform PUCT evaluator.
pub fn evaluate_state_with(state: &GameState, side: Side, registry: &CardRegistry, w: &Weights) -> f64 {
    if let GamePhase::GameOver(winner) = state.phase {
        return if winner == side { WIN_SCORE } else { -WIN_SCORE };
    }

    let own = state.resources(side);
    let opponent = state.resources(side.other());
    let mut score = (own.agenda_points.0 as f64 - opponent.agenda_points.0 as f64) * w.agenda_point_weight;
    if state.is_resolution_blocked() && current_actor(state) == Some(side) {
        score -= w.unresolved_decision_weight;
    }
    score += own.credits.0 as f64 * w.own_credit_weight;
    score -= opponent.credits.0 as f64 * w.opponent_credit_weight;

    match side {
        Side::Corp => {
            score -= state.corp.bad_publicity as f64 * w.bad_publicity_weight;
            for installed in &state.corp.installed {
                score += corp_install_value(installed, registry, w);
            }
            score += protected_agenda_ice(state, registry, w.agenda_protection_cap) as f64 * w.agenda_protection_weight;
            if w.installed_agenda_weight != 0.0 {
                score += installed_agendas(state, registry) as f64 * w.installed_agenda_weight;
            }
            if state.corp.r_and_d.len() >= w.rd_draw_reserve {
                score -= w.hq_floor.saturating_sub(state.corp.hq.len()) as f64 * w.hq_shortfall_weight;
            }
            score -= state.runner.grip.len() as f64 * w.opponent_grip_weight;
        }
        Side::Runner => {
            score -= state.runner.tags as f64 * w.tag_weight;
            score += state.runner.rig.len() as f64 * w.board_presence_weight;
            score += state.runner.memory_units.0 as f64 * w.memory_weight;
            score += breaker_coverage(state, registry) as f64 * w.breaker_coverage_weight;
            score -= breaker_savings_shortfall(state, registry) as f64 * w.savings_shortfall_weight;
            score -= w.grip_floor.saturating_sub(state.runner.grip.len()) as f64 * w.grip_shortfall_weight;
            if let Some(run) = &state.active_run {
                if run_is_breakable(state, run, registry) {
                    score += w.active_run_weight;
                }
                score -= pending_subroutines(run) as f64 * w.pending_subroutine_weight;
                score -= strength_shortfall(state, run, registry) as f64 * w.strength_shortfall_weight;
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

fn corp_install_value(installed: &InstalledCard, registry: &CardRegistry, w: &Weights) -> f64 {
    let def = registry.get(&installed.card);
    let is_ice = def.is_some_and(|d| matches!(d.card_type, CardType::Ice(_)));
    let mut value = if installed.rezzed {
        w.board_presence_weight + if is_ice { w.rezzed_ice_weight } else { w.rezzed_asset_weight }
    } else {
        w.unrezzed_install_weight
    };
    if let Some(required) = def.and_then(|d| d.advancement_requirement) {
        value += installed.advancement_tokens.min(required) as f64 * w.advancement_weight;
    }
    value
}

/// Installed, unscored agendas — what `installed_agenda_weight` counts.
fn installed_agendas(state: &GameState, registry: &CardRegistry) -> usize {
    use netrunner_core::rules::InstallSlot;
    state
        .corp
        .installed
        .iter()
        .filter(|card| card.slot == InstallSlot::Root)
        .filter(|card| registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Agenda))
        .count()
}

/// ICE in front of each installed, unscored agenda, each server's count
/// capped at `cap` (`Weights::agenda_protection_cap`), summed over agendas.
fn protected_agenda_ice(state: &GameState, registry: &CardRegistry, cap: usize) -> usize {
    use netrunner_core::rules::InstallSlot;
    state
        .corp
        .installed
        .iter()
        .filter(|card| card.slot == InstallSlot::Root)
        .filter(|card| registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Agenda))
        .map(|agenda| {
            state
                .corp
                .installed
                .iter()
                .filter(|ice| ice.slot == InstallSlot::Ice && ice.server == agenda.server)
                .count()
                .min(cap)
        })
        .sum()
}

/// How many of the three ICE subtypes the rig can break: a rig card whose
/// abilities contain `Effect::BreakSubroutines` covers its `restrict_to`
/// subtype, or all three when unrestricted (an AI breaker).
fn breaker_coverage(state: &GameState, registry: &CardRegistry) -> usize {
    rig_coverage(state, registry).iter().filter(|c| **c).count()
}

/// The subtypes the rig can break, as `covers` flags OR-ed over every rig
/// card.
fn rig_coverage(state: &GameState, registry: &CardRegistry) -> [bool; 3] {
    let mut covered = [false; 3];
    for card in &state.runner.rig {
        let Some(def) = registry.get(&card.card) else { continue };
        for (slot, flag) in covers(def).into_iter().enumerate() {
            covered[slot] |= flag;
        }
    }
    covered
}

/// Which ICE subtypes `def` can break — indexed Barrier, Code Gate,
/// Sentry. `IceType` is not `Hash`, and three flags say it more plainly
/// than a set would anyway.
fn covers(def: &CardDefinition) -> [bool; 3] {
    let mut covered = [false; 3];
    let slot = |subtype: IceType| match subtype {
        IceType::Barrier => 0,
        IceType::CodeGate => 1,
        IceType::Sentry => 2,
    };
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
    covered
}

/// Credits the Runner is short of the cheapest grip breaker worth
/// installing: one that covers a subtype the rig cannot break and fits in
/// free memory. Zero with no such card, or once it is affordable. Printed
/// cost, ignoring install discounts — over-estimating the target only
/// makes the Runner save one click longer.
fn breaker_savings_shortfall(state: &GameState, registry: &CardRegistry) -> u32 {
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

    /// A breaker with a printed install cost and memory cost, for the
    /// savings term.
    fn costed_breaker(id: &str, restrict_to: Option<IceType>, cost: u32) -> CardDefinition {
        CardDefinition { cost, memory_cost: Some(1), ..breaker(id, restrict_to) }
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

    /// Why the weight sits above the run term: with a thin grip a draw
    /// beats an open-server run; with the floor met, the run wins again.
    #[test]
    fn drawing_beats_an_open_run_below_the_grip_floor_and_not_above_it() {
        use netrunner_core::rules::ServerId;
        let registry = CardRegistry::new();
        let holding = |cards: usize, running: bool| {
            let mut state = GameState::new(0);
            state.runner.grip = (0..cards).map(|i| CardId(format!("card_{i}"))).collect();
            if running {
                state.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
            }
            evaluate_state(&state, Side::Runner, &registry)
        };
        assert!(holding(GRIP_FLOOR, false) > holding(GRIP_FLOOR - 1, true), "draw up to the floor first");
        assert!(holding(GRIP_FLOOR, true) > holding(GRIP_FLOOR + 1, false), "then run rather than keep drawing");
    }

    fn asset(id: &str, cost: u32) -> CardDefinition {
        CardDefinition { card_type: CardType::Asset, ..ice(id, cost) }
    }

    fn corp_cards(prefix: &str, n: usize) -> Vec<CardId> {
        (0..n).map(|i| CardId(format!("{prefix}_{i}"))).collect()
    }

    /// The whole point of `REZZED_ASSET_WEIGHT`: a 2-cost asset is worth
    /// its rez.
    #[test]
    fn a_two_cost_asset_is_worth_rezzing() {
        let registry = CardRegistry::from_cards(vec![asset("nico_campaign", 2)]);
        let nico = |rezzed, credits| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state.corp.installed = vec![InstalledCard {
                card: CardId("nico_campaign".to_string()),
                install_id: InstallId(1),
                rezzed,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        assert!(nico(true, 3) > nico(false, 5), "paying 2 to rez Nico Campaign is progress");
    }

    /// `UNREZZED_INSTALL_WEIGHT`'s side effect: paying the 1[c] to put a
    /// second ICE on a server beats a credit click, from an HQ above the
    /// floor (at the floor the card's `HQ_SHORTFALL_WEIGHT` tips it back).
    #[test]
    fn a_second_ice_on_a_server_beats_a_credit_click() {
        let registry = CardRegistry::from_cards(vec![ice("palisade", 3)]);
        let board = |installed_count, credits, hq| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state.corp.r_and_d = corp_cards("rd", RD_DRAW_RESERVE);
            state.corp.hq = corp_cards("palisade", hq);
            state.corp.installed = (0..installed_count)
                .map(|i| InstalledCard { card: CardId("palisade".to_string()), install_id: InstallId(i), ..Default::default() })
                .collect();
            evaluate_state(&state, Side::Corp, &registry)
        };
        // Second ICE: paid 1 to install it from an above-floor HQ, versus clicking for a credit.
        assert!(board(2, 4, HQ_FLOOR) > board(1, 6, HQ_FLOOR + 1));
    }

    /// The HQ term is a shortfall below a floor, and a thin R&D switches
    /// it off so the Corp does not draw itself to a deck-out.
    #[test]
    fn an_hq_below_the_floor_costs_something_and_a_thin_r_and_d_switches_it_off() {
        let registry = CardRegistry::new();
        let holding = |hq, rd| {
            let mut state = GameState::new(0);
            state.corp.hq = corp_cards("hq", hq);
            state.corp.r_and_d = corp_cards("rd", rd);
            evaluate_state(&state, Side::Corp, &registry)
        };
        let stocked = RD_DRAW_RESERVE + 10;
        assert!((holding(1, stocked) - holding(0, stocked) - HQ_SHORTFALL_WEIGHT).abs() < 1e-9);
        assert_eq!(holding(HQ_FLOOR + 1, stocked), holding(HQ_FLOOR, stocked), "cards past the floor are worth nothing");
        assert_eq!(holding(0, RD_DRAW_RESERVE - 1), holding(HQ_FLOOR, RD_DRAW_RESERVE - 1), "no draw pressure on a thin R&D");
    }

    /// Below the floor the Corp draws rather than clicking for a credit;
    /// at the floor it still installs rather than stalling — the trap the
    /// `UNREZZED_INSTALL_WEIGHT` bump exists to avoid.
    #[test]
    fn the_corp_draws_below_the_hq_floor_and_still_installs_from_it() {
        let registry = CardRegistry::from_cards(vec![asset("clearinghouse", 0)]);
        let state = |hq, credits, installed| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state.corp.r_and_d = corp_cards("rd", RD_DRAW_RESERVE + 10);
            state.corp.hq = corp_cards("clearinghouse", hq);
            state.corp.installed = (0..installed)
                .map(|i| InstalledCard { card: CardId("clearinghouse".to_string()), install_id: InstallId(i), ..Default::default() })
                .collect();
            evaluate_state(&state, Side::Corp, &registry)
        };
        assert!(state(HQ_FLOOR, 5, 0) > state(HQ_FLOOR - 1, 6, 0), "draw up to the floor rather than click for a credit");
        assert!(state(HQ_FLOOR - 1, 5, 1) > state(HQ_FLOOR, 6, 0), "install from the floor rather than click for a credit");
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

    /// The Carmen case behind `BREAKER_COVERAGE_WEIGHT`'s size: a 5-cost
    /// breaker is worth installing over an open run, even from an at-floor
    /// grip where the install also costs a grip card.
    #[test]
    fn a_five_cost_breaker_beats_an_open_run_even_from_an_at_floor_grip() {
        use netrunner_core::rules::{MemoryUnits, ServerId};
        let registry = CardRegistry::from_cards(vec![costed_breaker("carmen", Some(IceType::Sentry), 5)]);
        let mut ran = GameState::new(0);
        ran.runner.resources.credits = Credits(5);
        ran.runner.memory_units = MemoryUnits(4);
        ran.runner.grip = corp_cards("grip", GRIP_FLOOR - 1);
        ran.runner.grip.push(CardId("carmen".to_string()));
        let mut installed = ran.clone();
        installed.runner.grip.pop();
        installed.runner.resources.credits = Credits(0);
        installed.runner.memory_units = MemoryUnits(3);
        installed.runner.rig = vec![rig_card("carmen")];
        ran.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
        assert!(evaluate_state(&installed, Side::Runner, &registry) > evaluate_state(&ran, Side::Runner, &registry));
    }

    /// An installed agenda is worth more for each ICE in front of it, up
    /// to the cap; an asset behind the same ICE gets nothing.
    #[test]
    fn an_installed_agenda_is_worth_more_behind_ice_up_to_the_cap() {
        use netrunner_core::rules::{InstallSlot, ServerId};
        let mut agenda = ice("offworld_office", 0);
        agenda.card_type = CardType::Agenda;
        let registry = CardRegistry::from_cards(vec![agenda, asset("nico_campaign", 2), ice("palisade", 3)]);
        let board = |root: &str, ice_count: usize| {
            let mut state = GameState::new(0);
            state.corp.r_and_d = corp_cards("rd", RD_DRAW_RESERVE + 10);
            state.corp.hq = corp_cards("hq", HQ_FLOOR);
            state.corp.installed = vec![InstalledCard {
                card: CardId(root.to_string()),
                install_id: InstallId(0),
                server: ServerId::Remote(0),
                ..Default::default()
            }];
            for i in 0..ice_count {
                state.corp.installed.push(InstalledCard {
                    card: CardId("palisade".to_string()),
                    install_id: InstallId(i as u32 + 1),
                    server: ServerId::Remote(0),
                    slot: InstallSlot::Ice,
                    ..Default::default()
                });
            }
            evaluate_state(&state, Side::Corp, &registry)
        };
        let per_ice = |root: &str, n| board(root, n) - board(root, n - 1) - UNREZZED_INSTALL_WEIGHT;
        assert!((per_ice("offworld_office", 1) - AGENDA_PROTECTION_WEIGHT).abs() < 1e-9);
        assert!((per_ice("offworld_office", AGENDA_PROTECTION_CAP) - AGENDA_PROTECTION_WEIGHT).abs() < 1e-9);
        assert!(per_ice("offworld_office", AGENDA_PROTECTION_CAP + 1).abs() < 1e-9, "past the cap an ICE is just an ICE");
        assert!(per_ice("nico_campaign", 1).abs() < 1e-9, "an asset is not protected by this term");
    }

    // ----- `Weights`: the terms only a personality moves -----

    #[test]
    fn the_default_weights_are_the_constants_and_score_identically() {
        let mut state = GameState::new(0);
        state.corp.resources.agenda_points = AgendaPoints(2);
        state.runner.grip = corp_cards("g", 4);
        for side in [Side::Corp, Side::Runner] {
            assert_eq!(evaluate_state(&state, side, &empty()), evaluate_state_with(&state, side, &empty(), &Weights::default()));
        }
        assert_eq!(Weights::default().opponent_grip_weight, 0.0, "the balanced Corp does not play for the flatline");
        assert_eq!(Weights::default().installed_agenda_weight, 0.0, "the balanced Corp has no install preference by type");
    }

    #[test]
    fn an_installed_agenda_weight_prefers_the_agenda_on_the_table_to_the_ice() {
        let mut agenda = ice("agenda", 0);
        agenda.card_type = CardType::Agenda;
        agenda.advancement_requirement = Some(3);
        let registry = CardRegistry::from_cards(vec![agenda, ice("wall", 0)]);
        let install = |id: &str, slot: netrunner_core::rules::InstallSlot| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId(id.to_string()),
                install_id: InstallId(1),
                slot,
                server: netrunner_core::rules::ServerId::Remote(0),
                ..Default::default()
            }];
            state
        };
        let with_agenda = install("agenda", netrunner_core::rules::InstallSlot::Root);
        let with_ice = install("wall", netrunner_core::rules::InstallSlot::Ice);
        let balanced = Weights::default();
        assert_eq!(
            evaluate_state_with(&with_agenda, Side::Corp, &registry, &balanced),
            evaluate_state_with(&with_ice, Side::Corp, &registry, &balanced),
            "balanced: an unrezzed install is an unrezzed install"
        );
        let rush = Weights { installed_agenda_weight: 1.0, ..balanced };
        assert!(evaluate_state_with(&with_agenda, Side::Corp, &registry, &rush) > evaluate_state_with(&with_ice, Side::Corp, &registry, &rush));
    }

    #[test]
    fn a_positive_opponent_grip_weight_makes_the_corp_want_the_runners_hand_thin() {
        let mut full = GameState::new(0);
        full.runner.grip = corp_cards("g", 5);
        let mut thin = full.clone();
        thin.runner.grip.truncate(1);
        let balanced = Weights::default();
        assert_eq!(evaluate_state_with(&full, Side::Corp, &empty(), &balanced), evaluate_state_with(&thin, Side::Corp, &empty(), &balanced));
        let trap = Weights { opponent_grip_weight: 0.5, ..balanced };
        let delta = evaluate_state_with(&thin, Side::Corp, &empty(), &trap) - evaluate_state_with(&full, Side::Corp, &empty(), &trap);
        assert!((delta - 2.0).abs() < 1e-9, "four cards of damage at 0.5 each");
    }
}
