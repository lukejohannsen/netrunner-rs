//! What the terms read off the cards and the board, shared by both arms:
//! what a run costs to break and whether the rig can, the trap and ambush
//! recognisers (read off a card's text, never its name), the rig's
//! coverage of the three ICE subtypes, and the fort's ICE. Nothing here
//! is a weight; every function is a fact a term prices.

use super::*;

/// Whether the Runner can pay to break every pending subroutine on each
/// rezzed ICE it has not yet passed, out of its own credits plus the run's
/// bad-publicity pool. ICE before `run.position` is already behind the
/// Runner; unrezzed ICE is treated as passable (see `ACTIVE_RUN_WEIGHT`).
/// One rezzed ICE no rig card can break makes the whole run unbreakable —
/// a run that stops at the third ICE is worth no more than one that stops
/// at the first.
pub(super) fn run_is_breakable(state: &GameState, run: &RunState, registry: &CardRegistry) -> bool {
    remaining_break_cost(state, run, registry).is_some_and(|total| total <= run_pool(state, run))
}

/// The credits the Runner can put towards breaking in `run`: its own,
/// the run's bad-publicity pool, and the credits the card that began the
/// run brought with it (Phase 5 §31) — Overclock's "you get 5[credit] to
/// spend during that run", `RunState::bonus_run_credits`. The last are
/// counted only when the card lets them pay for a break: the run's own
/// word (`run_credits_pay_for`) is read the way the engine reads it, with
/// no word meaning anything. Before this the leaf read the Runner's own
/// credits and nothing else, so an Overclock run through ICE the Runner
/// could not otherwise afford read as unbreakable — the card's one
/// reason — and the planner played it in 0 of 192 games.
pub(super) fn run_pool(state: &GameState, run: &RunState) -> u32 {
    state.runner.resources.credits.0 + run.bad_publicity_credits + run_credits_for_breaking(run)
}

/// What `run_pool` counts of the run's own credits: all of them when
/// they pay for anything or for using icebreakers, none otherwise.
pub(super) fn run_credits_for_breaking(run: &RunState) -> u32 {
    use netrunner_core::dsl::PaysFor;
    match &run.run_credits_pay_for {
        None | Some(PaysFor::UsingIcebreakers) => run.bonus_run_credits,
        Some(_) => 0,
    }
}

/// Whether a run-ending prevention a card armed stands over `run` and
/// would hold the first "end the run" off (Phase 5 §31): Shred's "the
/// first time the Corp would end that run, prevent the run from ending
/// unless the Corp reveals and trashes X cards from HQ at random", read
/// as the engine resolves it (`prevention::run_ending`) — nothing is
/// prevented over an empty root, because X is then nothing to pay. Over a
/// root with a card in it the Corp either pays in cards from HQ or the
/// run goes on, and this reading takes the run as going on: the Corp's
/// HQ paying for a run it could not otherwise have stopped is the other
/// thing the card is for.
pub(super) fn run_ending_prevented(state: &GameState, run: &RunState) -> bool {
    use netrunner_core::rules::lingering::Lingering;
    use netrunner_core::rules::InstallSlot;
    state.lingering.iter().any(|effect| matches!(effect.what, Lingering::PreventRunEnding(_)) && effect.holds(state))
        && state.corp.installed.iter().any(|card| card.server == run.server && card.slot == InstallSlot::Root)
}

/// The credits still to be spent breaking this run's rezzed ICE, or
/// `None` when one of them no rig card can break. Split out of
/// `run_is_breakable` so the same number both gates the run and pays for
/// it: what the Runner can afford to trash at the end is what it has
/// left **after** breaking in, not the credits it starts the run with.
/// Before this the Runner could break in and arrive unable to trash the
/// asset it came for.
///
/// **Charging the run for those credits as well was measured and
/// rejected** (Phase 5 §20). Subtracting `remaining_break_cost ×
/// own_credit_weight` from the run's value makes one hidden access lose
/// to 3[c] of ICE, and the Runner stopped running: 17.2 runs a game →
/// 14.6, and the Corp won 0.239 → 0.259 over the whole pool (three seeds
/// × 192) and +0.029 on the trap decks (six seeds × 216, t 4.4). A
/// breach is worth more than the credits it costs, because the credits
/// come back and the agenda does not.
///
/// **One piece no rig card can break is passed for nothing when a
/// run-ending prevention stands** (`run_ending_prevented`, Shred): the
/// first "end the run" is held off once, so the first such piece stops
/// nothing — its other subroutines fire, which the encounter's own terms
/// price when the run gets there — and a second is unbreakable as ever.
pub(super) fn remaining_break_cost(state: &GameState, run: &RunState, registry: &CardRegistry) -> Option<u32> {
    let mut total = 0;
    let mut stock = fresh_stock(state);
    let mut prevention = run_ending_prevented(state, run);
    for ice in run.ice.iter().skip(run.position).filter(|ice| ice.rezzed) {
        match cheapest_break_cost(state, ice, registry, &mut stock) {
            Some(cost) => total += cost,
            None if prevention => prevention = false,
            None => return None,
        }
    }
    Some(total)
}

/// How much of each rig card's stock — the counters a break removes, the
/// hosted copies it turns facedown — a server's pricing has spent so far,
/// one count per rig position: the ledger `cheapest_break_cost` draws on
/// and adds to as it walks a server's ICE, so that one hosted copy of
/// Matryoshka does not price a two-ICE server as breakable twice over.
/// Greedy, ICE by ICE in the order the run meets them: the cheapest card
/// for the first piece may be the only card that could have taken the
/// second, and a server of two or three pieces does not need better. Each
/// card holds one kind of stock (no card in the pool pays in both counters
/// and copies), so one count a card is enough.
pub(super) type Stock = Vec<u32>;

/// A ledger with nothing spent, for one server's pricing.
pub(super) fn fresh_stock(state: &GameState) -> Stock {
    vec![0; state.runner.rig.len()]
}

/// What the rig would spend to break every *rezzed* piece of ice
/// protecting `server` outright, or `None` when one of them no rig card
/// can break — `remaining_break_cost`'s reading taken off the table
/// instead of a run, so a diagnostic can set the Runner's credits against
/// the price of the Corp's scoring remote at the moment the Corp scores
/// (Phase 5 §25 precept 2, the taxing window) and at each Runner turn
/// start. Each piece is read as a run would first meet it: every printed
/// subroutine pending, nothing gained. Unrezzed ice costs nothing here,
/// as it costs nothing in a run, which is the same optimism `run_is_
/// breakable` has and the same reason: a rez is the Corp's to make.
pub fn server_break_cost(state: &GameState, server: netrunner_core::rules::ServerId, registry: &CardRegistry) -> Option<u32> {
    use netrunner_core::rules::{EncounteredSubroutine, InstallSlot, RunIce};
    let mut total = 0;
    let mut stock = fresh_stock(state);
    for installed in state.corp.installed.iter().filter(|c| c.server == server && c.slot == InstallSlot::Ice && c.rezzed) {
        let def = registry.get(&installed.card)?;
        let CardType::Ice(ice_type) = def.card_type else { continue };
        let ice = RunIce {
            card_id: installed.card.clone(),
            install_id: installed.install_id,
            ice_type,
            subroutines: def
                .subroutines
                .iter()
                .enumerate()
                .map(|(id, definition)| EncounteredSubroutine { id, definition: definition.clone(), status: SubroutineStatus::Pending, gained: false })
                .collect(),
            rezzed: true,
        };
        total += cheapest_break_cost(state, &ice, registry, &mut stock)?;
    }
    Some(total)
}

/// The damage a known trap would do if accessed now: its fixed damage,
/// plus a point per hosted token for the kind that grows (Urtica
/// Cipher), and a paid trap's (Snare!, Byte!) only when the Corp holds
/// its price. Read off the same effects `punishes_access_with_damage`
/// recognises, so a trap the recogniser admits has a number here.
pub(super) fn trap_damage(state: &GameState, installed: &InstalledCard, def: &CardDefinition) -> usize {
    let mut damage = 0usize;
    let mut count = |effect: &Effect| match effect {
        Effect::DealDamage(_, n) => damage += *n,
        Effect::DealDamageAmount(_, Amount::HostedAdvancementTokens) => damage += installed.advancement_tokens as usize,
        _ => {}
    };
    for trigger in def.triggers.iter().filter(|trigger| trigger.trigger == Trigger::OnAccessed) {
        for effect in &trigger.effects {
            effect.for_each_effect(&mut count);
        }
    }
    if let Some(access) = &def.interactive_on_access {
        let affordable = match access.cost {
            netrunner_core::dsl::Cost::Credits(price) => state.corp.resources.credits.0 >= price,
            _ => true,
        };
        if affordable {
            for effect in &access.effects {
                effect.for_each_effect(&mut count);
            }
        }
    }
    damage
}

/// Unrezzed ICE still ahead of the Runner that the Corp could rez right
/// now and no rig card could then break. The counterpart to
/// `run_is_breakable`, which deliberately looks only at rezzed ICE — see
/// `UNREZZED_THREAT_WEIGHT` for why this is a separate weighted term
/// rather than a wider predicate.
///
/// **This is the evaluator's one window onto hidden information.** In a
/// determinized state `ice.card_id` is the *sampled* card, so two samples
/// that put different ICE behind the same install disagree here and
/// nowhere else. In a real `GameState` it is the true card, which is
/// correct for the Corp's own reasoning and is why the gym's shaped
/// reward should not turn this on for a Runner it computes from the
/// authoritative state.
///
/// Rez affordability is the printed cost against the Corp's credits,
/// ignoring what is added to it (`continuous::rez_cost_delta` — Tread
/// Lightly's +3, Fransofia Ward's +1) and any discount: over-estimating
/// what the Corp can pay only makes the Runner one ICE more cautious,
/// the same direction `breaker_savings_shortfall` already errs in.
pub(super) fn unbreakable_unrezzed_ice(state: &GameState, run: &RunState, registry: &CardRegistry) -> usize {
    run.ice.iter().skip(run.position).filter(|ice| is_unrezzed_threat(state, ice, registry)).count()
}

/// Whether this one ICE is what `unbreakable_unrezzed_ice` counts:
/// unrezzed, affordable to the Corp at its printed cost, breakable by
/// nothing in the rig, **and carrying a subroutine that ends the run**.
///
/// That last clause is ROADMAP Phase 2 §5 item 39 and it halves what the
/// term counts. `cheapest_break_cost` returning `None` says no rig card
/// can break the ICE, which is not the same claim as "it stops you": an
/// ICE that tags, or does damage, or drains credits costs the Runner
/// something and then lets the run continue. Measured over six 96-game
/// legs, of the counted ICE the Corp rezzed, the run stopped there
/// **0.904** of the time with an ETR subroutine and **0.077** without,
/// and the two classes were an even split of what was counted. Item 38
/// read that null as a missing *probability* — how likely the Corp is to
/// rez — and item 39 measured the rez rate at 0.937 against the Corp
/// actually seated, which left nothing to discount.
///
/// Public because `diag rez-rate` measures how often a real Corp rezzes
/// exactly the ICE this predicate flags, and a copy of the predicate in
/// the CLI would be free to drift from the one the evaluator uses — the
/// whole point of that measurement is that it prices *this* term.
pub fn is_unrezzed_threat(state: &GameState, ice: &RunIce, registry: &CardRegistry) -> bool {
    if ice.rezzed {
        return false;
    }
    let Some(definition) = registry.get(&ice.card_id) else { return false };
    definition.cost <= state.corp.resources.credits.0
        // Every subroutine of an unrezzed ICE is `Pending` by
        // construction, so this reads the same set `cheapest_break_cost`
        // prices.
        && ice.subroutines.iter().any(|subroutine| subroutine.definition.effect.can_end_the_run())
        && cheapest_break_cost(state, ice, registry, &mut fresh_stock(state)).is_none()
}

