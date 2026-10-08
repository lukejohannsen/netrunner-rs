//! The Corp's arm: what its installs are worth (`corp_install_value`),
//! where its ICE stands (`fort_value`), the traps it holds and reveals,
//! and the run it is paying to stop.

use super::*;

/// What the Corp loses by having turned its own trap face up: whatever
/// the rez added to the card's value, taken back, and
/// `revealed_trap_weight` more. Zero for anything that is not a face-up
/// trap. See `REVEALED_TRAP_WEIGHT`.
pub(super) fn revealed_trap_cost(state: &GameState, installed: &InstalledCard, registry: &CardRegistry, w: &Weights, rig: [bool; 3], horizon: u32) -> f64 {
    if !installed.rezzed || !registry.get(&installed.card).is_some_and(punishes_access_with_damage) {
        return 0.0;
    }
    let face_down = InstalledCard { rezzed: false, ..installed.clone() };
    corp_install_value(state, installed, registry, w, rig, horizon) - corp_install_value(state, &face_down, registry, w, rig, horizon)
        + w.revealed_trap_weight
}

/// What one Corp install is worth, to the Corp and (rezzed, through
/// `visible_install_value`) to the Runner. `horizon` is the turns the
/// stage expects (`stage::horizon`), over which a rezzed economy card's
/// declared income is counted at `future_credit_weight` — only once it
/// is face up, because a face-down asset pays nothing until its rez,
/// and the card in hand was already priced for what the rez would buy
/// (`fundamentals::declared_value`).
pub(super) fn corp_install_value(state: &GameState, installed: &InstalledCard, registry: &CardRegistry, w: &Weights, rig: [bool; 3], horizon: u32) -> f64 {
    let def = registry.get(&installed.card);
    let is_ice = def.is_some_and(|d| matches!(d.card_type, CardType::Ice(_)));
    let mut value = if installed.rezzed
        && let Some(def) = def
        && def.card_type == CardType::Agenda
    {
        // A faceup agenda (BANGUN: When Disaster Strikes) is an install
        // whose text is not active — "this does not make their abilities
        // active" — and is worth what its access does to the Runner
        // (§36). It was priced as a rezzed asset, so every agenda went
        // faceup for the asset's weight, whatever the access would cost.
        w.unrezzed_install_weight + faceup_agenda_punishment(state, installed, def, registry, w)
    } else if installed.rezzed {
        w.board_presence_weight + if is_ice { w.rezzed_ice_weight } else { w.rezzed_asset_weight }
    } else {
        w.unrezzed_install_weight
    };
    if installed.rezzed
        && !is_ice
        && w.future_credit_weight != 0.0
        && let Some(def) = def
    {
        value += future_credits(&declared_income(def), Some(installed.counters), horizon) * w.future_credit_weight;
        value += turn_installs(state, def, registry, horizon) * w.future_credit_weight;
    }
    // What the rig cannot break is what holds — and only once it is face
    // up, which is the whole point: a term paid on the face-down card too
    // is present on both sides of the rez and cancels out of the decision
    // it exists to win. Measured that way first, and it moved nothing
    // (`RezIce` 1,073 → 1,053, Corp 31 → 29 of 192). See
    // `UNBREAKABLE_ICE_WEIGHT`.
    if installed.rezzed
        && let Some(def) = def
        && let CardType::Ice(subtype) = &def.card_type
    {
        // Ice with none of the three types is broken only by a breaker
        // with no restriction, which covers all three; a rig of three typed
        // breakers reads as breaking it too, which it cannot. Close enough
        // for one card in the pool, and exact for the AI it is meant for.
        let broken = subtype_slot(*subtype).map_or(rig.iter().all(|covered| *covered), |slot| rig[slot]);
        if !broken {
            value += w.unbreakable_ice_weight;
        }
        let etr = def.subroutines.iter().filter(|sub| sub.effect.can_end_the_run()).count();
        value += etr as f64 * w.etr_subroutine_weight;
    }
    // The requirement as the table stands (Ontological Dependence lowers
    // its own), never below 0: a token past it counts for nothing here.
    if let Some(required) = continuous::advancement_requirement(state, registry, installed.install_id).map(|required| required.max(0) as u32) {
        value += installed.advancement_tokens.min(required) as f64 * w.advancement_weight;
        // Past the requirement a token is worth what it will *become* at
        // score time — `dividends` agenda counters each — and nothing at
        // all on an agenda that pays none. Valuing the promise rather
        // than adding a second constant keeps the two halves of one
        // mechanic on one weight: the same token is counted here while
        // the agenda is installed and by `scored_agenda_counters` after,
        // each at what the agenda's own use of it buys (`agenda_counter`).
        let dividends = def.and_then(|d| d.dividends).unwrap_or(0);
        if dividends > 0
            && let Some(def) = def
        {
            let excess = installed.advancement_tokens.saturating_sub(required);
            value += f64::from(excess * dividends) * agenda_counter(state, registry, def, w);
        }
    }
    if let Some(def) = def {
        // An ambush's `advancement_requirement` is `Some(0)`, so the line
        // above counted every one of its tokens as nothing — see
        // `AMBUSH_ADVANCEMENT_WEIGHT`. Capped rather than open-ended
        // because nothing else stops the Corp advancing it.
        // **A sprung trap is sunk.** Once the Runner has seen it
        // (`InstalledCard::seen_by_runner`) they will not run it again, so
        // another token buys nothing and the click is better spent.
        if damage_grows_with_advancement(def) && !installed.seen_by_runner {
            let counted = installed.advancement_tokens.min(w.ambush_advancement_cap);
            value += f64::from(counted) * w.ambush_advancement_weight;
        }
        // Face-down and dangerous. Only while unrezzed: once it is turned
        // up it is a known quantity and the Runner simply stops running
        // at it.
        // Only a *lure* trap, and only while it is still a secret: a hand
        // trap installed in a remote is a Snare! nobody will run, and this
        // term paying for one is what put 147 of them on the table.
        if w.ambush_weight != 0.0 && !installed.rezzed && !installed.seen_by_runner && is_lure_trap(def) {
            value += w.ambush_weight;
        }
    }
    value
}

/// What the run in progress costs the Corp in `score`: the flat term,
/// what both identities print about its success, and — where the plans
/// price them — the stakes less the tax the Runner still pays to break
/// in. The run is the one `run_is_breakable` admits; `score` charges
/// each part where it always has, and this is their sum, for the one
/// reading that needs the whole (`answered_by_a_held_end_the_run`).
fn charged_run_threat(state: &GameState, run: &RunState, registry: &CardRegistry, w: &Weights) -> f64 {
    let pays = identities::run_success(state, registry, run);
    let mut threat = w.active_run_against_weight + f64::from(pays.runner_credits + pays.runner_cards) * w.opponent_credit_weight
        - f64::from(pays.corp_credits) * w.own_credit_weight
        - f64::from(pays.corp_cards) * w.click_weight;
    if w.run_stakes_weight != 0.0 {
        threat += run_stakes(state, run, registry) * w.run_stakes_weight
            - f64::from(remaining_break_cost(state, run, registry).unwrap_or(0)) * w.opponent_credit_weight;
    }
    threat
}

/// The part of what `run` costs the Corp in `score` that a paid
/// end-the-run it holds already answers — LEO Construction's "trash 1
/// rezzed bioroid card in the root of or protecting the attacked server:
/// end the run", Event Horizon's and M.I.C.'s "[trash]: end the run"
/// (Phase 5 §43). Zero when no such ability is usable. Otherwise, up to and
/// including the run's last window (CR 6.9.4e), whatever the run costs
/// beyond the cheapest ability's price less `HELD_END_THE_RUN_EDGE`; and
/// once the run is past that window, the flat term, so the run costs what
/// its breach reaches and no more.
///
/// **Why.** A run ended by paying for it gave back the whole run term
/// whatever the run would have reached, so the one-ply chooser paid the
/// moment it could: LEO Construction ended 20 of 28 runs at their
/// initiation, before the Runner had approached any ICE that might have
/// stopped them, half the time for less than a point of score and half
/// the time paying with Mercia B4LL4RD (Agency against Stolen Goods and
/// Tickets Please, seed 2, 48 planner games each). The ability is as
/// good at the last window, which always opens, so before it the run
/// costs the Corp no more than the price, and paying is worse than
/// waiting by the edge. At the last
/// window the choice is the price against the run going on, and a run
/// past it is one the Corp chose to let through: what it costs then is
/// what the breach takes — the stakes, what the identities print about
/// it — and not `active_run_against_weight`, which prices a run that may
/// yet be stopped and with it in the comparison the Corp paid 2.0 of
/// Mercia for an R&D access worth 0.7 (26 of 39 uses at the last window,
/// most of them on R&D and HQ, before this). So the Corp pays exactly when
/// the breach would take more than the card. Read off the abilities the
/// engine says are usable now (`ability_is_usable`), which is what they
/// will be at the last window unless the board changes on the way in.
pub(super) fn answered_by_a_held_end_the_run(state: &GameState, run: &RunState, registry: &CardRegistry, w: &Weights, rig: [bool; 3], horizon: u32) -> f64 {
    let Some(price) = held_end_the_run_price(state, registry, w, rig, horizon) else { return 0.0 };
    if before_the_breach(run) {
        (charged_run_threat(state, run, registry, w) - (price - HELD_END_THE_RUN_EDGE)).max(0.0)
    } else {
        w.active_run_against_weight
    }
}

/// Whether `run` has not yet gone past its last paid-ability window: every
/// phase up to the movement phase, whose last window (CR 6.9.4e) is the
/// one after the Runner goes on from the innermost position. The success
/// and the breach have none (`paid_ability`'s
/// `open_window_if_at_checkpoint`), so nothing ends the run there.
fn before_the_breach(run: &RunState) -> bool {
    match run.phase {
        RunPhase::Initiation | RunPhase::ApproachIce | RunPhase::EncounterIce | RunPhase::Movement => true,
        RunPhase::AccessingCard | RunPhase::Success | RunPhase::Ended => false,
    }
}

/// The least the Corp would give up to end a run by a paid ability it can
/// use now — its identity's, a rezzed install's or a scored agenda's, an
/// ability that is not an action, an access ability or an interrupt —
/// priced by `end_the_run_cost`. `None` when no such ability is usable, or
/// none has a cost this evaluator can price.
fn held_end_the_run_price(state: &GameState, registry: &CardRegistry, w: &Weights, rig: [bool; 3], horizon: u32) -> Option<f64> {
    use netrunner_core::rules::{ability_is_usable, InstallId};
    let identity = state.corp.identity.iter().map(|card| (InstallId::CORP_IDENTITY, card));
    let installed = state.corp.installed.iter().filter(|card| card.rezzed).map(|card| (card.install_id, &card.card));
    let scored = state.corp.scored_agendas.iter().filter(|scored| scored.as_agenda.is_none()).map(|scored| (scored.install_id, &scored.card));
    identity
        .chain(installed)
        .chain(scored)
        .filter_map(|(install, card)| registry.get(card).map(|def| (install, card, def)))
        .flat_map(|(install, card, def)| def.abilities.iter().enumerate().map(move |(index, ability)| (install, card, index, ability)))
        .filter(|(_, _, _, ability)| {
            ability.trigger == Trigger::Paid && !ability.is_action() && !ability.access && ability.effect.prevents().is_none() && ability.effect.can_end_the_run()
        })
        .filter(|(install, card, index, _)| ability_is_usable(state, registry, Side::Corp, *install, card, *index))
        .filter_map(|(install, card, _, ability)| end_the_run_cost(state, registry, w, rig, horizon, install, card, ability.cost.as_ref()))
        .min_by(f64::total_cmp)
}

/// What paying `cost` takes from the Corp, in the evaluator's own terms:
/// credits at `own_credit_weight`, and a card it trashes at what the card
/// is worth installed (`corp_install_value`) — the source itself for
/// "[trash]:", the cheapest of the cards the engine would let it choose
/// for "trash 1 rezzed bioroid" (`eligible_positions`, the selection's own
/// question). `None` for a cost no reading prices, which then answers
/// nothing of the run.
#[allow(clippy::too_many_arguments)]
fn end_the_run_cost(
    state: &GameState,
    registry: &CardRegistry,
    w: &Weights,
    rig: [bool; 3],
    horizon: u32,
    install: netrunner_core::rules::InstallId,
    card: &netrunner_core::dsl::CardId,
    cost: Option<&Cost>,
) -> Option<f64> {
    let value = |installed: &InstalledCard| corp_install_value(state, installed, registry, w, rig, horizon);
    match cost {
        None => Some(0.0),
        Some(Cost::Credits(n)) => Some(f64::from(*n) * w.own_credit_weight),
        Some(Cost::TrashSelf) => state.find_corp_install(install).map(value),
        Some(Cost::Trash { from: from @ CardZoneRef::OwnInstalled, filter, count, .. }) => {
            let mut values: Vec<f64> = netrunner_core::rules::eligible_positions(state, registry, Side::Corp, from, filter, Some(install), Some(card))
                .into_iter()
                .filter_map(|position| state.corp.installed.get(position))
                .map(value)
                .collect();
            values.sort_by(f64::total_cmp);
            (values.len() >= *count as usize).then(|| values.iter().take(*count as usize).sum())
        }
        Some(Cost::AllOf(parts)) => parts.iter().map(|part| end_the_run_cost(state, registry, w, rig, horizon, install, card, Some(part))).sum(),
        Some(_) => None,
    }
}

