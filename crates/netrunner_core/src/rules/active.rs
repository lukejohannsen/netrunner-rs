//! Which cards are active — one answer, for everything that asks.
//!
//! A card's abilities work while it is active (CR 9.1): an identity, a scored
//! agenda, a rezzed install that is not an agenda, and everything in the rig.
//! That sentence was written out twice — in `listeners`, for who hears an
//! event, and again in `checkpoint`, for which copies of a unique card count —
//! with a comment in each pointing at the other, and the continuous-effect
//! layer would have been the third. It is here once; the three read it.
//!
//! **Faceup is not active for an agenda.** BANGUN installs agendas faceup, and
//! an agenda's abilities are live in a score area, not on the table. The old
//! per-event audiences asked every `rezzed` install, so a faceup Off the Books
//! spent its counters from a remote.

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
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ActiveCard<'a> {
    pub side: Side,
    pub card: &'a CardId,
    pub install: Option<InstallId>,
    /// The server a Corp install is in or protecting.
    pub server: Option<ServerId>,
    pub place: Place,
}

/// Every active card, the Corp's and then the Runner's, each side's in the
/// order `listeners` always asked them: the identity, the score area, then
/// the table. An iterator rather than a `Vec`, because `memory::refresh`
/// asks after every action and a search applies millions of them.
pub(crate) fn active_cards<'a>(state: &'a GameState, registry: &'a CardRegistry) -> impl Iterator<Item = ActiveCard<'a>> + 'a {
    corp(state, registry).chain(runner(state))
}

pub(crate) fn corp<'a>(state: &'a GameState, registry: &'a CardRegistry) -> impl Iterator<Item = ActiveCard<'a>> + 'a {
    let is_agenda = move |card: &CardId| registry.get(card).is_some_and(|definition| definition.card_type == CardType::Agenda);
    let identity =
        state.corp.identity.iter().map(|card| ActiveCard { side: Side::Corp, card, install: None, server: None, place: Place::Identity });
    let scored = state.corp.scored_agendas.iter().map(|scored| ActiveCard {
        side: Side::Corp,
        card: &scored.card,
        install: Some(scored.install_id),
        server: None,
        place: Place::ScoreArea,
    });
    let installed = state.corp.installed.iter().filter(move |installed| installed.rezzed && !is_agenda(&installed.card)).map(|installed| ActiveCard {
        side: Side::Corp,
        card: &installed.card,
        install: Some(installed.install_id),
        server: Some(installed.server),
        place: Place::Installed,
    });
    identity.chain(scored).chain(installed)
}

pub(crate) fn runner(state: &GameState) -> impl Iterator<Item = ActiveCard<'_>> + '_ {
    let identity =
        state.runner.identity.iter().map(|card| ActiveCard { side: Side::Runner, card, install: None, server: None, place: Place::Identity });
    let rig = state.runner.rig.iter().map(|installed| ActiveCard {
        side: Side::Runner,
        card: &installed.card,
        install: Some(installed.install_id),
        server: None,
        place: Place::Installed,
    });
    identity.chain(rig)
}
