//! The one scan that reads `CardDefinition::continuous`.
//!
//! Every question a standing effect can change — how strong is this
//! icebreaker, what does this install cost, how much memory is there — is put
//! here, about a [`Target`], and answered by walking the active cards
//! (`rules::active`) and asking each effect three things: is it the kind
//! being asked about, is the target one it applies to, is it on.
//!
//! **Who is asked is the Listener Rule's sentence again**: a card's text
//! about *itself* (`Scope::This`) applies wherever the card is — Carmen
//! prices herself from the grip, where she is not active — and every other
//! effect needs an active source. One more source is read off the run, for
//! the same reason `listeners` reads it: a persistent upgrade trashed during
//! a run on its server "still applies for the remainder of this run".
//!
//! **Derived, never cached.** There were six of these scans, one per field,
//! each in the handler that needed it; there is no seventh place to keep in
//! sync, and nothing here is written to `GameState`. If a profile ever says
//! the scan costs too much, memoise it for one `apply_action`, not on the
//! state a search clones.

use crate::cards::CardRegistry;
use crate::dsl::{card_matches_filter, CardDefinition, CardId, CardType, ContinuousEffect, ContinuousKind, IceType, Number, Scope};
use crate::rules::ability::{self, ResolutionContext};
use crate::rules::active::{self, ActiveCard};
use crate::rules::lingering;
use crate::rules::run::ServerId;
use crate::rules::state::{GameState, InstallId, InstallSlot, InstalledRunnerCard, Side};

/// What a question is about.
#[derive(Clone, Copy)]
pub(crate) enum Target<'a> {
    /// A player — their memory, their maximum hand size.
    Player(Side),
    /// A card that is not installed: one being installed, priced from a hand.
    Card(&'a CardDefinition),
    /// A card in the rig.
    Rig { card: &'a CardDefinition, install: InstallId },
    /// A Corp install, in the root of `server` or protecting it.
    Corp { card: &'a CardDefinition, install: InstallId, server: ServerId, root: bool },
}

impl<'a> Target<'a> {
    fn card(&self) -> Option<&'a CardDefinition> {
        match self {
            Target::Player(_) => None,
            Target::Card(card) | Target::Rig { card, .. } | Target::Corp { card, .. } => Some(card),
        }
    }

    fn install(&self) -> Option<InstallId> {
        match self {
            Target::Rig { install, .. } | Target::Corp { install, .. } => Some(*install),
            Target::Player(_) | Target::Card(_) => None,
        }
    }

    /// The Corp install `install`, as a target.
    pub(crate) fn corp_install(state: &GameState, registry: &'a CardRegistry, install: InstallId) -> Option<Self> {
        let installed = state.corp.installed.iter().find(|c| c.install_id == install)?;
        let card = registry.get(&installed.card)?;
        Some(Target::Corp { card, install, server: installed.server, root: installed.slot == InstallSlot::Root })
    }
}

/// A source of effects: a card, and where it stands.
struct Source<'a> {
    side: Side,
    card: &'a CardId,
    install: Option<InstallId>,
    server: Option<ServerId>,
    /// Only what the card says about itself is read — a source that is not
    /// active.
    own_text_only: bool,
    /// A persistent upgrade the Runner trashed this run: only what it says
    /// about its server survives it.
    server_text_only: bool,
}

impl<'a> From<ActiveCard<'a>> for Source<'a> {
    fn from(active: ActiveCard<'a>) -> Self {
        Source { side: active.side, card: active.card, install: active.install, server: active.server, own_text_only: false, server_text_only: false }
    }
}

/// Calls `found` with every effect that applies to `target` right now, its
/// number resolved as its source.
fn for_each_applying<'a>(
    state: &'a GameState,
    registry: &'a CardRegistry,
    target: Target<'a>,
    mut found: impl FnMut(&'a ContinuousEffect, &Source<'a>, &ResolutionContext<'a>),
) {
    let mut ask = |source: Source<'a>| {
        let Some(definition) = registry.get(source.card) else { return };
        if definition.continuous.is_empty() {
            return;
        }
        let ctx = match source.install {
            Some(install) => ResolutionContext::for_install(install, source.card),
            None => ResolutionContext::for_card(Some(source.card)),
        };
        for effect in &definition.continuous {
            let is_own_text = effect.applies_to == Scope::This;
            if source.own_text_only != is_own_text {
                continue;
            }
            if source.server_text_only && !matches!(effect.applies_to, Scope::RootOfThisServer(_)) {
                continue;
            }
            if !applies(state, &source, &effect.applies_to, &target) {
                continue;
            }
            if let Some(condition) = &effect.condition
                && ability::check_requirement(state, condition, source.side, &ctx, registry).is_err()
            {
                continue;
            }
            found(effect, &source, &ctx);
        }
    };

    // The card itself first, active or not.
    if let Some(card) = target.card() {
        ask(Source { side: card.side, card: &card.id, install: target.install(), server: None, own_text_only: true, server_text_only: false });
    }
    // A question about a player reaches only that player's own cards
    // (`Scope::Controller`), and it is the one asked after every action
    // (`memory::refresh`), so it does not walk the other side's table.
    match target {
        Target::Player(Side::Runner) => active::runner(state).for_each(|active| ask(active.into())),
        Target::Player(Side::Corp) => active::corp(state, registry).for_each(|active| ask(active.into())),
        _ => active::active_cards(state, registry).for_each(|active| ask(active.into())),
    }
    if let Some(run) = &state.active_run {
        for card in &run.persistent_trashed_upgrades {
            ask(Source { side: Side::Corp, card, install: None, server: Some(run.server), own_text_only: false, server_text_only: true });
        }
    }
}

