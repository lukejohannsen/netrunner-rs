//! Who hears an event: one rule, read off the cards.
//!
//! > **The subject of an event always hears it, wherever it is; every other
//! > card must be active** — a rezzed Corp install, a card in the rig, an
//! > identity, a scored agenda — **and the active player's cards come
//! > first.**
//!
//! `dispatcher::dispatch_event` used to decide this per event, by hand: a
//! `match` naming, for each `GameEvent`, the cards to ask. Every audience in
//! it was right for the cards that existed, and each was widened in Rust
//! when a card needed more ("the identity and, since Cacophony, the rig
//! too"). So a card whose text the DSL could already say — an asset that
//! reacts to an operation being played — could still need an engine edit
//! with no new mechanic in it, which is the "never hardcode card rules in
//! Rust" rule bent one audience at a time (ROADMAP Rules Audit, jinteki
//! comparison item 1).
//!
//! Here a card *listens*. `moments` says what an event is — which
//! `Trigger`s it is an occurrence of, what it is about, whose it is — and
//! is exhaustive over `GameEvent`, so a new event does not compile until
//! someone says what hears it. `plan_for` then asks every listener, and
//! what decides is on the card: its `TriggeredEffect::trigger`, the
//! `Subject` it names, and `Trigger::hears`.
//!
//! The special cases the old table carried fall out of the first clause
//! rather than being listed: an unrezzed trap hears its own access, an
//! event hears its own `OnPlay` from the play area, a stolen or forfeited
//! agenda hears its own leaving. The one audience that is still named is
//! `CompletedRun::persistent_trashed_upgrades`, because "this ability still
//! applies for the remainder of this run" (AMAZE Amusements) is a card
//! saying it outlives being active.

use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardSubtype, CardType, Hears, Subject, Trigger, TriggeredEffect};
use crate::rules::event::GameEvent;
use crate::rules::run::ServerId;
use crate::rules::state::{DeferredTrigger, GamePhase, GameState, Heard, InstallId, InstallSlot, Side};

/// What an event is about — the thing a `Subject::This` is compared with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum About {
    Nothing,
    /// A card, pinned to the copy the event happened to where the event or
    /// the state can say which. `None` is a card that has no handle any
    /// more (played, stolen, forfeited, trashed): only it can be "this".
    Card { card: CardId, install: Option<InstallId> },
    Server(ServerId),
}

/// One thing an event is an occurrence of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Moment {
    pub trigger: Trigger,
    pub about: About,
    /// Whose moment it is, for a trigger phrased about its controller
    /// (`Hears::OwnSide`): whose turn began, who installed, who dealt the
    /// damage.
    pub of: Option<Side>,
}

/// A card that may hear a moment.
#[derive(Debug, Clone)]
struct Listener {
    side: Side,
    card: CardId,
    install: Option<InstallId>,
    /// The server a Corp install is in or protecting, for a "this server".
    server: Option<ServerId>,
    /// Active cards hear whatever they listen for. The others — the subject
    /// itself when it is not active, a persistent upgrade trashed during
    /// the run — hear only what is about *them*.
    active: bool,
}