/// Agenda counters sitting on the Corp's scored agendas — Dividends,
/// spent by the agendas' own discard-phase triggers — each at what its
/// agenda's use of it buys (`agenda_counter`).
///
/// Without this the term above would evaporate at the moment it paid
/// off: a search comparing "score now" with "advance once more, then
/// score" sees the installed agenda gone in both branches, so unless the
/// counters survive into the score area the two branches are worth the
/// same and the extra click is pure cost.
pub(super) fn scored_agenda_counters(state: &GameState, registry: &CardRegistry, w: &Weights) -> f64 {
    state
        .corp
        .scored_agendas
        .iter()
        .filter(|scored| scored.agenda_counters > 0)
        .map(|scored| f64::from(scored.agenda_counters) * registry.get(&scored.card).map_or(w.agenda_counter_weight, |def| agenda_counter(state, registry, def, w)))
        .sum()
}

/// What the Runner's programs hosted on the Corp's ice cost it (Phase 5
/// §55): a trojan that derezzes its host on a count of its own counters
/// (`read::host_derez`, Tranquilizer) is the host's rez cost, at
/// `own_credit_weight`, for every turn the count is ripe within the
/// length of one count — the tax of keeping the piece up until the
/// Corp's next purge, which is the Corp's own decision and read no
/// further. Not the Runner's reading (`host_derez_value`: the rez a turn
/// over the horizon at the income discount), on purpose: at that rate a
/// purge off a piece already derezzed tied three credits, and in play the
/// Corp sat under a ripe trojan on 63 of its turns in 48 games, purged on
/// none of them, and had its ice derezzed 87 times. Read on a rezzed host and an
/// unrezzed one alike: the Corp knows its own ice's cost, and the piece
/// already derezzed by a ripe trojan is the one it must not pay for
/// again. So a purge (three clicks, every counter gone) is worth the
/// turns of tax it ends, dear against an expensive host and not against
/// a 1[c] one; an install over the host (CR 8.5.6b, the trojan trashed
/// with it at the checkpoint, 10.3.1g) is worth the tax gone; and a
/// trojan one counter short on a rezzed piece needs no term at all,
/// since the line ends in the Runner's start of turn where the derez
/// fires, and the planner purged there already.
pub(super) fn trojans_on_ice(state: &GameState, registry: &CardRegistry, w: &Weights) -> f64 {
    state
        .runner
        .rig
        .iter()
        .filter_map(|program| {
            let host = state.corp.installed.iter().find(|ice| Some(ice.install_id) == program.hosted_on_ice)?;
            let derez = read::host_derez(registry.get(&program.card)?)?;
            let delay = derez.at.saturating_sub(program.counters).div_ceil(derez.per_turn.max(1));
            Some(f64::from(registry.get(&host.card)?.cost) * w.own_credit_weight * f64::from(derez.at.saturating_sub(delay)))
        })
        .sum()
}

/// What one agenda counter on `def` is worth: what the agenda's own use of
/// it buys, at `identities::counter_worth`'s share (Phase 5 §37) — Off the
/// Books' search installed free, Sericulture Expansion's two advancement
/// counters, Project Ingatan's install from Archives — or
/// `agenda_counter_weight` for an agenda whose use no reading prices
/// (Embedded Reporting's operation set on R&D, Proprionegation's moved run).
/// At the flat weight Off the Books held its counter for good once the
/// planner judged the offer (§35): 2.0 against a search worth less.
fn agenda_counter(state: &GameState, registry: &CardRegistry, def: &CardDefinition, w: &Weights) -> f64 {
    identities::counter_worth(state, registry, def, w).unwrap_or(w.agenda_counter_weight)
}

/// What the counters on the Corp's identity are worth to it held: AU Co.'s
/// and Epiphany Analytica's at what spending them buys (`identities::
/// counter_worth`), and nothing for an identity whose counters no use
/// reads — Issuaq Adaptics' are points, and counted as points
/// (`evaluate_state_with`), NBN: Making News' are recurring credits for a
/// trace.
fn identity_counters(state: &GameState, registry: &CardRegistry, w: &Weights) -> f64 {
    if state.corp.identity_counters == 0 {
        return 0.0;
    }
    let Some(identity) = state.corp.identity.as_ref().and_then(|id| registry.get(id)) else { return 0.0 };
    f64::from(state.corp.identity_counters) * identities::counter_worth(state, registry, identity, w).unwrap_or(0.0)
}

/// What the Corp's ready agendas are worth held for a later score that
/// places counters on its identity a score now would not (Phase 5 §37):
/// Issuaq Adaptics places a power counter, a point, for an agenda "that you
/// did not install or advance this turn", so an agenda advanced to its
/// requirement this turn is worth its points and the counters' a turn
/// later, where the server it sits in would hold the Runner's next turn
/// out (`holds_a_turn`), at `LATER_SCORE_SHARE`. Read in every state the
/// agenda is ready in where a score now would place fewer — none outside
/// the Corp's action phase, where it cannot score at all: in the Corp's
/// turn it is the line that keeps the agenda against the one that scores
/// it (a line ends past the action phase, in the Runner's start of turn),
/// and in the Runner's it is what the Corp defends. Nothing in the Corp's
/// next action phase, where a score now places the counters and is the
/// line to take.
fn held_for_a_later_score(state: &GameState, registry: &CardRegistry, w: &Weights) -> f64 {
    let per = identities::points_per_identity_counter(state, registry);
    if per <= 0 {
        return 0.0;
    }
    let corps_turn = state.phase == GamePhase::Action(Side::Corp);
    let mut value = 0.0;
    for installed in state.corp.installed.iter().filter(|card| card.slot == netrunner_core::rules::InstallSlot::Root) {
        let Some(def) = registry.get(&installed.card).filter(|def| def.card_type == CardType::Agenda) else { continue };
        let Some(required) = continuous::advancement_requirement(state, registry, installed.install_id) else { continue };
        if (installed.advancement_tokens as i32) < required.max(0) {
            continue;
        }
        let later = identities::counters_on_score(state, registry, installed, def, true);
        let now = if corps_turn { identities::counters_on_score(state, registry, installed, def, false) } else { 0 };
        if later > now && holds_a_turn(state, installed.server, registry) {
            value += f64::from(def.agenda_points.unwrap_or(0) as i32 + later as i32 * per) * w.agenda_point_weight * LATER_SCORE_SHARE;
        }
    }
    value
}

/// The share of a later score a ready agenda held for it is worth. Any
/// share in (3/4, 1) makes the same choices over the pool's agendas: below
/// 1, so a held agenda never ties the score that would place the counter
/// now; above `P / (P + 1)` for the three-pointers, so holding one for a
/// counter's point beats scoring it bare. The middle of that range, the
/// tenth off standing for what `holds_a_turn` cannot see — a run event, a
/// breaker drawn and installed.
const LATER_SCORE_SHARE: f64 = 0.9;

/// Whether `server` would keep the Runner out for a turn: some piece of
/// its ICE the rig cannot break, or a break it cannot pay for out of what
/// it holds and a turn's clicks taken as credits (`taxing_cost`, the
/// Corp's own reading, with the rezzes its credits cover). A server with
/// no ICE holds nothing.
fn holds_a_turn(state: &GameState, server: netrunner_core::rules::ServerId, registry: &CardRegistry) -> bool {
    let next_turn = state.runner.resources.credits.0 + RUNNER_TURN_CLICKS;
    taxing_cost(state, server, registry).is_none_or(|cost| cost > next_turn)
}

/// The clicks a Runner's turn holds, each a credit at the guide's rate.
pub(super) const RUNNER_TURN_CLICKS: u32 = 4;

/// The agenda points in Archives, faceup or not — what
/// `archived_agenda_weight` charges the Corp.
pub(super) fn archived_agenda_points(state: &GameState, registry: &CardRegistry) -> u32 {
    state.corp.archives.iter().filter_map(|card| registry.get(&card.card)).filter_map(|def| def.agenda_points).sum()
}

/// Installed, unscored agendas — what `installed_agenda_weight` counts.
pub(super) fn installed_agendas(state: &GameState, registry: &CardRegistry) -> usize {
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
/// Where the Corp's ICE stands, priced — the three terms that made
/// `Glacier` build the fort it is named for (ROADMAP Phase 5 §19), and
/// every Corp profile since §23 (`CENTRAL_ICE_WEIGHT`).
///
/// **Why they exist.** A report from play: the `glacier` Corp "makes ICE
/// for days horizontally across as many servers as it can without ever
/// once putting down an agenda". `diag fort` confirmed the shape —
/// 4.3 remotes opened a game with about one piece each, under one piece
/// on each central, a quarter of its agendas installed behind no ICE at
/// all and only 16% of games with all three centrals iced when the first
/// agenda went down — and the reason is that no term in this evaluator
/// knew *which* server a piece of ICE was on. `corp_install_value` prices
/// a piece the same everywhere, and `protected_agenda_ice` counts ICE only
/// in front of an agenda that is already installed, so where ICE went was
/// whichever tie broke first, and a naked agenda install (+
/// `unrezzed_install_weight`) outbid the profile's own credit click.
///
/// - **Centrals**, `central_ice_weight` a piece up to `central_ice_cap` on
///   HQ and R&D, and one on Archives: the servers every Runner can run
///   from the first turn, and the ones a fort-builder covers first.
///   Archives is worth one piece — what is in it is mostly faceup and
///   already the Runner's to see.
/// - **The scoring remote**, `fort_weight` a piece up to `fort_cap` on the
///   deepest remote whose root is empty or holds an agenda (and any
///   upgrades: `fort_remote`). It is priced
///   before the agenda exists, which is what `protected_agenda_ice`
///   cannot do, and it stays priced once the agenda goes in and after it
///   is scored, so the same remote is used again. ICE in front of an
///   asset earns neither term: the spreading stops for a reason.
/// - **An exposed agenda**, `exposed_agenda_weight` subtracted for each
///   piece short of `fort_cap` in front of an installed agenda, so a
///   naked install loses to a credit click and the agenda waits in HQ for
///   the fort rather than going down in a new remote on turn 1.
///
/// Only ICE positions and the Corp's own installed cards are read, which
/// the Corp always knows, so nothing here reads a sampled card.
pub(super) fn fort_value(state: &GameState, registry: &CardRegistry, w: &Weights) -> f64 {
    use netrunner_core::rules::{InstallSlot, ServerId};
    let ice_on = |server: ServerId| {
        state.corp.installed.iter().filter(|card| card.server == server && card.slot == InstallSlot::Ice).count()
    };

    let centrals = ice_on(ServerId::Hq).min(w.central_ice_cap)
        + ice_on(ServerId::RnD).min(w.central_ice_cap)
        + ice_on(ServerId::Archives).min(1);

    let fort = fort_remote(state, registry, w).map_or(0, |server| ice_on(server).min(w.fort_cap));

    let exposure = exposure(state, registry, w);

    centrals as f64 * w.central_ice_weight + fort as f64 * w.fort_weight - exposure as f64 * w.exposed_agenda_weight
}

/// The pieces of ICE short of `fort_cap` in front of each installed
/// agenda, summed: `fort_value`'s exposure, and all of it that stays once
/// the Runner has beaten the wall (`score`, §45).
pub(super) fn exposure(state: &GameState, registry: &CardRegistry, w: &Weights) -> usize {
    use netrunner_core::rules::InstallSlot;
    let ice_on = |server: netrunner_core::rules::ServerId| {
        state.corp.installed.iter().filter(|card| card.server == server && card.slot == InstallSlot::Ice).count()
    };
    state
        .corp
        .installed
        .iter()
        .filter(|card| card.slot == InstallSlot::Root && registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Agenda))
        .map(|agenda| w.fort_cap.saturating_sub(ice_on(agenda.server)))
        .sum()
}