/// The fewest credits any rig card needs to pump up to `ice`'s strength
/// and break all of its pending subroutines, with what it has left of its
/// stock after `stock` says what this server's earlier ICE took; `None`
/// when no rig card can. The card chosen has its spend added to the
/// ledger. Ties go to the card that spends the least stock, so a plain
/// credit breaker is used ahead of a counter it could save. An ICE with
/// nothing pending costs nothing whatever the rig holds.
pub(super) fn cheapest_break_cost(state: &GameState, ice: &RunIce, registry: &CardRegistry, stock: &mut Stock) -> Option<u32> {
    if pending_on(ice) == 0 {
        return Some(0);
    }
    let (credits, spent, position) = state
        .runner
        .rig
        .iter()
        .enumerate()
        .filter_map(|(position, card)| {
            let already = stock.get(position).copied().unwrap_or(0);
            break_cost(state, card, ice, registry, already).map(|spend| (spend.credits, spend.stock, position))
        })
        .min_by_key(|(credits, spent, _)| (*credits, *spent))?;
    if let Some(slot) = stock.get_mut(position) {
        *slot += spent;
    }
    Some(credits)
}

/// What a rig card spends breaking one piece of ICE: credits, and the
/// units of its stock the activations draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Spend {
    pub credits: u32,
    pub stock: u32,
}

/// What one activation of a breaker's ability costs, and how many times
/// the card can pay it right now — `None` for a cost this reading does
/// not price, which skips the ability as an unpriced cost always has.
struct Price {
    credits: u32,
    /// How many activations the card's stock covers: counters for
    /// `RemoveCounters`, faceup hosted copies for `TurnHostedFacedown`;
    /// `None` for a cost with no stock, payable as often as the credits
    /// allow.
    stock: Option<u32>,
}

/// A printed cost read the way the engine would charge it, for a break of
/// `pending` subroutines (Phase 5 §28). Every cost shape a breaker in the
/// pool prints is here, and what each one charges:
///
/// - `Credits(c)`: `c`.
/// - `CreditsX`: the X of "X[credit]: Break X subroutines" is chosen to
///   cover them all, so one activation costs `pending` (Matryoshka,
///   Lobisomem).
/// - `CreditsAmount`: the table's number (Tremolo's "3[credit], 1 less for
///   each installed cybernetic hardware"), read by the engine's own
///   `amount_on_table`.
/// - `RemoveCounters(n)`: free, and only as many times as the card's
///   counters cover (Audrey v2's break, Hantu's pump).
/// - `TurnHostedFacedown`: free, and only as many times as the card has a
///   copy still faceup (Matryoshka) — `faceup_hosted`, never every hosted
///   copy: one turned facedown on the turn's first run is not back until
///   the Runner's next turn begins.
/// - `AllOf`: the credits summed, the tightest stock.
///
/// Before this, only `Credits` was priced and every other break was
/// skipped, so Matryoshka with a copy hosted and Lobisomem with a counter
/// read as breaking nothing — and a hosted copy, which the evaluator
/// could not see paying for anything, was never worth the click that
/// hosts it. Botulus's and Poison Vial's counter-costed
/// `BreakSubroutinesUnconditionally` is still not a spend this reading is
/// about (Botulus's is on its host ICE alone, which no cost says), and
/// Audrey v2's pump pays in a grip card, which is not priced either.
fn price_of(cost: Option<&Cost>, card: &InstalledRunnerCard, pending: u32, state: &GameState, registry: &CardRegistry) -> Option<Price> {
    Some(match cost {
        None => Price { credits: 0, stock: None },
        Some(Cost::Credits(credits)) => Price { credits: *credits, stock: None },
        Some(Cost::CreditsX { .. }) => Price { credits: pending, stock: None },
        Some(Cost::CreditsAmount(amount)) => Price { credits: netrunner_core::rules::amount_on_table(amount, state, registry), stock: None },
        Some(Cost::RemoveCounters(each)) => Price { credits: 0, stock: Some(card.counters / (*each).max(1)) },
        Some(Cost::TurnHostedFacedown) => Price { credits: 0, stock: Some(card.faceup_hosted()) },
        Some(Cost::AllOf(parts)) => {
            let mut price = Price { credits: 0, stock: None };
            for part in parts {
                let part = price_of(Some(part), card, pending, state, registry)?;
                price.credits += part.credits;
                price.stock = match (price.stock, part.stock) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
            }
            price
        }
        Some(_) => return None,
    })
}

/// Whether `card` could pay `cost` at least once right now, as far as its
/// stock goes — a break whose copies are all facedown, or whose counters
/// are gone, is a break the card does not have.
fn stocked(cost: Option<&Cost>, card: &InstalledRunnerCard, pending: u32, state: &GameState, registry: &CardRegistry) -> bool {
    price_of(cost, card, pending, state, registry).is_some_and(|price| price.stock.is_none_or(|stock| stock > 0))
}

/// What `card` would spend to break `ice` outright: pump credits to close
/// any strength shortfall, then break credits for every pending
/// subroutine, both read off its `Paid` abilities and each cost priced by
/// `price_of`, with `already` units of the card's stock spent on this
/// server's earlier ICE. `None` if the card has no break matching `ice`'s
/// subtype, a shortfall and no pump, or too little stock left for the
/// activations it would take. A requirement on the ability is not read
/// (Poison Vial's "only if you have already broken a subroutine"). The
/// pump and the break are bounded by the same stock separately, which is
/// exact while no card pays for both out of one pool. `BoostStrengthAmount`
/// (Unity's +X) is priced as +1 per activation: X counts Unity itself so
/// it is at least 1, and over-estimating a cost only makes the Runner save
/// one click longer.
pub(super) fn break_cost(state: &GameState, card: &InstalledRunnerCard, ice: &RunIce, registry: &CardRegistry, already: u32) -> Option<Spend> {
    let def = registry.get(&card.card)?;
    let pending = pending_on(ice);
    // The number the break contest uses — a pump bought inside the search
    // and what the table adds (Echelon, Rising Tide) included.
    let shortfall = (continuous::ice_strength(state, registry, ice) - continuous::breaker_strength(state, registry, card)).max(0) as u32;
    let mut cheapest_break: Option<(u32, u32)> = None;
    let mut cheapest_pump: Option<(u32, u32)> = None;
    let keep_min = |slot: &mut Option<(u32, u32)>, cost: (u32, u32)| *slot = Some(slot.map_or(cost, |c| c.min(cost)));
    for ability in def.abilities.iter().filter(|a| a.trigger == Trigger::Paid) {
        let Some(price) = price_of(ability.cost.as_ref(), card, pending, state, registry) else { continue };
        let left = price.stock.map(|stock| stock.saturating_sub(already));
        // `(credits, stock drawn)` for `activations` of this ability, or
        // nothing when the stock does not cover them.
        let spend = |activations: u32| -> Option<(u32, u32)> {
            if left.is_some_and(|left| left < activations) {
                return None;
            }
            Some((price.credits * activations, if price.stock.is_some() { activations } else { 0 }))
        };
        ability.effect.for_each_effect(&mut |effect| match effect {
            Effect::BreakSubroutines { count, restrict_to } if restrict_to.is_none_or(|r| ice_is(state, ice, r, registry)) => {
                let activations = match count {
                    SubroutineBreakCount::Fixed(n) => pending.div_ceil((*n).max(1)),
                    // X is chosen to cover them all (`price_of`).
                    SubroutineBreakCount::All | SubroutineBreakCount::ChosenNumber => 1,
                };
                if let Some(cost) = spend(activations) {
                    keep_min(&mut cheapest_break, cost);
                }
            }
            Effect::BoostStrength { amount, .. } => {
                if let Some(cost) = spend(shortfall.div_ceil((*amount).max(1))) {
                    keep_min(&mut cheapest_pump, cost);
                }
            }
            Effect::BoostStrengthAmount { .. } => {
                if let Some(cost) = spend(shortfall) {
                    keep_min(&mut cheapest_pump, cost);
                }
            }
            _ => {}
        });
    }
    let (break_credits, break_stock) = cheapest_break?;
    let (pump_credits, pump_stock) = if shortfall == 0 { (0, 0) } else { cheapest_pump? };
    Some(Spend { credits: break_credits + pump_credits, stock: break_stock + pump_stock })
}

/// Subroutines on `ice` still waiting to be broken or resolved.
pub(super) fn pending_on(ice: &RunIce) -> u32 {
    ice.subroutines.iter().filter(|s| s.status == SubroutineStatus::Pending).count() as u32
}

/// How far the strongest rig breaker able to break the encountered ICE's
/// subtype falls short of its strength — zero when a breaker matches or
/// exceeds it, when no breaker matches at all (there is nothing to pump),
/// or outside an encounter.
pub(super) fn strength_shortfall(state: &GameState, run: &RunState, registry: &CardRegistry) -> i32 {
    if run.phase != RunPhase::EncounterIce {
        return 0;
    }
    let Some(ice) = run.ice.get(run.position) else { return 0 };
    let best = state
        .runner
        .rig
        .iter()
        .filter(|card| breaks_subtype(state, card, ice, registry))
        .map(|card| continuous::breaker_strength(state, registry, card))
        .max();
    best.map_or(0, |strength| (continuous::ice_strength(state, registry, ice) - strength).max(0))
}

/// Whether `ice` is a `subtype` right now: the one it prints, or one the
/// table gives it (a hosted GAMEDRAGON™ Pro's "host ice gains barrier") —
/// `BreakSubroutines`' own test, so a break the engine would accept is one
/// this prices.
pub(super) fn ice_is(state: &GameState, ice: &RunIce, subtype: IceType, registry: &CardRegistry) -> bool {
    ice.ice_type == subtype || continuous::ice_gains_subtype(state, registry, ice.install_id, subtype)
}

/// Whether `card`'s abilities include a `BreakSubroutines` that applies to
/// `ice` — restricted to a subtype it has, or unrestricted — **that the
/// card could pay for right now** (`stocked`): a Matryoshka with every
/// copy facedown matches nothing, so there is nothing to pump.
pub(super) fn breaks_subtype(state: &GameState, card: &netrunner_core::rules::InstalledRunnerCard, ice: &RunIce, registry: &CardRegistry) -> bool {
    let Some(def) = registry.get(&card.card) else { return false };
    let mut found = false;
    for ability in def.abilities.iter().filter(|ability| stocked(ability.cost.as_ref(), card, pending_on(ice), state, registry)) {
        ability.effect.for_each_effect(&mut |effect| {
            if let Effect::BreakSubroutines { restrict_to, .. } = effect
                && restrict_to.is_none_or(|r| ice_is(state, ice, r, registry))
            {
                found = true;
            }
        });
    }
    found
}

/// Subroutines on the currently encountered ICE that have neither been
/// broken nor resolved. Zero outside an encounter.
pub(super) fn pending_subroutines(run: &RunState) -> usize {
    if run.phase != RunPhase::EncounterIce {
        return 0;
    }
    run.ice.get(run.position).map_or(0, |ice| pending_on(ice) as usize)
}

