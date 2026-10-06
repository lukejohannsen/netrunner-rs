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
use crate::dsl::{CardId, CardType, ContinuousKind};
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
    /// used during that run and never after. And an operation that is not
    /// trashed after it resolves (CR 8.6.6c: a lockdown,
    /// `CorpState::play_area`), active there until it is.
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
    // An identity that has lost its abilities (Direct Access's) has
    // nothing an active card is asked for — no trigger, no standing effect,
    // no paid ability — so it is not asked at all. Its handle is the one
    // `ActivateAbility` names it by.
    let identity = state
        .corp
        .identity
        .iter()
        .filter(|_| !crate::rules::lingering::loses_abilities(state, crate::rules::state::InstallId::CORP_IDENTITY))
        .map(|card| ActiveCard { side: Side::Corp, card, install: None, server: None, place: Place::Identity });
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
    // A lockdown in the play area (CR 1.8.3a), after the table: it was
    // played after anything installed there.
    let in_play = state.corp.play_area.iter().map(|played| ActiveCard {
        side: Side::Corp,
        card: &played.card,
        install: Some(played.handle),
        server: None,
        place: Place::PlayArea,
    });
    identity.chain(scored).chain(installed).chain(in_play)
}

pub(crate) fn runner<'a>(state: &'a GameState, registry: &'a CardRegistry) -> impl Iterator<Item = ActiveCard<'a>> + 'a {
    let identity = state
        .runner
        .identity
        .iter()
        .filter(|_| !crate::rules::lingering::loses_abilities(state, crate::rules::state::InstallId::RUNNER_IDENTITY))
        .map(|card| ActiveCard { side: Side::Runner, card, install: None, server: None, place: Place::Identity });
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

/// Whether the install `install` has lost all its abilities (CR 9.1.9a):
/// a card hosted on it says so for as long as it stays there (Hush's
/// `ContinuousKind::LosesAbilities`, about its `Host`), or a lingering
/// effect does (Klevetnik's, `lingering::loses_abilities`). **The one
/// question**, put by everything that reads what a card can do: who hears
/// an event (`listeners`), whose standing effects apply (`continuous`), who
/// may use a paid ability or an interrupt, and whose hosted credits pay.
/// A lost ability "is completely ignored", so each of them skips the card
/// rather than ask what it says.
///
/// A printed subroutine is not among what is lost — the one card in the
/// pool that takes a piece of ice's abilities keeps them — so a run's ice
/// is never asked this about its subroutines.
///
/// Read off the rig and the lingering list directly, never through the
/// continuous scan, which asks this of every source: a hosted card's effect
/// does not depend on its host's (CR 9.12.1e), so nothing here can loop.
pub(crate) fn lost_abilities(state: &GameState, registry: &CardRegistry, install: InstallId) -> bool {
    crate::rules::lingering::loses_abilities(state, install) || hosted_says(state, registry, install, ContinuousKind::LosesAbilities)
}

/// Whether the install `install` may have an ability another card gives it
/// (`TriggeredEffect::granted`, ZATO City Grid's): not if a card hosted on
/// it says it cannot gain abilities (Hush's `CannotGainAbilities`), and not
/// if it has lost them — a granted ability is lost with the rest.
pub(crate) fn may_have_granted(state: &GameState, registry: &CardRegistry, install: InstallId) -> bool {
    !hosted_says(state, registry, install, ContinuousKind::CannotGainAbilities) && !lost_abilities(state, registry, install)
}

/// Every install that has lost its abilities right now — what a scan that
/// asks [`lost_abilities`] of many cards reads instead, once. Empty, and
/// unallocated, in every game where no card takes abilities away.
pub(crate) fn installs_without_abilities(state: &GameState, registry: &CardRegistry) -> Vec<InstallId> {
    let mut lost: Vec<InstallId> = state
        .lingering
        .iter()
        .filter(|effect| effect.what == crate::rules::lingering::Lingering::LosesAbilities && effect.holds(state))
        .flat_map(|effect| match effect.on {
            crate::rules::lingering::On::Install(install) => vec![install],
            crate::rules::lingering::On::RootOfAttackedServer => crate::rules::lingering::in_attacked_root(state).collect(),
            _ => Vec::new(),
        })
        .collect();
    for card in state.runner.rig.iter().filter(|card| card.hosted_on_ice.is_some() || card.hosted_on_rig_card.is_some()) {
        if host_says(registry, &card.card, ContinuousKind::LosesAbilities)
            && let Some(host) = card.hosted_on_ice.or(card.hosted_on_rig_card)
        {
            lost.push(host);
        }
    }
    lost
}

/// Whether a card hosted on `install` declares `kind` about its host.
fn hosted_says(state: &GameState, registry: &CardRegistry, install: InstallId, kind: ContinuousKind) -> bool {
    state
        .runner
        .rig
        .iter()
        .filter(|card| card.hosted_on_ice == Some(install) || card.hosted_on_rig_card == Some(install))
        .any(|card| host_says(registry, &card.card, kind.clone()))
}

fn host_says(registry: &CardRegistry, card: &CardId, kind: ContinuousKind) -> bool {
    registry.get(card).is_some_and(|definition| definition.continuous.iter().any(|effect| effect.kind == kind && effect.applies_to == crate::dsl::Scope::Host))
}