/// The scoring remote `fort_value` prices: the deepest remote whose root
/// is empty or holds only what the fort is for. `None` with no remote
/// at all. **An upgrade is what a fort's root holds beside its agenda**
/// (§45): it protects the server and takes no agenda's place, so a root
/// holding Mercia B4LL4RD and Project Ingatan is the fort. Read as "not
/// only what the fort is for", it was no fort, and LEO Construction's
/// trash of the Mercia read as building one (+3.0, two pieces at
/// `fort_weight`): 8 of LEO's 15 uses in 96 Agency games were a Mercia
/// out of the root of the agenda's own remote. A trap the Runner has not seen is an agenda as far as the
/// fort is concerned — that is the bluff, and the Corp is the one player
/// who knows the difference; a trap already sprung is not: the remote is
/// spent, and going on icing it protects nothing. **Under the traps plan
/// a face-down card that is no agenda is a fort root too**
/// (`BLUFF_WEIGHT`): the asset the guide puts in the scoring server now
/// and then keeps the fort the fort, so it goes down behind the wall
/// rather than beside it, and stays face down there — a rezzed one is a
/// known asset and the remote stops being the fort until the agenda goes
/// in over it.
pub(super) fn fort_remote(state: &GameState, registry: &CardRegistry, w: &Weights) -> Option<netrunner_core::rules::ServerId> {
    use netrunner_core::rules::{InstallSlot, ServerId};
    let ice_on = |server: ServerId| {
        state.corp.installed.iter().filter(|card| card.server == server && card.slot == InstallSlot::Ice).count()
    };
    let is_agenda = |card: &InstalledCard| registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Agenda);
    let is_fort_root = |card: &InstalledCard| {
        is_agenda(card)
            || registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Upgrade)
            || (!card.seen_by_runner && registry.get(&card.card).is_some_and(is_lure_trap))
            || (w.bluff_weight != 0.0 && !card.rezzed && !card.seen_by_runner)
    };
    let mut remotes: Vec<ServerId> = Vec::new();
    for card in &state.corp.installed {
        if matches!(card.server, ServerId::Remote(_)) && !remotes.contains(&card.server) {
            remotes.push(card.server);
        }
    }
    remotes
        .into_iter()
        .filter(|server| {
            state.corp.installed.iter().filter(|card| card.server == *server && card.slot == InstallSlot::Root).all(is_fort_root)
        })
        .max_by_key(|server| ice_on(*server))
}

/// Installed, unscored agendas the Corp could finish next turn, and
/// those it could not (`read::next_turn_advancements` against what each
/// still needs). See `NEVER_ADVANCE_WEIGHT`.
pub(super) fn finishable_agendas(state: &GameState, registry: &CardRegistry) -> (usize, usize) {
    use netrunner_core::rules::InstallSlot;
    let reach = next_turn_advancements(state, registry);
    let mut can = 0;
    let mut cannot = 0;
    for card in state.corp.installed.iter().filter(|card| card.slot == InstallSlot::Root) {
        if !registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Agenda) {
            continue;
        }
        let Some(required) = continuous::advancement_requirement(state, registry, card.install_id).map(|required| required.max(0) as u32) else { continue };
        if required.saturating_sub(card.advancement_tokens) <= reach {
            can += 1;
        } else {
            cannot += 1;
        }
    }
    (can, cannot)
}

/// A face-down card that is no agenda in the fort remote — the bluff
/// the traps plan pays for, at most one. See `BLUFF_WEIGHT`.
pub(super) fn bluffed_root(state: &GameState, registry: &CardRegistry, w: &Weights) -> bool {
    use netrunner_core::rules::InstallSlot;
    fort_remote(state, registry, w).is_some_and(|fort| {
        state.corp.installed.iter().any(|card| {
            card.server == fort
                && card.slot == InstallSlot::Root
                && !card.rezzed
                && !card.seen_by_runner
                && registry.get(&card.card).is_some_and(|def| def.card_type != CardType::Agenda && !is_lure_trap(def))
        })
    })
}

/// Hand traps waiting in HQ, where they do their work. See
/// `HELD_TRAP_WEIGHT`.
pub(super) fn held_traps(state: &GameState, registry: &CardRegistry) -> usize {
    state.corp.hq.iter().filter(|card| registry.get(card).is_some_and(is_hand_trap)).count()
}

pub(super) fn protected_agenda_ice(state: &GameState, registry: &CardRegistry, cap: usize) -> usize {
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

/// What the Corp's identity does to the Runner for accessing `agenda`
/// faceup, at the rates the Corp reads the Runner's grip and tags by:
/// BANGUN's "do 2 meat damage and give the Runner 1 tag" is the grip it
/// takes below `opponent_grip_floor`, the hand size core damage would take,
/// and a tag the Corp holds a card to punish (`tag_leverage_weight`). Damage past the grip is counted to the grip's end: the
/// Runner reads that access as its flatline (§34) and stays out until it
/// has drawn, which is the agenda kept and no more. Zero for a Runner the
/// access cannot hurt, where the faceup agenda is the facedown one shown.
fn faceup_agenda_punishment(state: &GameState, installed: &InstalledCard, agenda: &CardDefinition, registry: &CardRegistry, w: &Weights) -> f64 {
    let pays = identities::on_access(state, registry, installed.server, agenda, installed);
    let grip = state.runner.grip.len();
    let shortfall = |held: usize| w.opponent_grip_floor.saturating_sub(held) as f64;
    let taken = (shortfall(grip.saturating_sub(pays.damage as usize)) - shortfall(grip)) * w.opponent_grip_shortfall_weight;
    let tagged = if pays.tags > 0 && holds_tag_punishment(state, registry) { f64::from(pays.tags.min(2)) * w.tag_leverage_weight } else { 0.0 };
    taken + f64::from(pays.core_damage) * w.core_damage_weight + tagged
}

/// The Corp's terms, added to `score` in the order `evaluate_state_with`
/// always added them — a sum's rounding follows its order, and this
/// module split is byte-identical to the one file it replaces.
pub(super) fn score(state: &GameState, registry: &CardRegistry, w: &Weights, horizon: u32, score: &mut f64) {
    *score -= state.corp.bad_publicity as f64 * w.bad_publicity_weight;
    // Computed once and handed down rather than read per install:
    // the rig does not change between two cards on the same board,
    // and `corp_install_value` is called for every one of them.
    let rig = rig_coverage(state, registry);
    for installed in &state.corp.installed {
        *score += corp_install_value(state, installed, registry, w, rig, horizon);
        *score -= revealed_trap_cost(state, installed, registry, w, rig, horizon);
    }
    *score -= trojans_on_ice(state, registry, w);
    *score += scored_agenda_counters(state, registry, w);
    *score += identity_counters(state, registry, w);
    *score += held_for_a_later_score(state, registry, w);
    *score -= f64::from(archived_agenda_points(state, registry)) * w.archived_agenda_weight;
    *score += protected_agenda_ice(state, registry, w.agenda_protection_cap) as f64 * w.agenda_protection_weight;
    if w.installed_agenda_weight != 0.0 {
        *score += installed_agendas(state, registry) as f64 * w.installed_agenda_weight;
    }
    // "Glacier, then fast advance": the fort terms fall away once the
    // Runner's rig beats the wall (`fort_until_beaten`, Stage 6), so the
    // last points are scored from hand where that rig is no use.
    //
    // Read between runs (§43, `between_runs`).
    let between = std::cell::OnceCell::new();
    let board = || between.get_or_init(|| between_runs(state));
    //
    // **What falls away is the fort's worth, not an agenda's exposure**
    // (§45): an agenda short of the wall is no safer for the Runner having
    // beaten it. With the whole of `fort_value` switched off, a fort whose
    // terms were net negative — an exposed agenda outweighing the ICE —
    // read beating it as a gain, and the Corp's trash of its own Brân 1.0
    // once scored +3.5.
    let fort_holds = !w.fort_until_beaten || !fort_beaten(board(), fort_remote(board(), registry, w), registry);
    if w.central_ice_weight != 0.0 || w.fort_weight != 0.0 || w.exposed_agenda_weight != 0.0 {
        if fort_holds {
            *score += fort_value(state, registry, w);
        } else {
            *score -= exposure(state, registry, w) as f64 * w.exposed_agenda_weight;
        }
    }
    if w.held_trap_weight != 0.0 {
        *score += held_traps(state, registry) as f64 * w.held_trap_weight;
    }
    *score += w.opponent_grip_floor.saturating_sub(state.runner.grip.len()) as f64 * w.opponent_grip_shortfall_weight;
    *score += state.runner.brain_damage as f64 * w.core_damage_weight;
    if state.corp.r_and_d.len() >= w.rd_draw_reserve {
        *score -= w.hq_floor.saturating_sub(state.corp.hq.len()) as f64 * w.hq_shortfall_weight;
    }
    // The mirror of the Runner's `active_run_weight`, on the same
    // `run_is_breakable` gate. Jacking out or ending the run
    // returns it, which is what makes `EndTheRun` priceable in
    // `continuation_upside`.
    if let Some(run) = &state.active_run
        && run_is_breakable(state, run, registry)
    {
        *score -= w.active_run_against_weight;
        // And what both identities print about its success (§34), read as
        // the Runner reads it: the credits and cards a Gabriel Santiago or
        // a Zahya Sadeghi takes are the Runner's, which ending the run
        // denies, and what the Corp's own identity takes is the Corp's.
        let pays = identities::run_success(state, registry, run);
        *score -= f64::from(pays.runner_credits + pays.runner_cards) * w.opponent_credit_weight;
        *score += f64::from(pays.corp_credits) * w.own_credit_weight + f64::from(pays.corp_cards) * w.click_weight;
    }
    // The guide's-rate terms, after everything above (see
    // `evaluate_state_with`).
    if w.rez_reserve_weight != 0.0 {
        let short = rez_reserve(state, registry).saturating_sub(state.corp.resources.credits.0);
        *score -= f64::from(short) * w.rez_reserve_weight;
    }
    if w.unaffordable_ice_weight != 0.0 {
        *score -= unaffordable_ice(state, registry) as f64 * w.unaffordable_ice_weight;
    }
    if w.taxing_window_weight != 0.0 {
        *score += agendas_under_the_window(board(), registry) as f64 * w.taxing_window_weight;
    }
    // The plans' terms (Stage 6), after the guide's rate.
    if w.run_stakes_weight != 0.0
        && let Some(run) = &state.active_run
        && run_is_breakable(state, run, registry)
    {
        *score -= run_stakes(state, run, registry) * w.run_stakes_weight;
        *score += f64::from(remaining_break_cost(state, run, registry).unwrap_or(0)) * w.opponent_credit_weight;
    }
    // What a paid end-the-run held for a later window already answers of
    // the run (§43), given back after the run's terms above rather than
    // folded into them, so a board with no such ability scores to the bit
    // what it did.
    if let Some(run) = &state.active_run
        && run_is_breakable(state, run, registry)
    {
        *score += answered_by_a_held_end_the_run(state, run, registry, w, rig, horizon);
    }
    if w.rez_held_weight != 0.0 {
        *score += held_rezzes(state, registry) as f64 * w.rez_held_weight;
    }
    if w.ice_order_weight != 0.0 {
        *score -= ice_out_of_order(state, registry) as f64 * w.ice_order_weight;
    }
    if w.never_advance_weight != 0.0 {
        let (can, cannot) = finishable_agendas(state, registry);
        *score += (can as f64 - cannot as f64) * w.never_advance_weight;
    }
    if w.lethal_threat_weight != 0.0 && damage_in_reach(state, registry) as usize > state.runner.grip.len() {
        *score += w.lethal_threat_weight;
    }
    if w.tag_leverage_weight != 0.0 && state.runner.tags > 0 && holds_tag_punishment(state, registry) {
        *score += f64::from(state.runner.tags.min(2)) * w.tag_leverage_weight;
    }
    if w.bluff_weight != 0.0 && bluffed_root(state, registry, w) {
        *score += w.bluff_weight;
    }
}

/// `state` as it stands between runs: the run taken off the board, and
/// with it what the run lends the rig — a breaker pumped for the
/// encounter, whose lingering effect no longer holds once there is no
/// run (`LingeringEffect::holds` asks the state), and the credits a run
/// event brought, which live on the run (Phase 5 §43). What the two
/// standing readings of the Runner's reach ask — whether the rig beats
/// the fort (`fort_beaten`), whether it can afford a server with an
/// agenda in it (`agendas_under_the_window`) — is asked of this board,
/// because they are about the Runner's next run and not the one under
/// way.
///
/// **Why.** Read during a run, both said yes for exactly as long as the
/// run went on, so ending it read as rebuilding the fort: at one of LEO
/// Construction's offers, in an Overclock run with Carmen pumped +3
/// against Ansel 1.0, the fort terms were off (13.0 of score) until the
/// run ended, and the Corp traded Ansel for the run with nothing on the
/// server worth that.
fn between_runs(state: &GameState) -> std::borrow::Cow<'_, GameState> {
    if state.active_run.is_none() {
        return std::borrow::Cow::Borrowed(state);
    }
    std::borrow::Cow::Owned(GameState { active_run: None, ..state.clone() })
}