/// Every effect the card declares anywhere — triggers, abilities and its
/// access interaction — so the recognisers below read a card's *text*
/// rather than its name. That distinction is the whole point: a card
/// named here would be a preference, and `personality` forbids those; a
/// shape recognised here values the next card with the same text for
/// free.
pub(super) fn for_each_declared_effect(def: &CardDefinition, f: &mut impl FnMut(&Effect)) {
    let declared = def
        .triggers
        .iter()
        .flat_map(|trigger| trigger.effects.iter())
        .chain(def.abilities.iter().map(|ability| &ability.effect))
        .chain(def.interactive_on_access.iter().flat_map(|access| access.effects.iter()));
    for effect in declared {
        effect.for_each_effect(f);
    }
}

/// An ambush that grows: its damage *is* its advancement-token count.
pub fn damage_grows_with_advancement(def: &CardDefinition) -> bool {
    let mut found = false;
    for_each_declared_effect(def, &mut |effect| {
        if matches!(effect, Effect::DealDamageAmount(_, Amount::HostedAdvancementTokens)) {
            found = true;
        }
    });
    found
}

/// An ambush at all: a non-agenda that answers being accessed with
/// damage, whether through an `OnAccessed` trigger (*Urtica Cipher*) or
/// a paid access interaction (*Snare!*).
pub fn punishes_access_with_damage(def: &CardDefinition) -> bool {
    // An agenda that hurts on access is not an ambush — the Runner takes
    // the points anyway — and an identity is never an install, so neither
    // is a card this term should price.
    if matches!(def.card_type, CardType::Agenda | CardType::Identity) {
        return false;
    }
    let deals_damage = |effect: &Effect| matches!(effect, Effect::DealDamage(..) | Effect::DealDamageAmount(..));
    let mut found = false;
    let on_access = def
        .triggers
        .iter()
        .filter(|trigger| trigger.trigger == Trigger::OnAccessed)
        .flat_map(|trigger| trigger.effects.iter())
        .chain(def.interactive_on_access.iter().flat_map(|access| access.effects.iter()));
    for effect in on_access {
        effect.for_each_effect(&mut |e| found |= deals_damage(e));
    }
    found
}

/// A trap the Corp plays like an agenda: it can be advanced and its
/// damage is its token count (Urtica Cipher). `fort_value` counts a remote
/// holding an unseen one as the fort.
pub fn is_lure_trap(def: &CardDefinition) -> bool {
    punishes_access_with_damage(def) && def.advancement_requirement.is_some() && damage_grows_with_advancement(def)
}

/// A trap that belongs in HQ and R&D: it punishes access but cannot take
/// a token, so a remote gives it nothing (Snare!, Byte!). See
/// `HELD_TRAP_WEIGHT`.
pub fn is_hand_trap(def: &CardDefinition) -> bool {
    punishes_access_with_damage(def) && !is_lure_trap(def)
}

/// How many of the three ICE subtypes the rig can break: a rig card whose
/// abilities contain `Effect::BreakSubroutines` covers its `restrict_to`
/// subtype, or all three when unrestricted (an AI breaker).
///
/// Public for the same reason `is_unrezzed_threat` is: `netrunner_cli diag
/// tempo` reports how far along its rig the Runner is when it starts
/// running, and a diagnostic that re-derived "coverage" itself would be
/// measuring its own copy rather than the term the Runner actually reads.
pub fn breaker_coverage(state: &GameState, registry: &CardRegistry) -> usize {
    rig_coverage(state, registry).iter().filter(|c| **c).count()
}

/// The subtypes the rig can break, as `covers` flags OR-ed over every rig
/// card.
pub(super) fn rig_coverage(state: &GameState, registry: &CardRegistry) -> [bool; 3] {
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
/// than a set would anyway. Shared with `observation`'s rig and coverage
/// blocks so the network is shown exactly what this evaluator counts.
pub fn covers(def: &CardDefinition) -> [bool; 3] {
    let mut covered = [false; 3];
    for ability in &def.abilities {
        ability.effect.for_each_effect(&mut |effect| {
            if let Effect::BreakSubroutines { restrict_to, .. } = effect {
                match restrict_to {
                    Some(subtype) => {
                        if let Some(slot) = subtype_slot(*subtype) {
                            covered[slot] = true;
                        }
                    }
                    None => covered = [true; 3],
                }
            }
        });
    }
    covered
}

/// A subtype's index into a `covers`/`rig_coverage` flag array. Shared so
/// that the Corp's `UNBREAKABLE_ICE_WEIGHT` indexes the same array the
/// Runner's `BREAKER_COVERAGE_WEIGHT` fills, rather than each end keeping
/// its own copy of the order. `None` for ice that prints none of the three
/// (`IceType::Other`), which has no flag of its own: the observation shares
/// these arrays and a fourth flag would move `OBS_SIZE` for one card.
pub(super) fn subtype_slot(subtype: IceType) -> Option<usize> {
    match subtype {
        IceType::Barrier => Some(0),
        IceType::CodeGate => Some(1),
        IceType::Sentry => Some(2),
        IceType::Other => None,
    }
}

/// What a card's declared text pays its owner, read off the DSL the way
/// the trap recognisers are — never off the card's name (Phase 5 §25
/// Stage 5). One reading serves three questions: what a play is worth
/// (`play_value`), what an installed economy card will still pay over the
/// turns left (`future_credits`), and so what the card is worth in hand.
///
/// A card *declares* an economy; it does not declare an income schedule,
/// so the schedule here is the one a person reads off the same words:
/// "when your turn begins, gain 1[c]" is a credit a turn; "[click]: take
/// 3[c] from this asset" is a click's use a turn, worth what it takes
/// over the credit the click would have bought; counters the credits are
/// taken from are the stock and bound the total; "[trash]: gain 2[c] for
/// each hosted counter" is the counters cashed once. Everything read is
/// a `Fixed` amount: a computed `Amount` is counted as nothing, the
/// cheaper direction.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Income {
    /// Credits a play nets its owner — the gains its "when you play"
    /// text declares less its play cost — for an event or operation.
    pub play_credits: i32,
    /// Cards the play draws.
    pub play_cards: u32,
    /// Clicks the play gives (Nanomanagement) or takes (Creative
    /// Commission), the click spent playing it not counted.
    pub play_clicks: i32,
    /// Credits each of its owner's turn starts pays while the card is
    /// active, recurring credits included.
    pub turn_credits: u32,
    /// Cards each turn start draws.
    pub turn_cards: u32,
    /// Credits one use of the card's own click ability takes (or, on a
    /// card whose turn starts pay off its counters, the counters one use
    /// places), and the clicks that use costs.
    pub click_credits: u32,
    pub click_cost: u32,
    /// The click ability trashes the card: one use, not one a turn.
    pub click_trashes: bool,
    /// The click ability places the credits on the card for its turn
    /// starts to pay off (Smartware Distributor) rather than taking them:
    /// the click is priced by the stock it adds when it is taken, so the
    /// future counts the stock and not the clicks.
    pub click_places: bool,
    /// The click ability begins a run (Red Team's "[click]: Run a central
    /// server … if successful, take 3[credit]"), so its click buys the run
    /// and is not charged against what the rider pays: the Runner makes
    /// more than one run a turn (18.6 runs in 13.1 turns, the planner's
    /// casual pass at Phase 5 §31), and a run it began by this card is one
    /// it would have made anyway. `click_credits` is then the rider's.
    pub click_runs: bool,
    /// Credits each successful run pays while the card is active, taken
    /// at one successful run a turn (the planner's casual pass: 11.6 in
    /// 13.1 turns, startup 10.2 in 12.6): a trigger on any successful run
    /// that gains credits, or that places counters on a card whose text
    /// cashes them (Pennyshaver's "whenever you make a successful run,
    /// place 1[credit] on this hardware"). Only a trigger about every
    /// successful run, with no condition — one narrowed to a server
    /// (Gabriel Santiago's HQ) or to this card's (Stowaway) is not a run
    /// a turn and is counted as nothing, the cheaper direction.
    pub run_credits: u32,
    /// Credits per hosted counter a trash-and-cash ability pays.
    pub cashout_per_counter: u32,
    /// The credits come off hosted counters, so the counters bound them;
    /// `printed_stock` is what the card places on itself when it arrives.
    pub stocked: bool,
    pub printed_stock: u32,
}

/// What one effect adds up to for `side`: credits, cards, clicks and
/// counters, and the cashout rate if it names one. A `Sequence` is the
/// sum; an `EffectIf` is taken as if its condition held (the card is
/// read for what it can do); a `PresentChoice` is the best option when
/// `side` chooses and the worst when the opponent does (Wildcat Strike
/// is the Corp's to answer). Anything else is nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Tally {
    credits: i32,
    cards: i32,
    clicks: i32,
    counters: i32,
    cashout_per_counter: u32,
}

impl Tally {
    fn add(self, other: Tally) -> Tally {
        Tally {
            credits: self.credits + other.credits,
            cards: self.cards + other.cards,
            clicks: self.clicks + other.clicks,
            counters: self.counters + other.counters,
            cashout_per_counter: self.cashout_per_counter.max(other.cashout_per_counter),
        }
    }

    /// The order two options of a choice are compared in: credits,
    /// cards and clicks alike, each about a click's worth.
    fn worth(self) -> i32 {
        self.credits + self.cards + self.clicks
    }
}

fn tally(effect: &Effect, side: Side) -> Tally {
    let own = |s: &Side| *s == side;
    match effect {
        Effect::GainCredits(s, n) if own(s) => Tally { credits: *n as i32, ..Default::default() },
        Effect::LoseCredits(s, n) if own(s) => Tally { credits: -(*n as i32), ..Default::default() },
        Effect::DrawCards(s, n) if own(s) => Tally { cards: *n as i32, ..Default::default() },
        Effect::GainClicks(s, n) if own(s) => Tally { clicks: *n as i32, ..Default::default() },
        Effect::LoseClicks(n) => Tally { clicks: -(*n as i32), ..Default::default() },
        Effect::AddCounters(n) => Tally { counters: *n as i32, ..Default::default() },
        Effect::RemoveCounters(Amount::Fixed(n)) => Tally { counters: -(*n as i32), ..Default::default() },
        Effect::GainCreditsPerCounter { side: s, credits_per_counter } if own(s) => {
            Tally { cashout_per_counter: *credits_per_counter, ..Default::default() }
        }
        Effect::TakeAllCountersAsCredits(s) if own(s) => Tally { cashout_per_counter: 1, ..Default::default() },
        Effect::Sequence(effects) => effects.iter().fold(Tally::default(), |sum, effect| sum.add(tally(effect, side))),
        Effect::EffectIf { effect, .. } => tally(effect, side),
        Effect::PresentChoice { chooser, options, .. } => {
            let tallies = options.iter().map(|option| tally(option, side));
            if *chooser == side { tallies.max_by_key(|t| t.worth()) } else { tallies.min_by_key(|t| t.worth()) }
                .unwrap_or_default()
        }
        _ => Tally::default(),
    }
}

/// What the rider on `run` pays `side` when the run succeeds (Phase 5
/// §31): the credits and cards its `on_success_effect` declares, tallied
/// as a play is (`tally`: a `Sequence` summed, an `EffectIf` as if its
/// condition held, the opponent's choice at its worst) — Clean Getaway's
/// "if successful, gain 6[credit]", Red Team's "take 3[credit] from this
/// resource", Jailbreak's and Joy Ride's draws. A rider that pays in
/// anything else is nothing here; the accesses it adds are
/// `rider_accesses`. The rider is seeded onto the run by the card that
/// began it and is read here only off a run the search itself began, so
/// it is never a sampled guess: a run in progress when the view was taken
/// carries none (`determinize` leaves it `None`).
pub(super) fn rider_income(run: &RunState, side: Side) -> (i32, u32) {
    let sum = run.on_success_effect.as_deref().map(|effect| tally(effect, side)).unwrap_or_default();
    (sum.credits, sum.cards.max(0) as u32)
}