/// What `event` is an occurrence of. Most events are an occurrence of
/// nothing: they record a state change no card in the pool prints a trigger
/// for. They are listed by name, not caught by `_`, so that adding a
/// `GameEvent` is a decision made here rather than a silence.
pub(crate) fn moments(state: &GameState, registry: &CardRegistry, event: &GameEvent) -> Vec<Moment> {
    let has_subtype = |card: &CardId, subtype: CardSubtype| registry.get(card).is_some_and(|c| c.subtypes.contains(&subtype));
    let card = |card: &CardId, install: Option<InstallId>| About::Card { card: card.clone(), install };
    let moment = |trigger, about: &About, of| Moment { trigger, about: about.clone(), of };
    match event {
        GameEvent::EventPlayed { side, card: played } => vec![moment(Trigger::OnPlay, &card(played, None), Some(*side))],

        GameEvent::OperationPlayed { side, card: played, .. } => {
            let about = card(played, None);
            let mut heard = vec![moment(Trigger::OnPlay, &about, Some(*side))];
            if has_subtype(played, CardSubtype::Transaction) {
                heard.push(moment(Trigger::OnTransactionPlayed, &about, Some(*side)));
            }
            heard.push(moment(Trigger::OnOperationPlayed, &about, Some(*side)));
            heard
        }

        GameEvent::ProgramInstalled { side, card: installed, .. } => {
            let about = card(installed, newest_rig_install(state, installed));
            let mut heard = vec![moment(Trigger::OnInstall, &about, Some(*side))];
            if has_subtype(installed, CardSubtype::Virus) {
                heard.push(moment(Trigger::OnVirusInstalled, &about, Some(*side)));
            }
            heard.push(moment(Trigger::OnCardInstalled, &about, Some(*side)));
            heard
        }
        GameEvent::HardwareInstalled { side, card: installed, .. } | GameEvent::ResourceInstalled { side, card: installed, .. } => {
            let about = card(installed, newest_rig_install(state, installed));
            vec![moment(Trigger::OnInstall, &about, Some(*side)), moment(Trigger::OnCardInstalled, &about, Some(*side))]
        }
        // The Corp's installs. `card` is `None` only in a masked copy of the
        // event, which the engine never dispatches.
        GameEvent::CardInstalled { side, install, card: installed, .. } => match installed {
            Some(installed) => vec![moment(Trigger::OnInstall, &card(installed, Some(*install)), Some(*side))],
            None => Vec::new(),
        },

        GameEvent::CardAccessed { card: accessed, server, install } => {
            // The access pinned the copy; the lookup is for an event that
            // carries none. A card accessed out of HQ or R&D has neither.
            let install = install.or_else(|| root_install_of(state, accessed, *server));
            vec![moment(Trigger::OnAccessed, &card(accessed, install), Some(Side::Runner))]
        }
        GameEvent::CardTrashedFromAccess { card: trashed, .. } => {
            vec![moment(Trigger::OnTrashedFromAccess, &card(trashed, None), Some(Side::Runner))]
        }

        // The agenda reacts from the score area, under the handle it kept
        // there — a "place a counter on this agenda" has nothing to place
        // onto otherwise. Scoring pushes, so the last match is this one.
        GameEvent::AgendaScored { card: scored, .. } => {
            let install = state.corp.scored_agendas.iter().rev().find(|entry| &entry.card == scored).map(|entry| entry.install_id);
            vec![moment(Trigger::OnAgendaScored, &card(scored, install), Some(Side::Corp))]
        }
        GameEvent::AgendaStolen { card: stolen, .. } => vec![moment(Trigger::OnAgendaStolen, &card(stolen, None), Some(Side::Runner))],
        GameEvent::AgendaForfeited { card: forfeited } => vec![moment(Trigger::OnForfeit, &card(forfeited, None), None)],

        GameEvent::IceRezzed { card: rezzed, install, .. } => vec![moment(Trigger::OnRez, &card(rezzed, Some(*install)), Some(Side::Corp))],
        GameEvent::CardAdvanced { install, card: advanced, .. } => match advanced {
            Some(advanced) => vec![moment(Trigger::OnAdvance, &card(advanced, Some(*install)), Some(Side::Corp))],
            None => Vec::new(),
        },
        // **Deliberately an occurrence of nothing.** CR 1.18.2: placing an
        // advancement counter is not advancing, so `Trigger::OnAdvance` does
        // not hear it — Weyland Consortium: Built to Last is not paid when
        // Key Performance Indicators or Syailendra places one. Kept apart
        // from the long list below so the next "on advance" card finds the
        // rule here instead of deriving it again. A card that triggers on a
        // *placement* would need a `Trigger` of its own; none prints one.
        GameEvent::AdvancementCountersPlaced { .. } => Vec::new(),
        GameEvent::AbilityGainedCredits { side, card: source } => {
            vec![moment(Trigger::OnAbilityGainedCredits, &card(source, None), Some(*side))]
        }

        GameEvent::TurnStarted { side, .. } => vec![moment(Trigger::OnTurnStart, &About::Nothing, Some(*side))],
        GameEvent::ActionPhaseEnded { side } => vec![moment(Trigger::OnActionPhaseEnd, &About::Nothing, Some(*side))],
        GameEvent::DiscardPhaseEnded { side } => vec![moment(Trigger::OnDiscardPhaseEnd, &About::Nothing, Some(*side))],
        GameEvent::BasicDrawActionTaken { side } => vec![moment(Trigger::OnBasicDrawAction, &About::Nothing, Some(*side))],

        GameEvent::RunInitiated { server } => vec![moment(Trigger::OnRunStart, &About::Server(*server), Some(Side::Runner))],
        GameEvent::IceApproached { server, .. } => vec![moment(Trigger::OnIceApproached, &About::Server(*server), Some(Side::Runner))],
        GameEvent::ServerApproached { server } => vec![moment(Trigger::OnApproachServer, &About::Server(*server), Some(Side::Runner))],
        GameEvent::IceEncountered { card_id, .. } => {
            vec![moment(Trigger::OnEncounter, &card(card_id, encountered_install(state)), Some(Side::Runner))]
        }
        // One event and three filters on it — the `Trigger` vocabulary grew
        // by audience here, which is what `TriggeredEffect` taking a filter
        // of its own is owed for.
        GameEvent::RunSucceeded { server } => {
            let about = About::Server(*server);
            let mut heard = vec![moment(Trigger::OnSuccessfulRun, &about, Some(Side::Runner))];
            if *server == ServerId::Hq {
                heard.push(moment(Trigger::OnSuccessfulRunOnHq, &about, Some(Side::Runner)));
            }
            if *server == ServerId::RnD {
                heard.push(moment(Trigger::OnSuccessfulRunOnRnD, &about, Some(Side::Runner)));
            }
            if matches!(server, ServerId::Hq | ServerId::RnD | ServerId::Archives) {
                heard.push(moment(Trigger::OnSuccessfulRunOnCentralServer, &about, Some(Side::Runner)));
            }
            heard
        }
        // Only the ordinary conclusions: a flatline or an agenda win
        // mid-access ends the game, and nothing resolves after that.
        GameEvent::RunCompleted { server } | GameEvent::RunJackedOut { server } | GameEvent::RunEndedByEffect { server } => {
            vec![moment(Trigger::OnRunEnded, &About::Server(*server), Some(Side::Runner))]
        }

        // Only the Corp deals damage in this pool, and "whenever you do
        // damage" is the dealer's.
        GameEvent::DamageTaken { .. } => vec![moment(Trigger::OnDamageDealt, &About::Nothing, Some(Side::Corp))],
        // One occurrence per batch, not per card: "trash 1 **or more**".
        GameEvent::CardsTrashedFromHq { .. } => vec![moment(Trigger::OnCardsTrashedFromHq, &About::Nothing, Some(Side::Corp))],
        // Only the Runner's tags are anyone's trigger, and removing none is
        // not removing one.
        GameEvent::TagsRemoved { side: Side::Runner, amount: 0 } => Vec::new(),
        GameEvent::TagRemoved { side: Side::Runner } | GameEvent::TagsRemoved { side: Side::Runner, .. } | GameEvent::TagsCleared { side: Side::Runner } => {
            vec![moment(Trigger::OnTagRemoved, &About::Nothing, Some(Side::Runner))]
        }
        GameEvent::TagsGiven { side: Side::Runner, .. } => vec![moment(Trigger::OnTagsGiven, &About::Nothing, Some(Side::Runner))],
        GameEvent::TagRemoved { side: Side::Corp }
        | GameEvent::TagsRemoved { side: Side::Corp, .. }
        | GameEvent::TagsCleared { side: Side::Corp }
        | GameEvent::TagsGiven { side: Side::Corp, .. } => Vec::new(),

        GameEvent::DamageAboutToResolve { .. } => vec![moment(Trigger::OnDamageAboutToResolve, &About::Nothing, None)],
        GameEvent::TrashAboutToResolve { .. } => vec![moment(Trigger::OnTrashAboutToResolve, &About::Nothing, None)],

        GameEvent::ClickSpent { .. }
        | GameEvent::CreditsGained { .. }
        | GameEvent::CardDrawn { .. }
        | GameEvent::SubroutineBroken { .. }
        | GameEvent::SubroutineFired { .. }
        | GameEvent::IceStrengthModified { .. }
        | GameEvent::IcePassed { .. }
        | GameEvent::IceBypassed { .. }
        | GameEvent::CardDerezzed { .. }
        | GameEvent::IceSwapped { .. }
        | GameEvent::CardMoved { .. }
        | GameEvent::TurnEnded { .. }
        | GameEvent::DiscardPending { .. }
        | GameEvent::CardDiscarded { .. }
        | GameEvent::CardAddedToBottomOfStack { .. }
        | GameEvent::CardHosted { .. }
        | GameEvent::IdentityFlipped { .. }
        | GameEvent::RunEndPrevented { .. }
        | GameEvent::RunRedirected { .. }
        | GameEvent::RunnerFlatlined
        | GameEvent::CreditsSpent { .. }
        | GameEvent::CardTrashed { .. }
        | GameEvent::CardRemovedFromGame { .. }
        | GameEvent::GameOver { .. }
        | GameEvent::AbilityActivated { .. }
        | GameEvent::AccessPassed { .. }
        | GameEvent::PaidAbilityWindowOpened { .. }
        | GameEvent::PriorityPassed { .. }
        | GameEvent::PaidAbilityWindowClosed
        | GameEvent::StrengthBoosted { .. }
        | GameEvent::TraceInitiated { .. }
        | GameEvent::TraceCorpBidSubmitted { .. }
        | GameEvent::TraceRunnerBidSubmitted { .. }
        | GameEvent::TraceAvoided { .. }
        | GameEvent::TraceSuccessful { .. }
        | GameEvent::TriggerOrderPending { .. }
        | GameEvent::TriggerOrderChosen { .. }
        | GameEvent::TriggerFired { .. }
        | GameEvent::VirusCountersPurged { .. }
        | GameEvent::BadPublicityCreditsSpent { .. }
        | GameEvent::BonusRunCreditsSpent { .. }
        | GameEvent::CardsSelected { .. }
        | GameEvent::PendingCardSelectionOffered { .. }
        | GameEvent::MemoryLimitExceeded { .. }
        | GameEvent::PendingServerChoiceOffered { .. }
        | GameEvent::BadPublicityGiven { .. }
        | GameEvent::BadPublicityRemoved { .. }
        | GameEvent::HandKept { .. }
        | GameEvent::MulliganTaken { .. }
        | GameEvent::AdditionalAccessGranted { .. }
        | GameEvent::AccessReplacementSet { .. }
        | GameEvent::AccessReplaced { .. }
        | GameEvent::CreditsLost { .. }
        | GameEvent::ClicksLost { .. }
        | GameEvent::ClicksGained { .. }
        | GameEvent::RecurringCreditsSpent { .. }
        | GameEvent::DamagePrevented { .. }
        | GameEvent::TrashPrevented { .. }
        | GameEvent::CountersAdded { .. }
        | GameEvent::CountersRemoved { .. }
        | GameEvent::MaxHandSizeGained { .. }
        | GameEvent::PendingChoicePresented { .. }
        | GameEvent::PendingChoiceResolved { .. }
        | GameEvent::PendingPaidChoiceOffered { .. }
        | GameEvent::PendingPaidChoiceAccepted { .. }
        | GameEvent::PendingPaidChoiceDeclined { .. } => Vec::new(),
    }
}