/// Whether `scope`, read from `source`, reaches `target`. `Scope::This` is
/// matched by who is asked (`Source::own_text_only`), not here.
fn applies(state: &GameState, source: &Source<'_>, scope: &Scope, target: &Target<'_>) -> bool {
    match (scope, target) {
        (Scope::This, _) => source.own_text_only,
        (Scope::Host, target) => {
            let Some(host) = target.install() else { return false };
            let Some(install) = source.install else { return false };
            state
                .runner
                .rig
                .iter()
                .find(|card| card.install_id == install)
                .is_some_and(|card| card.hosted_on_program == Some(host) || card.hosted_on_ice == Some(host))
        }
        (Scope::Controller, Target::Player(side)) => source.side == *side,
        (Scope::Installing(filter), Target::Card(card)) => card.side == source.side && card_matches_filter(card, filter),
        (Scope::Ice, Target::Corp { card, root: false, .. }) => matches!(card.card_type, CardType::Ice(_)),
        (Scope::RootOfThisServer(filter), Target::Corp { card, server, root: true, .. }) => {
            source.server == Some(*server) && card_matches_filter(card, filter)
        }
        _ => false,
    }
}

/// The sum of every applying effect `pick` reads a number off.
pub(crate) fn sum(state: &GameState, registry: &CardRegistry, target: Target<'_>, pick: fn(&ContinuousKind) -> Option<&Number>) -> i32 {
    let mut total = 0;
    for_each_applying(state, registry, target, |effect, _, ctx| {
        if let Some(number) = pick(&effect.kind) {
            total += number.per * ability::resolve_amount(&number.of, ctx, state, registry) as i32;
        }
    });
    total
}

/// Whether any applying effect is one `is` accepts.
pub(crate) fn any(state: &GameState, registry: &CardRegistry, target: Target<'_>, is: impl Fn(&ContinuousKind) -> bool) -> bool {
    let mut found = false;
    for_each_applying(state, registry, target, |effect, _, _| found |= is(&effect.kind));
    found
}

fn strength(kind: &ContinuousKind) -> Option<&Number> {
    match kind {
        ContinuousKind::Strength(number) => Some(number),
        _ => None,
    }
}

fn install_cost(kind: &ContinuousKind) -> Option<&Number> {
    match kind {
        ContinuousKind::InstallCost(number) => Some(number),
        _ => None,
    }
}

/// Memory the Runner's active cards grant, on top of the base.
pub(crate) fn memory(state: &GameState, registry: &CardRegistry) -> i32 {
    sum(state, registry, Target::Player(Side::Runner), |kind| match kind {
        ContinuousKind::Memory(number) => Some(number),
        _ => None,
    })
}

/// What `side`'s active cards add to its maximum hand size: an identity
/// (Haas-Bioroid: Precision Design), a scored agenda (Superconducting Hub),
/// a piece of hardware (T400 Memory Diamond) — for as long as each is one.
pub(crate) fn hand_size(state: &GameState, registry: &CardRegistry, side: Side) -> i32 {
    sum(state, registry, Target::Player(side), |kind| match kind {
        ContinuousKind::HandSize(number) => Some(number),
        _ => None,
    })
}

/// A rig card's strength right now: what is stored on the install (printed,
/// plus the boosts paid for) and what the table adds. **The one number** —
/// the break contest, the view and the bots' pricing all read this, where
/// the view used to show the stored half alone.
pub fn breaker_strength(state: &GameState, registry: &CardRegistry, card: &InstalledRunnerCard) -> i32 {
    let stored = lingering::rig_strength(state, card);
    let Some(definition) = registry.get(&card.card) else { return stored };
    stored + sum(state, registry, Target::Rig { card: definition, install: card.install_id }, strength)
}