/// The accesses the rider on `run` adds to a breach of `server` beyond
/// the first — Jailbreak's "access 1 additional card" (`Effect::
/// AddAdditionalAccess`, rewritten to the chosen server when the run was
/// begun). Read off the rider because the run's own count
/// (`additional_hq_access`, `additional_rd_access`) is written only when
/// the run succeeds.
pub(super) fn rider_accesses(run: &RunState, server: netrunner_core::rules::ServerId) -> u32 {
    let mut count = 0;
    if let Some(effect) = run.on_success_effect.as_deref() {
        effect.for_each_effect(&mut |effect| {
            if let Effect::AddAdditionalAccess { server: added, count: n } = effect
                && *added == server
            {
                count += *n;
            }
        });
    }
    count
}

/// `Income` for `def`, read as the struct's docs say.
pub(super) fn declared_income(def: &CardDefinition) -> Income {
    let side = def.side;
    let mut income = Income::default();
    if matches!(def.card_type, CardType::Event | CardType::Operation) {
        let play = def
            .triggers
            .iter()
            .filter(|trigger| trigger.trigger == Trigger::OnPlay)
            .flat_map(|trigger| trigger.effects.iter())
            .fold(Tally::default(), |sum, effect| sum.add(tally(effect, side)));
        income.play_credits = play.credits - def.cost as i32;
        income.play_cards = play.cards.max(0) as u32;
        income.play_clicks = play.clicks;
        return income;
    }
    // Counters every successful run places, cashed below if the card's
    // text cashes them.
    let mut run_counters = 0;
    for trigger in &def.triggers {
        let sum = trigger.effects.iter().fold(Tally::default(), |sum, effect| sum.add(tally(effect, side)));
        match trigger.trigger {
            Trigger::OnTurnStart => {
                income.turn_credits += sum.credits.max(0) as u32;
                income.turn_cards += sum.cards.max(0) as u32;
                if sum.counters < 0 {
                    income.stocked = true;
                }
            }
            Trigger::OnRez | Trigger::OnInstall => income.printed_stock += sum.counters.max(0) as u32,
            Trigger::OnSuccessfulRun if about_every_run(trigger) => {
                income.run_credits += sum.credits.max(0) as u32;
                run_counters += sum.counters.max(0) as u32;
            }
            _ => {}
        }
    }
    income.turn_credits += def.recurring_credits.unwrap_or(0);
    for ability in def.abilities.iter().filter(|ability| ability.trigger == Trigger::Paid) {
        let (clicks, trashes) = click_cost(ability.cost.as_ref());
        if clicks == 0 && !trashes {
            continue;
        }
        // A run the ability begins pays what its rider declares on
        // success (`run_rider`); the ability's own text around the run is
        // tallied as any other.
        let rider = run_rider(&ability.effect);
        let sum = tally(&ability.effect, side).add(rider.flatten().map(|rider| tally(rider, side)).unwrap_or_default());
        if sum.cashout_per_counter > 0 {
            income.cashout_per_counter = income.cashout_per_counter.max(sum.cashout_per_counter);
            continue;
        }
        // Credits the click takes off the card (Regolith, Telework), or
        // counters it places that the card's own turn starts pay off
        // (Smartware Distributor): either is credits for a click.
        let credits = if sum.credits > 0 {
            sum.credits as u32
        } else if sum.counters > 0 && income.stocked && income.turn_credits > 0 {
            sum.counters as u32
        } else {
            continue;
        };
        if credits > income.click_credits {
            income.click_credits = credits;
            income.click_cost = clicks;
            income.click_trashes = trashes;
            income.click_places = sum.credits <= 0;
            income.click_runs = rider.is_some();
            if sum.counters < 0 {
                income.stocked = true;
            }
        }
    }
    income.run_credits += run_counters * income.cashout_per_counter;
    income
}

/// Whether a trigger on a successful run is about every successful run:
/// any run's, on any server, with no condition to meet.
fn about_every_run(trigger: &netrunner_core::dsl::TriggeredEffect) -> bool {
    use netrunner_core::dsl::Subject;
    trigger.subject != Some(Subject::This) && trigger.when.is_none() && trigger.requirement.is_none() && !trigger.first_each_turn
}

/// The rider a click ability's run pays on success, if the ability begins
/// a run: `Some(Some(_))` for `PromptChooseServer`'s `on_success` (Red
/// Team's "take 3[credit] from this resource"), `Some(None)` for a run
/// with no rider (`InitiateRun`, Conduit's — whose accesses are read as
/// accesses, not income), `None` for an ability that begins no run.
fn run_rider(effect: &Effect) -> Option<Option<&Effect>> {
    match effect {
        Effect::PromptChooseServer { on_success, .. } => Some(on_success.as_deref()),
        Effect::InitiateRun(_) => Some(None),
        Effect::Sequence(effects) => {
            let riders: Vec<_> = effects.iter().filter_map(run_rider).collect();
            if riders.is_empty() { None } else { Some(riders.into_iter().flatten().next()) }
        }
        Effect::EffectIf { effect, .. } => run_rider(effect),
        _ => None,
    }
}

/// The clicks a cost takes and whether it trashes the card.
fn click_cost(cost: Option<&Cost>) -> (u32, bool) {
    match cost {
        None => (0, false),
        Some(Cost::Clicks(n)) => (*n, false),
        Some(Cost::TrashSelf) => (0, true),
        Some(Cost::AllOf(parts)) => parts.iter().fold((0, false), |(clicks, trashes), part| {
            let (c, t) = click_cost(Some(part));
            (clicks + c, trashes || t)
        }),
        Some(_) => (0, false),
    }
}

/// Credits an active card will still pay over `horizon` more of its
/// owner's turns, net of the clicks its uses cost: its turn-start credits
/// and cards (a card at a credit), its click ability used once a turn
/// for what it takes over the credit the click would have bought, both
/// bounded by the stock when the credits come off counters (`hosted`, or
/// the printed stock for a card not yet on the table), and its counters
/// cashed at the rate its text names. What a successful run pays it is a
/// run a turn's, and a click ability that begins a run is not charged its
/// click (Phase 5 §32: Pennyshaver, Red Team). Zero for a card that
/// declares no economy.
pub(super) fn future_credits(income: &Income, hosted: Option<u32>, horizon: u32) -> f64 {
    let stock = if income.stocked { Some(hosted.unwrap_or(income.printed_stock)) } else { None };
    let turn = (income.turn_credits + income.turn_cards) * horizon;
    let turn = stock.map_or(turn, |stock| turn.min(stock));
    // A run's credits are a run a turn's (`Income::run_credits`), and the
    // stock they are placed into is not printed, so nothing bounds them.
    let run = income.run_credits * horizon;
    // A click that begins a run buys the run (`Income::click_runs`).
    let charged = if income.click_runs { 0 } else { income.click_cost };
    let click = if income.click_credits > charged && !income.click_places {
        let net = income.click_credits - charged;
        if income.click_trashes {
            net
        } else {
            let uses = stock.map_or(horizon, |stock| horizon.min(stock / income.click_credits.max(1)));
            net * uses
        }
    } else {
        0
    };
    let cashout = income.cashout_per_counter * hosted.unwrap_or(income.printed_stock);
    f64::from(turn + run + click + cashout)
}

/// The Corp's rez reserve: the printed cost of the dearest unrezzed piece
/// of ICE it has installed, on any server — the one rez that stops a run
/// (the strategy guide: "they can afford the expensive run, or the rez
/// that stops it"). Zero with nothing face down. Printed cost, ignoring
/// what the table adds or takes off it, as `is_unrezzed_threat` reads it.
pub(super) fn rez_reserve(state: &GameState, registry: &CardRegistry) -> u32 {
    use netrunner_core::rules::InstallSlot;
    state
        .corp
        .installed
        .iter()
        .filter(|card| card.slot == InstallSlot::Ice && !card.rezzed)
        .filter_map(|card| registry.get(&card.card))
        .map(|def| def.cost)
        .max()
        .unwrap_or(0)
}

/// Unrezzed ICE the Corp could not rez right now at its printed cost —
/// the installs that are a promise the bank does not cover. See
/// `UNAFFORDABLE_ICE_WEIGHT`.
pub(super) fn unaffordable_ice(state: &GameState, registry: &CardRegistry) -> usize {
    use netrunner_core::rules::InstallSlot;
    let credits = state.corp.resources.credits.0;
    state
        .corp
        .installed
        .iter()
        .filter(|card| card.slot == InstallSlot::Ice && !card.rezzed)
        .filter_map(|card| registry.get(&card.card))
        .filter(|def| def.cost > credits)
        .count()
}

/// What the rig would spend to break into `server` if the Corp rezzed what
/// it can afford: `server_break_cost`'s reading with the unrezzed ICE the
/// Corp's credits cover added, taken in the order the Runner meets it,
/// each piece's rez paid out of one budget. `None` when a piece the Corp
/// could turn face up is one no rig card breaks. The Corp's reading of
/// its own taxing window, which it can make because the face-down ICE is
/// its own; the Runner's reading is `server_break_cost`, rezzed only.
pub(super) fn taxing_cost(state: &GameState, server: netrunner_core::rules::ServerId, registry: &CardRegistry) -> Option<u32> {
    use netrunner_core::rules::{EncounteredSubroutine, InstallSlot, RunIce};
    let mut total = 0;
    let mut budget = state.corp.resources.credits.0;
    let mut stock = fresh_stock(state);
    for installed in state.corp.installed.iter().filter(|c| c.server == server && c.slot == InstallSlot::Ice) {
        let def = registry.get(&installed.card)?;
        if !installed.rezzed {
            if def.cost > budget {
                continue;
            }
            budget -= def.cost;
        }
        let CardType::Ice(ice_type) = def.card_type else { continue };
        let ice = RunIce {
            card_id: installed.card.clone(),
            install_id: installed.install_id,
            ice_type,
            subroutines: def
                .subroutines
                .iter()
                .enumerate()
                .map(|(id, definition)| EncounteredSubroutine { id, definition: definition.clone(), status: SubroutineStatus::Pending, gained: false })
                .collect(),
            rezzed: true,
        };
        total += cheapest_break_cost(state, &ice, registry, &mut stock)?;
    }
    Some(total)
}

// ---------------------------------------------------------------------
// The Corp's plans (Stage 6): what the Corp reads about its own board
// and hand, and the Runner's public state, for the plan terms.
// ---------------------------------------------------------------------

/// The agenda points a breach of `run`'s server would reach, as the Corp
/// can count them: the agenda in a remote's root, HQ's agenda points at
/// one access over its size, R&D's at the deck's agenda density (the
/// Corp's own deck, whose order it does not know), and every agenda in
/// Archives, which a breach accesses whole. One access a run; the cards
/// a run event adds are not read. See `RUN_STAKES_WEIGHT`.
pub(super) fn run_stakes(state: &GameState, run: &RunState, registry: &CardRegistry) -> f64 {
    use netrunner_core::rules::{InstallSlot, ServerId};
    let points = |card: &netrunner_core::dsl::CardId| registry.get(card).and_then(|def| def.agenda_points).map_or(0.0, f64::from);
    let density = |cards: &[netrunner_core::dsl::CardId]| if cards.is_empty() { 0.0 } else { cards.iter().map(points).sum::<f64>() / cards.len() as f64 };
    match run.server {
        ServerId::Hq => density(&state.corp.hq),
        ServerId::RnD => density(&state.corp.r_and_d),
        ServerId::Archives => state.corp.archives.iter().map(|card| points(&card.card)).sum(),
        ServerId::Remote(_) => state
            .corp
            .installed
            .iter()
            .filter(|card| card.server == run.server && card.slot == InstallSlot::Root)
            .map(|card| points(&card.card))
            .sum(),
    }
}

