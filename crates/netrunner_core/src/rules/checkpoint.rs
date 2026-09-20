//! The checkpoint: what is true of the game whoever's text made it so.
//!
//! The Comprehensive Rules (10.3) put one step after anything happens and
//! *before* anything reacts to it: a player with enough agenda points has
//! won, and an older active copy of a unique card is trashed. They are
//! standing conditions — re-derivable from `GameState` alone — so they are
//! checked in one place, from `dispatcher::dispatch_event` before it plans
//! a single trigger and again at the end of every action, rather than by
//! whichever handler remembered.
//!
//! Until this existed each was a call someone had to make. The win was
//! checked after a steal and after a score, and in both places *after* the
//! triggers the event fired — so a Runner who stole the winning agenda out
//! of Jinteki: Personal Evolution with an empty grip was flatlined by the
//! identity's reaction to the steal that had already won them the game. The
//! unique rule was called from eight install paths and missed a ninth
//! (`Effect::InstallFromZoneIgnoringCost`), and being an install rule it
//! trashed a copy the rules leave alone: a facedown card is not active, so
//! a second facedown Spin Doctor sits beside the first until one is rezzed.
//!
//! **What is deliberately not here**, each for a reason rather than by
//! omission:
//! - *Deck-out and flatline* are not standing conditions but failed
//!   attempts — an empty R&D loses nothing until the Corp must draw from it
//!   (`turn::enter_start_of_turn`, `damage::apply_damage`).
//! - *The memory limit* parks a decision (which program?), so it belongs
//!   where a decision can be parked: the end of the action
//!   (`memory::enforce_limit`), not the middle of a dispatch.
//! - *The console limit* is a restriction on installing, and rejects.
//! - *Empty remotes* need no clearing: a server is derived from what is
//!   installed in it (`legal_actions::existing_remote_ids`).
//!
//! **Expired durations are here too** ([`expire_durations`]), and they are
//! the one check that is not a rule: whether a lingering effect still holds
//! is asked of the state at every read (`rules::lingering`), so sweeping
//! the list is garbage collection and nothing depends on it running.

use crate::cards::CardRegistry;
use crate::dsl::CardId;
use crate::rules::ability;
use crate::rules::active;
use crate::rules::event::GameEvent;
use crate::rules::lingering;
use crate::rules::state::{ArchivedCard, GameState, InstallId, Side};
use crate::rules::win;

/// Runs the standing checks. `event` is what just happened, when something
/// did: it says which copy of a unique card is the one that *became*
/// active, which the state alone cannot.
///
/// The win first: once a side has won, nothing else is the game's to do.
pub(crate) fn state_based(state: &mut GameState, registry: &CardRegistry, event: Option<&GameEvent>) -> Vec<GameEvent> {
    expire_durations(state);
    let mut events = win::check_win_conditions(state, registry);
    if !state.is_over() {
        events.extend(enforce_unique(state, registry, event));
    }
    events
}

/// Drops the lingering effects whose duration has run out. Five call sites
/// used to zero three fields by hand, and the one that forgot let a pump
/// carry into the next run (Rules Audit T8).
pub(crate) fn expire_durations(state: &mut GameState) {
    lingering::sweep(state);
}

/// The ◆ rule: of a side's *active* copies of a unique card, the one that
/// most recently became active stays and the rest are trashed — the Corp's
/// to Archives faceup (they were rezzed), the Runner's to the heap with
/// whatever they hosted.
///
/// "Most recently became active" is the copy a rez just turned faceup when
/// that is what happened, and otherwise the newest install: the Runner's
/// cards are active from the moment they are installed, and install handles
/// are monotonic.
fn enforce_unique(state: &mut GameState, registry: &CardRegistry, event: Option<&GameEvent>) -> Vec<GameEvent> {
    let is_unique = |card: &CardId| registry.get(card).is_some_and(|definition| definition.unique);
    let just_rezzed = match event {
        Some(GameEvent::IceRezzed { install, .. }) => Some(*install),
        _ => None,
    };
    let mut events = Vec::new();

    // Which copies count is `rules::active`'s to say — a faceup agenda is
    // not active, so BANGUN's are no more unique on the table than
    // facedown ones.
    let active_copies = |cards: &mut dyn Iterator<Item = active::ActiveCard<'_>>| -> Vec<(CardId, InstallId)> {
        cards
            .filter(|card| card.place == active::Place::Installed && is_unique(card.card))
            .filter_map(|card| card.install.map(|install| (card.card.clone(), install)))
            .collect()
    };
    let active = active_copies(&mut active::corp(state, registry));
    for install in older_copies(&active, just_rezzed) {
        let Some(position) = state.corp.installed.iter().position(|installed| installed.install_id == install) else { continue };
        let trashed = state.corp.installed.remove(position);
        state.corp.archives.push(ArchivedCard::faceup(trashed.card.clone()));
        events.push(GameEvent::CardTrashed { side: Side::Corp, card: trashed.card });
    }

    let active = active_copies(&mut active::runner(state));
    for install in older_copies(&active, None) {
        let Some(position) = state.runner.rig.iter().position(|installed| installed.install_id == install) else { continue };
        let trashed = state.runner.rig.remove(position);
        state.runner.heap.push(trashed.card.clone());
        events.push(GameEvent::CardTrashed { side: Side::Runner, card: trashed.card.clone() });
        events.extend(ability::cascade_trash_hosted_on_rig_card(state, &trashed));
    }
    events
}

