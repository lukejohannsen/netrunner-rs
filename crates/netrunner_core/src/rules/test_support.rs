//! Shared fixture helpers for this crate's tests.
//!
//! Actions name *installs* and *zone positions*, not cards (see
//! `state::InstallId` and `PlayerAction::ToggleCardSelection`). That is
//! right for the engine and wrong for a test, which reads far better saying
//! "rez *Palisade*" than "rez install 4". These bridge the two, so fixtures
//! keep naming cards.

use crate::dsl::CardId;
use crate::rules::pending_choice;
use crate::rules::state::{GameState, InstallId, PendingDecision};

/// A stable, distinct `InstallId` derived from a card's id — for the
/// fixture *constructors* (`corp_ice`, `installed_with_counters`, …) that
/// build a fresh install per call and so cannot carry a literal id.
///
/// Every fixture in this crate installs at most one copy of any given card,
/// which is exactly the condition under which keying on the name is
/// unambiguous — and [`install_of`], which resolves the other direction,
/// carries the same caveat. A fixture that installs two copies must assign
/// both ids itself.
///
/// The range sits far above anything `GameState::allocate_install_id`
/// reaches in a test, so a fixture install can never collide with one the
/// engine allocates later in the same test.
pub(crate) fn fixture_install_id(card: &str) -> InstallId {
    // FNV-1a. Any stable hash would do; this one avoids a dependency.
    let mut hash: u32 = 2_166_136_261;
    for byte in card.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    InstallId(1_000_000 + hash % 1_000_000)
}

/// The `InstallId` of the install of `card`, on either side.
///
/// Takes the **first** match — exactly what the production lookup did
/// before `InstallId`, and unambiguous for every fixture that installs at
/// most one copy of a card. A test that deliberately installs two must name
/// the ids itself; distinguishing them is the whole point of `InstallId`,
/// and `advancing_one_of_two_copies_advances_only_that_copy` does it.
pub(crate) fn install_of(state: &GameState, card: &str) -> InstallId {
    let id = CardId(card.to_string());
    let found = state
        .corp
        .installed
        .iter()
        .find(|c| c.card == id)
        .map(|c| c.install_id)
        .or_else(|| state.runner.rig.iter().find(|c| c.card == id).map(|c| c.install_id))
        .unwrap_or_else(|| panic!("{card} is not installed on either side"));

    // A fixture that hand-builds several installs with `..Default::default()`
    // gives them all `InstallId::PLACEHOLDER`, and every action then
    // resolves to whichever was listed first — silently acting on the wrong
    // card, which is precisely the aliasing `InstallId` exists to end.
    // Fail loudly instead: such a fixture must set its own ids.
    if found == InstallId::PLACEHOLDER {
        let placeholders = state.corp.installed.iter().filter(|c| c.install_id == InstallId::PLACEHOLDER).count()
            + state.runner.rig.iter().filter(|c| c.install_id == InstallId::PLACEHOLDER).count();
        assert!(
            placeholders <= 1,
            "{placeholders} installs share InstallId::PLACEHOLDER, so {card} cannot be addressed \
             unambiguously — give this fixture's installs explicit ids"
        );
    }
    found
}

/// The position of `card` within the parked `ChooseCards` decision's source
/// zone — what `PlayerAction::ToggleCardSelection` names.
///
/// Same first-match caveat as [`install_of`]: a test selecting two copies of
/// one card must spell the positions out, as
/// `carnivore_can_select_two_copies_of_the_same_card` does.
pub(crate) fn position_of(state: &GameState, card: &str) -> usize {
    let Some(PendingDecision::ChooseCards { side, source, .. }) = &state.pending_decision else {
        panic!("no ChooseCards decision is parked");
    };
    let id = CardId(card.to_string());
    pending_choice::zone_card_ids(state, *side, source)
        .iter()
        .position(|c| *c == id)
        .unwrap_or_else(|| panic!("{card} is not in the pending decision's source zone"))
}