/// The advancement tokens the Corp could place next turn: one a click
/// and a credit, and what the operations in HQ declare
/// (`PlaceAdvancementCounters` in an `OnPlay`, Seamless Launch's two
/// for a click and a credit, Touch-ups' two for a click and two), the
/// declared ones first, within three clicks and the credits it holds.
/// The never-advance line's arithmetic: "Next turn, Seamless Launch
/// places 2 advancement counters on a card you did not install this
/// turn, and one more advance scores a 3-advancement agenda". See
/// `NEVER_ADVANCE_WEIGHT`.
pub(super) fn next_turn_advancements(state: &GameState, registry: &CardRegistry) -> u32 {
    let mut declared: Vec<(u32, u32)> = state
        .corp
        .hq
        .iter()
        .filter_map(|card| registry.get(card))
        .filter(|def| def.card_type == CardType::Operation)
        .filter_map(|def| {
            let mut tokens = 0;
            for trigger in def.triggers.iter().filter(|trigger| trigger.trigger == Trigger::OnPlay) {
                for effect in &trigger.effects {
                    effect.for_each_effect(&mut |effect| {
                        if let Effect::PlaceAdvancementCounters(Amount::Fixed(n)) = effect {
                            tokens += *n;
                        }
                    });
                }
            }
            (tokens > 0).then_some((tokens, def.cost))
        })
        .collect();
    declared.sort_by_key(|&(tokens, _)| std::cmp::Reverse(tokens));
    let mut clicks = CORP_CLICKS_A_TURN;
    let mut credits = state.corp.resources.credits.0;
    let mut tokens = 0;
    for (placed, cost) in declared {
        if clicks == 0 {
            break;
        }
        if cost <= credits {
            clicks -= 1;
            credits -= cost;
            tokens += placed;
        }
    }
    tokens + clicks.min(credits)
}

/// The Corp's clicks a turn, for a reading of what next turn can do.
const CORP_CLICKS_A_TURN: u32 = 3;

/// The fixed damage the operations in HQ could deal next turn — each
/// `DealDamage` in an `OnPlay` whose play requirement and trigger
/// requirement are met now (none, or "if the Runner is tagged"), the
/// largest first, within three clicks and the credits the Corp holds.
/// Scorched Earth's four against a tagged Runner; nothing for an
/// operation whose damage reads the turn (Neurospike), which the
/// planner's own line prices when it scores. See `LETHAL_THREAT_WEIGHT`.
pub(super) fn damage_in_reach(state: &GameState, registry: &CardRegistry) -> u32 {
    use netrunner_core::dsl::EffectRequirement;
    let met = |requirement: &Option<EffectRequirement>| match requirement {
        None => true,
        Some(EffectRequirement::IsTagged) => state.runner.tags > 0,
        Some(_) => false,
    };
    let mut held: Vec<(u32, u32)> = state
        .corp
        .hq
        .iter()
        .filter_map(|card| registry.get(card))
        .filter(|def| def.card_type == CardType::Operation && met(&def.play_requirement))
        .filter_map(|def| {
            let mut damage = 0;
            for trigger in def.triggers.iter().filter(|trigger| trigger.trigger == Trigger::OnPlay && met(&trigger.requirement)) {
                for effect in &trigger.effects {
                    effect.for_each_effect(&mut |effect| {
                        if let Effect::DealDamage(_, n) = effect {
                            damage += *n as u32;
                        }
                    });
                }
            }
            (damage > 0).then_some((damage, def.cost))
        })
        .collect();
    held.sort_by_key(|&(damage, _)| std::cmp::Reverse(damage));
    let mut clicks = CORP_CLICKS_A_TURN;
    let mut credits = state.corp.resources.credits.0;
    let mut damage = 0;
    for (dealt, cost) in held {
        if clicks == 0 {
            break;
        }
        if cost <= credits {
            clicks -= 1;
            credits -= cost;
            damage += dealt;
        }
    }
    damage
}

/// The face-down ICE the Corp could rez at its printed cost in the
/// server the Runner is running — the rezzes it is holding — and none
/// with no run on. See `REZ_HELD_WEIGHT`.
pub(super) fn held_rezzes(state: &GameState, registry: &CardRegistry) -> usize {
    use netrunner_core::rules::InstallSlot;
    let Some(run) = &state.active_run else { return 0 };
    let credits = state.corp.resources.credits.0;
    state
        .corp
        .installed
        .iter()
        .filter(|card| card.server == run.server && card.slot == InstallSlot::Ice && !card.rezzed)
        .filter_map(|card| registry.get(&card.card))
        .filter(|def| def.cost <= credits)
        .count()
}

/// A card whose text asks for a tag on the Runner: "play only if the
/// Runner is tagged" (Scorched Earth, Retribution), a trigger with it as
/// the requirement, or an effect conditioned on it (Orbital
/// Superiority's scored half). Read off the DSL, as every recogniser
/// here is. See `TAG_LEVERAGE_WEIGHT`.
pub fn punishes_tags(def: &CardDefinition) -> bool {
    use netrunner_core::dsl::EffectRequirement;
    if matches!(def.play_requirement, Some(EffectRequirement::IsTagged))
        || def.triggers.iter().any(|trigger| matches!(trigger.requirement, Some(EffectRequirement::IsTagged)))
    {
        return true;
    }
    let mut found = false;
    for_each_declared_effect(def, &mut |effect| {
        if let Effect::EffectIf { condition: EffectRequirement::IsTagged, .. } = effect {
            found = true;
        }
    });
    found
}

/// Whether the Corp holds, in HQ, a card that punishes a tag.
pub(super) fn holds_tag_punishment(state: &GameState, registry: &CardRegistry) -> bool {
    state.corp.hq.iter().filter_map(|card| registry.get(card)).any(punishes_tags)
}

/// Adjacent pairs of ICE on one server, outermost first, where the outer
/// piece cannot end the run and the inner piece can — the taxing piece
/// outside the stopping one, which the guide's habit puts the other way
/// round. Every piece, face down or not: the Corp knows its own ICE.
/// See `ICE_ORDER_WEIGHT`.
pub(super) fn ice_out_of_order(state: &GameState, registry: &CardRegistry) -> usize {
    use netrunner_core::rules::InstallSlot;
    let ends_the_run = |card: &InstalledCard| {
        registry.get(&card.card).is_some_and(|def| def.subroutines.iter().any(|sub| sub.effect.can_end_the_run()))
    };
    let mut servers: Vec<netrunner_core::rules::ServerId> = Vec::new();
    for card in state.corp.installed.iter().filter(|card| card.slot == InstallSlot::Ice) {
        if !servers.contains(&card.server) {
            servers.push(card.server);
        }
    }
    servers
        .into_iter()
        .map(|server| {
            let stops: Vec<bool> = state
                .corp
                .installed
                .iter()
                .filter(|card| card.server == server && card.slot == InstallSlot::Ice)
                .map(ends_the_run)
                .collect();
            stops.windows(2).filter(|pair| !pair[0] && pair[1]).count()
        })
        .sum()
}

/// Whether the Runner has beaten the wall in front of `fort`: there is a
/// wall — at least `depth` pieces, the fort's own cap — their rig covers
/// the subtype of every piece on it, face down or not, and their credits
/// cover what breaking in would cost with the Corp's affordable rezzes
/// made (`taxing_cost`). "Once the Runner has spent everything building
/// a rig that beats the wall, score the last points from hand, where that
/// rig is no use." A remote with no ICE is not a wall anyone beat: read
/// without the depth, every fresh remote was "beaten" the turn it was
/// made and the fort terms fell away before there was a fort. See
/// `Weights::fort_until_beaten`.
pub(super) fn fort_beaten(state: &GameState, fort: netrunner_core::rules::ServerId, depth: usize, registry: &CardRegistry) -> bool {
    use netrunner_core::rules::InstallSlot;
    let rig = rig_coverage(state, registry);
    let wall: Vec<&InstalledCard> = state.corp.installed.iter().filter(|card| card.server == fort && card.slot == InstallSlot::Ice).collect();
    if wall.len() < depth.max(1) {
        return false;
    }
    let covered = wall.iter().all(|card| match registry.get(&card.card).map(|def| &def.card_type) {
        Some(CardType::Ice(subtype)) => subtype_slot(*subtype).map_or(rig.iter().all(|c| *c), |slot| rig[slot]),
        _ => true,
    });
    covered && taxing_cost(state, fort, registry).is_some_and(|cost| cost <= state.runner.resources.credits.0)
}

// ---------------------------------------------------------------------
// The Runner's plans, and the identity both chairs read (Stage 7): what
// the Runner reads off the Corp's identity, the ICE the Corp has shown,
// and the public count of what the Corp has drawn and not played.
// ---------------------------------------------------------------------

/// The Corp's faction, read off its identity — public in every view.
/// `None` in a fixture with no identity.
pub(super) fn corp_faction(state: &GameState, registry: &CardRegistry) -> Option<netrunner_core::card::Faction> {
    state.corp.identity.as_ref().and_then(|id| registry.get(id)).and_then(|def| def.faction)
}

/// Whether a Corp of `faction` is one the guide says punishes runs:
/// "Jinteki with its net damage, NBN with its tags." Shared with `diag
/// precepts`, whose last-click line bins by the same two.
pub fn punishes_runs(faction: Option<netrunner_core::card::Faction>) -> bool {
    use netrunner_core::card::Faction;
    matches!(faction, Some(Faction::Jinteki | Faction::Nbn))
}

/// Whether the Corp across the table punishes runs: by its identity
/// (`punishes_runs`), or by what it has shown — a rezzed piece of ICE
/// whose subroutines deal damage or give tags. Either is public. See
/// `LAST_CLICK_RUN_WEIGHT`.
pub(super) fn corp_punishes_runs(state: &GameState, registry: &CardRegistry) -> bool {
    use netrunner_core::rules::InstallSlot;
    if punishes_runs(corp_faction(state, registry)) {
        return true;
    }
    state
        .corp
        .installed
        .iter()
        .filter(|card| card.slot == InstallSlot::Ice && card.rezzed)
        .filter_map(|card| registry.get(&card.card))
        .any(|def| {
            let mut punishes = false;
            for sub in &def.subroutines {
                sub.effect.for_each_effect(&mut |effect| {
                    if matches!(effect, Effect::DealDamage(..) | Effect::DealDamageAmount(..) | Effect::GiveTags(_)) {
                        punishes = true;
                    }
                });
            }
            punishes
        })
}

/// Whether `run` has something unknown in the way: face-down ICE still
/// ahead of the Runner, or a card the breach would show it for the
/// first time — HQ's and R&D's are always unknown, a remote's or
/// Archives' only while face down and never accessed. What there is to
/// fear on a last-click run; a run through rezzed ICE into cards the
/// Runner has already seen fears nothing.
pub(super) fn unknown_ahead(state: &GameState, run: &RunState) -> bool {
    use netrunner_core::rules::{InstallSlot, ServerId};
    if run.ice.iter().skip(run.position).any(|ice| !ice.rezzed) {
        return true;
    }
    match run.server {
        ServerId::Hq | ServerId::RnD => true,
        ServerId::Archives => state.corp.archives.iter().any(|card| card.facedown),
        ServerId::Remote(_) => state
            .corp
            .installed
            .iter()
            .any(|card| card.server == run.server && card.slot == InstallSlot::Root && !card.rezzed && !card.seen_by_runner),
    }
}