/// Every trigger `event` fires, in the order they resolve: the active
/// player's cards first, and on each side the subject, the identity, the
/// score area, the installed cards in install order.
///
/// An entry is planned only where a `TriggeredEffect` on the card hears the
/// moment, so the plan is what reacts, not who was asked — which is also
/// what `ChooseTriggerOrder` should be counting.
pub(crate) fn plan_for(state: &GameState, registry: &CardRegistry, event: &GameEvent) -> Vec<(Side, DeferredTrigger)> {
    let moments = moments(state, registry, event);
    if moments.is_empty() {
        return Vec::new();
    }
    let mut plan = Vec::new();
    for listener in listeners(state, registry, &moments) {
        let Some(definition) = registry.get(&listener.card) else { continue };
        for moment in &moments {
            if !definition.triggers.iter().any(|triggered| hears(triggered, &listener, moment)) {
                continue;
            }
            // "Whenever you install a virus program, you may place 1 virus
            // counter on **it**" (Cookbook): an installed card's reaction
            // acts on the virus, not on itself. The identity's does not
            // (Noise mills the Corp). The one place who reacts and what the
            // effect acts on differ, carried over as it was; a
            // `CardTarget` for "the card that triggered this" is what would
            // retire it.
            let acts_on_the_subject = moment.trigger == Trigger::OnVirusInstalled && listener.install.is_some();
            let (target, target_install) = match (&moment.about, acts_on_the_subject) {
                (About::Card { card, install }, true) => (Some(card.clone()), *install),
                _ => (None, None),
            };
            let heard = match (listener.active, is_this(&listener, moment)) {
                (true, true) => Heard::AsBoth,
                (true, false) => Heard::AsBystander,
                (false, _) => Heard::AsSubject,
            };
            plan.push((
                listener.side,
                DeferredTrigger {
                    card: listener.card.clone(),
                    install: listener.install,
                    trigger: moment.trigger,
                    target,
                    target_install,
                    event: Some(event.clone()),
                    continuation: None,
                    heard,
                },
            ));
        }
    }
    plan
}

