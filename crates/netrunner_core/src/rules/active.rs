//! Which cards are active — one answer, for everything that asks.
//!
//! A card's abilities work while it is active (CR 9.1): an identity, a scored
//! agenda, a rezzed install that is not an agenda, everything in the rig, and
//! the event whose run is under way (CR 8.6.5).
//! That sentence was written out twice — in `listeners`, for who hears an
//! event, and again in `checkpoint`, for which copies of a unique card count —
//! with a comment in each pointing at the other, and the continuous-effect
//! layer would have been the third. It is here once; the three read it.
//!
//! **Faceup is not active for an agenda**, unless its own text installs it
//! faceup (CR 3.2.3a: Sacrifice Zone Expansion's "Install only faceup").
//! BANGUN installs agendas faceup, and an agenda's abilities are live in a
//! score area, not on the table. The old per-event audiences asked every
//! `rezzed` install, so a faceup Off the Books spent its counters from a
//! remote.
//!
//! **A card added to a score area "as an agenda" is not active**: it has lost
//! everything it printed (CR 10.1.3), so there is nothing to hear or apply.
//! Word on the Street in the Corp's score area is a −1, not a resource.

use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardType};
use crate::rules::run::ServerId;
use crate::rules::state::{GameState, InstallId, Side};

/// Where an active card is. `checkpoint` wants only what is installed; a
/// scored agenda keeps the install handle it was scored with, so the handle
/// alone does not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Identity,
    ScoreArea,
    Installed,
    /// The event that started the active run, active in the play area for
    /// as long as the run lasts (CR 8.6.5, `run::run_event`): Eye for an
    /// Eye's "If successful" and its "Access →" are its own text, heard and
    /// used during that run and never after.
    PlayArea,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ActiveCard<'a> {
    pub side: Side,
    pub card: &'a CardId,
    pub install: Option<InstallId>,
    /// The server a Corp install is in or protecting, and the one a
    /// Trojan's host ice protects — Stowaway's "whenever you make a
    /// successful run on **this server**". A rig card that is not hosted
    /// on ice is in no server.
    pub server: Option<ServerId>,
    pub place: Place,
}

/// Every active card, the Corp's and then the Runner's, each side's in the
/// order `listeners` always asked them: the identity, the score area, then
/// the table. An iterator rather than a `Vec`, because `memory::refresh`
/// asks after every action and a search applies millions of them.
pub(crate) fn active_cards<'a>(state: &'a GameState, registry: &'a CardRegistry) -> impl Iterator<Item = ActiveCard<'a>> + 'a {
    corp(state, registry).chain(runner(state, registry))
}

pub(crate) fn corp<'a>(state: &'a GameState, registry: &'a CardRegistry) -> impl Iterator<Item = ActiveCard<'a>> + 'a {
    // An installed agenda is active only faceup by its own text.
    let inactive_when_rezzed =
        move |card: &CardId| registry.get(card).is_some_and(|definition| definition.card_type == CardType::Agenda && !definition.installs_faceup);
    let identity =
        state.corp.identity.iter().map(|card| ActiveCard { side: Side::Corp, card, install: None, server: None, place: Place::Identity });
    let scored = state.corp.scored_agendas.iter().filter(|scored| scored.as_agenda.is_none()).map(|scored| ActiveCard {
        side: Side::Corp,
        card: &scored.card,
        install: Some(scored.install_id),
        server: None,
        place: Place::ScoreArea,
    });
    let installed = state.corp.installed.iter().filter(move |installed| installed.rezzed && !inactive_when_rezzed(&installed.card)).map(|installed| ActiveCard {
        side: Side::Corp,
        card: &installed.card,
        install: Some(installed.install_id),
        server: Some(installed.server),
        place: Place::Installed,
    });
    identity.chain(scored).chain(installed)
}

pub(crate) fn runner<'a>(state: &'a GameState, registry: &'a CardRegistry) -> impl Iterator<Item = ActiveCard<'a>> + 'a {
    let identity =
        state.runner.identity.iter().map(|card| ActiveCard { side: Side::Runner, card, install: None, server: None, place: Place::Identity });
    let rig = state.runner.rig.iter().map(|installed| ActiveCard {
        side: Side::Runner,
        card: &installed.card,
        install: Some(installed.install_id),
        server: installed.hosted_on_ice.and_then(|host| state.corp.installed.iter().find(|ice| ice.install_id == host)).map(|ice| ice.server),
        place: Place::Installed,
    });
    let run_event = crate::rules::run::run_event(state, registry).into_iter().map(|card| ActiveCard {
        side: Side::Runner,
        card,
        install: None,
        server: None,
        place: Place::PlayArea,
    });
    identity.chain(rig).chain(run_event)
}