/// Whether the Runner has just begun a run on its last click: its own
/// action phase, no clicks left, and the run still at its initiation —
/// the state the planner prices a run's line at, and no state a
/// jack-out is offered in (the first movement phase comes after, CR
/// 6.6.3), so the term this reads for is paid on starting the run and
/// never on staying in it. See `LAST_CLICK_RUN_WEIGHT`.
pub(super) fn last_click_run(state: &GameState) -> bool {
    state.phase == GamePhase::Action(Side::Runner)
        && state.runner.resources.clicks.0 == 0
        && state.active_run.as_ref().is_some_and(|run| run.phase == RunPhase::Initiation)
}

/// The most damage one access is feared to deal: the largest a known
/// trap would do now — rezzed or seen on the table (`trap_damage`), or
/// face up in Archives — and, against a Jinteki Corp, at least
/// `TYPICAL_NET_DAMAGE`, because "a threat of flatline behind every
/// face-down card" is that faction's chapter. A face-down card never
/// seen is not read: its identity in a sample is a guess. See
/// `FEARED_FLATLINE_WEIGHT`.
pub(super) fn damage_feared(state: &GameState, registry: &CardRegistry) -> usize {
    use netrunner_core::card::Faction;
    let mut feared = if corp_faction(state, registry) == Some(Faction::Jinteki) { TYPICAL_NET_DAMAGE } else { 0 };
    for installed in state.corp.installed.iter().filter(|card| card.rezzed || card.seen_by_runner) {
        if let Some(def) = registry.get(&installed.card)
            && punishes_access_with_damage(def)
        {
            feared = feared.max(trap_damage(state, installed, def));
        }
    }
    for archived in state.corp.archives.iter().filter(|card| !card.facedown) {
        if let Some(def) = registry.get(&archived.card)
            && punishes_access_with_damage(def)
        {
            let fixed = InstalledCard { card: archived.card.clone(), ..Default::default() };
            feared = feared.max(trap_damage(state, &fixed, def));
        }
    }
    feared
}

/// The ICE subtypes the Corp has shown the Runner, as `rig_coverage`'s
/// flags: a piece rezzed on the table, one once rezzed and face down
/// again (`seen_by_runner`, set at the rez), or one face up in
/// Archives. "Install breakers for the ICE the Corp has actually
/// rezzed." See `UNSHOWN_BREAKER_WEIGHT`.
pub(super) fn ice_shown(state: &GameState, registry: &CardRegistry) -> [bool; 3] {
    use netrunner_core::rules::InstallSlot;
    let mut shown = [false; 3];
    let mut show = |card: &netrunner_core::dsl::CardId| {
        if let Some(CardType::Ice(subtype)) = registry.get(card).map(|def| &def.card_type)
            && let Some(slot) = subtype_slot(*subtype)
        {
            shown[slot] = true;
        }
    };
    for installed in state.corp.installed.iter().filter(|card| card.slot == InstallSlot::Ice && (card.rezzed || card.seen_by_runner)) {
        show(&installed.card);
    }
    for archived in state.corp.archives.iter().filter(|card| !card.facedown) {
        show(&archived.card);
    }
    shown
}

/// The subtypes the rig covers that the Corp has not shown — a breaker
/// installed for ICE the Corp might have.
pub(super) fn unshown_coverage(state: &GameState, registry: &CardRegistry) -> usize {
    let shown = ice_shown(state, registry);
    rig_coverage(state, registry).into_iter().zip(shown).filter(|(covered, shown)| *covered && !shown).count()
}

/// The agenda points one access is expected to find in the Corp's hand
/// and in its deck, as the Runner can count them: `(hq, rd)`, each per
/// card. The deck must hold the points its size demands (CR 1.4.6: 18
/// at 40–44 cards, 20 at 45–49, two more for each five over), and the
/// Runner counts the deck by every Corp card it can see a place for.
/// R&D is at the deck's density — its order is nobody's to know. HQ is
/// what the Corp has drawn less what is accounted for: the points it
/// scored, the points the Runner stole, the agendas face up in Archives
/// and the installed agendas the Runner has seen; whatever is left of
/// the drawn share is somewhere the Runner has not looked — HQ, a
/// face-down card in Archives, a face-down root it has not accessed —
/// and is spread over those cards evenly, HQ's share at most the hand's
/// cards at the pool's dearest point value. (Attributed to HQ alone it
/// was every agenda the Corp had installed in a remote, and the Runner
/// ran HQ for the cards on the table.) "HQ when the Corp is holding
/// cards without scoring." See `RUNNER_STAKES_WEIGHT`.
pub(super) fn agenda_points_expected(state: &GameState, registry: &CardRegistry) -> (f64, f64) {
    let points = |card: &netrunner_core::dsl::CardId| registry.get(card).and_then(|def| def.agenda_points).map_or(0.0, f64::from);
    let counted = state.corp.r_and_d.len() + state.corp.hq.len() + state.corp.archives.len() + state.corp.installed.len()
        + state.corp.scored_agendas.len()
        + state.runner.scored_agendas.len();
    if counted == 0 {
        return (0.0, 0.0);
    }
    // No legal deck is under forty cards (CR 1.4.3: the identity's
    // minimum, and every identity in the pool prints 40 or more), so a fixture's
    // handful is read as the smallest deck there is, not as a deck
    // that is half agendas.
    let deck = counted.max(SMALLEST_CORP_DECK);
    let required = f64::from(2 * (deck as u32 / 5) + 2);
    let density = required / deck as f64;
    let drawn = (counted - state.corp.r_and_d.len()) as f64;
    let accounted = f64::from(state.corp.resources.agenda_points.0.max(0)) + f64::from(state.runner.resources.agenda_points.0.max(0))
        + state.corp.archives.iter().filter(|card| !card.facedown).map(|card| points(&card.card)).sum::<f64>()
        + state.corp.installed.iter().filter(|card| card.seen_by_runner).map(|card| points(&card.card)).sum::<f64>();
    let unaccounted = (drawn * density - accounted).max(0.0);
    let unseen = state.corp.hq.len()
        + state.corp.archives.iter().filter(|card| card.facedown).count()
        + state.corp.installed.iter().filter(|card| card.slot == netrunner_core::rules::InstallSlot::Root && !card.rezzed && !card.seen_by_runner).count();
    let hq = if unseen == 0 { 0.0 } else { (unaccounted / unseen as f64).min(DEAREST_AGENDA_POINTS) };
    (hq, density)
}

/// The most points one agenda in the pool is worth, which bounds what
/// a hand of `n` cards can hold.
const DEAREST_AGENDA_POINTS: f64 = 3.0;
/// The fewest cards a Corp deck may have: the least any identity in the
/// pool prints as its minimum (CR 1.4.3).
const SMALLEST_CORP_DECK: usize = 40;

/// The R&D accesses `def` declares beyond the first: a fixed count
/// (`AddAdditionalAccess` on R&D — The Maker's Eye's two, Devadatta
/// Drone's one) and a count read off its hosted counters
/// (`AddAdditionalAccessAmount` — Conduit's), at `counters` when the
/// card is on the table and at what it places on itself when it is in
/// hand. See `RD_ACCESS_WEIGHT`.
///
/// **The counters its own successful runs place are promised too**
/// (Phase 5 §32): Conduit's "whenever a successful run on R&D ends, you
/// may place 1 virus counter on this program" grows the count every run
/// it makes, at a run a turn over `horizon` — the rate `Income::
/// run_credits` reads a run at. Before, Conduit in hand promised what it
/// places on itself on install, nothing, and the rig plan never
/// installed it; the growth is read on the table as in hand, so the
/// install takes nothing the hand had.
pub(super) fn rd_accesses(def: &CardDefinition, counters: Option<u32>, horizon: u32) -> u32 {
    use netrunner_core::rules::ServerId;
    let mut fixed = 0;
    let mut per_counter = false;
    for_each_declared_effect(def, &mut |effect| match effect {
        Effect::AddAdditionalAccess { server: ServerId::RnD, count } => fixed += *count,
        Effect::AddAdditionalAccessAmount { server: ServerId::RnD, amount: Amount::HostedCounters } => per_counter = true,
        _ => {}
    });
    let hosted = if per_counter { counters.unwrap_or_else(|| declared_income(def).printed_stock) } else { 0 };
    // A counter placed on a successful run, the "you may" taken: a
    // choice's options are compared by what they pay (`Tally::worth`),
    // which a counter is not, so the placement is looked for outright.
    let grows = per_counter
        && def.triggers.iter().filter(|trigger| trigger.trigger == Trigger::OnSuccessfulRun && trigger.requirement.is_none()).any(|trigger| {
            let mut places = false;
            for effect in &trigger.effects {
                effect.for_each_effect(&mut |effect| places |= matches!(effect, Effect::AddCounters(n) if *n > 0));
            }
            places
        });
    fixed + hosted + if grows { horizon } else { 0 }
}

/// The R&D accesses the rig promises: `rd_accesses` summed over the
/// Runner's installed cards, each at its own counters.
pub(super) fn rig_rd_accesses(state: &GameState, registry: &CardRegistry, horizon: u32) -> u32 {
    state.runner.rig.iter().filter_map(|card| registry.get(&card.card).map(|def| rd_accesses(def, Some(card.counters), horizon))).sum()
}