/// Every copy in `active` that is not its card's survivor, oldest first.
fn older_copies(active: &[(CardId, InstallId)], keep: Option<InstallId>) -> Vec<InstallId> {
    let mut older = Vec::new();
    for (card, install) in active {
        let copies = || active.iter().filter(|(other, _)| other == card).map(|(_, install)| *install);
        let survivor = keep.filter(|keep| copies().any(|install| install == *keep)).or_else(|| copies().max());
        if Some(*install) != survivor {
            older.push(*install);
        }
    }
    older.sort();
    older
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardDefinition, CardType, DamageType, Effect, Subject, Trigger, TriggeredEffect};
    use crate::rules::dispatcher::dispatch_event;
    use crate::rules::run::ServerId;
    use crate::rules::state::{GamePhase, InstalledCard};

    fn unique(id: &str, side: Side, card_type: CardType) -> CardDefinition {
        CardDefinition { id: CardId(id.to_string()), title: id.to_string(), side, card_type, unique: true, ..Default::default() }
    }

    fn rezzed(id: &str, install: u32, server: ServerId) -> InstalledCard {
        InstalledCard { install_id: InstallId(install), card: CardId(id.to_string()), server, rezzed: true, ..Default::default() }
    }

    /// The order the rules give and the engine did not: the steal was
    /// dispatched, the identity's reaction flatlined the Runner, and only
    /// then was the score area looked at — by which time the game was over
    /// the other way.
    #[test]
    fn a_steal_that_reaches_the_threshold_wins_before_anything_reacts_to_it() {
        let agenda = CardDefinition {
            id: CardId("the_last_agenda".to_string()),
            side: Side::Corp,
            card_type: CardType::Agenda,
            agenda_points: Some(7),
            advancement_requirement: Some(5),
            ..Default::default()
        };
        let personal_evolution = CardDefinition {
            id: CardId("personal_evolution".to_string()),
            side: Side::Corp,
            card_type: CardType::Identity,
            triggers: vec![TriggeredEffect {
                trigger: Trigger::OnAgendaStolen,
                subject: Some(Subject::Any), when: None, acts_on_subject: false,
                text: None,
                effects: vec![Effect::DealDamage(DamageType::Net, 1)],
                requirement: None,
            }],
            ..Default::default()
        };
        let registry = CardRegistry::from_cards(vec![agenda, personal_evolution]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.corp.identity = Some(CardId("personal_evolution".to_string()));
        state.runner.scored_agendas = vec![CardId("the_last_agenda".to_string())];
        assert!(state.runner.grip.is_empty(), "one net damage would flatline");

        let stolen = GameEvent::AgendaStolen { card: CardId("the_last_agenda".to_string()), agenda_points: 7 };
        let events = dispatch_event(&mut state, &registry, &stolen).unwrap();

        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(events, vec![GameEvent::GameOver { winner: Side::Runner }], "and the identity never reacted");
    }

    #[test]
    fn the_copy_that_became_active_stays_and_the_older_active_copy_is_trashed() {
        let registry = CardRegistry::from_cards(vec![unique("spin_doctor", Side::Corp, CardType::Asset)]);
        let mut state = GameState { phase: GamePhase::Action(Side::Corp), ..Default::default() };
        state.corp.installed = vec![rezzed("spin_doctor", 1, ServerId::Remote(0)), rezzed("spin_doctor", 2, ServerId::Remote(1))];

        // With nothing to say which, the newer install is the newer copy…
        let mut by_install = state.clone();
        let events = state_based(&mut by_install, &registry, None);
        assert_eq!(by_install.corp.installed.iter().map(|c| c.install_id).collect::<Vec<_>>(), vec![InstallId(2)]);
        assert_eq!(by_install.corp.archives, vec![ArchivedCard::faceup(CardId("spin_doctor".to_string()))]);
        assert_eq!(events, vec![GameEvent::CardTrashed { side: Side::Corp, card: CardId("spin_doctor".to_string()) }]);

        // …but a rez says which copy just became active, and it may be the
        // one installed first.
        let rez = GameEvent::IceRezzed { card: CardId("spin_doctor".to_string()), server: ServerId::Remote(0), install: InstallId(1) };
        state_based(&mut state, &registry, Some(&rez));
        assert_eq!(state.corp.installed.iter().map(|c| c.install_id).collect::<Vec<_>>(), vec![InstallId(1)]);
    }

    /// Facedown is not active, so there is nothing for the rule to be about
    /// — and an agenda is not active on the table faceup either.
    #[test]
    fn copies_that_are_not_active_are_left_alone() {
        let registry = CardRegistry::from_cards(vec![
            unique("spin_doctor", Side::Corp, CardType::Asset),
            CardDefinition { agenda_points: Some(1), advancement_requirement: Some(2), ..unique("one_of_a_kind", Side::Corp, CardType::Agenda) },
        ]);
        let mut state = GameState { phase: GamePhase::Action(Side::Corp), ..Default::default() };
        state.corp.installed = vec![
            rezzed("spin_doctor", 1, ServerId::Remote(0)),
            InstalledCard { rezzed: false, ..rezzed("spin_doctor", 2, ServerId::Remote(1)) },
            rezzed("one_of_a_kind", 3, ServerId::Remote(2)),
            rezzed("one_of_a_kind", 4, ServerId::Remote(3)),
        ];
        assert!(state_based(&mut state, &registry, None).is_empty());
        assert_eq!(state.corp.installed.len(), 4);
    }
}