/// Whether `moment` is about `listener` itself — or, for a moment about a
/// server, about the server it is in.
fn is_this(listener: &Listener, moment: &Moment) -> bool {
    match &moment.about {
        About::Nothing => false,
        About::Card { install: Some(install), .. } => listener.install == Some(*install),
        // A card with no handle left is "this" only to itself, and it is
        // listening only because it is the subject.
        About::Card { install: None, card } => !listener.active && &listener.card == card,
        About::Server(server) => listener.server == Some(*server),
    }
}

/// Whether one `TriggeredEffect` on `listener` hears `moment`.
fn hears(triggered: &TriggeredEffect, listener: &Listener, moment: &Moment) -> bool {
    if triggered.trigger != moment.trigger {
        return false;
    }
    if triggered.trigger.hears() == Hears::OwnSide && moment.of.is_some_and(|side| side != listener.side) {
        return false;
    }
    let is_this = is_this(listener, moment);
    match triggered.subject {
        Some(Subject::This) => is_this,
        Some(Subject::Any) => listener.active,
        // A moment about nothing, where there is no "this" to name — or a
        // Rust fixture that named none, which hears what either reading
        // would. `CardDefinition::validate` refuses a card file that tries.
        None => listener.active || is_this,
    }
}

fn listeners(state: &GameState, registry: &CardRegistry, moments: &[Moment]) -> Vec<Listener> {
    let mut corp: Vec<Listener> = Vec::new();
    let mut runner: Vec<Listener> = Vec::new();

    corp.extend(state.corp.identity.iter().map(|id| Listener { side: Side::Corp, card: id.clone(), install: None, server: None, active: true }));
    // The score area before the table: the order `DiscardPhaseEnded` always
    // asked in, and the only one the old audiences agreed on.
    corp.extend(state.corp.scored_agendas.iter().map(|scored| Listener {
        side: Side::Corp,
        card: scored.card.clone(),
        install: Some(scored.install_id),
        server: None,
        active: true,
    }));
    // Faceup is not active for an agenda: BANGUN installs agendas faceup,
    // and an agenda's abilities are live in a score area, not on the table.
    // The old audiences asked every `rezzed` install, so a faceup Off the
    // Books spent its counters from a remote.
    let is_agenda = |card: &CardId| registry.get(card).is_some_and(|definition| definition.card_type == CardType::Agenda);
    corp.extend(state.corp.installed.iter().filter(|installed| installed.rezzed && !is_agenda(&installed.card)).map(|installed| Listener {
        side: Side::Corp,
        card: installed.card.clone(),
        install: Some(installed.install_id),
        server: Some(installed.server),
        active: true,
    }));
    runner.extend(state.runner.identity.iter().map(|id| Listener { side: Side::Runner, card: id.clone(), install: None, server: None, active: true }));
    runner.extend(state.runner.rig.iter().map(|installed| Listener {
        side: Side::Runner,
        card: installed.card.clone(),
        install: Some(installed.install_id),
        server: None,
        active: true,
    }));

    // The subject leads its side: "when you score this agenda" before the
    // identity's "whenever you score an agenda". It is already listening
    // if it is active; where it is not — face down, in a hand or a deck,
    // gone from the table — it listens for what is about itself.
    for moment in moments {
        let About::Card { card, install } = &moment.about else { continue };
        let Some(side) = registry.get(card).map(|definition| definition.side) else { continue };
        let group = match side {
            Side::Corp => &mut corp,
            Side::Runner => &mut runner,
        };
        if group.first().is_some_and(|first| &first.card == card && first.install == *install) {
            continue;
        }
        let subject = match install.and_then(|install| group.iter().position(|l| l.install == Some(install))) {
            Some(position) => group.remove(position),
            None => {
                let server = install.and_then(|install| state.corp.installed.iter().find(|c| c.install_id == install)).map(|c| c.server);
                Listener { side, card: card.clone(), install: *install, server, active: false }
            }
        };
        group.insert(0, subject);
    }
    // "This ability still applies for the remainder of this run": a
    // persistent upgrade the Runner trashed during the run still hears the
    // run on its server end. Read off the snapshot, since `active_run` is
    // cleared by the time a run's end is dispatched.
    if moments.iter().any(|moment| moment.trigger == Trigger::OnRunEnded)
        && let Some(completed) = &state.last_completed_run
    {
        corp.extend(completed.persistent_trashed_upgrades.iter().map(|card| Listener {
            side: Side::Corp,
            card: card.clone(),
            install: None,
            server: Some(completed.server),
            active: false,
        }));
    }

    match active_side(state) {
        Side::Corp => corp.into_iter().chain(runner).collect(),
        Side::Runner => runner.into_iter().chain(corp).collect(),
    }
}

