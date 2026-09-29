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
    remaining_break_cost(state, run, registry)
        .is_some_and(|total| total <= state.runner.resources.credits.0 + run.bad_publicity_credits)
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
pub(super) fn remaining_break_cost(state: &GameState, run: &RunState, registry: &CardRegistry) -> Option<u32> {
    let mut total = 0;
    for ice in run.ice.iter().skip(run.position).filter(|ice| ice.rezzed) {
        total += cheapest_break_cost(state, ice, registry)?;
    }
    Some(total)
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
        total += cheapest_break_cost(state, &ice, registry)?;
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
        && cheapest_break_cost(state, ice, registry).is_none()
}

/// The fewest credits any rig card needs to pump up to `ice`'s strength
/// and break all of its pending subroutines; `None` when no rig card can.
/// An ICE with nothing pending costs nothing whatever the rig holds.
pub(super) fn cheapest_break_cost(state: &GameState, ice: &RunIce, registry: &CardRegistry) -> Option<u32> {
    if pending_on(ice) == 0 {
        return Some(0);
    }
    state.runner.rig.iter().filter_map(|card| break_cost(state, card, ice, registry)).min()
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
pub(super) fn break_cost(state: &GameState, card: &InstalledRunnerCard, ice: &RunIce, registry: &CardRegistry) -> Option<u32> {
    let def = registry.get(&card.card)?;
    let pending = pending_on(ice);
    // The number the break contest uses — a pump bought inside the search
    // and what the table adds (Echelon, Rising Tide) included.
    let shortfall = (continuous::ice_strength(state, registry, ice) - continuous::breaker_strength(state, registry, card)).max(0) as u32;
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
            Effect::BreakSubroutines { count, restrict_to } if restrict_to.is_none_or(|r| ice_is(state, ice, r, registry)) => {
                let activations = match count {
                    SubroutineBreakCount::Fixed(n) => pending.div_ceil((*n).max(1)),
                    // X is chosen to cover them all; only an X cost names
                    // it, and one is not a plain credit cost, so this is
                    // skipped above.
                    SubroutineBreakCount::All | SubroutineBreakCount::ChosenNumber => 1,
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
/// `ice` — restricted to a subtype it has, or unrestricted.
pub(super) fn breaks_subtype(state: &GameState, card: &netrunner_core::rules::InstalledRunnerCard, ice: &RunIce, registry: &CardRegistry) -> bool {
    let Some(def) = registry.get(&card.card) else { return false };
    let mut found = false;
    for ability in &def.abilities {
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
            subject: None, when: None, acts_on_subject: false, first_each_turn: false, from_heap: false,
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
}
