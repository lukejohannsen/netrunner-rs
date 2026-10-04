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
///
/// **The breach is of the server the run will approach, not the one it
/// attacked** (Phase 5 §31): Maintenance Access's "run Archives; if you
/// pass all ice, this run becomes a run on HQ instead" is
/// `RunState::redirect_on_approach`, so its run is read as the HQ access
/// it is, through Archives' ICE. And the accesses the run's own rider
/// adds (`rider_accesses`, Jailbreak) are counted with the ones the run
/// already carries.
pub(super) fn access_prospect(state: &GameState, run: &RunState, registry: &CardRegistry, w: &Weights, credits: u32, horizon: u32) -> f64 {
    let server = run.redirect_on_approach.unwrap_or(run.server);
    breach_worth(state, run, registry, w, credits, horizon, runs_earlier_this_turn(state, server))
}

/// `access_prospect` with the runs already made on the server this turn
/// given rather than read: `shut_doors` reads a server for a run to come,
/// on a turn when nothing it shows has been seen.
fn breach_worth(state: &GameState, run: &RunState, registry: &CardRegistry, w: &Weights, credits: u32, horizon: u32, earlier: usize) -> f64 {
    use netrunner_core::rules::{InstallSlot, ServerId};
    let server = run.redirect_on_approach.unwrap_or(run.server);
    // What the rig adds at the breach (Docklands Pass, `rig_breach_accesses`)
    // beside what the run's own rider adds (Phase 5 §32), and what the
    // identities print about the breach (Mercury Chrome, §34).
    let promised = rider_accesses(run, server) + rig_breach_accesses(state, registry, server) + identities::run_success(state, registry, run).accesses;
    let seen = earlier > 0;
    let mut hidden = 0.0_f64;
    let mut tokens = 0u32;
    let mut finishable = 0usize;
    let mut ambushes = 0usize;
    let mut damage = 0usize;
    let mut tags = 0u32;
    let mut trash_gain = 0.0;
    // The first trash of the turn pays what the identities print about it
    // (René "Loup" Arcemont's credit and card, §34): one card's trash, the
    // one it lifts most from not worth it to worth it.
    let first_trash = identities::on_trash_while_accessing(state, registry, server);
    let first_trash = f64::from(first_trash.runner_credits) * w.own_credit_weight + f64::from(first_trash.runner_cards) * w.click_weight;
    let mut first_trash_lift = 0.0_f64;
    // What both identities print about a steal (§36) — Jinteki: Personal
    // Evolution's net damage, Thule Subsea's core damage or its click and
    // 2[credit], Poétrï's install — paid for every agenda the breach
    // takes: the ones it cannot miss (`known_steals`), and the chance of
    // one in each card it has not seen (`expected_steals`), an agenda
    // being `TYPICAL_AGENDA_POINTS` of the points the Runner expects
    // there.
    let steal = identities::on_steal(state, registry, server, credits);
    let reads_steals = steal != identities::Pays::default();
    let (hq_points, density) = if reads_steals { agenda_points_expected(state, registry) } else { (0.0, 0.0) };
    let mut known_steals = 0u32;
    let mut expected_steals = 0.0_f64;
    let corp_credits = state.corp.resources.credits.0;
    for installed in state.corp.installed.iter().filter(|card| card.server == server && card.slot == InstallSlot::Root) {
        if installed.rezzed || installed.seen_by_runner {
            let Some(def) = registry.get(&installed.card) else { continue };
            if punishes_access_with_damage(def) {
                ambushes += 1;
                damage += trap_damage(state, installed, def);
            }
            // What the Corp's identity does to an access of this card
            // (BANGUN's 2 meat damage and a tag for a faceup agenda, §34),
            // counted with a trap's damage toward the flatline it can be.
            let punished = identities::on_access(state, registry, server, def, installed);
            damage += punished.damage as usize;
            tags += punished.tags;
            // A faceup agenda, or one the Runner has accessed and could not
            // steal, is a steal the breach cannot miss — the Archives
            // reading below, in a root. Only a BANGUN Corp turns an agenda
            // faceup on the table, so before §34 this was nowhere, and the
            // planner Runner walked into 2 meat damage and a tag for a
            // steal it had never priced.
            if def.card_type == CardType::Agenda && can_pay_to_steal(state, registry, def, credits) {
                trash_gain += f64::from(def.agenda_points.unwrap_or(0)) * w.agenda_point_weight;
                known_steals += 1;
            }
            if let Some(cost) = def.trash_cost
                && cost <= credits
            {
                let removed = visible_install_value(state, installed, registry, w, horizon) * w.opponent_board_weight
                    + if installed.rezzed && !matches!(def.card_type, CardType::Ice(_)) { w.dismantle_weight } else { 0.0 };
                let gain = removed - f64::from(cost) * w.own_credit_weight;
                trash_gain += gain.max(0.0);
                first_trash_lift = first_trash_lift.max((gain + first_trash).max(0.0) - gain.max(0.0));
            }
        } else if !seen {
            hidden += 1.0;
            expected_steals += hq_points / TYPICAL_AGENDA_POINTS;
            tokens += installed.advancement_tokens;
            // "Read the counters on it and the Corp's credits": the
            // Corp's credits are public, the card's requirement is not,
            // so a typical one stands in. See `FINISHABLE_INSTALL_WEIGHT`.
            if corp_credits + installed.advancement_tokens >= TYPICAL_ADVANCEMENT_REQUIREMENT {
                finishable += 1;
            }
        }
    }
    // The Runner's plans (Stage 7): the stakes as the Runner counts
    // them on a central, the pressure plan's HQ and the rig plan's R&D,
    // each per fresh access and on the same breakability gate as the
    // hidden access itself. Zero at the reference's weights.
    let mut plans = 0.0;
    match server {
        ServerId::Hq => {
            let held = state.corp.hq.len();
            let accesses = (1 + (run.additional_hq_access + promised) as usize).min(held);
            let fresh = if held == 0 { 0.0 } else { ((held - 1) as f64 / held as f64).powi(earlier as i32) };
            hidden += accesses as f64 * fresh;
            expected_steals += accesses as f64 * fresh * hq_points / TYPICAL_AGENDA_POINTS;
            if w.runner_stakes_weight != 0.0 || w.hq_pressure_weight != 0.0 {
                let (hq_points, _) = agenda_points_expected(state, registry);
                plans += accesses as f64 * fresh * (hq_points * w.runner_stakes_weight + w.hq_pressure_weight);
            }
        }
        ServerId::RnD if !seen => {
            let accesses = (1 + (run.additional_rd_access + promised) as usize).min(state.corp.r_and_d.len());
            hidden += accesses as f64;
            expected_steals += accesses as f64 * density / TYPICAL_AGENDA_POINTS;
            if w.runner_stakes_weight != 0.0 {
                let (_, density) = agenda_points_expected(state, registry);
                plans += accesses as f64 * density * w.runner_stakes_weight;
            }
            plans += accesses.saturating_sub(1) as f64 * w.rd_access_weight;
        }
        ServerId::RnD | ServerId::Remote(_) => {}
        ServerId::Archives => {
            for archived in &state.corp.archives {
                if archived.facedown {
                    if !seen {
                        hidden += 1.0;
                        expected_steals += hq_points / TYPICAL_AGENDA_POINTS;
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
                        known_steals += 1;
                    }
                }
            }
        }
    }
    // A steal the breach cannot miss deals its damage as a trap does,
    // toward the same flatline.
    damage += known_steals as usize * steal.damage as usize;
    // "The Runner is flatlined immediately if they suffer more damage than
    // they have cards in their grip" (CR 1.7.2b).
    let grip = state.runner.grip.len();
    let trap = if damage > grip {
        w.lethal_trap_weight
    } else {
        damage as f64 * w.known_trap_damage_weight
    };
    // A steal the breach may make deals its damage at its chance — and
    // when that damage would be the flatline, the chance of an agenda is
    // the chance of losing the game: an empty grip on a run into R&D
    // under Jinteki: Personal Evolution.
    let risked = if steal.damage > 0 && damage <= grip && damage + steal.damage as usize > grip {
        expected_steals.min(1.0) * w.lethal_trap_weight
    } else {
        expected_steals * f64::from(steal.damage) * w.known_trap_damage_weight
    };
    // NBN: Reality Plus's 2[credit] for the turn's first tag, on the tags
    // the breach deals.
    let tagged = identities::on_tags(state, registry, server, tags).to_runner(w);
    hidden * w.active_run_weight + f64::from(tokens) * w.advanced_card_prospect_weight
        + finishable as f64 * w.finishable_install_weight
        - ambushes as f64 * w.known_ambush_weight
        - trap
        - f64::from(tags) * w.tag_weight
        + tagged
        + trash_gain
        + first_trash_lift
        + plans
        + (f64::from(known_steals) + expected_steals) * steal.to_runner(w)
        - risked
}