/// Installed, unscored agendas in a server the Runner cannot afford to
/// break into right now, were the Corp to rez what its credits cover
/// (`read::taxing_cost`, `None` — a piece no rig card breaks — counted
/// as shut to the Runner). See `TAXING_WINDOW_WEIGHT`.
pub(super) fn agendas_under_the_window(state: &GameState, registry: &CardRegistry) -> usize {
    use netrunner_core::rules::InstallSlot;
    let runner = state.runner.resources.credits.0;
    state
        .corp
        .installed
        .iter()
        .filter(|card| card.slot == InstallSlot::Root)
        .filter(|card| registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Agenda))
        .filter(|agenda| taxing_cost(state, agenda.server, registry).is_none_or(|cost| cost > runner))
        .count()
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A trojan on the Corp's ice costs it the host's rez for every turn
    /// its count is ripe within one count's length (Phase 5 §55):
    /// Tranquilizer at one counter on Brân 1.0 is one turn of tax, at
    /// three it is three; a purge (every counter gone) ends it, and
    /// against Brân the three clicks pay; derezzed by it, the piece costs
    /// the same as rezzed; a Whitespace host is its share of a Brân.
    #[test]
    fn a_trojan_on_the_corps_ice_costs_the_hosts_rez_a_turn() {
        use netrunner_core::rules::{InstallSlot, InstalledCard, InstalledRunnerCard, ServerId};
        let mut pool = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut pool);
        let w = Weights::default().at_the_guides_rate();
        let mut state = GameState::new(0);
        state.corp.installed = vec![InstalledCard { card: CardId("bran_1_0".to_string()), install_id: InstallId(1), server: ServerId::Hq, slot: InstallSlot::Ice, rezzed: true, ..Default::default() }];
        assert_eq!(trojans_on_ice(&state, &pool, &w), 0.0, "nothing hosted");
        state.runner.rig.push(InstalledRunnerCard { card: CardId("tranquilizer".to_string()), install_id: InstallId(10), hosted_on_ice: Some(InstallId(1)), counters: 1, ..Default::default() });
        let bran = f64::from(pool.get(&CardId("bran_1_0".to_string())).expect("Brân 1.0").cost);
        let one = trojans_on_ice(&state, &pool, &w);
        assert!((one - bran * w.own_credit_weight).abs() < 1e-9, "one turn of tax: {one}");
        state.runner.rig[0].counters = 3;
        let ripe = trojans_on_ice(&state, &pool, &w);
        assert!((ripe - 3.0 * one).abs() < 1e-9, "three turns of tax: {ripe}");
        state.runner.rig[0].counters = 0;
        assert_eq!(trojans_on_ice(&state, &pool, &w), 0.0, "a purge ends it");
        assert!(ripe > 3.0 * w.click_weight, "against Brân the purge's three clicks pay: {ripe}");
        state.runner.rig[0].counters = 3;
        state.corp.installed[0].rezzed = false;
        assert_eq!(trojans_on_ice(&state, &pool, &w), ripe, "derezzed by it, the piece costs what it would to put back");
        state.corp.installed[0].card = CardId("whitespace".to_string());
        let whitespace = f64::from(pool.get(&CardId("whitespace".to_string())).expect("Whitespace").cost);
        assert!((trojans_on_ice(&state, &pool, &w) - ripe * whitespace / bran).abs() < 1e-9);
    }
    use crate::eval::test_support::*;
    use crate::plans::{Plan, Style};
    use netrunner_core::dsl::{CardId, DamageType, Trigger, TriggeredEffect};
    use netrunner_core::rules::{Credits, GameState, InstallId, InstalledRunnerCard};

    /// An agenda advanced to its requirement this turn under Issuaq
    /// Adaptics is worth its points and the counter's a turn later, held
    /// behind ICE the rig cannot break (§37) — and nothing more where the
    /// wall would not hold, or where a score now places the counter.
    #[test]
    fn a_ready_agenda_is_held_for_issuaqs_counter_behind_a_wall_that_holds() {
        use netrunner_core::rules::{GameEvent, GamePhase, InstallSlot, ServerId};
        let pool = pool();
        let w = every_corp();
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.identity = Some(CardId("issuaq_adaptics_sustaining_diversity".to_string()));
        let agenda = printed(&pool, "offworld_office");
        let required = agenda.advancement_requirement.expect("an agenda");
        state.corp.installed = vec![
            InstalledCard { card: agenda.id.clone(), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Root, advancement_tokens: required, ..Default::default() },
            InstalledCard { card: CardId("ice_wall".to_string()), install_id: InstallId(2), server: ServerId::Remote(0), slot: InstallSlot::Ice, rezzed: true, ..Default::default() },
        ];
        let later = f64::from(agenda.agenda_points.unwrap() + 1) * w.agenda_point_weight * LATER_SCORE_SHARE;
        assert_eq!(held_for_a_later_score(&state, &pool, &w), 0.0, "advanced on an earlier turn: a score now places the counter");
        let advance = GameEvent::CardAdvanced { install: InstallId(1), card: Some(agenda.id.clone()), advancement_tokens: required };
        netrunner_core::rules::dispatch_event(&mut state, &pool, &advance).expect("the agenda is advanced");
        assert_eq!(held_for_a_later_score(&state, &pool, &w), later, "advanced this turn: held for the counter");
        assert!(later > f64::from(agenda.agenda_points.unwrap()) * w.agenda_point_weight, "and worth more than the score now");
        state.phase = GamePhase::Action(Side::Runner);
        assert_eq!(held_for_a_later_score(&state, &pool, &w), later, "in the Runner's turn, what the Corp defends");
        state.runner.rig = vec![rig_card("corroder")];
        state.runner.resources.credits = Credits(10);
        assert_eq!(held_for_a_later_score(&state, &pool, &w), 0.0, "a wall the Runner breaks holds nothing");
        state.runner.rig.clear();
        state.corp.identity = None;
        assert_eq!(held_for_a_later_score(&state, &pool, &w), 0.0, "no identity counts a later score");
    }

    /// Issuaq Adaptics' counters are points, on both chairs (§37), and the
    /// stage reads them in what the Corp needs to win.
    #[test]
    fn issuaqs_counters_are_points_on_both_chairs() {
        use crate::eval::stage::{stage, Stage};
        let pool = pool();
        let w = every_corp();
        let mut state = GameState::new(0);
        state.corp.identity = Some(CardId("issuaq_adaptics_sustaining_diversity".to_string()));
        let before = (evaluate_state_with(&state, Side::Corp, &pool, &w), evaluate_state_with(&state, Side::Runner, &pool, &w));
        state.corp.identity_counters = 2;
        let after = (evaluate_state_with(&state, Side::Corp, &pool, &w), evaluate_state_with(&state, Side::Runner, &pool, &w));
        assert!((after.0 - before.0 - 2.0 * w.agenda_point_weight).abs() < 1e-9, "{before:?} → {after:?}");
        assert!((after.1 - before.1 + 2.0 * w.agenda_point_weight).abs() < 1e-9, "{before:?} → {after:?}");
        state.corp.resources.agenda_points = netrunner_core::rules::AgendaPoints(3);
        assert_eq!(stage(&state, &pool), Stage::Late, "three points and two counters is two from seven");
        state.corp.identity_counters = 0;
        assert_eq!(stage(&state, &pool), Stage::Early);
    }

    /// An agenda in Archives is half its points to the Corp (§37): one it
    /// cannot score, and one an Archives run steals.
    #[test]
    fn an_agenda_in_archives_costs_the_corp_half_its_points() {
        use netrunner_core::rules::ArchivedCard;
        let pool = pool();
        let w = every_corp();
        let mut state = GameState::new(0);
        state.corp.archives = vec![ArchivedCard::faceup(CardId("hedge_fund".to_string()))];
        let kept = evaluate_state_with(&state, Side::Corp, &pool, &w);
        state.corp.archives.push(ArchivedCard { card: CardId("offworld_office".to_string()), facedown: true });
        let lost = evaluate_state_with(&state, Side::Corp, &pool, &w);
        assert!((kept - lost - 2.0 * w.archived_agenda_weight).abs() < 1e-9, "{kept} against {lost}");
        assert!(w.archived_agenda_weight < w.agenda_point_weight);
    }

    /// A faceup agenda under BANGUN: When Disaster Strikes is an install
    /// worth what its access does to the Runner (§36), not a rezzed
    /// asset's weight — the same as the facedown one against a grip two
    /// meat damage cannot bring below the floor, more against one it can.
    #[test]
    fn a_faceup_agenda_is_worth_its_punishment_and_not_an_assets_weight() {
        let pool = pool();
        let w = every_corp();
        let mut state = GameState::new(0);
        state.corp.identity = Some(CardId("bangun_when_disaster_strikes".to_string()));
        let agenda = |rezzed: bool| InstalledCard {
            card: CardId("offworld_office".to_string()),
            install_id: InstallId(1),
            server: netrunner_core::rules::ServerId::Remote(0),
            slot: netrunner_core::rules::InstallSlot::Root,
            rezzed,
            ..Default::default()
        };
        let value = |state: &GameState, rezzed: bool| corp_install_value(state, &agenda(rezzed), &pool, &w, [true; 3], 9);
        state.runner.grip = corp_cards("grip", 9);
        assert!((value(&state, true) - value(&state, false)).abs() < 1e-9, "a full grip: {} against {}", value(&state, true), value(&state, false));
        state.runner.grip.truncate(4);
        assert!(value(&state, true) >= value(&state, false) + 2.0 * w.opponent_grip_shortfall_weight - 1e-9, "two meat into four cards");
    }

    #[test]
    fn advancement_is_valued_up_to_the_requirement_and_no_further() {
        let at = advancement_value_at(None);
        assert!(at(1) > at(0));
        assert!(at(3) > at(2));
        assert_eq!(at(4), at(3), "tokens past the requirement are worth nothing — score instead");
    }

    /// The exception, and the reason `agenda_counter_weight` exists: on a
    /// Dividends agenda an excess token is not waste, it is one counter
    /// per `dividends` at score time. It must be worth more than the
    /// credit-click it costs (0.4 + 0.4) and far less than a point of
    /// agenda, so the Corp still takes the points.
    #[test]
    fn a_token_past_the_requirement_is_worth_its_dividends_and_no_more() {
        let plain = advancement_value_at(None);
        let dividend = advancement_value_at(Some(1));
        assert_eq!(dividend(3), plain(3), "up to the requirement the two agendas are identical");
        assert!(dividend(4) > dividend(3) + 0.8, "an excess token must beat the click and credit it costs");
        assert!(dividend(4) - dividend(3) < AGENDA_POINT_WEIGHT, "and never rival taking the points");
        assert_eq!(dividend(5) - dividend(4), dividend(4) - dividend(3), "each excess token is worth the same");

        let double = advancement_value_at(Some(2));
        assert!(
            (double(4) - double(3) - 2.0 * (dividend(4) - dividend(3))).abs() < f64::EPSILON,
            "two counters per token is worth twice one"
        );
    }

    /// The zero this term exists to remove. An ambush carries
    /// `advancement_requirement: Some(0)`, so every token it held was
    /// counted by `.min(required)` as nothing and no Corp ever advanced
    /// one — while the card's damage *is* that count.
    #[test]
    fn advancing_an_ambush_is_worth_more_than_the_click_it_costs_and_stops_at_the_cap() {
        let mut ambush = ice("clearinghouse", 0);
        ambush.card_type = CardType::Asset;
        ambush.advancement_requirement = Some(0);
        ambush.triggers = vec![TriggeredEffect {
            subject: None, when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_discard: false, from_runner_score_area: false,
            text: None,
            trigger: Trigger::OnTurnStart,
            effects: vec![Effect::DealDamageAmount(DamageType::Meat, Amount::HostedAdvancementTokens)],
            requirement: None,
        }];
        let registry = CardRegistry::from_cards(vec![ambush]);
        let at = |tokens| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId("clearinghouse".to_string()),
                install_id: InstallId(1),
                advancement_tokens: tokens,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        let cap = Weights::default().ambush_advancement_cap;
        assert!(at(1) > at(0) + 0.8, "advancing must beat the click and credit it costs");
        assert_eq!(at(cap + 1), at(cap), "and stop at the cap — nothing else would stop it");
    }

    /// `ambush_weight` is a profile term, off for the balanced Corp, and
    /// it applies only while the card is face down.
    #[test]
    fn a_face_down_ambush_is_worth_more_only_to_a_corp_that_plays_for_damage() {
        let on_access = TriggeredEffect {
            subject: None, when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_discard: false, from_runner_score_area: false,
            text: None,
            trigger: Trigger::OnAccessed,
            effects: vec![Effect::DealDamageAmount(DamageType::Net, Amount::HostedAdvancementTokens)],
            requirement: None,
        };
        let mut urtica = ice("urtica", 0);
        urtica.card_type = CardType::Asset;
        urtica.advancement_requirement = Some(0);
        urtica.triggers = vec![on_access.clone()];
        let mut snare = ice("snare", 0);
        snare.card_type = CardType::Asset;
        snare.triggers = vec![TriggeredEffect { effects: vec![Effect::DealDamage(DamageType::Net, 3)], ..on_access }];
        let registry = CardRegistry::from_cards(vec![urtica, snare]);
        let value = |id: &str, rezzed, seen, w: &Weights| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId(id.to_string()),
                install_id: InstallId(1),
                server: netrunner_core::rules::ServerId::Remote(0),
                rezzed,
                seen_by_runner: seen,
                ..Default::default()
            }];
            evaluate_state_with(&state, Side::Corp, &registry, w)
        };
        let base = Weights::default();
        let trap = Plan::Traps.weights();
        let hidden = |w: &Weights| value("urtica", false, false, w);
        assert_eq!(hidden(&base), hidden(&base), "balanced is indifferent — the term is zero");
        assert!(hidden(&trap) > hidden(&base) + base.unrezzed_install_weight - trap.unrezzed_install_weight);
        assert!(value("urtica", true, false, &trap) < hidden(&trap), "face up it is a known quantity, not a threat");
        // A trap the Runner has already sprung is sunk: the bonus for
        // hiding it is gone, and so is the reason to advance it.
        assert!(value("urtica", false, true, &trap) < hidden(&trap), "seen, it is one more face-down install");
        // And a trap that cannot be advanced was never worth a remote:
        // it belongs in HQ, where `held_trap_weight` pays for it.
        assert_eq!(
            value("snare", false, false, &trap),
            value("snare", false, false, &Weights { ambush_weight: 0.0, ..trap }),
            "a hand trap installed is not what `ambush_weight` buys"
        );
    }

    /// The Corp plays a lure trap like an agenda and keeps a hand trap in
    /// HQ: ICE in front of the one, and no install at all for the other.
    /// And once the Runner has seen the trap, both stop paying — the
    /// remote is spent.
    #[test]
    fn a_lure_trap_is_worth_icing_and_a_hand_trap_is_worth_holding() {
        use netrunner_core::rules::{InstallSlot, ServerId};
        let on_access = TriggeredEffect {
            subject: None, when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_discard: false, from_runner_score_area: false,
            text: None,
            trigger: Trigger::OnAccessed,
            effects: vec![Effect::DealDamageAmount(DamageType::Net, Amount::HostedAdvancementTokens)],
            requirement: None,
        };
        let mut urtica = ice("urtica", 0);
        urtica.card_type = CardType::Asset;
        urtica.advancement_requirement = Some(0);
        urtica.triggers = vec![on_access.clone()];
        let mut snare = ice("snare", 0);
        snare.card_type = CardType::Asset;
        snare.triggers = vec![TriggeredEffect { effects: vec![Effect::DealDamage(DamageType::Net, 3)], ..on_access }];
        assert!(is_lure_trap(&urtica) && !is_hand_trap(&urtica));
        assert!(is_hand_trap(&snare) && !is_lure_trap(&snare));
        let registry = CardRegistry::from_cards(vec![urtica, snare, ice("wall", 1)]);

        let with_ice = |pieces: usize, seen: bool, w: &Weights| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId("urtica".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                seen_by_runner: seen,
                ..Default::default()
            }];
            for n in 0..pieces {
                state.corp.installed.push(InstalledCard {
                    card: CardId("wall".to_string()),
                    install_id: InstallId(10 + n as u32),
                    server: ServerId::Remote(0),
                    slot: InstallSlot::Ice,
                    rezzed: true,
                    ..Default::default()
                });
            }
            evaluate_state_with(&state, Side::Corp, &registry, w)
        };
        // Measured against the same board without the term, so the ICE's
        // own worth (presence, rez, what the rig cannot break) cancels.
        let w = Weights::default();
        // The trap's remote is the fort, so it is iced like one.
        let bare = Weights { fort_weight: 0.0, ..w };
        let lure = |pieces: usize, seen: bool| with_ice(pieces, seen, &w) - with_ice(pieces, seen, &bare);
        assert!((lure(1, false) - w.fort_weight).abs() < 1e-9, "one piece in front of the trap: {}", lure(1, false));
        assert!((lure(2, false) - 2.0 * w.fort_weight).abs() < 1e-9, "two, as for an agenda: {}", lure(2, false));
        assert_eq!(lure(0, false), 0.0, "and there is nothing to pay for a trap in the open");
        assert_eq!(lure(1, true), 0.0, "a sprung trap is not worth icing");

        // A hand trap is worth more in HQ than in a remote, in every
        // profile, so the install never wins.
        for (personality, w) in corp_profiles() {
            let mut held = GameState::new(0);
            held.corp.hq = vec![CardId("snare".to_string())];
            let mut installed = GameState::new(0);
            installed.corp.installed = vec![InstalledCard {
                card: CardId("snare".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                ..Default::default()
            }];
            assert!(
                evaluate_state_with(&held, Side::Corp, &registry, &w) > evaluate_state_with(&installed, Side::Corp, &registry, &w),
                "{personality:?} would install a Snare!"
            );
        }
    }

    /// The lever the "no Corp damage term" note above asks for: a hand
    /// trap is a paid interaction, so springing one is a Corp action that
    /// makes the grip smaller. Three net damage into a grip of five beats
    /// the four credits it costs; into a full grip it does not.
    #[test]
    fn springing_a_paid_trap_beats_its_price_only_where_the_grip_is_thin() {
        let registry = CardRegistry::from_cards(vec![]);
        let w = Weights::default();
        let at = |grip: usize, credits: u32| {
            let mut state = GameState::new(0);
            state.runner.grip = vec![CardId("filler".to_string()); grip];
            state.corp.resources.credits = Credits(credits);
            evaluate_state_with(&state, Side::Corp, &registry, &w)
        };
        let price = 4;
        let sprung = |grip: usize| at(grip.saturating_sub(3), 10 - price) - at(grip, 10);
        assert!(sprung(5) > 0.0, "a grip of five is worth 4[c] to cut to two: {}", sprung(5));
        assert!(sprung(8) < 0.0, "a grip of eight is not: {}", sprung(8));
    }

    /// A trap is never worth rezzing, to any Corp: face down it fires on
    /// access, and face up it only tells the Runner where not to run. On
    /// `main` every profile but `Trap` rezzed every one (Phase 5 §20),
    /// because the rez was +1.0 at a rez cost of 0. Both kinds of trap,
    /// and a Clearinghouse, which needs its rez, still gets one.
    #[test]
    fn no_corp_profile_would_rather_its_trap_were_face_up() {
        let on_access = |effect| TriggeredEffect {
            subject: Some(netrunner_core::dsl::Subject::This), when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_discard: false, from_runner_score_area: false,
            text: None,
            trigger: Trigger::OnAccessed,
            effects: vec![effect],
            requirement: None,
        };
        let mut urtica = ice("urtica", 0);
        urtica.card_type = CardType::Asset;
        urtica.advancement_requirement = Some(0);
        urtica.triggers = vec![on_access(Effect::DealDamageAmount(DamageType::Net, Amount::HostedAdvancementTokens))];
        let mut snare = ice("snare", 0);
        snare.card_type = CardType::Asset;
        snare.triggers = vec![on_access(Effect::DealDamage(DamageType::Net, 3))];
        let mut clearinghouse = ice("clearinghouse", 0);
        clearinghouse.card_type = CardType::Asset;
        clearinghouse.advancement_requirement = Some(0);
        clearinghouse.triggers = vec![TriggeredEffect {
            trigger: Trigger::OnTurnStart,
            subject: None,
            effects: vec![Effect::DealDamageAmount(DamageType::Meat, Amount::HostedAdvancementTokens)],
            ..on_access(Effect::DealDamage(DamageType::Net, 0))
        }];
        let registry = CardRegistry::from_cards(vec![urtica, snare, clearinghouse]);
        let value = |id: &str, rezzed, tokens, w: &Weights| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId(id.to_string()),
                install_id: InstallId(1),
                server: netrunner_core::rules::ServerId::Remote(0),
                rezzed,
                advancement_tokens: tokens,
                ..Default::default()
            }];
            evaluate_state_with(&state, Side::Corp, &registry, w)
        };
        for (personality, w) in corp_profiles() {
            for (trap, tokens) in [("urtica", 0), ("urtica", 2), ("snare", 0)] {
                let gain = value(trap, true, tokens, &w) - value(trap, false, tokens, &w);
                assert!(gain <= -w.revealed_trap_weight + 1e-9, "{personality:?} would rez {trap} at {tokens} tokens: {gain}");
            }
            assert!(
                value("clearinghouse", true, 0, &w) > value("clearinghouse", false, 0, &w),
                "{personality:?}: a card that needs its rez still wants one"
            );
        }
    }

    /// The half that makes the first half survive being cashed in: a
    /// search comparing "score now" with "advance once more, then score"
    /// sees the install gone either way, so the counters have to be worth
    /// something in the score area or the extra click is pure cost.
    #[test]
    fn counters_on_a_scored_agenda_outlive_the_install_that_carried_them() {
        use netrunner_core::rules::ScoredAgenda;
        let registry = CardRegistry::from_cards(vec![]);
        let scored = |agenda_counters| {
            let mut state = GameState::new(0);
            state.corp.scored_agendas = vec![ScoredAgenda {
                card: CardId("offworld_office".to_string()),
                install_id: InstallId(1),
                agenda_counters,
                scored_on_turn: 0,
                installed_on_scoring_turn: false, advanced_on_scoring_turn: false,
                as_agenda: None,
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        assert!(scored(1) > scored(0) + 0.8, "a counter carried into the score area beats the click that bought it");
        assert_eq!(scored(2) - scored(1), scored(1) - scored(0));
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

    /// The two terms that make the Corp rez the ICE that holds rather
    /// than the ICE that is cheap. Both are paid **only once the card is
    /// face up**, which is the arithmetic the first cut got wrong: paid
    /// on the face-down card too they sit on both sides of the rez and
    /// cancel out of the decision they exist to win.
    #[test]
    fn the_corp_prices_ice_by_what_it_stops_and_only_once_it_is_face_up() {
        use netrunner_core::dsl::SubroutineDef;
        use netrunner_core::rules::InstallSlot;
        let mut pharos = ice("pharos", 7);
        pharos.subroutines = vec![
            SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None },
            SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None },
        ];
        let mut tithe = ice("tithe", 1);
        tithe.subroutines =
            vec![SubroutineDef { text: String::new(), effect: Effect::GainCredits(Side::Corp, 1), only_breakable_by: None }];
        let registry = CardRegistry::from_cards(vec![pharos, tithe, breaker("fracter", Some(IceType::Barrier))]);

        let board = |id: &str, rezzed, rig: Vec<InstalledRunnerCard>| {
            let mut state = GameState::new(0);
            state.runner.rig = rig;
            state.corp.installed = vec![InstalledCard {
                card: CardId(id.to_string()),
                install_id: InstallId(1),
                slot: InstallSlot::Ice,
                rezzed,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        let naked = || vec![];
        let fracter = || vec![rig_card("fracter")];

        // Face down, the two pieces are worth the same: nothing about
        // what they stop has been revealed, and nothing may cancel.
        assert_eq!(board("pharos", false, naked()), board("tithe", false, naked()));
        assert_eq!(board("pharos", false, naked()), board("pharos", false, fracter()));

        // Face up, two run-enders against none is worth exactly two of
        // this weight, and a rig with no fracter is worth one more.
        let rez_gain = |id: &str, rig: Vec<InstalledRunnerCard>| board(id, true, rig) - board(id, false, naked());
        assert!((rez_gain("pharos", naked()) - rez_gain("tithe", naked()) - 2.0 * ETR_SUBROUTINE_WEIGHT).abs() < 1e-9);
        assert!((rez_gain("pharos", naked()) - rez_gain("pharos", fracter()) - UNBREAKABLE_ICE_WEIGHT).abs() < 1e-9);

        // The decision the pair exists to win: a 7-cost run-ender the rig
        // cannot break is worth rezzing, where `REZZED_ICE_WEIGHT` minus
        // its cost alone leaves it face-down forever.
        let paid = rez_gain("pharos", naked()) - 7.0 * OWN_CREDIT_WEIGHT;
        assert!(paid > 0.0, "an expensive run-ender the rig cannot break is worth rezzing, got {paid}");
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
            evaluate_state_with(&state, Side::Corp, &registry, &without_fort(Weights::default()))
        };
        let per_ice = |root: &str, n| board(root, n) - board(root, n - 1) - UNREZZED_INSTALL_WEIGHT;
        assert!((per_ice("offworld_office", 1) - AGENDA_PROTECTION_WEIGHT).abs() < 1e-9);
        assert!((per_ice("offworld_office", AGENDA_PROTECTION_CAP) - AGENDA_PROTECTION_WEIGHT).abs() < 1e-9);
        assert!(per_ice("offworld_office", AGENDA_PROTECTION_CAP + 1).abs() < 1e-9, "past the cap an ICE is just an ICE");
        assert!(per_ice("nico_campaign", 1).abs() < 1e-9, "an asset is not protected by this term");
    }

    /// Where a fort goes is how a Corp plays, not a style (Phase 5 §23):
    /// every Corp profile carries the fort terms, at one set of values. A
    /// profile that wants its own should have to beat these in the style
    /// matrix first.
    #[test]
    fn every_corp_profile_prices_where_its_ice_stands() {
        let base = Weights::default();
        for (personality, w) in corp_profiles() {
            assert_eq!(
                (w.central_ice_weight, w.central_ice_cap, w.fort_weight, w.fort_cap, w.exposed_agenda_weight),
                (base.central_ice_weight, base.central_ice_cap, base.fort_weight, base.fort_cap, base.exposed_agenda_weight),
                "{personality:?}"
            );
            assert!(w.fort_weight > 0.0 && w.central_ice_weight > 0.0 && w.exposed_agenda_weight > 0.0);
        }
    }

    /// The order a person builds a fort in, at one ply. A first piece on
    /// HQ beats a second piece in front of an asset; a piece on an empty
    /// remote beats one in front of an asset; the agenda goes into the
    /// two-deep remote rather than a new one; and a naked agenda is worth
    /// less than the credit its click could have taken instead. In every
    /// Corp profile, `rush` included.
    #[test]
    fn every_corp_ices_the_centrals_builds_one_remote_and_waits_for_it() {
        use netrunner_core::rules::ServerId::{self, Hq, Remote};
        let registry = fort_registry();
        for (personality, w) in corp_profiles() {
        let score = |state: &GameState| evaluate_state_with(state, Side::Corp, &registry, &w);
        let asset = [("campaign", Remote(0))];

        let on_hq = fort_board(&[Remote(0), Hq], &asset);
        let on_asset = fort_board(&[Remote(0), Remote(0)], &asset);
        assert!(score(&on_hq) > score(&on_asset), "{personality:?}: HQ's first piece before an asset's second");

        let fort = fort_board(&[Remote(0), Remote(1)], &asset);
        assert!(score(&fort) > score(&on_asset), "{personality:?}: an empty remote's piece before an asset's second");

        let walled = [Hq, ServerId::RnD, ServerId::Archives, Remote(1), Remote(1)];
        let behind = fort_board(&walled, &[("plan", Remote(1))]);
        let naked = fort_board(&walled, &[("plan", Remote(2))]);
        assert!(score(&behind) > score(&naked), "{personality:?}: the agenda goes behind the fort");

        let held = fort_board(&walled, &[]);
        let mut banked = held.clone();
        banked.corp.resources.credits = Credits(held.corp.resources.credits.0 + 1);
        assert!(score(&naked) < score(&banked), "{personality:?}: a naked agenda loses to a credit click");
        assert!(score(&behind) > score(&banked), "{personality:?}: and the agenda behind the fort beats it");
        }
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
        // Without the fort terms, which price an agenda by the ICE in front
        // of it and an ICE by the server it is on.
        let balanced = without_fort(Weights::default());
        assert_eq!(
            evaluate_state_with(&with_agenda, Side::Corp, &registry, &balanced),
            evaluate_state_with(&with_ice, Side::Corp, &registry, &balanced),
            "balanced: an unrezzed install is an unrezzed install"
        );
        let rush = Weights { installed_agenda_weight: 1.0, ..balanced };
        assert!(evaluate_state_with(&with_agenda, Side::Corp, &registry, &rush) > evaluate_state_with(&with_ice, Side::Corp, &registry, &rush));
    }

    /// The Corp's mirror of `active_run_weight`, on the same gate: a run
    /// the Runner can finish costs the Corp; one they cannot get through
    /// does not, because there is nothing there for the Corp to pay to
    /// stop.
    #[test]
    fn a_breakable_run_costs_the_corp_and_an_unbreakable_one_does_not() {
        use netrunner_core::rules::{RunIce, ServerId};
        let registry = CardRegistry::from_cards(vec![priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        let w = Weights::default();

        // The run term against the *same* board with no run, so every
        // other term cancels — credits especially, which the Corp scores
        // through `opponent_credit_weight`.
        let run_term = |ice: Vec<RunIce>, credits: u32, rig: Vec<InstalledRunnerCard>| {
            let registry = with_printed_ice(&registry, &ice);
            let mut idle = GameState::new(0);
            idle.runner.resources.credits = Credits(credits);
            idle.runner.rig = rig;
            let mut running = idle.clone();
            running.active_run = Some(RunState { server: ServerId::Hq, ice, position: 0, ..Default::default() });
            evaluate_state_with(&running, Side::Corp, &registry, &w)
                - evaluate_state_with(&idle, Side::Corp, &registry, &w)
        };

        // Nothing rezzed in the way: the run is breakable by definition.
        let delta = run_term(Vec::new(), 0, Vec::new());
        assert!((delta + w.active_run_against_weight).abs() < 1e-9, "an open run costs the Corp the term: {delta}");

        // A rezzed Barrier the Runner has no breaker for: not the Corp's
        // problem, and the same predicate the Runner's own term uses.
        let delta = run_term(vec![run_ice(1, IceType::Barrier, 1, true)], 5, Vec::new());
        assert_eq!(delta, 0.0, "a run into ice the Runner cannot break costs the Corp nothing");

        // The same ice with a breaker and the credits to use it: the Corp
        // is paying again, which is the gate doing its job in both
        // directions.
        let armed = InstalledRunnerCard { base_strength: 3, ..rig_card("cleaver") };
        let delta = run_term(vec![run_ice(1, IceType::Barrier, 1, true)], 5, vec![armed]);
        assert!((delta + w.active_run_against_weight).abs() < 1e-9, "a breakable run costs the Corp: {delta}");
    }

    /// The decision the run term was sized to win, and the reason it
    /// exists: *Anoetic Void* offers the Corp "pay 2, discard 2 from HQ,
    /// end the run", and before the term the continuation priced at zero
    /// so it was declined every time (ROADMAP Phase 3 §1).
    ///
    /// Checked through `pending_decision_upside`, which is what the
    /// evaluator credits beside `pending_decision_upside_weight` when the
    /// Corp is sitting on the parked selection.
    #[test]
    fn ending_a_run_is_worth_exactly_the_term_it_removes() {
        use netrunner_core::rules::{PendingChoiceResume, RunIce, ServerId};
        let registry = CardRegistry::new();
        let w = Weights::default();

        let parked = |ice: Vec<RunIce>, hq: usize| {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Runner);
            state.corp.hq = (0..hq).map(|i| CardId(format!("hq{i}"))).collect();
            state.corp.r_and_d = (0..20).map(|i| CardId(format!("rd{i}"))).collect();
            state.active_run = Some(RunState { server: ServerId::Hq, ice, position: 0, ..Default::default() });
            state.pending_decision = Some(PendingDecision::ChooseCards {
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
            state
        };

        // A live run, and an HQ well above the floor so the two discards
        // are free: the upside is the run term and nothing else.
        let state = parked(Vec::new(), 8);
        let upside = pending_decision_upside(&state, Side::Corp, &registry, &w);
        assert!(
            (upside - w.active_run_against_weight).abs() < 1e-9,
            "ending the run recovers exactly the term: {upside}"
        );
        // And it clears the bar that decides the offer: the parked
        // decision costs `unresolved_decision_weight` against
        // `pending_decision_upside_weight` plus this.
        assert!(
            upside + w.pending_decision_upside_weight > w.unresolved_decision_weight,
            "the Corp has to come out ahead for Anoetic Void to be accepted"
        );

        // No run to end: the continuation is worth nothing, and the offer
        // goes back to being declined.
        let mut no_run = parked(Vec::new(), 8);
        no_run.active_run = None;
        assert_eq!(pending_decision_upside(&no_run, Side::Corp, &registry, &w), 0.0);
    }

    /// The decision `FUTURE_CREDIT_WEIGHT` was sized for: taking the
    /// credits off Regolith Mining License beats the credit click, at the
    /// guide's rate, even though the term banks the stock — because a
    /// credit later is worth less than one now.
    #[test]
    fn taking_credits_off_an_installed_economy_card_beats_the_credit_click() {
        use netrunner_core::rules::Clicks;
        let pool = pool();
        let w = guide();
        let regolith = |counters: u32, credits: u32, clicks: u32| {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Corp);
            state.corp.resources.credits = Credits(credits);
            state.corp.resources.clicks = Clicks(clicks);
            state.corp.installed = vec![InstalledCard {
                card: CardId("regolith_mining_license".to_string()),
                install_id: InstallId(1),
                rezzed: true,
                counters,
                ..Default::default()
            }];
            evaluate_state_with(&state, Side::Corp, &pool, &w)
        };
        let took = regolith(12, 8, 2);
        let clicked = regolith(15, 6, 2);
        assert!(took > clicked, "3[c] off the license beats 1[c] for the click: {took} vs {clicked}");
        assert!(regolith(15, 5, 3) > regolith(12, 8, 3) - 3.0 * w.own_credit_weight, "and the stock on the card is worth something");
    }

    /// PAD Campaign is worth its install and rez early and not late: the
    /// same card, the same price, a different horizon.
    #[test]
    fn an_economy_asset_pays_for_its_rez_early_and_not_late() {
        let pool = pool();
        let w = guide();
        let board = |rezzed: bool, credits: u32, stage_points: i32| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state.runner.resources.agenda_points = netrunner_core::rules::AgendaPoints(stage_points);
            state.corp.installed = vec![InstalledCard { card: CardId("pad_campaign".to_string()), install_id: InstallId(1), rezzed, ..Default::default() }];
            evaluate_state_with(&state, Side::Corp, &pool, &w)
        };
        // Points are the Runner's, so the Corp's own score does not move
        // with them except through the stage.
        let early = board(true, 3, 0) - board(false, 5, 0);
        let late = board(true, 3, 6) - board(false, 5, 6);
        assert!(early > late, "{early} vs {late}");
        assert!(early > w.rezzed_asset_weight, "early the rez is worth more than the bare asset term");
    }

    /// The Corp's savings term: short of the rez that stops a run, a
    /// credit is worth more, and once the rez is affordable the term is
    /// gone — so it never fights the rez.
    #[test]
    fn a_credit_is_worth_more_while_the_corp_cannot_afford_its_dearest_rez() {
        use netrunner_core::rules::InstallSlot;
        let registry = CardRegistry::from_cards(vec![ice("pharos", 7)]);
        let w = guide();
        let holding = |credits: u32| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state.corp.installed = vec![InstalledCard { card: CardId("pharos".to_string()), install_id: InstallId(1), slot: InstallSlot::Ice, ..Default::default() }];
            evaluate_state_with(&state, Side::Corp, &registry, &w)
        };
        assert!(((holding(5) - holding(4)) - (w.own_credit_weight + w.rez_reserve_weight)).abs() < 1e-9, "a credit toward the reserve");
        assert!(((holding(7) - holding(6)) - (w.own_credit_weight + w.rez_reserve_weight + w.unaffordable_ice_weight)).abs() < 1e-9, "the credit that covers the rez is worth the promise too");
        assert!(((holding(8) - holding(7)) - w.own_credit_weight).abs() < 1e-9, "a credit past it is a credit");
        // The cover: the same install is worth `unaffordable_ice_weight`
        // less on 2[c] than on 9[c], over and above the reserve it is
        // short of — so the first piece on an open central still goes
        // down as a bluff (the fort term carries it), and a piece the
        // fort does not want waits for the credits.
        let install_gain = |credits: u32| {
            let mut held = GameState::new(0);
            held.corp.resources.credits = Credits(credits);
            held.corp.r_and_d = corp_cards("rd", RD_DRAW_RESERVE + 10);
            held.corp.hq = corp_cards("hq", HQ_FLOOR);
            held.corp.hq.push(CardId("pharos".to_string()));
            let mut installed = held.clone();
            installed.corp.hq.pop();
            installed.corp.installed = vec![InstalledCard { card: CardId("pharos".to_string()), install_id: InstallId(1), slot: InstallSlot::Ice, server: netrunner_core::rules::ServerId::Remote(0), ..Default::default() }];
            evaluate_state_with(&installed, Side::Corp, &registry, &w) - evaluate_state_with(&held, Side::Corp, &registry, &w)
        };
        let poor = install_gain(2);
        let rich = install_gain(9);
        assert!(((rich - poor) - (w.unaffordable_ice_weight + 5.0 * w.rez_reserve_weight)).abs() < 1e-9, "{rich} vs {poor}");
        // The credit click while short of the reserve is worth the credit
        // and the reserve it closes.
        assert!(poor < w.own_credit_weight + w.rez_reserve_weight, "broke, a credit click beats the install: {poor}");
        assert!(rich > w.own_credit_weight, "with the rez in the bank the install is worth making: {rich}");
        let reference = Weights::default();
        let mut short = GameState::new(0);
        short.corp.resources.credits = Credits(0);
        short.corp.installed = vec![InstalledCard { card: CardId("pharos".to_string()), install_id: InstallId(1), slot: InstallSlot::Ice, ..Default::default() }];
        let mut flush = short.clone();
        flush.corp.resources.credits = Credits(7);
        assert!(((evaluate_state_with(&flush, Side::Corp, &registry, &reference) - evaluate_state_with(&short, Side::Corp, &registry, &reference)) - 7.0 * reference.own_credit_weight).abs() < 1e-9, "the reference reads no reserve");
    }

    /// The taxing window, read by the Corp: an agenda behind ICE the
    /// Runner cannot afford is worth more than the same agenda when they
    /// can, and a naked agenda is under no window at all.
    #[test]
    fn an_installed_agenda_is_worth_more_while_the_runner_cannot_afford_its_server() {
        use netrunner_core::rules::{InstallSlot, ServerId};
        let mut wall = ice("wall", 3);
        wall.subroutines = vec![netrunner_core::dsl::SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }];
        let registry = CardRegistry::from_cards(vec![advanceable("plan", 3), wall, priced_breaker("cleaver", Some(IceType::Barrier), (2, 1), (1, 1))]);
        let w = guide();
        let board = |pieces: usize, runner_credits: u32| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(10);
            state.runner.resources.credits = Credits(runner_credits);
            state.runner.rig = vec![InstalledRunnerCard { base_strength: 3, ..rig_card("cleaver") }];
            state.corp.installed = vec![InstalledCard { card: CardId("plan".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), ..Default::default() }];
            for n in 0..pieces {
                state.corp.installed.push(InstalledCard { card: CardId("wall".to_string()), install_id: InstallId(10 + n as u32), server: ServerId::Remote(0), slot: InstallSlot::Ice, rezzed: true, ..Default::default() });
            }
            evaluate_state_with(&state, Side::Corp, &registry, &w)
        };
        // Two pieces at 2[c] a break: the window is open under 4[c].
        let open = board(2, 3) - board(2, 4);
        assert!((open - (w.taxing_window_weight + w.opponent_credit_weight)).abs() < 1e-9, "{open}");
        assert!(((board(0, 0) - board(0, 4)) - 4.0 * w.opponent_credit_weight).abs() < 1e-9, "no window on a naked agenda");
    }

    // ----- The Corp's plans (Stage 6) -----

    /// LEO Construction's end-the-run held (§43): up to the run's last
    /// window it caps what the run costs the Corp at the price of ending
    /// it, less the edge, so paying is worse than waiting; a run let past
    /// that window costs what its breach reaches, so at the last window
    /// the Corp pays for a run on its agenda and not for one on an empty
    /// remote.
    #[test]
    fn a_held_end_the_run_answers_the_run_until_its_breach() {
        use netrunner_core::rules::{InstallSlot, RunPhase, ServerId};
        let pool = pool();
        let mut registry = CardRegistry::from_cards(vec![advanceable("plan", 3)]);
        for id in ["leo_construction_labor_solutions", "mercia_b4ll4rd"] {
            registry.insert(printed(&pool, id));
        }
        let w = every_corp();
        let mercia = InstalledCard { card: CardId("mercia_b4ll4rd".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Root, rezzed: true, ..Default::default() };
        let plan = InstalledCard { card: CardId("plan".to_string()), install_id: InstallId(2), server: ServerId::Remote(0), slot: InstallSlot::Root, ..Default::default() };
        let board = |agenda: bool, with_mercia: bool, phase: Option<RunPhase>| {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Runner);
            state.corp.identity = Some(CardId("leo_construction_labor_solutions".to_string()));
            state.corp.installed = [with_mercia.then(|| mercia.clone()), agenda.then(|| plan.clone())].into_iter().flatten().collect();
            state.active_run = phase.map(|phase| RunState { server: ServerId::Remote(0), phase, ..Default::default() });
            state
        };
        let score = |state: &GameState| evaluate_state_with(state, Side::Corp, &registry, &w);
        let used = |agenda: bool| score(&board(agenda, false, None));

        // Up to and at the last window, the ability is worth more held
        // than used — by the edge, when the run threatens more than it.
        for phase in [RunPhase::Initiation, RunPhase::ApproachIce, RunPhase::Movement] {
            let held = score(&board(true, true, Some(phase)));
            assert!((held - used(true) - HELD_END_THE_RUN_EDGE).abs() < 1e-9, "{phase:?}: held {held}, used {}", used(true));
            assert!(score(&board(false, true, Some(phase))) > used(false), "{phase:?}");
        }
        // Past it, the run costs what the breach reaches: more than Mercia
        // on the agenda, less on an empty remote.
        assert!(score(&board(true, true, Some(RunPhase::Success))) < used(true), "the Corp pays for the run on its agenda");
        assert!(score(&board(false, true, Some(RunPhase::Success))) > used(false), "and keeps Mercia against an empty remote");
        let breach = score(&board(false, true, None)) - score(&board(false, true, Some(RunPhase::Success)));
        assert!(breach.abs() < 1e-9, "an empty remote's breach takes nothing: {breach}");

        // Without a bioroid to trash the ability cannot be used, and the
        // run is charged whole wherever it stands.
        let charged = |phase| score(&board(true, false, None)) - score(&board(true, false, Some(phase)));
        assert!((charged(RunPhase::Initiation) - charged(RunPhase::Success)).abs() < 1e-9);
        assert!(charged(RunPhase::Success) > w.active_run_against_weight);
    }

    /// The stakes of a run are what the breach would reach, counted as
    /// the Corp can count them.
    #[test]
    fn the_stakes_of_a_run_are_the_agenda_points_the_breach_would_reach() {
        use netrunner_core::rules::{ArchivedCard, InstallSlot, ServerId};
        let registry = CardRegistry::from_cards(vec![advanceable("plan", 3), asset("campaign", 2)]);
        let mut state = GameState::new(0);
        state.corp.hq = vec![CardId("plan".to_string()), CardId("campaign".to_string()), CardId("campaign".to_string()), CardId("campaign".to_string())];
        state.corp.r_and_d = vec![CardId("plan".to_string()), CardId("campaign".to_string()), CardId("campaign".to_string()), CardId("campaign".to_string()), CardId("campaign".to_string())];
        state.corp.archives = vec![ArchivedCard { card: CardId("plan".to_string()), facedown: true }, ArchivedCard { card: CardId("plan".to_string()), facedown: false }];
        state.corp.installed = vec![
            InstalledCard { card: CardId("plan".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Root, ..Default::default() },
            InstalledCard { card: CardId("campaign".to_string()), install_id: InstallId(2), server: ServerId::Remote(1), slot: InstallSlot::Root, ..Default::default() },
        ];
        let stakes = |server| run_stakes(&state, &RunState { server, ..Default::default() }, &registry);
        assert!((stakes(ServerId::Remote(0)) - 2.0).abs() < 1e-9, "the agenda in the root");
        assert_eq!(stakes(ServerId::Remote(1)), 0.0, "an asset is no stake");
        assert_eq!(stakes(ServerId::Remote(2)), 0.0, "an empty remote");
        assert!((stakes(ServerId::Hq) - 0.5).abs() < 1e-9, "one 2-point agenda in four");
        assert!((stakes(ServerId::RnD) - 0.4).abs() < 1e-9, "the deck's density");
        assert!((stakes(ServerId::Archives) - 4.0).abs() < 1e-9, "every agenda in Archives");
    }

    /// "Rez it when the run matters": a Palisade approached in front of
    /// an agenda, by a Runner with no fracter, is rezzed — the rez turns
    /// the run unbreakable and recovers the stakes — and the same piece
    /// approached on an empty remote by a Runner who breaks it anyway is
    /// held. The reference rezzes both.
    #[test]
    fn a_rez_that_stops_a_run_on_an_agenda_is_made_and_one_that_stops_nothing_is_held() {
        use netrunner_core::rules::{InstallSlot, RunIce, ServerId};
        let pool = pool();
        // A fracter strong enough to break Palisade (+2 in a remote) for
        // one credit: the tax is a credit, so the rez is held; a dearer
        // break is a reason to rez.
        let mut registry = CardRegistry::from_cards(vec![advanceable("plan", 3), priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        registry.insert(printed(&pool, "palisade"));
        let rez_gain = |w: &Weights, agenda: bool, fracter: bool| {
            let board = |rezzed: bool, credits: u32| {
                let mut state = GameState::new(0);
                state.phase = GamePhase::Action(Side::Runner);
                state.corp.resources.credits = Credits(credits);
                state.runner.resources.credits = Credits(8);
                if fracter {
                    state.runner.rig = vec![InstalledRunnerCard { base_strength: 5, ..rig_card("cleaver") }];
                }
                state.corp.installed = vec![InstalledCard { card: CardId("palisade".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Ice, rezzed, ..Default::default() }];
                if agenda {
                    state.corp.installed.push(InstalledCard { card: CardId("plan".to_string()), install_id: InstallId(2), server: ServerId::Remote(0), slot: InstallSlot::Root, ..Default::default() });
                }
                let ice = RunIce { rezzed, ..run_ice(2, IceType::Barrier, 1, rezzed) };
                let ice = RunIce { card_id: CardId("palisade".to_string()), install_id: InstallId(1), ..ice };
                state.active_run = Some(RunState { server: ServerId::Remote(0), ice: vec![ice], position: 0, ..Default::default() });
                evaluate_state_with(&state, Side::Corp, &registry, w)
            };
            board(true, 5) - board(false, 8)
        };
        // The term as measured, not as shipped: it ships at zero
        // (`REZ_HELD_WEIGHT`), and this is what it does when it is on.
        let w = Weights { rez_held_weight: 1.5, ..every_corp() };
        assert!(rez_gain(&w, true, false) > 0.0, "in front of an agenda, against no fracter: {}", rez_gain(&w, true, false));
        assert!(rez_gain(&w, false, true) < 0.0, "on an empty remote, against a fracter: {}", rez_gain(&w, false, true));
        assert!(rez_gain(&w, true, false) > rez_gain(&w, false, false), "the same rez is worth more in front of the agenda");
        assert!(rez_gain(&every_corp(), false, true) > 0.0, "shipped at zero, the planner rezzes on approach as the reference does");
        // The held rez is the run's: with no run on, an affordable
        // face-down piece is worth what it always was, so the term is no
        // reason to install one.
        let mut idle = GameState::new(0);
        idle.corp.resources.credits = Credits(8);
        idle.corp.installed = vec![InstalledCard { card: CardId("palisade".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Ice, ..Default::default() }];
        assert_eq!(held_rezzes(&idle, &registry), 0);
        assert_eq!(evaluate_state_with(&idle, Side::Corp, &registry, &w), evaluate_state_with(&idle, Side::Corp, &registry, &Weights { rez_held_weight: 0.0, ..w }));
        let reference = Weights::default();
        assert!(rez_gain(&reference, false, true) > 0.0, "the reference rezzes on approach");
    }

    /// The taxing piece outside the stopping one costs the order term,
    /// and the other way round costs nothing.
    #[test]
    fn a_taxing_piece_outside_a_stopping_one_is_out_of_order() {
        use netrunner_core::rules::{InstallSlot, ServerId};
        let pool = pool();
        let registry = CardRegistry::from_cards(vec![printed(&pool, "palisade"), printed(&pool, "tithe")]);
        let board = |outer: &str, inner: &str| {
            let mut state = GameState::new(0);
            // The outermost piece is first in `installed` (`ability.rs`).
            state.corp.installed = vec![
                InstalledCard { card: CardId(outer.to_string()), install_id: InstallId(1), server: ServerId::Hq, slot: InstallSlot::Ice, ..Default::default() },
                InstalledCard { card: CardId(inner.to_string()), install_id: InstallId(2), server: ServerId::Hq, slot: InstallSlot::Ice, ..Default::default() },
            ];
            state
        };
        assert_eq!(ice_out_of_order(&board("tithe", "palisade"), &registry), 1);
        assert_eq!(ice_out_of_order(&board("palisade", "tithe"), &registry), 0);
        assert_eq!(ice_out_of_order(&board("palisade", "palisade"), &registry), 0);
        let w = every_corp();
        let delta = evaluate_state_with(&board("palisade", "tithe"), Side::Corp, &registry, &w) - evaluate_state_with(&board("tithe", "palisade"), Side::Corp, &registry, &w);
        assert!((delta - w.ice_order_weight).abs() < 1e-9, "{delta}");
        assert_eq!(evaluate_state(&board("palisade", "tithe"), Side::Corp, &registry), evaluate_state(&board("tithe", "palisade"), Side::Corp, &registry), "the reference reads no order");
    }

    /// The never-advance line's condition: an installed agenda the Corp
    /// can finish next turn is worth more than one it cannot, and a
    /// Seamless Launch in hand is two of the tokens.
    #[test]
    fn an_installed_agenda_the_corp_can_finish_next_turn_is_worth_more_than_one_it_cannot() {
        use netrunner_core::rules::{InstallSlot, ServerId};
        let pool = pool();
        let registry = CardRegistry::from_cards(vec![advanceable("plan", 4), printed(&pool, "seamless_launch")]);
        let board = |credits: u32, seamless: bool| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            if seamless {
                state.corp.hq = vec![CardId("seamless_launch".to_string())];
            }
            state.corp.installed = vec![InstalledCard { card: CardId("plan".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Root, ..Default::default() }];
            state
        };
        assert_eq!(next_turn_advancements(&board(3, false), &registry), 3, "three clicks at a credit each");
        assert_eq!(next_turn_advancements(&board(1, false), &registry), 1, "bounded by the credits");
        assert_eq!(next_turn_advancements(&board(3, true), &registry), 4, "Seamless Launch's two for a click and a credit, then two more");
        assert_eq!(finishable_agendas(&board(3, false), &registry), (0, 1));
        assert_eq!(finishable_agendas(&board(3, true), &registry), (1, 0));
        let w = every_corp();
        let with = evaluate_state_with(&board(3, true), Side::Corp, &registry, &w);
        let without = evaluate_state_with(&board(3, false), Side::Corp, &registry, &w);
        // The card in hand is not priced held (Stage 5), so the whole
        // difference is the term switching sign.
        assert!((with - without - 2.0 * w.never_advance_weight).abs() < 1e-9, "{with} vs {without}");
    }

    /// The kill plan reads the punishment it holds: a tag is worth
    /// something while Scorched Earth is in HQ, and a grip the damage
    /// would exceed is the threat.
    #[test]
    fn a_kill_corp_values_a_tag_it_holds_the_punishment_for_and_a_grip_under_its_damage() {
        let pool = pool();
        let registry = CardRegistry::from_cards(vec![printed(&pool, "scorched_earth"), printed(&pool, "hedge_fund")]);
        assert!(punishes_tags(&printed(&pool, "scorched_earth")) && punishes_tags(&printed(&pool, "retribution")) && punishes_tags(&printed(&pool, "orbital_superiority")));
        assert!(!punishes_tags(&printed(&pool, "hedge_fund")) && !punishes_tags(&printed(&pool, "public_trail")));
        let board = |tags: u32, grip: usize, held: &str, credits: u32| {
            let mut state = GameState::new(0);
            state.runner.tags = tags;
            state.runner.grip = corp_cards("g", grip);
            state.corp.resources.credits = Credits(credits);
            state.corp.hq = vec![CardId(held.to_string())];
            state
        };
        assert_eq!(damage_in_reach(&board(1, 5, "scorched_earth", 3), &registry), 4);
        assert_eq!(damage_in_reach(&board(0, 5, "scorched_earth", 3), &registry), 0, "the play requirement is not met");
        assert_eq!(damage_in_reach(&board(1, 5, "scorched_earth", 2), &registry), 0, "not affordable");
        let w = planned(&[crate::plans::Plan::Kill]);
        let score = |state: &GameState| evaluate_state_with(state, Side::Corp, &registry, &w);
        let tagged = score(&board(1, 5, "scorched_earth", 3)) - score(&board(0, 5, "scorched_earth", 3));
        assert!((tagged - w.tag_leverage_weight).abs() < 1e-9, "a tag the Corp can punish: {tagged}");
        assert_eq!(score(&board(1, 5, "hedge_fund", 3)), score(&board(0, 5, "hedge_fund", 3)), "a tag nothing in hand punishes");
        // A card off the grip is also the grip-shortfall term's.
        let lethal = score(&board(1, 3, "scorched_earth", 3)) - score(&board(1, 4, "scorched_earth", 3)) - w.opponent_grip_shortfall_weight;
        assert!((lethal - w.lethal_threat_weight).abs() < 1e-9, "a grip of three under four meat: {lethal}");
        let reference = Style::of(crate::plans::Plan::Kill).weights();
        assert_eq!(evaluate_state_with(&board(1, 3, "scorched_earth", 3), Side::Corp, &registry, &reference), evaluate_state_with(&board(0, 3, "scorched_earth", 3), Side::Corp, &registry, &reference), "the reference reads no leverage");
    }

    /// The traps plan puts an asset in the scoring remote: a face-down
    /// asset behind the fort keeps the fort the fort and earns the bluff,
    /// where under any other plan the same install makes the remote no
    /// fort at all; and a rezzed asset there is a known asset.
    #[test]
    fn a_traps_corp_puts_an_asset_in_the_fort_and_keeps_the_fort() {
        use netrunner_core::rules::ServerId::{self, Remote};
        let registry = fort_registry();
        let walled = [ServerId::Hq, ServerId::RnD, ServerId::Archives, Remote(0), Remote(0)];
        let mut in_fort = fort_board(&walled, &[("campaign", Remote(0))]);
        let beside = fort_board(&walled, &[("campaign", Remote(1))]);
        let traps = planned(&[crate::plans::Plan::Traps]);
        let glacier = planned(&[crate::plans::Plan::Glacier]);
        let score = |state: &GameState, w: &Weights| evaluate_state_with(state, Side::Corp, &registry, w);
        assert!(score(&in_fort, &traps) > score(&beside, &traps), "traps: the asset goes behind the wall");
        assert!(score(&in_fort, &glacier) < score(&beside, &glacier), "glacier: the fort is for agendas");
        assert!(bluffed_root(&in_fort, &registry, &traps) && !bluffed_root(&beside, &registry, &traps));
        assert_eq!(fort_remote(&in_fort, &registry, &traps), Some(Remote(0)));
        assert_eq!(fort_remote(&in_fort, &registry, &glacier), None, "no fort holds only an asset");
        for card in &mut in_fort.corp.installed {
            if card.card.0 == "campaign" {
                card.rezzed = true;
            }
        }
        assert!(!bluffed_root(&in_fort, &registry, &traps), "rezzed, it is a known asset");
    }

    /// An upgrade is what a fort's root holds beside its agenda (§45): a
    /// remote with Mercia B4LL4RD and an agenda behind two pieces is the
    /// fort, and trashing the upgrade out of it is no fort gained. Read as
    /// "not only what the fort is for", the remote was no fort and LEO
    /// Construction's trash of the Mercia scored the two pieces at
    /// `fort_weight` (+3.0).
    #[test]
    fn a_root_holding_an_upgrade_is_still_the_fort() {
        use netrunner_core::rules::ServerId::{self, Remote};
        let mut registry = fort_registry();
        let mut upgrade = printed(&pool(), "mercia_b4ll4rd");
        upgrade.id = CardId("grid".to_string());
        registry.insert(upgrade);
        let walled = [ServerId::Hq, ServerId::RnD, Remote(0), Remote(0)];
        let with = fort_board(&walled, &[("plan", Remote(0)), ("grid", Remote(0))]);
        let without = fort_board(&walled, &[("plan", Remote(0))]);
        let alone = fort_board(&walled, &[("grid", Remote(0))]);
        let w = planned(&[crate::plans::Plan::Glacier]);
        assert_eq!(fort_remote(&with, &registry, &w), Some(Remote(0)));
        assert_eq!(fort_remote(&alone, &registry, &w), Some(Remote(0)), "the agenda goes in beside it");
        assert_eq!(fort_value(&with, &registry, &w), fort_value(&without, &registry, &w), "the upgrade's trash gains no fort");
    }

    /// "Glacier, then fast advance": once the Runner's rig breaks every
    /// subtype and their credits cover the break into the fort, the
    /// fort's worth falls away and glacier alone keeps paying it. An
    /// agenda short of the wall keeps paying its exposure (§45): it is no
    /// safer for the Runner having beaten the wall. And the Corp cannot
    /// unbeat the wall by taking its own ICE off it (§45): a piece fewer
    /// is a cheaper break, never a wall the rig no longer beats.
    #[test]
    fn the_fort_falls_away_once_the_runner_beats_the_wall_when_fast_advance_follows_glacier() {
        use crate::plans::Plan;
        use netrunner_core::rules::{InstallSlot, ServerId};
        let pool = pool();
        let mut registry = CardRegistry::from_cards(vec![
            advanceable("plan", 3),
            priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1)),
            priced_breaker("decoder", Some(IceType::CodeGate), (1, 1), (1, 1)),
            priced_breaker("killer", Some(IceType::Sentry), (1, 1), (1, 1)),
        ]);
        registry.insert(printed(&pool, "palisade"));
        let board = |runner_credits: u32, rig: &[&str], walls: u32| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(5);
            state.runner.resources.credits = Credits(runner_credits);
            state.runner.rig = rig.iter().map(|id| InstalledRunnerCard { base_strength: 3, ..rig_card(id) }).collect();
            state.corp.installed = (0..walls)
                .map(|n| InstalledCard { card: CardId("palisade".to_string()), install_id: InstallId(1 + n), server: ServerId::Remote(0), slot: InstallSlot::Ice, rezzed: true, ..Default::default() })
                .collect();
            state.corp.installed.push(InstalledCard { card: CardId("plan".to_string()), install_id: InstallId(9), server: ServerId::Remote(1), slot: InstallSlot::Root, ..Default::default() });
            state
        };
        let full = ["cleaver", "decoder", "killer"];
        let rich = board(20, &full, 2);
        let poor = board(0, &full, 2);
        assert!(fort_beaten(&rich, Some(ServerId::Remote(0)), &registry) && !fort_beaten(&poor, Some(ServerId::Remote(0)), &registry));
        assert!(!fort_beaten(&board(20, &["cleaver"], 2), Some(ServerId::Remote(0)), &registry), "a rig that breaks barriers alone has not beaten a Corp that can still ice a code gate");
        assert!(fort_beaten(&board(20, &full, 1), Some(ServerId::Remote(0)), &registry), "a piece fewer does not unbeat the wall");
        assert!(fort_beaten(&rich, None, &registry), "with no fort, the rig is what beats it");
        let stacked = planned(&[Plan::Glacier, Plan::FastAdvance]);
        let alone = planned(&[Plan::Glacier]);
        let swing = |w: &Weights| evaluate_state_with(&rich, Side::Corp, &registry, w) - evaluate_state_with(&poor, Side::Corp, &registry, w);
        // The fort is two pieces and the agenda is naked: its two pieces'
        // worth is dropped when the wall is beaten, and the exposure is not.
        let dropped = -(2.0 * stacked.fort_weight);
        assert!((swing(&stacked) - swing(&alone) - dropped).abs() < 1e-9, "{} vs {}", swing(&stacked), swing(&alone));
        // Taking a piece off a beaten wall is never a gain: before §45 a
        // wall one piece short of the fort's depth was no wall the rig had
        // beaten, and the fort terms came back whole.
        let score = |state: &GameState| evaluate_state_with(state, Side::Corp, &registry, &stacked);
        assert!(score(&board(20, &full, 1)) < score(&rich), "{} against {}", score(&board(20, &full, 1)), score(&rich));
    }

    /// What a run lends the rig lasts the run, so the fort is read as it
    /// stands between runs (§43): a breaker pumped for the run beats the
    /// wall while the run goes on, and the fort terms stay on, so ending
    /// the run does not read as building the fort back.
    #[test]
    fn a_pump_that_lasts_the_run_does_not_beat_the_fort() {
        use crate::plans::Plan;
        use netrunner_core::rules::lingering::{Lingering, LingeringEffect, On, Until};
        use netrunner_core::rules::{InstallSlot, ServerId};
        let pool = pool();
        let mut registry = CardRegistry::from_cards(vec![
            advanceable("plan", 3),
            priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1)),
            priced_breaker("decoder", Some(IceType::CodeGate), (1, 1), (1, 1)),
            priced_breaker("killer", Some(IceType::Sentry), (1, 1), (1, 1)),
        ]);
        registry.insert(printed(&pool, "palisade"));
        let board = |pumped: bool| {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Runner);
            state.corp.resources.credits = Credits(5);
            state.runner.resources.credits = Credits(5);
            state.runner.rig = vec![
                InstalledRunnerCard { install_id: InstallId(10), ..rig_card("cleaver") },
                InstalledRunnerCard { install_id: InstallId(11), ..rig_card("decoder") },
                InstalledRunnerCard { install_id: InstallId(12), ..rig_card("killer") },
            ];
            state.corp.installed = vec![
                InstalledCard { card: CardId("palisade".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Ice, rezzed: true, ..Default::default() },
                InstalledCard { card: CardId("palisade".to_string()), install_id: InstallId(2), server: ServerId::Remote(0), slot: InstallSlot::Ice, rezzed: true, ..Default::default() },
                InstalledCard { card: CardId("plan".to_string()), install_id: InstallId(3), server: ServerId::Remote(1), slot: InstallSlot::Root, ..Default::default() },
            ];
            // A run on HQ, which has no ice: the run's own term is the same
            // either way, and only the pump differs.
            state.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
            if pumped {
                state.lingering.push(LingeringEffect { what: Lingering::Strength(10), on: On::Install(InstallId(10)), until: Until::EndOfRun, source: CardId("cleaver".to_string()) });
            }
            state
        };
        let (pumped, plain) = (board(true), board(false));
        assert!(fort_beaten(&pumped, Some(ServerId::Remote(0)), &registry), "the pump beats the wall while the run lasts");
        assert!(!fort_beaten(&plain, Some(ServerId::Remote(0)), &registry));
        let w = planned(&[Plan::Glacier, Plan::FastAdvance]);
        let score = |state: &GameState| evaluate_state_with(state, Side::Corp, &registry, &w);
        assert!((score(&pumped) - score(&plain)).abs() < 1e-9, "the fort holds between runs: {} vs {}", score(&pumped), score(&plain));
    }
}