/// Whether a boost to the icebreaker `install` lasts the run rather than the
/// encounter.
pub(crate) fn boosts_last_the_run(state: &GameState, registry: &CardRegistry, install: InstallId) -> bool {
    let Some(card) = state.runner.rig.iter().find(|card| card.install_id == install).and_then(|card| registry.get(&card.card)) else { return false };
    any(state, registry, Target::Rig { card, install }, |kind| matches!(kind, ContinuousKind::BoostsLastTheRun))
}

/// Whether the ice `install` has gained `subtype` — on top of the one it
/// prints, which the caller already knows.
pub fn ice_gains_subtype(state: &GameState, registry: &CardRegistry, install: InstallId, subtype: IceType) -> bool {
    let Some(target) = Target::corp_install(state, registry, install) else { return false };
    any(state, registry, target, |kind| *kind == ContinuousKind::GainSubtype(subtype))
}

/// What installing `card` costs the Runner right now, read-only.
pub(crate) fn install_cost_of(state: &GameState, registry: &CardRegistry, card: &CardDefinition) -> u32 {
    (card.cost as i32 + sum(state, registry, Target::Card(card), install_cost)).max(0) as u32
}

/// [`install_cost_of`], for the install that is actually happening: spends
/// the "first time each turn" of every effect that lowered it.
pub(crate) fn pay_install_cost_of(state: &mut GameState, registry: &CardRegistry, card: &CardDefinition) -> u32 {
    let cost = install_cost_of(state, registry, card);
    let mut spent = Vec::new();
    for_each_applying(state, registry, Target::Card(card), |effect, source, _| {
        if install_cost(&effect.kind).is_some()
            && let Some(condition) = &effect.condition
        {
            spent.push((condition.clone(), source.side, source.install, source.card.clone()));
        }
    });
    for (condition, side, install, card) in spent {
        let ctx = match install {
            Some(install) => ResolutionContext::for_install(install, &card),
            None => ResolutionContext::for_card(Some(&card)),
        };
        ability::consume_requirement(state, &condition, side, &ctx);
    }
    cost
}

/// What the table adds to the cost of rezzing the Corp install `install`.
pub(crate) fn rez_cost_delta(state: &GameState, registry: &CardRegistry, install: InstallId) -> i32 {
    let Some(target) = Target::corp_install(state, registry, install) else { return 0 };
    sum(state, registry, target, |kind| match kind {
        ContinuousKind::RezCost(number) => Some(number),
        _ => None,
    })
}