/// The points of an agenda the Runner expects to find, for turning the
/// points a breach is expected to reach (`agenda_points_expected`) into
/// the agendas it is expected to steal: the pool's 44 agendas average 1.9.
const TYPICAL_AGENDA_POINTS: f64 = 2.0;

/// The credits the Corp would spend rezzing the unrezzed ICE still ahead
/// of the Runner in `run`, `TYPICAL_REZ_COST` a piece out of what the
/// Corp has, outermost first, until it runs out — what precept 8's run
/// makes the Corp pay. See `FORCED_REZ_WEIGHT`. Reads the Corp's
/// credits and the ICE's position, never the sampled card under it.
/// **What the run adds to every rez is added** (Phase 5 §31): Tread
/// Lightly's "the rez cost of each piece of ice is increased by
/// 3[credit]" is a lingering effect on each piece of ICE
/// (`lingering::ice_rez_cost`), and a rez under it costs the Corp that
/// much more of what it has — the card's whole point, and the one term
/// at the guide's rate that reads the Corp's rez.
pub(super) fn forced_rez_credits(state: &GameState, run: &RunState) -> u32 {
    let mut budget = state.corp.resources.credits.0;
    let mut spent = 0;
    let each = TYPICAL_REZ_COST.saturating_add_signed(netrunner_core::rules::lingering::ice_rez_cost(state));
    for _ in run.ice.iter().skip(run.position).filter(|ice| !ice.rezzed) {
        let rez = each.min(budget);
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
    state.corp.installed.iter().map(|installed| visible_install_value(state, installed, registry, w, horizon)).sum()
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
pub(super) fn visible_install_value(state: &GameState, installed: &InstalledCard, registry: &CardRegistry, w: &Weights, horizon: u32) -> f64 {
    if installed.rezzed {
        corp_install_value(state, installed, registry, w, [true; 3], horizon)
    } else {
        w.unrezzed_install_weight + f64::from(installed.advancement_tokens) * w.advancement_weight
    }
}

/// What the Runner's grip is worth, summed over `install_delta` and
/// floored at zero per card; scaled by `held_card_weight`.
pub(super) fn held_cards_value(state: &GameState, registry: &CardRegistry, w: &Weights, horizon: u32) -> f64 {
    let rig = rig_coverage(state, registry);
    let shown = shown_for(state, registry, w);
    state
        .runner
        .grip
        .iter()
        .filter_map(|card| registry.get(card))
        .map(|def| install_delta(def, held_price(state, registry, def), rig, shown, w, horizon).max(0.0))
        .sum()
}

/// What installing a held card would cost the Runner, as the engine asks
/// it (`continuous::install_cost_of`): the printed cost and every
/// discount on the table, the identity's among them — Kate "Mac"
/// McCaffrey's "lower the install cost of the first program or piece of
/// hardware you install each turn by 1" (Phase 5 §37). Read as the table
/// stands: once the turn's first such install is made, the rest are at
/// their printed cost until the turn ends, and at the leaf the next one is
/// the turn after's.
pub(super) fn held_price(state: &GameState, registry: &CardRegistry, def: &CardDefinition) -> u32 {
    netrunner_core::rules::continuous::install_cost_of(state, registry, def)
}

/// What an installer on the table is worth (Phase 5 §30): `(hosted,
/// promised)` — what the programs it hosts are worth where they are, and
/// the clicks the programs still in the grip promise. An installer is a
/// rig card whose text installs from the cards it hosts — a `Paid`
/// ability with `Effect::InstallRunnerCardFromZone { from: HostedOnSource }`
/// in it (Madani's
/// "once per turn → 0[credit]: install 1 hosted program"), read off the
/// DSL and never the name; a card hosted as a break's stock (Matryoshka's
/// copies) is not read here, because nothing installs it.
///
/// **A hosted program is a program one turn from the table:** its
/// `install_delta` less a click, the turn's wait at the guide's rate —
/// not the grip's half, because its install is free and certain, and not
/// the whole, because it is not on the table yet. **A grip program
/// promises half a click:** the install will cost none, once a host
/// click still to be paid has been. Both over the programs the Runner
/// would install at all (`install_delta` above zero), the promise capped
/// at the turns the stage expects (`horizon`).
///
/// Three readings were measured before this one (the §30 entry). With
/// the promise read off hosted cards alone, an empty Madani was worth its
/// presence and nothing, and the planner installed it in 1 of 192 games.
/// With the grip and the host promising the same click it put Madani on
/// the table at random's rate and hosted on it in 1–10 games of 48
/// against random's 29–48: a host was a click for nothing, and an
/// unhosted Madani cost the Runner its games. With a hosted program at a
/// whole click and a grip one at half, the host was a tie with a credit
/// click at two programs and a fifth of a click at three, which the grip
/// floor outbid, and the planner hosted on it in 0 games. The wait is
/// what a hosted program is worth over a held one, and it is what makes
/// the host a move: one strong breaker hosted is worth half its delta
/// over the click, and the free install that follows is worth the click.
pub(super) fn hosted_installs_value(state: &GameState, registry: &CardRegistry, w: &Weights, horizon: u32) -> (f64, f64) {
    let rig = rig_coverage(state, registry);
    let shown = shown_for(state, registry, w);
    // Programs: what the installer installs. A grip hardware is not a
    // click it will ever save.
    let delta = |def: &CardDefinition| {
        if def.card_type == CardType::Program { install_delta(def, held_price(state, registry, def), rig, shown, w, horizon).max(0.0) } else { 0.0 }
    };
    let mut hosted = 0.0;
    let mut promised = 0.0;
    for host in state.runner.rig.iter().filter(|host| registry.get(&host.card).is_some_and(installs_from_host)) {
        hosted += host
            .hosted_cards
            .iter()
            .filter_map(|card| registry.get(card))
            .map(|def| (delta(def) - w.click_weight).max(0.0))
            .sum::<f64>();
        promised += 0.5 * state.runner.grip.iter().filter_map(|card| registry.get(card)).filter(|def| delta(def) > 0.0).count() as f64;
    }
    (hosted, promised.min(f64::from(horizon)))
}

/// Whether a card's text installs the cards it hosts.
pub(super) fn installs_from_host(def: &CardDefinition) -> bool {
    def.abilities.iter().filter(|ability| ability.trigger == Trigger::Paid).any(|ability| {
        let mut found = false;
        ability.effect.for_each_effect(&mut |effect| {
            if matches!(effect, Effect::InstallRunnerCardFromZone { from: netrunner_core::dsl::CardZoneRef::HostedOnSource, .. }) {
                found = true;
            }
        });
        found
    })
}

/// The ICE the Corp has shown, for the terms that condition on it, or
/// every subtype when no term does — so the reference's arithmetic is
/// untouched by the reading. See `UNSHOWN_BREAKER_WEIGHT`.
pub(super) fn shown_for(state: &GameState, registry: &CardRegistry, w: &Weights) -> [bool; 3] {
    if w.unshown_breaker_weight != 0.0 { ice_shown(state, registry) } else { [true; 3] }
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
/// minus its price (`held_price`, the printed cost less what the table
/// takes off it) and the memory it takes. The same arithmetic
/// the install itself scores (the grip term aside), so a card is "live"
/// in hand exactly when the Runner would install it — leaving the memory
/// out made a held Cleaver worth more than the installed one and the
/// install a net loss. Zero for anything that is not a program, hardware
/// or resource. A subtype the card would cover that the Corp has not
/// shown (`shown`, from `shown_for`) is worth its coverage less
/// `unshown_breaker_weight`, and an R&D access the card promises is
/// worth `rd_access_weight` (Stage 7; both zero at the reference).
pub(super) fn install_delta(def: &CardDefinition, price: u32, rig: [bool; 3], shown: [bool; 3], w: &Weights, horizon: u32) -> f64 {
    if !matches!(def.card_type, CardType::Program | CardType::Hardware | CardType::Resource) {
        return 0.0;
    }
    let new_coverage = covers(def).iter().zip(rig).filter(|(grip, rig)| **grip && !rig).count();
    let unshown = covers(def).iter().zip(rig).zip(shown).filter(|((grip, rig), shown)| **grip && !rig && !shown).count();
    let promised = if w.rd_access_weight != 0.0 { f64::from(rd_accesses(def, None, horizon)) * w.rd_access_weight } else { 0.0 };
    // The same future credits `rig_income` will count once the card is
    // installed, so an economy card is live in hand exactly when the
    // Runner would install it — the breaker's arithmetic, for money.
    let income = if w.future_credit_weight != 0.0 {
        future_credits(&declared_income(def), None, horizon) * w.future_credit_weight
    } else {
        0.0
    };
    w.board_presence_weight + new_coverage as f64 * w.breaker_coverage_weight + income + promised
        - unshown as f64 * w.unshown_breaker_weight
        - f64::from(price) * w.own_credit_weight
        - f64::from(def.memory_cost.unwrap_or(0)) * w.memory_weight
}

/// Credits the Runner is short of the cheapest grip breaker worth
/// installing: one that covers a subtype the rig cannot break and fits in
/// free memory. Zero with no such card, or once it is affordable. Its
/// price is `held_price`'s: it was the printed cost, on the ground that
/// over-estimating the target only made the Runner save one click longer,
/// and under Kate "Mac" McCaffrey that click is every breaker's (§37).
/// Only a breaker for ICE the
/// Corp has shown is saved for when `shown` says which (Stage 7); the
/// reference passes every subtype.
pub(super) fn breaker_savings_shortfall(state: &GameState, registry: &CardRegistry, shown: [bool; 3]) -> u32 {
    let rig = rig_coverage(state, registry);
    let target = state
        .runner
        .grip
        .iter()
        .filter_map(|card| registry.get(card))
        .filter(|def| def.memory_cost.unwrap_or(0) <= state.runner.memory_units.0)
        .filter(|def| covers(def).iter().zip(rig).zip(shown).any(|((grip, rig), shown)| *grip && !rig && shown))
        .map(|def| held_price(state, registry, def))
        .min();
    target.map_or(0, |cost| cost.saturating_sub(state.runner.resources.credits.0))
}

/// What the Corp's ICE keeps from the Runner, read off a run (Phase 5
/// §38): for every server behind rezzed ICE, what a breach of it is worth
/// (`breach_worth`, read as a run to come would find it, nothing there
/// seen yet) at the share its ICE shuts it — all of it when a piece is one
/// no rig card breaks, and `cost / (cost + RUNNER_TURN_CLICKS)` when the
/// rig breaks the lot for `cost` (`server_break_cost`), so a door that
/// costs a turn's clicks in credits is half shut. Subtracted at
/// `shut_door_weight`.
///
/// Before it the Runner read the ICE only on a run, as the leaf's break
/// cost, and where a piece stood was nothing to it until it ran there: Tāo
/// Salonga's "you may swap 2 installed pieces of ice" whenever an agenda is
/// scored or stolen, which is a reading of where each piece stands against
/// the rig and nothing else, was taken 0 times in 306 offers (48 games a
/// pairing, both Tāo decks).
///
/// **The Runner's credits are not read.** With them, a credit click that
/// crossed a door's price would open it, and every economy decision would
/// carry a share of every server's stakes; the reading is the ICE against
/// the rig. **Rezzed ICE only**, as `server_break_cost` reads it and for
/// its reason: a face-down piece is a card the Runner has not seen, and a
/// rez is the Corp's to make — so a face-down piece swapped in front of a
/// server costs nothing here, the same optimism a run's gate has. A swap
/// within one server moves nothing here, and is a tie with the decline.
pub(super) fn shut_doors(state: &GameState, registry: &CardRegistry, w: &Weights, horizon: u32) -> f64 {
    use netrunner_core::rules::ServerId;
    let mut servers = vec![ServerId::Hq, ServerId::RnD, ServerId::Archives];
    for card in &state.corp.installed {
        if matches!(card.server, ServerId::Remote(_)) && !servers.contains(&card.server) {
            servers.push(card.server);
        }
    }
    let credits = state.runner.resources.credits.0;
    servers
        .into_iter()
        .map(|server| {
            let shut = match server_break_cost(state, server, registry) {
                None => 1.0,
                Some(0) => return 0.0,
                Some(cost) => f64::from(cost) / f64::from(cost + RUNNER_TURN_CLICKS),
            };
            let run = RunState { server, ..RunState::default() };
            shut * breach_worth(state, &run, registry, w, credits, horizon, 0).max(0.0)
        })
        .sum()
}

/// The Runner's terms, added to `score` in the order `evaluate_state_with`
/// always added them — see `corp::score`.
pub(super) fn score(state: &GameState, registry: &CardRegistry, w: &Weights, horizon: u32, score: &mut f64) {
    *score -= state.runner.tags as f64 * w.tag_weight;
    *score -= state.runner.brain_damage as f64 * w.core_damage_weight;
    *score += state.runner.rig.len() as f64 * w.board_presence_weight;
    *score += state.runner.memory_units.0 as f64 * w.memory_weight;
    *score += breaker_coverage(state, registry) as f64 * w.breaker_coverage_weight;
    *score -= breaker_savings_shortfall(state, registry, shown_for(state, registry, w)) as f64 * w.savings_shortfall_weight;
    *score -= w.grip_floor.saturating_sub(state.runner.grip.len()) as f64 * w.grip_shortfall_weight;
    *score += held_cards_value(state, registry, w, horizon) * w.held_card_weight;
    // A program hosted on an installer is one turn from the table, and
    // the grip's programs promise it clicks (`hosted_installs_value`).
    let (hosted, promised) = hosted_installs_value(state, registry, w, horizon);
    *score += hosted + promised * w.click_weight;
    *score -= visible_corp_board(state, registry, w, horizon) * w.opponent_board_weight;
    if state.this_turn.times(Trigger::OnSuccessfulRun) > 0 {
        *score += w.successful_run_weight;
    }
    if let Some(run) = &state.active_run {
        let pool = run_pool(state, run);
        if let Some(due) = remaining_break_cost(state, run, registry).filter(|due| *due <= pool) {
            *score += access_prospect(state, run, registry, w, pool - due, horizon);
            // What the card that began the run put on it (Phase 5 §31).
            // The run's own credits pay the breaks the Runner's would
            // have, and are gone when the run is: worth the breaks they
            // cover and no more (Overclock). The run is never charged
            // for its breaks (see `remaining_break_cost`), so this is
            // the one reading of them — what the Runner keeps. The
            // rider pays on success, at the rate a play is read at: a
            // credit a credit, a card a click (Clean Getaway, Red Team's
            // run, Joy Ride).
            *score += f64::from(due.min(run_credits_for_breaking(run))) * w.own_credit_weight;
            let (credits, cards) = rider_income(run, Side::Runner);
            *score += f64::from(credits) * w.own_credit_weight + f64::from(cards) * w.click_weight;
            // What both identities print about the run succeeding, at the
            // same rates (§34): Gabriel Santiago's 2[credit] for the turn's
            // first HQ run, Zahya Sadeghi's credit an access, Dewi
            // Subrotoputri's credit or card — and what the Corp's pays it.
            let pays = identities::run_success(state, registry, run);
            *score += f64::from(pays.runner_credits) * w.own_credit_weight + f64::from(pays.runner_cards) * w.click_weight
                - f64::from(pays.corp_credits + pays.corp_cards) * w.opponent_credit_weight;
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
    // The Runner's plans (Stage 7), after the guide's rate; the stakes,
    // the pressure plan's HQ and the rig plan's run are inside
    // `access_prospect`, on its gate.
    if w.unshown_breaker_weight != 0.0 {
        *score -= unshown_coverage(state, registry) as f64 * w.unshown_breaker_weight;
    }
    if w.shut_door_weight != 0.0 {
        *score -= shut_doors(state, registry, w, horizon) * w.shut_door_weight;
    }
    if w.last_click_run_weight != 0.0
        && last_click_run(state)
        && let Some(run) = &state.active_run
        && unknown_ahead(state, run)
        && corp_punishes_runs(state, registry)
    {
        *score -= w.last_click_run_weight;
    }
    if w.feared_flatline_weight != 0.0 && state.runner.grip.len() <= damage_feared(state, registry) {
        *score -= w.feared_flatline_weight;
    }
    if w.dismantle_weight != 0.0 {
        let trashable = state
            .corp
            .installed
            .iter()
            .filter(|card| card.rezzed && registry.get(&card.card).is_some_and(|def| !matches!(def.card_type, CardType::Ice(_))))
            .count();
        *score -= trashable as f64 * w.dismantle_weight;
    }
    if w.rd_access_weight != 0.0 {
        *score += f64::from(rig_rd_accesses(state, registry, horizon)) * w.rd_access_weight;
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

    /// Kate "Mac" McCaffrey's "lower the install cost of the first program
    /// or piece of hardware you install each turn by 1" is in a held card's
    /// price (§37): the grip is worth a credit more and the breaker saved
    /// for is a credit nearer, until the turn's first such install is made.
    #[test]
    fn kates_discount_is_in_a_held_cards_price() {
        use netrunner_core::rules::MemoryUnits;
        let pool = pool();
        let corroder = printed(&pool, "corroder");
        let mut state = GameState::new(0);
        state.runner.memory_units = MemoryUnits(4);
        state.runner.grip = vec![corroder.id.clone()];
        assert_eq!(held_price(&state, &pool, &corroder), corroder.cost);
        let printed_shortfall = breaker_savings_shortfall(&state, &pool, [true; 3]);
        state.runner.identity = Some(CardId("kate_mac_mccaffrey_digital_tinker".to_string()));
        assert_eq!(held_price(&state, &pool, &corroder), corroder.cost - 1);
        assert_eq!(breaker_savings_shortfall(&state, &pool, [true; 3]), printed_shortfall - 1);
        // The turn's first program is installed: the next is at its printed cost.
        state.phase = netrunner_core::rules::GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = netrunner_core::rules::Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip.push(CardId("gordian_blade".to_string()));
        let install = netrunner_core::rules::legal_actions_for(&state, &pool, Side::Runner)
            .into_iter()
            .find(|action| matches!(action, netrunner_core::rules::PlayerAction::InstallProgram { card_id, .. } if card_id.0 == "gordian_blade"))
            .expect("Gordian Blade can be installed");
        state = netrunner_core::rules::apply_action(&state, &pool, install).expect("it is").0;
        assert_eq!(held_price(&state, &pool, &corroder), corroder.cost, "\"the first … each turn\"");
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
        assert_eq!(breaker_savings_shortfall(&state, &registry, [true; 3]), 2);
        state.runner.rig = vec![rig_card("cleaver")];
        assert_eq!(breaker_savings_shortfall(&state, &registry, [true; 3]), 0, "Barrier is already covered");
        state.runner.rig.clear();
        state.runner.memory_units = MemoryUnits(0);
        assert_eq!(breaker_savings_shortfall(&state, &registry, [true; 3]), 0, "no memory to install it into");
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

    /// A program hosted on an installer is one turn from the table — its
    /// install delta less a click — and the grip's programs promise half
    /// a click each, one a turn over the horizon; a program not worth
    /// installing is worth nothing either way, and a copy hosted on
    /// Matryoshka — a break's stock, which nothing installs — is read by
    /// nothing here (Phase 5 §30).
    #[test]
    fn a_hosted_program_is_one_turn_from_the_table_and_the_grip_promises_half_a_click() {
        let pool = pool();
        let registry = CardRegistry::from_cards(vec![
            printed(&pool, "madani"),
            printed(&pool, "matryoshka"),
            costed_breaker("cleaver", Some(IceType::Barrier), 1),
            costed_breaker("carmen", Some(IceType::Sentry), 1),
            costed_breaker("dear_corroder", Some(IceType::Barrier), 9),
        ]);
        let w = guide();
        let board = |host: &str, hosted: &[&str], grip: &[&str]| {
            let mut state = GameState::new(0);
            state.runner.rig = vec![InstalledRunnerCard {
                hosted_cards: hosted.iter().map(|c| CardId(c.to_string())).collect(),
                ..rig_card(host)
            }];
            state.runner.grip = grip.iter().map(|c| CardId(c.to_string())).collect();
            state
        };
        let delta = |id: &str| {
            let def = printed(&registry, id);
            install_delta(&def, def.cost, [false; 3], [true; 3], &w, 9)
        };
        assert!(delta("cleaver") > w.click_weight && delta("carmen") > w.click_weight);
        assert!(delta("dear_corroder") < 0.0, "9[c] for one subtype is not worth installing");

        let hosted = board("madani", &["cleaver", "carmen"], &[]);
        let waiting = delta("cleaver") - w.click_weight + delta("carmen") - w.click_weight;
        assert_eq!(hosted_installs_value(&hosted, &registry, &w, 9), (waiting, 0.0));
        let in_grip = board("madani", &[], &["cleaver", "carmen"]);
        assert_eq!(hosted_installs_value(&in_grip, &registry, &w, 9), (0.0, 1.0), "the grip's programs promise half a click each");
        let crowded = board("madani", &[], &["cleaver", "carmen", "cleaver"]);
        assert_eq!(hosted_installs_value(&crowded, &registry, &w, 1).1, 1.0, "one install a turn, one turn left");
        let dead = board("madani", &["dear_corroder"], &["dear_corroder"]);
        assert_eq!(hosted_installs_value(&dead, &registry, &w, 9), (0.0, 0.0), "a program never installed is worth nothing hosted and promises nothing");
        let stock = board("matryoshka", &["matryoshka"], &["cleaver"]);
        assert_eq!(hosted_installs_value(&stock, &registry, &w, 9), (0.0, 0.0), "a hosted copy is a break's stock, not a program waiting");
        let none = board("cleaver", &[], &["carmen"]);
        assert_eq!(hosted_installs_value(&none, &registry, &w, 9), (0.0, 0.0), "no installer, no promise");

        // The whole score: Madani on the table is worth the clicks the
        // grip promises over a hardware with no text, which is what puts
        // it there; and hosting the two is worth their wait over the
        // grip's half and the promise, less the grip floor.
        let cleaver = delta("cleaver");
        let carmen = delta("carmen");
        let mut registry = registry;
        registry.insert(CardDefinition { card_type: CardType::Hardware, side: Side::Runner, ..ice("plain", 2) });
        let mut bare = in_grip.clone();
        bare.runner.rig = vec![rig_card("plain")];
        let with = evaluate_state_with(&in_grip, Side::Runner, &registry, &w);
        let without = evaluate_state_with(&bare, Side::Runner, &registry, &w);
        assert!((with - without - w.click_weight).abs() < 1e-9, "{with} vs {without}");
        let hosted_score = evaluate_state_with(&hosted, Side::Runner, &registry, &w);
        let held_in_grip = (cleaver + carmen) * w.held_card_weight + w.click_weight;
        let expected = waiting - held_in_grip - 2.0 * w.grip_shortfall_weight;
        assert!((hosted_score - with - expected).abs() < 1e-9, "{hosted_score} vs {with}: expected {expected}");
        assert!(expected + 2.0 * w.grip_shortfall_weight > w.click_weight, "hosting two breakers is worth the click, floor aside");
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
        assert!(install_delta(&telework, telework.cost, rig, [true; 3], &w, horizon(Stage::Early)) > install_delta(&telework, telework.cost, rig, [true; 3], &Weights::default(), 9) + 1.0);
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
            access_prospect(&state, &run, &pool, &w, 5, horizon(stage(&state, &pool)))
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

    /// Precept 13's last click: the same run begun with a click in hand
    /// beats one begun on the last click against a Corp that punishes
    /// runs, and only against one. Read at the run's initiation, so the
    /// door a run can be left through is never opened by it.
    #[test]
    fn a_run_on_the_last_click_against_a_punishing_corp_costs_the_click_kept() {
        use netrunner_core::card::Faction;
        use netrunner_core::rules::{Clicks, ServerId};
        // The term ships at zero (`LAST_CLICK_RUN_WEIGHT`); this is the
        // decision it wins when it is on.
        let w = Weights { last_click_run_weight: 1.0, ..every_runner() };
        let begun = |faction: Faction, clicks: u32, phase: RunPhase| {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Runner);
            state.corp.hq = corp_cards("hq", 4);
            state.runner.grip = corp_cards("grip", 5);
            state.runner.resources.clicks = Clicks(clicks);
            let (mut state, identity) = against(state, faction);
            state.active_run = Some(RunState { server: ServerId::Hq, phase, ..Default::default() });
            let registry = CardRegistry::from_cards(vec![identity]);
            evaluate_state_with(&state, Side::Runner, &registry, &w)
        };
        let kept = begun(Faction::Jinteki, 1, RunPhase::Initiation) - begun(Faction::Jinteki, 0, RunPhase::Initiation);
        assert!((kept - (w.click_weight + w.last_click_run_weight)).abs() < 1e-9, "{kept}: a click kept, and the fear it answers");
        let against_hb = begun(Faction::HaasBioroid, 1, RunPhase::Initiation) - begun(Faction::HaasBioroid, 0, RunPhase::Initiation);
        assert!((against_hb - w.click_weight).abs() < 1e-9, "{against_hb}: no fear of a Corp that does not punish runs");
        assert_eq!(begun(Faction::Nbn, 0, RunPhase::Initiation), begun(Faction::Jinteki, 0, RunPhase::Initiation), "NBN's tags are feared like Jinteki's damage");
        let under_way = begun(Faction::Jinteki, 0, RunPhase::Movement) - begun(Faction::HaasBioroid, 0, RunPhase::Movement);
        assert_eq!(under_way, 0.0, "a run under way is not worth leaving for it");
    }

    /// A Corp that has shown a tagging or damaging piece of ICE punishes
    /// runs whatever its faction; a rezzed Barrier that only ends the
    /// run does not.
    #[test]
    fn a_corp_whose_rezzed_ice_tags_punishes_runs_whatever_its_faction() {
        use netrunner_core::card::Faction;
        use netrunner_core::dsl::{Amount, SubroutineDef};
        use netrunner_core::rules::{InstallSlot, ServerId};
        let mut funhouse = ice("funhouse", 5);
        funhouse.subroutines = vec![SubroutineDef { text: String::new(), effect: Effect::GiveTags(Amount::Fixed(1)), only_breakable_by: None }];
        let mut wall = ice("wall", 3);
        wall.subroutines = vec![SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }];
        let (state, identity) = against(GameState::new(0), Faction::WeylandConsortium);
        let registry = CardRegistry::from_cards(vec![funhouse, wall, identity]);
        let with = |state: &GameState, card: &str, rezzed: bool| {
            let mut state = state.clone();
            state.corp.installed = vec![InstalledCard { card: CardId(card.to_string()), install_id: InstallId(1), slot: InstallSlot::Ice, server: ServerId::Hq, rezzed, ..Default::default() }];
            corp_punishes_runs(&state, &registry)
        };
        assert!(!corp_punishes_runs(&state, &registry), "Weyland with nothing shown");
        assert!(with(&state, "funhouse", true));
        assert!(!with(&state, "funhouse", false), "face down, the ICE has shown nothing");
        assert!(!with(&state, "wall", true), "an end-the-run subroutine punishes nothing");
    }

    /// "Keep your grip larger than the damage you could take": against
    /// Jinteki the Runner draws up past a Snare!'s three before it runs;
    /// against Weyland the floor is the floor; a trap it has seen sets
    /// the number exactly.
    #[test]
    fn the_grip_is_kept_larger_than_the_damage_feared() {
        use netrunner_core::card::Faction;
        use netrunner_core::rules::ServerId;
        let w = every_runner();
        let holding = |faction: Faction, grip: usize, seen_trap: Option<u32>| {
            let mut state = GameState::new(0);
            state.runner.grip = corp_cards("grip", grip);
            let (mut state, identity) = against(state, faction);
            let mut cards = vec![identity, ambush("urtica_cipher")];
            if let Some(tokens) = seen_trap {
                let mut urtica = ambush("urtica_cipher");
                urtica.triggers[0].effects = vec![Effect::DealDamage(netrunner_core::dsl::DamageType::Net, 2), Effect::DealDamageAmount(netrunner_core::dsl::DamageType::Net, Amount::HostedAdvancementTokens)];
                cards[1] = urtica;
                state.corp.installed = vec![InstalledCard { card: CardId("urtica_cipher".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), advancement_tokens: tokens, seen_by_runner: true, ..Default::default() }];
            }
            let registry = CardRegistry::from_cards(cards);
            (evaluate_state_with(&state, Side::Runner, &registry, &w), damage_feared(&state, &registry))
        };
        assert_eq!(holding(Faction::Jinteki, 3, None).1, TYPICAL_NET_DAMAGE);
        let step = holding(Faction::Jinteki, 4, None).0 - holding(Faction::Jinteki, 3, None).0;
        assert!((step - w.feared_flatline_weight).abs() < 1e-9, "{step}: the fourth card lifts the fear (the floor is met at three)");
        assert_eq!(holding(Faction::Jinteki, 5, None).0, holding(Faction::Jinteki, 4, None).0, "and a fifth is worth nothing more");
        assert_eq!(holding(Faction::WeylandConsortium, 3, None).1, 0, "no fear of a Corp that is not Jinteki");
        assert_eq!(holding(Faction::WeylandConsortium, 4, None).0, holding(Faction::WeylandConsortium, 3, None).0);
        assert_eq!(holding(Faction::WeylandConsortium, 3, Some(3)).1, 5, "a seen Urtica Cipher with three tokens is five");
        assert!(holding(Faction::WeylandConsortium, 6, Some(3)).0 > holding(Faction::WeylandConsortium, 5, Some(3)).0 + w.feared_flatline_weight - 1e-9);
    }

    /// Precept 9 as arithmetic: an HQ run into a Corp that has drawn
    /// much and scored little is worth more than R&D at the deck's
    /// density, and one into a Corp that has scored what it drew is
    /// worth less.
    #[test]
    fn an_hq_run_is_worth_the_agendas_the_corp_is_holding() {
        use netrunner_core::rules::{AgendaPoints, ServerId};
        // The term ships at zero (`RUNNER_STAKES_WEIGHT`); this is the
        // decision it wins when it is on.
        let w = Weights { runner_stakes_weight: 1.0, ..every_runner() };
        let registry = pool();
        // A 49-card deck: 20 in R&D, 5 in HQ, 20 face up in Archives and
        // 4 face down on the table — 29 drawn, 11.8 points' worth
        // expected of them, and 9 cards the Runner has not seen.
        let corp_that = |scored: i32, stolen: i32| {
            let mut state = GameState::new(0);
            state.corp.r_and_d = corp_cards("rd", 20);
            state.corp.hq = corp_cards("hq", 5);
            state.corp.archives = (0..20).map(|i| netrunner_core::rules::ArchivedCard { card: CardId(format!("ar_{i}")), facedown: false }).collect();
            state.corp.installed = (0..4).map(|i| InstalledCard { card: CardId(format!("in_{i}")), install_id: InstallId(i + 1), server: ServerId::Remote(0), ..Default::default() }).collect();
            state.corp.resources.agenda_points = AgendaPoints(scored);
            state.runner.resources.agenda_points = AgendaPoints(stolen);
            state
        };
        let holding = corp_that(2, 2);
        let (hq, rd) = agenda_points_expected(&holding, &registry);
        assert!((rd - 20.0 / 49.0).abs() < 1e-9, "{rd}");
        assert!((hq - (29.0 * 20.0 / 49.0 - 4.0) / 9.0).abs() < 1e-9, "{hq}: 7.8 points unaccounted for over the nine cards not seen — the hand and the face-down roots");
        let mut buried = holding.clone();
        buried.corp.archives.iter_mut().for_each(|card| card.facedown = true);
        let (hq_buried, _) = agenda_points_expected(&buried, &registry);
        assert!(hq_buried < hq, "a face-down pile could hold them too: {hq_buried} against {hq}");
        let run = |state: &GameState, server| access_prospect(state, &RunState { server, ..Default::default() }, &registry, &w, 0, 9);
        assert!(run(&holding, ServerId::Hq) > run(&holding, ServerId::RnD), "HQ over R&D while the Corp holds without scoring");
        let scoring = corp_that(6, 6);
        let (hq, _) = agenda_points_expected(&scoring, &registry);
        assert_eq!(hq, 0.0, "every drawn agenda is accounted for");
        assert!(run(&scoring, ServerId::Hq) < run(&scoring, ServerId::RnD), "and R&D over HQ once they are");
        assert_eq!(access_prospect(&holding, &RunState { server: ServerId::Hq, ..Default::default() }, &registry, &Weights::default(), 0, 9), ACTIVE_RUN_WEIGHT, "the reference reads no density");
    }

    /// Precept 11: a breaker for ICE the Corp has not shown is worth
    /// less on the table and in hand, and is not saved for; the moment a
    /// piece of that subtype is rezzed anywhere it is worth what it was.
    #[test]
    fn a_breaker_for_ice_the_corp_has_not_shown_is_worth_less_and_not_saved_for() {
        use netrunner_core::rules::{InstallSlot, MemoryUnits, ServerId};
        let w = every_runner();
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3), ice("wall", 3)]);
        let board = |shown: bool, cleaver_in: &str| {
            let mut state = GameState::new(0);
            state.runner.resources.credits = Credits(0);
            state.runner.memory_units = MemoryUnits(4);
            state.runner.grip = corp_cards("filler", GRIP_FLOOR);
            match cleaver_in {
                "rig" => state.runner.rig = vec![rig_card("cleaver")],
                "grip" => state.runner.grip.push(CardId("cleaver".to_string())),
                _ => {}
            }
            state.corp.installed = vec![InstalledCard { card: CardId("wall".to_string()), install_id: InstallId(1), slot: InstallSlot::Ice, server: ServerId::Hq, rezzed: shown, ..Default::default() }];
            state
        };
        let rig = |shown| evaluate_state_with(&board(shown, "rig"), Side::Runner, &registry, &w) - evaluate_state_with(&board(shown, ""), Side::Runner, &registry, &w);
        assert!((rig(true) - rig(false) - w.unshown_breaker_weight).abs() < 1e-9, "{} vs {}", rig(true), rig(false));
        assert!(rig(false) > 0.0, "still worth having");
        assert_eq!(unshown_coverage(&board(false, "rig"), &registry), 1);
        assert_eq!(unshown_coverage(&board(true, "rig"), &registry), 0);
        // With the credits in hand, so the savings term is out of it.
        let held = |shown| {
            let mut with = board(shown, "grip");
            with.runner.resources.credits = Credits(3);
            let mut without = board(shown, "");
            without.runner.resources.credits = Credits(3);
            evaluate_state_with(&with, Side::Runner, &registry, &w) - evaluate_state_with(&without, Side::Runner, &registry, &w)
        };
        assert!(held(true) > held(false), "and less live in hand: {} vs {}", held(true), held(false));
        assert_eq!(breaker_savings_shortfall(&board(false, "grip"), &registry, ice_shown(&board(false, "grip"), &registry)), 0, "not saved for");
        assert_eq!(breaker_savings_shortfall(&board(true, "grip"), &registry, ice_shown(&board(true, "grip"), &registry)), 3, "saved for once shown");
        // The reference's arithmetic is untouched by the reading.
        let reference = |shown| evaluate_state(&board(shown, "rig"), Side::Runner, &registry) - evaluate_state(&board(shown, ""), Side::Runner, &registry);
        assert_eq!(reference(true), reference(false));
    }

    /// The dismantle plan trashes at 3[c] where a balanced Runner stops
    /// at 2[c], and the run that reaches the asset is worth starting for
    /// it; a Criminal runs HQ over R&D when both are open; a Shaper
    /// prices The Maker's Eye's accesses and a counter on Conduit.
    #[test]
    fn each_runner_plan_prices_its_chapters_lever() {
        use crate::plans::Plan;
        use netrunner_core::rules::ServerId;
        let mut nico = asset("nico_campaign", 2);
        nico.trash_cost = Some(3);
        let registry = CardRegistry::from_cards(vec![nico]);
        let mut state = GameState::new(0);
        state.runner.resources.credits = Credits(5);
        state.corp.hq = corp_cards("hq", 4);
        state.corp.r_and_d = corp_cards("rd", 20);
        state.corp.installed = vec![InstalledCard { card: CardId("nico_campaign".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), rezzed: true, ..Default::default() }];
        let remote = RunState { server: ServerId::Remote(0), ..Default::default() };
        assert_eq!(access_prospect(&state, &remote, &registry, &every_runner(), 5, 9), 0.0, "3[c] is too dear for the balanced Runner");
        let anarch = planned_runner(&[Plan::Dismantle]);
        let trash = access_prospect(&state, &remote, &registry, &anarch, 5, 9);
        assert!(trash > 0.0, "{trash}: the Anarch pays 3[c] for it");
        let mut trashed = state.clone();
        trashed.corp.installed.clear();
        trashed.runner.resources.credits = Credits(2);
        assert!(evaluate_state_with(&trashed, Side::Runner, &registry, &anarch) > evaluate_state_with(&state, Side::Runner, &registry, &anarch), "and trashes it once accessed");
        assert!(evaluate_state_with(&trashed, Side::Runner, &registry, &every_runner()) < evaluate_state_with(&state, Side::Runner, &registry, &every_runner()), "where the balanced Runner keeps its credits");

        let criminal = planned_runner(&[Plan::Pressure]);
        let run = |w: &Weights, server| access_prospect(&state, &RunState { server, ..Default::default() }, &registry, w, 5, 9);
        assert!(run(&criminal, ServerId::Hq) > run(&criminal, ServerId::RnD), "HQ over R&D under pressure");
        let without = Weights { hq_pressure_weight: 0.0, ..criminal };
        assert!((run(&criminal, ServerId::Hq) - run(&without, ServerId::Hq) - criminal.hq_pressure_weight).abs() < 1e-9);
        assert_eq!(run(&criminal, ServerId::RnD), run(&without, ServerId::RnD));

        let shaper = planned_runner(&[Plan::Rig]);
        let makers_eye = RunState { server: ServerId::RnD, additional_rd_access: 2, ..Default::default() };
        let extra = access_prospect(&state, &makers_eye, &registry, &shaper, 5, 9) - access_prospect(&state, &makers_eye, &registry, &every_runner(), 5, 9);
        assert!((extra - 2.0 * shaper.rd_access_weight).abs() < 1e-9, "{extra}: two accesses beyond the first");
        let pool = pool();
        let mut with_conduit = state.clone();
        with_conduit.runner.rig = vec![InstalledRunnerCard { card: CardId("conduit".to_string()), counters: 3, ..Default::default() }];
        let mut one_more = with_conduit.clone();
        one_more.runner.rig[0].counters = 4;
        let counter = evaluate_state_with(&one_more, Side::Runner, &pool, &shaper) - evaluate_state_with(&with_conduit, Side::Runner, &pool, &shaper);
        assert!((counter - shaper.rd_access_weight).abs() < 1e-9, "{counter}: a counter on Conduit is an R&D access");
        assert_eq!(evaluate_state_with(&one_more, Side::Runner, &pool, &every_runner()), evaluate_state_with(&with_conduit, Side::Runner, &pool, &every_runner()), "and nothing to another plan");
        assert_eq!(rd_accesses(&printed(&pool, "the_makers_eye"), None, 9), 2);
        assert_eq!(rd_accesses(&printed(&pool, "devadatta_drone"), None, 9), 1);
        assert_eq!(rd_accesses(&printed(&pool, "conduit"), Some(3), 9), 3 + 9, "its counters, and one its runs place a turn");
        assert_eq!(rd_accesses(&printed(&pool, "conduit"), None, 5), 5, "in hand, the counters its runs will place (Phase 5 §32)");
        assert_eq!(rd_accesses(&printed(&pool, "leech"), None, 9), 0, "a counter a run places is an access only on a card that accesses by its counters");

        // The rig's breach access is the run's (Phase 5 §32): Docklands
        // Pass makes an HQ run one more fresh access, and an R&D run none.
        let mut docklands = state.clone();
        docklands.runner.rig = vec![InstalledRunnerCard { card: CardId("docklands_pass".to_string()), ..Default::default() }];
        let on = |state: &GameState, server| access_prospect(state, &RunState { server, ..Default::default() }, &pool, &every_runner(), 5, 9);
        let hq_more = on(&docklands, ServerId::Hq) - on(&state, ServerId::Hq);
        assert!((hq_more - every_runner().active_run_weight).abs() < 1e-9, "{hq_more}: one more HQ access");
        assert_eq!(on(&docklands, ServerId::RnD), on(&state, ServerId::RnD));
    }

    /// A run a card's text began is priced with what the text put on it
    /// (Phase 5 §31), each read off the run the engine built: the run's
    /// own credits make a break the Runner cannot pay for payable and are
    /// worth the breaks they cover (Overclock), and nothing when their word
    /// is not breaking; the rider pays its credits at a credit each on
    /// success (Clean Getaway, and Red Team's "if this card is installed"
    /// as if it were); a run redirected on approach is the breach of the
    /// server it goes to (Maintenance Access); a standing run-ending
    /// prevention passes the first unbreakable piece over a root with a
    /// card in it and nothing over an empty one (Shred); and a rez tax on
    /// each piece of ICE is what the forced rez costs the Corp (Tread
    /// Lightly).
    #[test]
    fn a_run_a_card_began_is_priced_with_what_the_card_put_on_it() {
        use netrunner_core::dsl::{EffectRequirement, EndRunPrevention, PaysFor};
        use netrunner_core::rules::lingering::{Lingering, LingeringEffect, On, Until};
        use netrunner_core::rules::{InstallSlot, ServerId};
        let w = guide();
        let registry = CardRegistry::from_cards(vec![priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        let wall = run_ice(1, IceType::Barrier, 2, true);
        let sentry = run_ice(1, IceType::Sentry, 1, true);
        let registry = with_printed_ice(&registry, &[wall.clone(), sentry.clone()]);
        let mut base = GameState::new(0);
        base.runner.rig = vec![InstalledRunnerCard { base_strength: 1, ..rig_card("cleaver") }];
        base.corp.hq = corp_cards("hq", 3);
        let score = |state: &GameState| evaluate_state_with(state, Side::Runner, &registry, &w);
        let with_run = |state: &GameState, run: RunState| {
            let mut state = state.clone();
            state.active_run = Some(run);
            state
        };
        let hq = |ice: Vec<RunIce>| RunState { server: ServerId::Hq, ice, position: 0, ..Default::default() };

        // Overclock: 0[c] against a 1[c] break is no run; five run
        // credits make it one, worth the access and the credit kept.
        let broke = with_run(&base, hq(vec![wall.clone()]));
        let overclocked = with_run(&base, RunState { bonus_run_credits: 5, ..hq(vec![wall.clone()]) });
        let gained = score(&overclocked) - score(&broke);
        assert!((gained - (w.active_run_weight + w.own_credit_weight)).abs() < 1e-9, "{gained}");
        let for_trashing = with_run(&base, RunState { bonus_run_credits: 5, run_credits_pay_for: Some(PaysFor::TrashCosts), ..hq(vec![wall.clone()]) });
        assert_eq!(score(&for_trashing), score(&broke), "credits that pay for trashing break nothing");
        let for_breakers = with_run(&base, RunState { bonus_run_credits: 5, run_credits_pay_for: Some(PaysFor::Using(netrunner_core::dsl::CardFilter::Icebreaker)), ..hq(vec![wall.clone()]) });
        assert_eq!(score(&for_breakers), score(&overclocked));

        // Clean Getaway and Red Team: the rider's credits on success.
        let mut paid = base.clone();
        paid.runner.resources.credits = Credits(1);
        let plain = with_run(&paid, hq(vec![wall.clone()]));
        let getaway = with_run(&paid, RunState { on_success_effect: Some(Box::new(Effect::GainCredits(Side::Runner, 6))), ..hq(vec![wall.clone()]) });
        let rider = score(&getaway) - score(&plain);
        assert!((rider - 6.0 * w.own_credit_weight).abs() < 1e-9, "{rider}");
        let red_team = Effect::EffectIf {
            condition: EffectRequirement::ThisCardIsInstalled,
            effect: Box::new(Effect::Sequence(vec![Effect::RemoveCounters(Amount::Fixed(3)), Effect::GainCredits(Side::Runner, 3)])),
        };
        let team = with_run(&paid, RunState { on_success_effect: Some(Box::new(red_team)), ..hq(vec![wall.clone()]) });
        assert!((score(&team) - score(&plain) - 3.0 * w.own_credit_weight).abs() < 1e-9);
        assert_eq!(score(&with_run(&base, RunState { on_success_effect: Some(Box::new(Effect::GainCredits(Side::Runner, 6))), ..hq(vec![wall.clone()]) })), score(&broke), "no rider pays on a run that cannot get in");

        // Maintenance Access: a run on an empty Archives that becomes a
        // run on HQ is the HQ access.
        let archives = with_run(&base, RunState { server: ServerId::Archives, ..Default::default() });
        let redirected = with_run(&base, RunState { server: ServerId::Archives, redirect_on_approach: Some(ServerId::Hq), ..Default::default() });
        assert!((score(&redirected) - score(&archives) - w.active_run_weight).abs() < 1e-9);

        // Shred: a sentry nothing in the rig breaks stops the run, unless
        // the first "end the run" is held off — over a root with a card.
        let mut remote = base.clone();
        remote.corp.installed.push(InstalledCard { card: CardId("hq1".to_string()), install_id: InstallId(7), server: ServerId::Remote(0), slot: InstallSlot::Root, ..Default::default() });
        let prevention = LingeringEffect {
            what: Lingering::PreventRunEnding(EndRunPrevention::UnlessCorpTrashesRootCountFromHq),
            on: On::Player(Side::Corp),
            until: Until::EndOfRun,
            source: CardId("shred".to_string()),
        };
        let run = |ice: Vec<RunIce>| RunState { server: ServerId::Remote(0), ice, position: 0, ..Default::default() };
        // The prevention holds for the run, so it is read on a state
        // the run is on.
        let due = |state: &GameState, ice: Vec<RunIce>| {
            let state = with_run(state, run(ice));
            remaining_break_cost(&state, state.active_run.as_ref().unwrap(), &registry)
        };
        assert_eq!(due(&remote, vec![sentry.clone()]), None);
        let mut shredded = remote.clone();
        shredded.lingering = vec![prevention.clone()];
        assert_eq!(due(&shredded, vec![sentry.clone()]), Some(0), "the first is passed");
        assert_eq!(due(&shredded, vec![sentry.clone(), wall.clone()]), Some(1), "and the wall still costs its credit");
        assert_eq!(due(&shredded, vec![sentry.clone(), sentry.clone()]), None, "the second is not");
        let mut empty_root = base.clone();
        empty_root.lingering = vec![prevention];
        assert_eq!(due(&empty_root, vec![sentry.clone()]), None, "nothing to pay over an empty root, so the run ends");
        assert!(score(&with_run(&shredded, run(vec![sentry.clone()]))) > score(&with_run(&remote, run(vec![sentry]))));

        // Tread Lightly: 3[c] more on each rez the run forces.
        let mut taxed = base.clone();
        taxed.corp.resources.credits = Credits(20);
        let unrezzed = || vec![run_ice(3, IceType::Barrier, 1, false), run_ice(3, IceType::Barrier, 1, false)];
        assert_eq!(forced_rez_credits(&taxed, &hq(unrezzed())), 2 * TYPICAL_REZ_COST);
        taxed.lingering = vec![LingeringEffect { what: Lingering::RezCost(3), on: On::EachIce, until: Until::EndOfRun, source: CardId("tread_lightly".to_string()) }];
        let run = with_run(&taxed, hq(unrezzed()));
        assert_eq!(forced_rez_credits(&run, run.active_run.as_ref().unwrap()), 2 * (TYPICAL_REZ_COST + 3));
    }

    /// The Runner reads where the Corp's ICE stands off a run (§38): a
    /// server behind a piece no rig card breaks is shut, its breach
    /// subtracted whole; behind a piece the rig breaks it is shut by the
    /// share the break's price is of itself and a turn's clicks; and a
    /// remote with nothing in it shuts nothing worth having. So moving the
    /// code gate off R&D onto the empty remote, and the Ice Wall onto R&D,
    /// is worth the share of R&D's access it opens — and the Runner's
    /// credits move none of it.
    #[test]
    fn a_server_behind_ice_the_rig_cannot_break_is_a_shut_door() {
        use netrunner_core::rules::{InstallSlot, InstalledCard, ServerId};
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut state = GameState::new(0);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 10];
        state.corp.hq = vec![CardId("hedge_fund".to_string()); 2];
        let ice = |card: &str, id: u32, server| InstalledCard { card: CardId(card.to_string()), install_id: InstallId(id), server, slot: InstallSlot::Ice, rezzed: true, ..Default::default() };
        state.corp.installed = vec![ice("enigma", 2, ServerId::RnD), ice("ice_wall", 3, ServerId::Remote(1))];
        state.runner.resources.credits = Credits(8);
        state.runner.rig = vec![InstalledRunnerCard { card: CardId("corroder".to_string()), install_id: InstallId(4), base_strength: 2, ..Default::default() }];
        let w = Weights::default().at_the_guides_rate().with_plans(Side::Runner, &crate::plans::Style::BALANCED);
        let access = w.active_run_weight;
        assert!((shut_doors(&state, &registry, &w, 3) - access).abs() < 1e-9, "R&D is shut, the empty remote is worth nothing");
        let mut swapped = state.clone();
        swapped.corp.installed[0].server = ServerId::Remote(1);
        swapped.corp.installed[1].server = ServerId::RnD;
        // Corroder breaks Ice Wall's one subroutine for 1[c]: a fifth shut.
        assert!((shut_doors(&swapped, &registry, &w, 3) - access / 5.0).abs() < 1e-9);
        let gain = evaluate_state_with(&swapped, Side::Runner, &registry, &w) - evaluate_state_with(&state, Side::Runner, &registry, &w);
        assert!((gain - access * 0.8 * w.shut_door_weight).abs() < 1e-9, "{gain}: the swap is worth the share of the access it opens");
        let mut rich = state.clone();
        rich.runner.resources.credits = Credits(30);
        assert_eq!(shut_doors(&rich, &registry, &w, 3), shut_doors(&state, &registry, &w, 3), "credits open no door");
    }
}
