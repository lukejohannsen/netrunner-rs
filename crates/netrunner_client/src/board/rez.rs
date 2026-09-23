//! A rez that gives the card nothing to do: a trap's.
//!
//! Byte!, Snare! and Urtica Cipher do their work when the Runner accesses
//! them, face down — the subject of a moment hears it wherever it is (the
//! Listener Rule), and an access interaction resolves on the card as
//! accessed. Rezzing one gains the Corp nothing and shows the Runner what
//! it is, which no player does. The rules still allow it (CR 5.7.1b: the
//! Corp may rez non-ice cards in each paid ability window), and the
//! person decided it stays allowed (22 September 2026), so the engine
//! lists it and a client keeps it on the card's menu. What a client does
//! not do is present it as a move: it earns no glow
//! ([`ActionMap::affordance`](super::ActionMap::affordance)) and it is no
//! reason to hold the Corp in a window ([`crate::play::lone_pass`]).
//! Before this, an installed trap stopped the Corp in every window of the
//! Runner's turn to offer "Rez Urtica Cipher".
//!
//! Read off the card, never a list of names, so the next trap is covered
//! and a card that does something rezzed is not. **Nothing in the pool
//! rewards the Corp for rezzing any card** (checked 22 September 2026):
//! every `OnRez` trigger is `subject: This` but Barry "Baz" Wong's, a
//! Runner identity that hears Barrier ICE; the `Rezzed` filters are
//! LEO Construction's and Mycoweb's (Bioroids, ICE) and Maglectric
//! Rapid's and Charm Offensive's, which help the Runner. A card that
//! changes that would make this a real move again —
//! `exactly_the_traps_gain_nothing_by_a_rez` is where to find out.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardDefinition, CardType, Subject, Trigger};
use netrunner_core::rules::PlayerAction;
use netrunner_core::view::ClientView;

/// Whether rezzing `card` gives it nothing to do: a non-ice card with no
/// ability, standing effect, subroutine, recurring or hosted credits, and
/// no trigger but its own access — which, with an access interaction,
/// works face down.
pub fn gains_nothing(card: &CardDefinition) -> bool {
    !matches!(card.card_type, CardType::Ice(_))
        && card.abilities.is_empty()
        && card.continuous.is_empty()
        && card.subroutines.is_empty()
        && card.recurring_credits.is_none()
        && card.pays_for.is_empty()
        && card.triggers.iter().all(|t| t.trigger == Trigger::OnAccessed && t.subject == Some(Subject::This))
}

/// Whether `action` rezzes a card that [`gains_nothing`] by it, as the
/// viewer knows the card — the Corp always knows its own face-down cards.
pub fn is_idle_rez(action: &PlayerAction, view: &ClientView, registry: &CardRegistry) -> bool {
    match action {
        PlayerAction::RezIce { ice } => crate::actions::installed_card_id(view, ice).and_then(|card| registry.get(&card)).is_some_and(gains_nothing),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::Side;

    /// The pool's traps and nothing else. A new card that lands here is a
    /// trap to be glad of; a card that leaves, or a card elsewhere that
    /// starts rewarding a rez, is the moment to re-read the module doc.
    #[test]
    fn exactly_the_traps_gain_nothing_by_a_rez() {
        let registry = crate::decks::sample_deck_registry();
        let mut idle: Vec<&str> = registry.iter().filter(|card| card.side == Side::Corp && card.card_type != CardType::Agenda && card.card_type != CardType::Identity && gains_nothing(card)).map(|card| card.id.0.as_str()).collect();
        idle.sort_unstable();
        assert_eq!(idle, ["byte", "snare", "urtica_cipher"]);
    }

    /// The Corp's view of an unrezzed Urtica Cipher (install 1) and an
    /// unrezzed Nico Campaign (install 2), with `legal` as its actions.
    fn corp_view(legal: Vec<PlayerAction>) -> (CardRegistry, ClientView) {
        use netrunner_core::dsl::CardId;
        use netrunner_core::rules::{GameState, InstallId, InstallSlot, InstalledCard, ServerId};
        let registry = crate::decks::sample_deck_registry();
        let mut state = GameState::new(0);
        state.corp.installed = [(1, "urtica_cipher", 0), (2, "nico_campaign", 1)]
            .map(|(id, card, remote)| InstalledCard { install_id: InstallId(id), card: CardId(card.to_string()), server: ServerId::Remote(remote), slot: InstallSlot::Root, ..Default::default() })
            .to_vec();
        let mut view = netrunner_core::view::build_client_view(&state, &registry, Side::Corp);
        view.legal_actions = legal;
        (registry, view)
    }

    /// A pass beside a trap's rez is taken for the Corp, a pass beside a
    /// real rez is not; and the trap's rez stays on its card's menu,
    /// labelled, with no glow.
    #[test]
    fn a_traps_rez_is_on_its_menu_but_never_lit_or_waited_for() {
        use crate::board::{ActionMap, Target};
        use netrunner_core::rules::InstallId;
        let pass = PlayerAction::PassPriority { side: Side::Corp };
        let rez = |id| PlayerAction::RezIce { ice: InstallId(id) };

        let (registry, view) = corp_view(vec![pass.clone(), rez(1)]);
        assert!(is_idle_rez(&rez(1), &view, &registry) && !is_idle_rez(&rez(2), &view, &registry));
        assert_eq!(crate::play::lone_pass(&view, &registry), Some(pass.clone()), "a trap is no reason to stop");
        let map = ActionMap::build(&view, &registry);
        assert_eq!(map.affordance(&Target::Install(InstallId(1))), None, "and no reason to glow");
        let entries = map.for_install(InstallId(1));
        assert_eq!(entries.len(), 1, "but the rez is still on the card's menu");
        assert!(map.entries[entries[0]].label.contains("works face down"), "{}", map.entries[entries[0]].label);
        assert_eq!(map.affordance_of_entry(entries[0]), None);

        let (registry, view) = corp_view(vec![pass.clone(), rez(1), rez(2)]);
        assert_eq!(crate::play::lone_pass(&view, &registry), None, "Nico Campaign's rez is a real choice");
        assert!(ActionMap::build(&view, &registry).affordance(&Target::Install(InstallId(2))).is_some());
    }
}