/// The accesses beyond the first the rig adds when the run on `server`
/// breaches it (Phase 5 §32): a rig card's trigger on a breach or a
/// successful run of that server whose text adds accesses outright —
/// Docklands Pass's "the first time each turn you breach HQ, access 1
/// additional card". Its "first time" is read off the turn log, as the
/// engine judges it (`TriggeredEffect::first_each_turn`): a run on HQ
/// after the turn's first HQ breach gets nothing. Counted only where the
/// access is free and certain: a trigger with a condition (Manuel Lattes
/// de Moura's tag, Pretty Mary da Silva's access limit) or an access
/// behind a cost (Rotary's tag, Devadatta Drone's counter, Cupellation's
/// trash) is nothing here, the cheaper direction. The run leaf prices a
/// run before its breach, so without this the card that pays at the
/// breach was worth nothing to the run it pays on, and the planner never
/// installed it.
pub(super) fn rig_breach_accesses(state: &GameState, registry: &CardRegistry, server: netrunner_core::rules::ServerId) -> u32 {
    use netrunner_core::dsl::{EventFilter, Subject};
    use netrunner_core::rules::ServerId;
    use netrunner_core::rules::turn_log::{Class, ServerClass};
    let class = match server {
        ServerId::Hq => ServerClass::Hq,
        ServerId::RnD => ServerClass::RnD,
        // A breach of Archives or a remote accesses every card already.
        ServerId::Archives | ServerId::Remote(_) => return 0,
    };
    fn outright(effect: &Effect, server: ServerId) -> u32 {
        match effect {
            Effect::AddAdditionalAccess { server: added, count } if *added == server => *count,
            Effect::Sequence(effects) => effects.iter().map(|effect| outright(effect, server)).sum(),
            Effect::EffectIf { effect, .. } => outright(effect, server),
            _ => 0,
        }
    }
    state
        .runner
        .rig
        .iter()
        .filter_map(|card| registry.get(&card.card))
        .flat_map(|def| def.triggers.iter())
        .filter(|trigger| matches!(trigger.trigger, Trigger::OnBreach | Trigger::OnSuccessfulRun))
        .filter(|trigger| trigger.subject != Some(Subject::This) && trigger.requirement.is_none())
        .filter(|trigger| match &trigger.when {
            None => true,
            Some(EventFilter::Server(servers)) => servers.contains(&server),
            Some(_) => false,
        })
        .filter(|trigger| !trigger.first_each_turn || state.this_turn.times_about(trigger.trigger, Class::Server(class)) == 0)
        .map(|trigger| trigger.effects.iter().map(|effect| outright(effect, server)).sum::<u32>())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::test_support::*;
    use netrunner_core::dsl::{CardDefinition, CardId, DamageType, Trigger, TriggeredEffect};
    use netrunner_core::rules::{Credits, GameState, InstalledRunnerCard};

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

    /// The ambush recogniser reads a card's *text*, so it must fire on a
    /// card that grows its damage and not on one that merely deals it.
    #[test]
    fn an_ambush_is_recognised_by_its_text_and_a_plain_damage_card_is_not() {
        let mut growing = ice("urtica", 0);
        growing.card_type = CardType::Asset;
        growing.advancement_requirement = Some(0);
        growing.triggers = vec![TriggeredEffect {
            subject: None, when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_heap: false,
            text: None,
            trigger: Trigger::OnAccessed,
            effects: vec![Effect::DealDamageAmount(DamageType::Net, Amount::HostedAdvancementTokens)],
            requirement: None,
        }];
        assert!(damage_grows_with_advancement(&growing));
        assert!(punishes_access_with_damage(&growing));

        let mut flat = growing.clone();
        flat.triggers[0].effects = vec![Effect::DealDamage(DamageType::Net, 3)];
        assert!(!damage_grows_with_advancement(&flat), "a fixed amount does not grow");
        assert!(punishes_access_with_damage(&flat), "but it is still an ambush");

        let mut elsewhere = growing.clone();
        elsewhere.triggers[0].trigger = Trigger::OnTurnStart;
        assert!(damage_grows_with_advancement(&elsewhere));
        assert!(!punishes_access_with_damage(&elsewhere), "damage on your own turn is not an access punish");

        let mut agenda = growing.clone();
        agenda.card_type = CardType::Agenda;
        assert!(!punishes_access_with_damage(&agenda), "an agenda that hurts on access is not an ambush to install");
    }

    /// The evaluator's one window onto hidden information. Everything
    /// else it reads is public, the searching side's own zones, or a
    /// count `determinize` preserves — see ROADMAP Phase 2 §5 item 36.
    #[test]
    fn unbreakable_unrezzed_ice_counts_what_the_corp_can_rez_and_the_rig_cannot_break() {
        use netrunner_core::rules::ServerId;
        fn ice_card(id: &str, ice_type: IceType, cost: u32) -> CardDefinition {
            CardDefinition {
                id: CardId(id.to_string()),
                title: id.to_string(),
                side: Side::Corp,
                card_type: CardType::Ice(ice_type),
                cost,
                strength: Some(5),
                ..CardDefinition::default()
            }
        }
        let registry = CardRegistry::from_cards(vec![
            ice_card("ice", IceType::Sentry, 4),
            priced_breaker("carmen", Some(IceType::Sentry), (1, 2), (2, 1)),
        ]);
        let run = |rezzed, position| RunState {
            server: ServerId::Hq,
            ice: vec![RunIce { card_id: CardId("ice".to_string()), ..run_ice(5, IceType::Sentry, 1, rezzed) }],
            position,
            ..Default::default()
        };

        let mut state = GameState::new(0);
        state.corp.resources.credits = Credits(4);
        assert_eq!(unbreakable_unrezzed_ice(&state, &run(false, 0), &registry), 1, "rezzable and unbreakable");

        state.corp.resources.credits = Credits(3);
        assert_eq!(unbreakable_unrezzed_ice(&state, &run(false, 0), &registry), 0, "an ICE the Corp cannot pay to rez is not a threat");

        state.corp.resources.credits = Credits(4);
        assert_eq!(unbreakable_unrezzed_ice(&state, &run(true, 0), &registry), 0, "a rezzed ICE is `run_is_breakable`'s job, not this one");
        assert_eq!(unbreakable_unrezzed_ice(&state, &run(false, 1), &registry), 0, "ICE the run has already passed is behind the Runner");

        // A breaker that covers the subtype and can reach the strength
        // makes it breakable at a price, so it stops being a threat.
        state.runner.rig = vec![InstalledRunnerCard { base_strength: 5, ..rig_card("carmen") }];
        assert_eq!(unbreakable_unrezzed_ice(&state, &run(false, 0), &registry), 0, "the rig covers it");

        // ROADMAP Phase 2 §5 item 39: unbreakable is not the same claim
        // as "it stops you". An ICE the rig cannot touch, that the Corp
        // can afford, whose subroutine only tags, is a toll and not a
        // wall — and counting it was half of what this term counted.
        state.runner.rig = Vec::new();
        let mut tagging = run(false, 0);
        tagging.ice[0].subroutines[0].definition.effect = Effect::GiveTags(Amount::Fixed(1));
        assert_eq!(unbreakable_unrezzed_ice(&state, &tagging, &registry), 0, "no subroutine ends the run");
        tagging.ice[0].subroutines[0].definition.effect =
            Effect::Sequence(vec![Effect::GiveTags(Amount::Fixed(1)), Effect::EndTheRun]);
        assert_eq!(unbreakable_unrezzed_ice(&state, &tagging, &registry), 1, "buried in a sequence still ends it");

        // The term is off by default, so none of this moves a score
        // until a `Weights` says otherwise.
        assert_eq!(Weights::default().unrezzed_threat_weight, 0.0);
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

    /// The economy is read off the pool's own cards, as printed: what
    /// each play nets, what each install pays a turn, and what stock the
    /// credits come off. A card that declares no economy reads as
    /// nothing.
    #[test]
    fn declared_income_reads_the_pools_economy_cards_as_printed() {
        let pool = pool();
        let income = |id: &str| declared_income(&printed(&pool, id));
        let hedge = income("hedge_fund");
        assert_eq!((hedge.play_credits, hedge.play_cards, hedge.play_clicks), (4, 0, 0), "5 for 9");
        assert_eq!(income("diesel").play_cards, 3);
        assert_eq!(income("nanomanagement").play_clicks, 2);
        let commission = income("creative_commission");
        assert_eq!((commission.play_credits, commission.play_clicks), (4, -1));
        // A choice the Corp makes for the Runner is read at its worst
        // option; one the owner makes at its best.
        assert_eq!(income("wildcat_strike").play_cards, 4, "the Corp gives the cards, not the six credits");
        let planogram = income("predictive_planogram");
        assert!(planogram.play_credits == 3 || planogram.play_cards == 3, "one of the two, the Corp's choice: {planogram:?}");

        let pad = income("pad_campaign");
        assert_eq!((pad.turn_credits, pad.stocked), (1, false), "a credit a turn, for as long as it stands");
        let regolith = income("regolith_mining_license");
        assert_eq!((regolith.click_credits, regolith.click_cost, regolith.stocked, regolith.printed_stock), (3, 1, true, 15));
        let telework = income("telework_contract");
        assert_eq!((telework.click_credits, telework.stocked, telework.printed_stock), (3, true, 9));
        let nico = income("nico_campaign");
        assert_eq!((nico.turn_credits, nico.stocked, nico.printed_stock), (3, true, 9));
        let smartware = income("smartware_distributor");
        assert_eq!((smartware.turn_credits, smartware.stocked, smartware.click_credits, smartware.click_places), (1, true, 3, true), "a click places three credits its turns pay off");
        assert!(!regolith.click_places);
        let fermenter = income("fermenter");
        assert_eq!((fermenter.cashout_per_counter, fermenter.printed_stock), (2, 1));
        let rioters = income("rent_rioters");
        assert_eq!((rioters.click_credits, rioters.click_cost, rioters.click_trashes), (9, 3, true));

        let wall = income("palisade");
        assert_eq!(wall, Income::default(), "ICE declares no economy");
        assert_eq!(income("offworld_office").turn_credits, 0, "an agenda's text is read once it is scored, not here");
    }

    /// What an active card will still pay: a turn's income over the
    /// horizon, bounded by the stock where the credits come off counters;
    /// a click's use once a turn for what it takes over the credit the
    /// click would have bought; a cashout at the printed rate.
    #[test]
    fn future_credits_are_bounded_by_the_stock_and_the_horizon() {
        let pool = pool();
        let future = |id: &str, hosted: Option<u32>, horizon: u32| future_credits(&declared_income(&printed(&pool, id)), hosted, horizon);
        assert_eq!(future("pad_campaign", None, 9), 9.0);
        assert_eq!(future("pad_campaign", None, 2), 2.0);
        assert_eq!(future("nico_campaign", Some(9), 9), 9.0, "three turns of three, then the card is gone");
        assert_eq!(future("nico_campaign", Some(3), 9), 3.0, "one turn left on it");
        assert_eq!(future("regolith_mining_license", Some(15), 9), 10.0, "five uses of 3 for a click: 2 net each");
        assert_eq!(future("regolith_mining_license", Some(15), 2), 4.0, "two uses in the turns left");
        assert_eq!(future("regolith_mining_license", None, 9), 10.0, "in hand, at its printed stock");
        assert_eq!(future("telework_contract", Some(9), 9), 6.0, "three uses, one a turn");
        assert_eq!(future("smartware_distributor", Some(0), 9), 0.0, "nothing placed, nothing paid");
        assert_eq!(future("smartware_distributor", Some(3), 9), 3.0);
        assert_eq!(future("fermenter", Some(4), 9), 8.0, "cashed at 2 a counter");
        assert_eq!(future("rent_rioters", None, 9), 6.0, "9 for three clicks, once");
        assert_eq!(future("palisade", None, 9), 0.0);
    }

    /// A card that pays on a run (Phase 5 §32). Red Team's click begins a
    /// run whose rider takes 3[credit] off its twelve: the rider's credits
    /// are the click's, the click is not charged because it buys the run,
    /// and the counters bound it to four uses. Pennyshaver's successful
    /// runs place a credit each, cashed by its click: a credit a turn,
    /// unbounded. A trigger narrowed to a server or to this card's server
    /// is not a run a turn, and a run ability with no rider pays nothing.
    #[test]
    fn a_card_that_pays_on_a_run_is_read_at_a_run_a_turn() {
        let pool = pool();
        let income = |id: &str| declared_income(&printed(&pool, id));
        let future = |id: &str, hosted: Option<u32>, horizon: u32| future_credits(&income(id), hosted, horizon);
        let red_team = income("red_team");
        assert_eq!((red_team.click_credits, red_team.click_runs, red_team.stocked, red_team.printed_stock), (3, true, true, 12));
        assert_eq!(future("red_team", None, 9), 12.0, "four runs of 3, the click buying the run");
        assert_eq!(future("red_team", Some(3), 9), 3.0, "one run left on it");
        assert_eq!(future("red_team", None, 2), 6.0, "two runs in the turns left");
        let penny = income("pennyshaver");
        assert_eq!((penny.run_credits, penny.cashout_per_counter), (1, 1));
        assert_eq!(future("pennyshaver", None, 9), 9.0, "a credit a successful run, a run a turn");
        assert_eq!(future("pennyshaver", Some(3), 5), 8.0, "and what it holds, cashed");
        assert_eq!(income("stowaway").run_credits, 0, "a run on its own server is not a run a turn");
        assert_eq!(income("gabriel_santiago_consummate_professional").run_credits, 0, "nor is the first on HQ");
        assert_eq!(income("leech").run_credits, 0, "counters nothing cashes are not credits");
        assert_eq!(income("baker"), Income::default(), "a run with no rider pays nothing");
        assert_eq!(future("regolith_mining_license", None, 9), 10.0, "a click that begins no run is still charged");
    }

    /// The accesses the rig adds at the breach are the run's (Phase 5
    /// §32): Docklands Pass's on HQ, the first time each turn, and on no
    /// other server; a condition or a cost in the way counts nothing.
    #[test]
    fn the_rigs_breach_accesses_are_the_runs_the_first_time_each_turn() {
        use netrunner_core::rules::turn_log::Class;
        use netrunner_core::rules::ServerId;
        let pool = pool();
        let mut state = GameState::new(0);
        state.runner.rig = vec![InstalledRunnerCard { card: CardId("docklands_pass".to_string()), ..Default::default() }];
        assert_eq!(rig_breach_accesses(&state, &pool, ServerId::Hq), 1);
        assert_eq!(rig_breach_accesses(&state, &pool, ServerId::RnD), 0);
        assert_eq!(rig_breach_accesses(&state, &pool, ServerId::Archives), 0);
        // The turn's first HQ breach, counted as the engine counts it (on
        // a table with nothing to hear it).
        let mut breached = GameState::new(0);
        netrunner_core::rules::dispatch_event(&mut breached, &pool, &netrunner_core::rules::GameEvent::BreachBegun { server: ServerId::Hq }).expect("a breach nobody hears");
        let hq = netrunner_core::rules::turn_log::ServerClass::Hq;
        assert_eq!(breached.this_turn.times_about(Trigger::OnBreach, Class::Server(hq)), 1);
        breached.runner.rig = state.runner.rig.clone();
        assert_eq!(rig_breach_accesses(&breached, &pool, ServerId::Hq), 0, "the turn's second HQ breach");
        for id in ["rotary", "manuel_lattes_de_moura", "devadatta_drone", "cupellation", "pretty_mary_da_silva"] {
            state.runner.rig = vec![InstalledRunnerCard { card: CardId(id.to_string()), ..Default::default() }];
            assert_eq!(rig_breach_accesses(&state, &pool, ServerId::Hq) + rig_breach_accesses(&state, &pool, ServerId::RnD), 0, "{id}: behind a condition or a cost");
        }
    }

    /// The rez reserve is the dearest face-down piece, wherever it is;
    /// the taxing cost counts the face-down ICE the Corp's credits cover,
    /// in the order the Runner meets it.
    #[test]
    fn the_rez_reserve_and_the_taxing_cost_read_the_corps_own_ice() {
        use netrunner_core::rules::{InstallSlot, ServerId};
        let registry = CardRegistry::from_cards(vec![
            with_etr(ice("cheap", 2)),
            with_etr(ice("dear", 6)),
            priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1)),
        ]);
        let piece = |id: &str, n: u32, server: ServerId, rezzed: bool| InstalledCard {
            card: CardId(id.to_string()),
            install_id: netrunner_core::rules::InstallId(n),
            server,
            slot: InstallSlot::Ice,
            rezzed,
            ..Default::default()
        };
        let mut state = GameState::new(0);
        state.runner.rig = vec![InstalledRunnerCard { base_strength: 3, ..rig_card("cleaver") }];
        state.corp.installed = vec![piece("cheap", 1, ServerId::Hq, true), piece("dear", 2, ServerId::Remote(0), false), piece("cheap", 3, ServerId::Remote(0), false)];
        assert_eq!(rez_reserve(&state, &registry), 6);
        state.corp.installed[1].rezzed = true;
        assert_eq!(rez_reserve(&state, &registry), 2, "the dear one is face up now");
        state.corp.installed[1].rezzed = false;

        // Each piece breaks for 1[c] once rezzed. With no credits the
        // Corp rezzes nothing on the remote; with 6 it rezzes the dear
        // piece it meets first and cannot afford the cheap one after.
        state.corp.resources.credits = Credits(0);
        assert_eq!(taxing_cost(&state, ServerId::Remote(0), &registry), Some(0));
        assert_eq!(taxing_cost(&state, ServerId::Hq, &registry), Some(1), "rezzed ICE is priced whatever the Corp holds");
        state.corp.resources.credits = Credits(6);
        assert_eq!(taxing_cost(&state, ServerId::Remote(0), &registry), Some(1));
        state.corp.resources.credits = Credits(8);
        assert_eq!(taxing_cost(&state, ServerId::Remote(0), &registry), Some(2));
        state.runner.rig.clear();
        assert_eq!(taxing_cost(&state, ServerId::Remote(0), &registry), None, "no rig card breaks it at any price");
    }

    fn with_etr(mut ice: CardDefinition) -> CardDefinition {
        ice.subroutines = vec![netrunner_core::dsl::SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }];
        ice
    }

    /// A break paid in hosted copies (Matryoshka, Phase 5 §28) is priced
    /// at the X it names and bounded by the copies still faceup: with
    /// none hosted the card breaks nothing, one copy breaks one piece of
    /// ICE at a credit a subroutine, and a server of two pieces needs two
    /// copies — the ledger carries the first piece's spend to the second.
    #[test]
    fn a_break_paid_in_hosted_copies_is_priced_and_bounded_by_them() {
        let pool = pool();
        let registry = CardRegistry::from_cards(vec![printed(&pool, "matryoshka")]);
        let copies = |hosted: usize, facedown: u32| InstalledRunnerCard {
            base_strength: 2,
            hosted_cards: vec![CardId("matryoshka".to_string()); hosted],
            turned_facedown: facedown,
            ..rig_card("matryoshka")
        };
        let ice = run_ice(2, IceType::Sentry, 2, true);
        let registry = with_printed_ice(&registry, std::slice::from_ref(&ice));
        let mut state = GameState::new(0);
        let price = |state: &GameState| cheapest_break_cost(state, &ice, &registry, &mut fresh_stock(state));

        state.runner.rig = vec![copies(0, 0)];
        assert_eq!(price(&state), None, "nothing hosted, nothing to turn facedown");
        state.runner.rig = vec![copies(1, 0)];
        assert_eq!(price(&state), Some(2), "X[c] for X subroutines, one copy turned");
        state.runner.rig = vec![copies(1, 1)];
        assert_eq!(price(&state), None, "a copy already turned facedown is not back until the turn begins");
        state.runner.rig = vec![copies(2, 1)];
        assert_eq!(price(&state), Some(2));

        // Two pieces in one server: one copy prices the first and is
        // spent; the second is unbreakable until a second copy is hosted.
        let two = |state: &GameState| {
            let mut stock = fresh_stock(state);
            let first = cheapest_break_cost(state, &ice, &registry, &mut stock);
            let second = cheapest_break_cost(state, &ice, &registry, &mut stock);
            (first, second)
        };
        state.runner.rig = vec![copies(1, 0)];
        assert_eq!(two(&state), (Some(2), None));
        state.runner.rig = vec![copies(2, 0)];
        assert_eq!(two(&state), (Some(2), Some(2)));

        // The same reading through a run: `remaining_break_cost` prices
        // the whole server, so the planner's run leaf sees it.
        let run = |ice: Vec<RunIce>| RunState { server: netrunner_core::rules::ServerId::Hq, ice, position: 0, ..Default::default() };
        state.runner.rig = vec![copies(1, 0)];
        assert_eq!(remaining_break_cost(&state, &run(vec![ice.clone()]), &registry), Some(2));
        assert_eq!(remaining_break_cost(&state, &run(vec![ice.clone(), ice.clone()]), &registry), None);
        state.runner.rig = vec![copies(2, 0)];
        assert_eq!(remaining_break_cost(&state, &run(vec![ice.clone(), ice.clone()]), &registry), Some(4));

        // A pump against a stockless card has nothing to pump: the
        // break the card cannot pay for is not a break it has.
        let tall = run_ice(4, IceType::Sentry, 1, true);
        let registry = with_printed_ice(&registry, std::slice::from_ref(&tall));
        state.runner.rig = vec![copies(0, 0)];
        assert!(!breaks_subtype(&state, &state.runner.rig[0], &tall, &registry));
        state.runner.rig = vec![copies(1, 0)];
        assert!(breaks_subtype(&state, &state.runner.rig[0], &tall, &registry));
        assert_eq!(cheapest_break_cost(&state, &tall, &registry, &mut fresh_stock(&state)), Some(3), "two pumps at 1[c] and one subroutine at 1[c]");
    }

    /// Every other cost shape a breaker in the pool prints, read as the
    /// engine charges it: a counter a break (Audrey v2) and a counter a
    /// pump (Hantu), each bounded by the counters; X[c] with a counter
    /// (Lobisomem); and a credit cost the table reduces (Tremolo).
    #[test]
    fn counter_and_reduced_costs_are_priced_as_the_engine_charges_them() {
        use netrunner_core::dsl::CardSubtype;
        let pool = pool();
        let registry = CardRegistry::from_cards(vec![
            printed(&pool, "audrey_v2"),
            printed(&pool, "hantu"),
            printed(&pool, "lobisomem"),
            printed(&pool, "tremolo"),
            CardDefinition { subtypes: vec![CardSubtype::Cybernetic], ..printed(&pool, "t400_memory_diamond") },
        ]);
        let with_counters = |id: &str, counters: u32, strength: i32| InstalledRunnerCard { counters, base_strength: strength, ..rig_card(id) };
        let price = |state: &GameState, ice: &RunIce| {
            let registry = with_printed_ice(&registry, std::slice::from_ref(ice));
            cheapest_break_cost(state, ice, &registry, &mut fresh_stock(state))
        };
        let mut state = GameState::new(0);

        // Audrey v2: a counter breaks up to two; three subroutines take
        // two counters and no credits.
        let three = run_ice(0, IceType::Barrier, 3, true);
        state.runner.rig = vec![with_counters("audrey_v2", 1, 0)];
        assert_eq!(price(&state, &three), None, "one counter covers two of three");
        state.runner.rig = vec![with_counters("audrey_v2", 2, 0)];
        assert_eq!(price(&state, &three), Some(0));

        // Hantu: 1[c] a subroutine, a counter for +2; strength 4 against
        // its 2 takes one counter.
        let sentry = run_ice(4, IceType::Sentry, 1, true);
        state.runner.rig = vec![with_counters("hantu", 0, 2)];
        assert_eq!(price(&state, &sentry), None, "no counter to pump with");
        state.runner.rig = vec![with_counters("hantu", 1, 2)];
        assert_eq!(price(&state, &sentry), Some(1));

        // Lobisomem: X[c] and a power counter for X barrier subroutines.
        let barrier = run_ice(2, IceType::Barrier, 3, true);
        state.runner.rig = vec![with_counters("lobisomem", 0, 2)];
        assert_eq!(price(&state, &barrier), None);
        state.runner.rig = vec![with_counters("lobisomem", 1, 2)];
        assert_eq!(price(&state, &barrier), Some(3));

        // Tremolo: 3[c] for up to two, 1[c] less per cybernetic hardware.
        let two = run_ice(2, IceType::Barrier, 2, true);
        state.runner.rig = vec![with_counters("tremolo", 0, 2)];
        assert_eq!(price(&state, &two), Some(3));
        state.runner.rig.push(rig_card("t400_memory_diamond"));
        assert_eq!(price(&state, &two), Some(2), "one cybernetic hardware installed");
    }
}