/// Whose turn it is — the active player, whose triggers resolve first.
///
/// Read off `phase`, which is total, rather than asking who may act right
/// now: the Corp can hold priority during the Runner's turn, and the Runner
/// is still the active player. `GameOver(side)` names the winner; harmless,
/// since nothing fires once the game has ended.
pub(crate) fn active_side(state: &GameState) -> Side {
    match state.phase {
        GamePhase::Mulligan(side)
        | GamePhase::StartOfTurn(side)
        | GamePhase::Action(side)
        | GamePhase::Discard { side, .. }
        | GamePhase::GameOver(side) => side,
    }
}

/// The most recently installed rig copy of `card` — the one a
/// `*Installed { card }` event is about, since install handles are
/// monotonic and the install handlers push last.
fn newest_rig_install(state: &GameState, card: &CardId) -> Option<InstallId> {
    state.runner.rig.iter().rev().find(|c| &c.card == card).map(|c| c.install_id)
}

/// The root install of `card` on `server`, for an access that pinned none.
fn root_install_of(state: &GameState, card: &CardId, server: ServerId) -> Option<InstallId> {
    state.corp.installed.iter().find(|c| &c.card == card && c.server == server && c.slot == InstallSlot::Root).map(|c| c.install_id)
}

/// The install of the ICE being encountered.
fn encountered_install(state: &GameState) -> Option<InstallId> {
    state.active_run.as_ref().and_then(|run| run.ice.get(run.position)).map(|ice| ice.install_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardDefinition, Effect};
    use crate::rules::state::{InstalledCard, InstalledRunnerCard, ScoredAgenda};

    fn listens(id: &str, side: Side, card_type: CardType, trigger: Trigger, subject: Option<Subject>) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type,
            triggers: vec![TriggeredEffect { subject, text: None, trigger, effects: vec![Effect::GainCredits(side, 1)], requirement: None }],
            ..Default::default()
        }
    }

    fn registry(cards: Vec<CardDefinition>) -> CardRegistry {
        let mut registry = CardRegistry::new();
        cards.into_iter().for_each(|card| registry.insert(card));
        registry
    }

    fn on_the_table(id: &str, install: u32, server: ServerId, rezzed: bool) -> InstalledCard {
        InstalledCard { install_id: InstallId(install), card: CardId(id.to_string()), server, rezzed, ..Default::default() }
    }

    fn in_the_rig(id: &str, install: u32) -> InstalledRunnerCard {
        InstalledRunnerCard { install_id: InstallId(install), card: CardId(id.to_string()), ..Default::default() }
    }

    fn who(plan: &[(Side, DeferredTrigger)]) -> Vec<(&str, Heard)> {
        plan.iter().map(|(_, due)| (due.card.0.as_str(), due.heard)).collect()
    }

    /// The card the comparison with jinteki said the engine could not
    /// deliver: text the DSL could always say, and an audience
    /// (`OperationPlayed` went to the Corp's identity alone) that would
    /// have needed a Rust edit to reach it.
    #[test]
    fn an_asset_that_reacts_to_an_operation_being_played_is_asked() {
        let registry = registry(vec![
            listens("hedge_fund", Side::Corp, CardType::Operation, Trigger::OnPlay, Some(Subject::This)),
            listens("press_office", Side::Corp, CardType::Asset, Trigger::OnOperationPlayed, Some(Subject::Any)),
        ]);
        let mut state = GameState { phase: GamePhase::Action(Side::Corp), ..Default::default() };
        state.corp.installed = vec![on_the_table("press_office", 1, ServerId::Remote(0), true)];

        let played = GameEvent::OperationPlayed { side: Side::Corp, card: CardId("hedge_fund".to_string()), from_archives: false };
        assert_eq!(who(&plan_for(&state, &registry, &played)), vec![("hedge_fund", Heard::AsSubject), ("press_office", Heard::AsBystander)]);

        // Face down it is not active, and the operation is not about it.
        state.corp.installed[0].rezzed = false;
        assert_eq!(who(&plan_for(&state, &registry, &played)), vec![("hedge_fund", Heard::AsSubject)]);
    }

    /// Live on `main` until the listener scan ran beside the old audiences:
    /// BANGUN installs agendas faceup, the rezzed table was asked
    /// `OnAgendaScored`, and Orbital Superiority did its 4 meat damage from a
    /// remote when a different agenda was scored — six times in the 256-seed
    /// view sweep.
    #[test]
    fn an_agenda_installed_faceup_does_not_hear_another_agenda_being_scored() {
        let registry = registry(vec![
            listens("offworld_office", Side::Corp, CardType::Agenda, Trigger::OnAgendaScored, Some(Subject::This)),
            listens("orbital_superiority", Side::Corp, CardType::Agenda, Trigger::OnAgendaScored, Some(Subject::This)),
        ]);
        let mut state = GameState { phase: GamePhase::Action(Side::Corp), ..Default::default() };
        state.corp.installed = vec![on_the_table("orbital_superiority", 1, ServerId::Remote(0), true)];
        state.corp.scored_agendas = vec![ScoredAgenda { install_id: InstallId(2), ..ScoredAgenda::plain(CardId("offworld_office".to_string())) }];

        let scored = GameEvent::AgendaScored { card: CardId("offworld_office".to_string()), agenda_points: 2, server: ServerId::Remote(1) };
        assert_eq!(who(&plan_for(&state, &registry, &scored)), vec![("offworld_office", Heard::AsBoth)]);
    }

    /// And faceup is not active for an agenda whatever it listens for: an
    /// agenda's abilities are live in a score area, not on the table.
    #[test]
    fn an_agenda_is_active_in_the_score_area_and_not_on_the_table() {
        let registry = registry(vec![listens("off_the_books", Side::Corp, CardType::Agenda, Trigger::OnDiscardPhaseEnd, None)]);
        let mut state = GameState { phase: GamePhase::Action(Side::Corp), ..Default::default() };
        state.corp.installed = vec![on_the_table("off_the_books", 1, ServerId::Remote(0), true)];
        let ended = GameEvent::DiscardPhaseEnded { side: Side::Corp };
        assert!(plan_for(&state, &registry, &ended).is_empty());

        state.corp.scored_agendas = vec![ScoredAgenda { install_id: InstallId(2), ..ScoredAgenda::plain(CardId("off_the_books".to_string())) }];
        assert_eq!(plan_for(&state, &registry, &ended).iter().map(|(_, due)| due.install).collect::<Vec<_>>(), vec![Some(InstallId(2))]);
    }

    /// The subject hears what is about it from wherever it is; nothing else
    /// inactive hears anything.
    #[test]
    fn a_face_down_card_hears_its_own_access_and_no_one_elses() {
        let registry = registry(vec![listens("snare", Side::Corp, CardType::Asset, Trigger::OnAccessed, Some(Subject::This))]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.corp.installed = vec![on_the_table("snare", 1, ServerId::Remote(0), false), on_the_table("snare", 2, ServerId::Remote(1), false)];

        let accessed = GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Remote(1), install: Some(InstallId(2)) };
        let plan = plan_for(&state, &registry, &accessed);
        assert_eq!(plan.iter().map(|(_, due)| (due.install, due.heard)).collect::<Vec<_>>(), vec![(Some(InstallId(2)), Heard::AsSubject)]);

        // Out of HQ there is no install at all, and it still hears it.
        let from_hq = GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Hq, install: None };
        assert_eq!(plan_for(&state, &registry, &from_hq).iter().map(|(_, due)| (due.install, due.heard)).collect::<Vec<_>>(), vec![(None, Heard::AsSubject)]);
    }

    #[test]
    fn this_server_is_the_server_the_card_is_in() {
        let registry = registry(vec![listens("anoetic_void", Side::Corp, CardType::Upgrade, Trigger::OnApproachServer, Some(Subject::This))]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.corp.installed = vec![on_the_table("anoetic_void", 1, ServerId::Remote(0), true), on_the_table("anoetic_void", 2, ServerId::Hq, true)];

        let plan = plan_for(&state, &registry, &GameEvent::ServerApproached { server: ServerId::Hq });
        assert_eq!(plan.iter().map(|(_, due)| due.install).collect::<Vec<_>>(), vec![Some(InstallId(2))]);
    }

    /// "When **your** turn begins" is the side's whose turn it is.
    #[test]
    fn a_trigger_phrased_about_its_controller_hears_only_its_own_sides_moment() {
        let registry = registry(vec![
            listens("pad_campaign", Side::Corp, CardType::Asset, Trigger::OnTurnStart, None),
            listens("smartware", Side::Runner, CardType::Resource, Trigger::OnTurnStart, None),
        ]);
        let mut state = GameState { phase: GamePhase::StartOfTurn(Side::Corp), ..Default::default() };
        state.corp.installed = vec![on_the_table("pad_campaign", 1, ServerId::Remote(0), true)];
        state.runner.rig = vec![in_the_rig("smartware", 2)];

        let began = |side| GameEvent::TurnStarted { side, clicks: 3 };
        assert_eq!(who(&plan_for(&state, &registry, &began(Side::Corp))), vec![("pad_campaign", Heard::AsBystander)]);
        assert_eq!(who(&plan_for(&state, &registry, &began(Side::Runner))), vec![("smartware", Heard::AsBystander)]);
    }

    /// The active player's triggers first, and on a side the subject, then
    /// install order — what `order_active_first` pinned for the two events
    /// that used it, now for every event. It is also what moved: a rezzed
    /// card's own reaction used to resolve before the Runner's on the
    /// Runner's turn.
    #[test]
    fn the_active_players_cards_come_first_and_a_side_keeps_install_order() {
        let registry = registry(vec![
            listens("ping", Side::Corp, CardType::Asset, Trigger::OnRez, Some(Subject::This)),
            listens("watcher", Side::Corp, CardType::Asset, Trigger::OnRez, Some(Subject::Any)),
            listens("baz", Side::Runner, CardType::Resource, Trigger::OnRez, Some(Subject::Any)),
            listens("baz_two", Side::Runner, CardType::Resource, Trigger::OnRez, Some(Subject::Any)),
        ]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.corp.installed = vec![on_the_table("watcher", 1, ServerId::Remote(0), true), on_the_table("ping", 2, ServerId::Hq, true)];
        state.runner.rig = vec![in_the_rig("baz", 3), in_the_rig("baz_two", 4)];

        let rezzed = GameEvent::IceRezzed { card: CardId("ping".to_string()), server: ServerId::Hq, install: InstallId(2) };
        let order = |state: &GameState| plan_for(state, &registry, &rezzed).into_iter().map(|(side, due)| (side, due.card.0)).collect::<Vec<_>>();
        let named = |side, id: &str| (side, id.to_string());
        assert_eq!(
            order(&state),
            vec![named(Side::Runner, "baz"), named(Side::Runner, "baz_two"), named(Side::Corp, "ping"), named(Side::Corp, "watcher")]
        );
        state.phase = GamePhase::Action(Side::Corp);
        assert_eq!(
            order(&state),
            vec![named(Side::Corp, "ping"), named(Side::Corp, "watcher"), named(Side::Runner, "baz"), named(Side::Runner, "baz_two")]
        );
    }

    /// CR 1.18.2, kept where the next "on advance" card will look.
    #[test]
    fn placing_an_advancement_counter_is_an_occurrence_of_nothing() {
        let registry = registry(vec![listens("built_to_last", Side::Corp, CardType::Identity, Trigger::OnAdvance, Some(Subject::Any))]);
        let mut state = GameState { phase: GamePhase::Action(Side::Corp), ..Default::default() };
        state.corp.identity = Some(CardId("built_to_last".to_string()));
        let card = Some(CardId("ice_wall".to_string()));

        let advanced = GameEvent::CardAdvanced { install: InstallId(1), card: card.clone(), advancement_tokens: 1 };
        assert_eq!(who(&plan_for(&state, &registry, &advanced)), vec![("built_to_last", Heard::AsBystander)]);
        let placed = GameEvent::AdvancementCountersPlaced { install: InstallId(1), card, advancement_tokens: 1 };
        assert!(plan_for(&state, &registry, &placed).is_empty());
    }
}