/// What the table adds to the cost of trashing the Corp install `install`.
pub(crate) fn trash_cost_delta(state: &GameState, registry: &CardRegistry, install: InstallId) -> i32 {
    let Some(target) = Target::corp_install(state, registry, install) else { return 0 };
    sum(state, registry, target, |kind| match kind {
        ContinuousKind::TrashCost(number) => Some(number),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{Amount, CardFilter, EffectRequirement};
    use crate::rules::run::RunState;
    use crate::rules::state::{GamePhase, InstalledCard};

    fn flat(per: i32) -> Number {
        Number { per, of: Amount::Fixed(1) }
    }

    fn prints(id: &str, side: Side, card_type: CardType, kind: ContinuousKind, applies_to: Scope) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type,
            continuous: vec![ContinuousEffect { kind, applies_to, condition: None, text: None }],
            ..Default::default()
        }
    }

    fn blank(id: &str, side: Side, card_type: CardType, cost: u32) -> CardDefinition {
        CardDefinition { id: CardId(id.to_string()), title: id.to_string(), side, card_type, cost, ..Default::default() }
    }

    fn in_the_rig(id: &str, install: u32) -> InstalledRunnerCard {
        InstalledRunnerCard { install_id: InstallId(install), card: CardId(id.to_string()), ..Default::default() }
    }

    fn on_the_table(id: &str, install: u32, server: ServerId, slot: InstallSlot, rezzed: bool) -> InstalledCard {
        InstalledCard { install_id: InstallId(install), card: CardId(id.to_string()), server, slot, rezzed, ..Default::default() }
    }

    /// The Listener Rule's sentence, for standing effects: what a card says
    /// about itself applies from the grip, where it is not active, and what
    /// it says about anything else does not.
    #[test]
    fn a_cards_own_text_applies_wherever_it_is_and_nothing_else_of_it_does() {
        let mut cheap = prints("cheap", Side::Runner, CardType::Program, ContinuousKind::InstallCost(flat(-2)), Scope::This);
        cheap.cost = 5;
        let mut generous =
            prints("generous", Side::Runner, CardType::Hardware, ContinuousKind::InstallCost(flat(-1)), Scope::Installing(CardFilter::Any));
        generous.cost = 3;
        let registry = CardRegistry::from_cards(vec![cheap.clone(), generous.clone()]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.runner.grip = vec![cheap.id.clone(), generous.id.clone()];

        assert_eq!(install_cost_of(&state, &registry, &cheap), 3, "its own discount, from the grip");
        assert_eq!(install_cost_of(&state, &registry, &generous), 3, "a card in the grip discounts nothing else — itself included");

        state.runner.rig = vec![in_the_rig("generous", 1)];
        assert_eq!(install_cost_of(&state, &registry, &cheap), 2, "installed, it is active");
    }

    /// A facedown upgrade is not active; a rezzed one reaches its own root
    /// and no other; and a persistent one the Runner trashed this run still
    /// reaches the server being run.
    #[test]
    fn an_upgrade_reaches_its_own_root_while_rezzed_and_for_the_run_it_was_trashed_in() {
        let grid =
            prints("grid", Side::Corp, CardType::Upgrade, ContinuousKind::TrashCost(flat(2)), Scope::RootOfThisServer(CardFilter::CardType(CardType::Asset)));
        let registry = CardRegistry::from_cards(vec![blank("asset", Side::Corp, CardType::Asset, 0), grid]);
        let mut state = GameState::default();
        state.corp.installed = vec![
            on_the_table("grid", 1, ServerId::Remote(0), InstallSlot::Root, false),
            on_the_table("asset", 2, ServerId::Remote(0), InstallSlot::Root, false),
            on_the_table("asset", 3, ServerId::Remote(1), InstallSlot::Root, false),
        ];
        let (beside_it, elsewhere) = (InstallId(2), InstallId(3));

        assert_eq!(trash_cost_delta(&state, &registry, beside_it), 0, "facedown");
        state.corp.installed[0].rezzed = true;
        assert_eq!(trash_cost_delta(&state, &registry, beside_it), 2);
        assert_eq!(trash_cost_delta(&state, &registry, elsewhere), 0, "another server's root");

        state.corp.installed.remove(0);
        assert_eq!(trash_cost_delta(&state, &registry, beside_it), 0, "gone, and no run to persist through");
        state.active_run =
            Some(RunState { server: ServerId::Remote(0), persistent_trashed_upgrades: vec![CardId("grid".to_string())], ..Default::default() });
        assert_eq!(trash_cost_delta(&state, &registry, beside_it), 2, "persistent, for the remainder of the run");
        assert_eq!(trash_cost_delta(&state, &registry, elsewhere), 0);
    }

    /// `Scope::Host` is a relation between two installs, not two cards: the
    /// second copy of the host gets nothing.
    #[test]
    fn a_hosted_card_reaches_the_install_it_is_hosted_on_and_not_its_twin() {
        let mut breaker = blank("breaker", Side::Runner, CardType::Program, 0);
        breaker.strength = Some(1);
        let pad = prints("pad", Side::Runner, CardType::Hardware, ContinuousKind::Strength(flat(1)), Scope::Host);
        let registry = CardRegistry::from_cards(vec![breaker, pad]);
        let mut state = GameState::default();
        let strong = |install: u32| InstalledRunnerCard { base_strength: 1, ..in_the_rig("breaker", install) };
        state.runner.rig = vec![strong(1), strong(2), InstalledRunnerCard { hosted_on_program: Some(InstallId(2)), ..in_the_rig("pad", 3) }];

        assert_eq!(breaker_strength(&state, &registry, &state.runner.rig[0]), 1);
        assert_eq!(breaker_strength(&state, &registry, &state.runner.rig[1]), 2);
        assert!(!boosts_last_the_run(&state, &registry, InstallId(2)), "a kind nobody prints is not found");
    }

    /// Two copies of "the first program you install each turn costs 1[c]
    /// less" are two abilities, each with its own first time: the install
    /// that uses both spends both, and the next pays in full. The field this
    /// replaced returned the first source it found and shared one flag.
    #[test]
    fn two_copies_of_a_first_time_each_turn_discount_stack_and_are_each_spent() {
        let mut optimizer =
            prints("optimizer", Side::Runner, CardType::Hardware, ContinuousKind::InstallCost(flat(-1)), Scope::Installing(CardFilter::CardType(CardType::Program)));
        optimizer.continuous[0].condition = Some(EffectRequirement::OncePerTurn("first_program".to_string()));
        let program = blank("program", Side::Runner, CardType::Program, 3);
        let hardware = blank("hardware", Side::Runner, CardType::Hardware, 3);
        let registry = CardRegistry::from_cards(vec![optimizer, program.clone(), hardware.clone()]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.runner.rig = vec![in_the_rig("optimizer", 1), in_the_rig("optimizer", 2)];

        assert_eq!(install_cost_of(&state, &registry, &hardware), 3, "not a program");
        assert_eq!(install_cost_of(&state, &registry, &program), 1);
        assert_eq!(install_cost_of(&state, &registry, &program), 1, "asking spends nothing");
        assert_eq!(pay_install_cost_of(&mut state, &registry, &program), 1);
        assert_eq!(install_cost_of(&state, &registry, &program), 3, "both first times are spent");
    }
}
