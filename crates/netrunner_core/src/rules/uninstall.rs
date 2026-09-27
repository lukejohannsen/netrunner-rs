//! The one door a Corp install leaves the table through.
//!
//! A card is uninstalled "when it stops being installed for any reason"
//! (CR 8.5.1b): trashed by the Runner or by a card's text, scored (CR
//! 1.17.5), stolen, removed from the game, or trashed by the ◆ rule. Nine
//! sites each took the card out of `CorpState::installed` themselves, which
//! was harmless while nothing cared *that* a card was leaving, only where
//! it went. Luana Campos cares: "[interrupt] → When this asset would be
//! uninstalled, take all hosted bad publicity." An interrupt resolves
//! while the card is still there (CR 9.9.4b), and only a door every site
//! goes through can hold the card still long enough to ask it.
//!
//! **Where the card goes is still the site's.** Archives faceup or
//! facedown, the score area, the removed-from-game pile: each site knew
//! that before and still does. The door takes the card off the table and
//! nothing else, so that "about to leave" is said in one place.
//!
//! **What leaves with it is still the site's too** (a piece of ice's
//! Trojans, `ability::cascade_trash_hosted_programs`): the sites that
//! cascaded still do, and folding the cascade in here would change what
//! the others do, which is a separate question from this door's.

use crate::cards::CardRegistry;
use crate::dsl::Trigger;
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::state::{GameState, InstallId, InstalledCard};

/// Takes `install` off the table because an instruction uninstalls it,
/// after announcing it to the card when the card's own text interrupts its
/// uninstalling (`Trigger::OnWouldBeUninstalled`). `Ok(None)` if nothing by
/// that handle is installed — the "already gone" leniency every removal
/// site had, since a card can leave while a trash of it is parked.
///
/// **Announced only when the card is rezzed and prints the interrupt.**
/// An interrupt is marked pending only if it is active (CR 9.9.4b), and a
/// facedown card announced would be a card named to the Runner. Nothing but
/// the card itself hears the moment (`Trigger::OnWouldBeUninstalled` is
/// printed about "this asset" alone), so an announcement nobody could hear
/// is left out, as a tag or a trash about to happen is when nobody could
/// prevent it (the Prevention Rule).
pub(crate) fn corp_install(
    state: &mut GameState,
    registry: &CardRegistry,
    install: InstallId,
) -> Result<Option<(InstalledCard, Vec<GameEvent>)>, RulesError> {
    let Some(installed) = state.find_corp_install(install) else { return Ok(None) };
    let interrupts = installed.rezzed
        && registry
            .get(&installed.card)
            .is_some_and(|definition| definition.triggers.iter().any(|t| t.trigger == Trigger::OnWouldBeUninstalled));
    let mut events = Vec::new();
    if interrupts {
        let card = installed.card.clone();
        dispatcher::emit(state, registry, &mut events, GameEvent::AboutToBeUninstalled { card, install })?;
    }
    // Asked again: what the interrupt did may have taken the card already.
    Ok(take(state, install).map(|removed| (removed, events)))
}

/// Takes `install` off the table at a checkpoint (the ◆ rule, CR 10.3.1d).
/// A checkpoint carries out no instruction, and an interrupt window opens
/// only for one that is imminent (CR 9.9.4), so nothing is announced. It
/// also could not be: the checkpoint runs inside `dispatch_event`, whose
/// own checkpoint would find the older copy still installed and trash it
/// again.
pub(crate) fn corp_install_at_checkpoint(state: &mut GameState, install: InstallId) -> Option<InstalledCard> {
    take(state, install)
}

fn take(state: &mut GameState, install: InstallId) -> Option<InstalledCard> {
    let position = state.corp.installed.iter().position(|c| c.install_id == install)?;
    Some(state.corp.installed.remove(position))
}
