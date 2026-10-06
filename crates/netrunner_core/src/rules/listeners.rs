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
//! `persistent_trashed_upgrades` (the run's, and the completed run's for
//! its end), because "this ability still applies for the remainder of this
//! run" (AMAZE Amusements) is a card saying it outlives being active.

use crate::cards::CardRegistry;
use crate::dsl::{CardId, EventFilter, Hears, IceFacts, Subject, Trigger, TriggeredEffect};
use crate::rules::active;
use crate::rules::turn_log::{self, AsOf};
use crate::rules::event::GameEvent;
use crate::rules::run::ServerId;
use crate::rules::state::{DeferredTrigger, GamePhase, GameState, Heard, InstallId, InstallSlot, Side, WouldHappen};

/// What an event is about — the thing a `Subject::This` is compared with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum About {
    Nothing,
    /// A card, pinned to the copy the event happened to where the event or
    /// the state can say which. `None` is a card that has no handle any
    /// more (played, stolen, forfeited, trashed): only it can be "this".
    ///
    /// `installed` is whether the card was on the table when it happened,
    /// kept apart from `install` because a card the Runner has just
    /// trashed was installed and has no handle left: giving it one would
    /// have `dispatcher::still_applies` look for the install and stand the
    /// trigger down.
    Card { card: CardId, install: Option<InstallId>, installed: bool },
    Server(ServerId),
    /// A kind of damage.
    Damage(crate::dsl::DamageType),
    /// A number of cards no event names one by one.
    Cards(u32),
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
    /// What was true of the ice, for a moment about a piece of ice in a
    /// run (`Trigger::is_about_ice_in_a_run`) — read off the event, so a
    /// `when` asked again where the trigger fires gets the same answer.
    pub ice: Option<IceFacts>,
    /// Whether an install came out of HQ, for a Corp install's moment —
    /// read off the event (`GameEvent::CardInstalled::from_hq`) for the
    /// same reason as `ice`.
    pub from_hq: Option<bool>,
    /// The server a Corp card was installed in, read off the event for
    /// `EventFilter::InstalledIn`.
    pub installed_in: Option<ServerId>,
    /// Where a trashed card was (`GameEvent::CardTrashed::from`), for the
    /// same reason as `from_hq`: the card is in a discard pile by the time
    /// its trash is heard (`EventFilter::TrashedFrom`, Strike Fund).
    pub trashed_from: Option<crate::dsl::TrashedFrom>,
    /// The object whose abilities did it, for a moment that names one —
    /// the install that fully broke the ice (CR 6.5.7b,
    /// `GameEvent::IceFullyBroken::by`), which Lobisomem's "whenever **it**
    /// fully breaks" asks through `EventFilter::ByThis`.
    pub by: Option<InstallId>,
    /// The card the moment is about was active the instant it happened,
    /// though it is not now — a rezzed install trashed on access, which
    /// meets its own trigger condition as it goes (CR 4.6.6i's Warroid
    /// Tracker: "The Runner trashes a rezzed Warroid Tracker. This meets
    /// the trigger condition of Warroid Tracker's ability"). Hostile
    /// Architecture's "the first time each turn the Runner trashes any of
    /// your installed cards (including this asset)" is about any card, so
    /// it is heard by active cards, and without this its own trash was
    /// heard by nothing. Read off the access, which still holds the install
    /// as it was presented (`AccessState::pending_install_rezzed`).
    /// A trash by a card's text says so itself
    /// (`GameEvent::CardTrashed::install`'s `rezzed`).
    pub was_active: bool,
    /// How a Corp install stood as it was trashed off the table — read off
    /// the event (`GameEvent::CardTrashed::install`), or for an access
    /// trash off the run, since the card is in Archives by the time either
    /// is heard (`EventFilter::TrashedFromThisServer`).
    pub trashed_install: Option<crate::rules::TrashedInstall>,
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
    /// A card in the Runner's heap, which hears its `from_heap` triggers
    /// and nothing else (CR 9.1.8b, Jeitinho).
    in_heap: bool,
    /// An agenda in the Runner's score area, which hears its
    /// `from_runner_score_area` triggers and nothing else (CR 4.5.4,
    /// Project Vacheron).
    in_runner_score_area: bool,
}

/// The card `event` is about as an occurrence of `trigger`, for a
/// trigger that names it (`CardFilter::ThatCard`, Divested Trust's "the
/// stolen agenda"). `None` for a moment about a server or nothing.
pub(crate) fn card_about(state: &GameState, event: &GameEvent, trigger: Trigger) -> Option<CardId> {
    moments(state, event).into_iter().find(|moment| moment.trigger == trigger).and_then(|moment| match moment.about {
        About::Card { card, .. } => Some(card),
        About::Nothing | About::Server(_) | About::Damage(_) | About::Cards(_) => None,
    })
}

/// What `event` is an occurrence of. Most events are an occurrence of
/// nothing: they record a state change no card in the pool prints a trigger
/// for. They are listed by name, not caught by `_`, so that adding a
/// `GameEvent` is a decision made here rather than a silence.
pub(crate) fn moments(state: &GameState, event: &GameEvent) -> Vec<Moment> {
    let card = |card: &CardId, install: Option<InstallId>| About::Card { card: card.clone(), install, installed: install.is_some() };
    let moment = |trigger, about: &About, of| Moment { trigger, about: about.clone(), of, ice: None, from_hq: None, installed_in: None, trashed_from: None, by: None, was_active: false, trashed_install: None };
    // A moment about the ice at `position` in the run's ice. Where the run
    // or the ice has gone by the time the moment is asked again — a
    // trigger fired after the run ended — it is about nothing, and keeps
    // what the event says was true of the ice.
    let ice_moment = |trigger, position: u32, facts: IceFacts| {
        let about = state
            .active_run
            .as_ref()
            .and_then(|run| run.ice.get(position as usize))
            .map_or(About::Nothing, |ice| About::Card { card: ice.card_id.clone(), install: Some(ice.install_id), installed: true });
        Moment { trigger, about, of: Some(Side::Runner), ice: Some(facts), from_hq: None, installed_in: None, trashed_from: None, was_active: false, by: None, trashed_install: None }
    };
    match event {
        GameEvent::EventPlayed { side, card: played, .. } => {
            let about = card(played, None);
            vec![moment(Trigger::OnPlay, &about, Some(*side)), moment(Trigger::OnCardPlayed, &about, Some(*side))]
        }

        // What kind of operation, program or server is the listening card's
        // to ask (`TriggeredEffect::when`), not a second moment: "whenever
        // you play a transaction" is `OnCardPlayed` heard by a card
        // that means transactions.
        // A lockdown is heard as the copy it is in the play area, where it
        // stays (CR 3.5.1c): its own "when you play this operation" is
        // that copy's, and so is the choice it makes.
        GameEvent::OperationPlayed { side, card: played, .. } => {
            let handle = state.corp.play_area.iter().rev().find(|in_play| &in_play.card == played).map(|in_play| in_play.handle);
            let about = About::Card { card: played.clone(), install: handle, installed: false };
            vec![moment(Trigger::OnPlay, &about, Some(*side)), moment(Trigger::OnCardPlayed, &about, Some(*side))]
        }

        GameEvent::ProgramInstalled { side, card: installed, .. }
        | GameEvent::HardwareInstalled { side, card: installed, .. }
        | GameEvent::ResourceInstalled { side, card: installed, .. } => {
            let about = card(installed, newest_rig_install(state, installed));
            vec![moment(Trigger::OnInstall, &about, Some(*side)), moment(Trigger::OnCardInstalled, &about, Some(*side))]
        }
        // The Corp's installs. `card` is `None` only in a masked copy of the
        // event, which the engine never dispatches.
        GameEvent::CardInstalled { side, install, card: installed, from_hq, server } => match installed {
            Some(installed) => vec![Moment { from_hq: Some(*from_hq), installed_in: Some(*server), ..moment(Trigger::OnInstall, &card(installed, Some(*install)), Some(*side)) }],
            None => Vec::new(),
        },

        GameEvent::CardAccessed { card: accessed, server, install } => {
            // The access pinned the copy; the lookup is for an event that
            // carries none. A card accessed out of HQ or R&D has neither.
            let install = install.or_else(|| root_install_of(state, accessed, *server));
            vec![moment(Trigger::OnAccessed, &card(accessed, install), Some(Side::Runner))]
        }
        GameEvent::CardTrashedFromAccess { card: trashed, install, .. } => {
            let about = About::Card { card: trashed.clone(), install: None, installed: install.is_some() };
            let access = state.active_run.as_ref().and_then(|run| run.access_state.as_ref());
            let was_active = install.is_some() && access.is_some_and(|access| access.pending_install == *install && access.pending_install_rezzed);
            // Accessed in the root of the server the run is on; never ice.
            let server = state.active_run.as_ref().map(|run| run.server);
            let trashed_install = install.and(server).map(|server| crate::rules::TrashedInstall { server, rezzed: was_active, installing: false });
            vec![Moment { was_active, trashed_install, ..moment(Trigger::OnTrashedFromAccess, &about, Some(Side::Runner)) }]
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
        // Not scored (CR 1.17.3f), and nothing prints "when a card is added
        // to a score area".
        GameEvent::AddedToScoreAreaAsAgenda { .. } | GameEvent::AgendaAddedToScoreArea { .. } => Vec::new(),
        GameEvent::CardAddedToHand { .. } => Vec::new(),
        // Whoever turned it facedown: Saci hears its host derezzed by a
        // Tranquilizer as by a Corp's cost.
        GameEvent::CardDerezzed { install, card: Some(derezzed) } => vec![moment(Trigger::OnDerez, &card(derezzed, Some(*install)), None)],
        GameEvent::CardDerezzed { card: None, .. } => Vec::new(),

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
        // A cost paid; nothing prints "when a counter is removed".
        GameEvent::AdvancementCountersRemoved { .. } => Vec::new(),
        // Every use is using the card (CR 9.1.6, The Back); only an action
        // is "taking an action on" it (CR 9.5.2a, Juli Moreira Lee).
        GameEvent::AbilityActivated { side, card_id, install, action, .. } => {
            let used = moment(Trigger::OnAbilityUsed, &card(card_id, *install), Some(*side));
            match action {
                true => vec![moment(Trigger::OnActionTaken, &card(card_id, *install), Some(*side)), used],
                false => vec![used],
            }
        }
        GameEvent::AbilityGainedCredits { side, card: source } => {
            vec![moment(Trigger::OnAbilityGainedCredits, &card(source, None), Some(*side))]
        }
        GameEvent::AbilityTookCredits { side, card: source } => {
            vec![moment(Trigger::OnAbilityTookCredits, &card(source, None), Some(*side))]
        }

        GameEvent::TurnStarted { side, .. } => vec![moment(Trigger::OnTurnStart, &About::Nothing, Some(*side))],
        GameEvent::ActionPhaseEnded { side } => vec![moment(Trigger::OnActionPhaseEnd, &About::Nothing, Some(*side))],
        GameEvent::DiscardPhaseEnded { side } => vec![moment(Trigger::OnDiscardPhaseEnd, &About::Nothing, Some(*side))],
        GameEvent::IdentityFlipped { side } => vec![moment(Trigger::OnIdentityFlipped, &About::Nothing, Some(*side))],
        // No card in the pool hears a mark being identified; the cards that
        // read the mark read it (`lingering::mark`).
        GameEvent::MarkIdentified { .. } => Vec::new(),
        GameEvent::BasicDrawActionTaken { side } => vec![moment(Trigger::OnBasicDrawAction, &About::Nothing, Some(*side))],

        GameEvent::RunInitiated { server } => vec![moment(Trigger::OnRunStart, &About::Server(*server), Some(Side::Runner))],
        GameEvent::IceApproached { server, .. } => vec![moment(Trigger::OnIceApproached, &About::Server(*server), Some(Side::Runner))],
        GameEvent::ServerApproached { server } => vec![moment(Trigger::OnApproachServer, &About::Server(*server), Some(Side::Runner))],
        GameEvent::IceEncountered { card_id, .. } => {
            vec![moment(Trigger::OnEncounter, &card(card_id, encountered_install(state)), Some(Side::Runner))]
        }
        GameEvent::RunSucceeded { server } => vec![moment(Trigger::OnSuccessfulRun, &About::Server(*server), Some(Side::Runner))],
        GameEvent::BreachBegun { server } => vec![moment(Trigger::OnBreach, &About::Server(*server), Some(Side::Runner))],
        // The run's four moments about the ice itself. The outermost piece
        // of ice is the first in the run's list (CR 4.6.9b; `RunState::ice`
        // is outermost first).
        GameEvent::IcePassed { position, after_fully_breaking, rezzed_as, .. } => vec![ice_moment(
            Trigger::OnIcePassed,
            *position,
            IceFacts {
                outermost: *position == 0,
                after_fully_breaking: *after_fully_breaking,
                rezzed_code_gate_or_sentry: rezzed_as.iter().any(|ice_type| matches!(ice_type, crate::dsl::IceType::CodeGate | crate::dsl::IceType::Sentry)),
                ..IceFacts::default()
            },
        )],
        GameEvent::SubroutineBroken { strength, printed, .. } => {
            let position = state.active_run.as_ref().map_or(0, |run| run.position as u32);
            vec![ice_moment(Trigger::OnSubroutineBroken, position, IceFacts { at_most_zero_strength: *strength <= 0, printed_subroutine: *printed, ..IceFacts::default() })]
        }
        // Only an encountered piece of ice resolves a subroutine here: a
        // card that resolves one by its text (Nanisivik Grid) announces
        // nothing.
        GameEvent::SubroutineFired { .. } => {
            let position = state.active_run.as_ref().map_or(0, |run| run.position as u32);
            vec![ice_moment(Trigger::OnSubroutineResolved, position, IceFacts::default())]
        }
        GameEvent::IceFullyBroken { position, by, .. } => vec![Moment { by: *by, ..ice_moment(Trigger::OnIceFullyBroken, *position, IceFacts::default()) }],
        GameEvent::IceBypassed { position, .. } => vec![ice_moment(Trigger::OnIceBypassed, *position, IceFacts::default())],
        // About the ice by its install, which the event carries: an
        // encounter "end the run" ended is heard after the run is gone.
        GameEvent::EncounterEnded { card_id, install } => {
            vec![moment(Trigger::OnEncounterEnded, &card(card_id, Some(*install)), Some(Side::Runner))]
        }
        // Only the Runner's spending is anyone's trigger, and only during a
        // run: Shackleton Grid's "during a run against this server" is the
        // moment's server, as "this server" is for every run moment.
        // Only the Runner's spending is anyone's trigger: Shackleton Grid's
        // only during a run, and spending off an installed card (The
        // Twinning) in a run or out of one. The Corp's spending is an
        // occurrence of nothing until a card listens for it.
        GameEvent::CreditsSpentFromOutsidePool { side: Side::Runner, run_against, first_host, .. } => {
            let mut heard = Vec::new();
            if let Some(server) = run_against {
                heard.push(moment(Trigger::OnCreditsSpentOutsidePool, &About::Server(*server), Some(Side::Runner)));
            }
            // About the card the credits came off (Keiko's "an installed
            // companion card"), one per payment however many gave.
            if let Some((host, install)) = first_host {
                heard.push(moment(Trigger::OnCreditsSpentFromInstalledCard, &card(host, Some(*install)), Some(Side::Runner)));
            }
            heard
        }
        GameEvent::CreditsSpentFromOutsidePool { side: Side::Corp, .. } => Vec::new(),
        // Heard by whoever carried it out; the rules' trashes are nobody's.
        // A rezzed install was active the instant it left, and hears its
        // own trash as one (CR 4.6.6i): Hostile Architecture's and Yakov
        // Erikovich Avdakov's "(including this …)".
        GameEvent::CardTrashed { card: trashed, from, by: Some(by), install, .. } => {
            let about = About::Card { card: trashed.clone(), install: None, installed: from.installed() };
            let was_active = install.is_some_and(|install| install.rezzed);
            vec![Moment { trashed_from: Some(*from), was_active, trashed_install: *install, ..moment(Trigger::OnCardTrashed, &about, Some(*by)) }]
        }
        // The one rules' trash that is a moment: an event leaving the play
        // area as it finishes resolving (CR 3.7.1), or a lockdown as the
        // Corp's turn begins (3.5.1c), nobody's, so heard by a card whose
        // trash is passive — Aniccam's "an event is trashed"
        // (`EventFilter::Anyone`) — and by no "you trash" a filter asks of.
        GameEvent::CardTrashed { card: trashed, from: from @ crate::dsl::TrashedFrom::PlayArea, by: None, .. } => {
            let about = About::Card { card: trashed.clone(), install: None, installed: false };
            vec![Moment { trashed_from: Some(*from), ..moment(Trigger::OnCardTrashed, &about, None) }]
        }
        GameEvent::CardTrashed { by: None, .. } => Vec::new(),
        // The Corp purges, whatever the card that made it (CR 10.1.2).
        GameEvent::VirusCountersPurged { .. } => vec![moment(Trigger::OnVirusCountersPurged, &About::Nothing, Some(Side::Corp))],
        GameEvent::CardMoved { install, card: moved, .. } => match moved {
            Some(moved) => vec![moment(Trigger::OnCardMoved, &card(moved, Some(*install)), Some(Side::Corp))],
            None => Vec::new(),
        },
        GameEvent::CardsLookedAt { .. } => Vec::new(),
        // A selection that reveals what it chose (Hyoubu Institute's own
        // "reveal the top card of the stack") is its chooser revealing
        // each card.
        GameEvent::CardsSelected { side, cards, revealed: true } => cards
            .iter()
            .map(|revealed| moment(Trigger::OnCardRevealed, &About::Card { card: revealed.clone(), install: None, installed: false }, Some(*side)))
            .collect(),
        // Heard by whoever revealed it, about the card, which both players
        // now see (Hyoubu Institute's "you reveal a card").
        GameEvent::CardRevealed { card: revealed, by, .. } => {
            vec![moment(Trigger::OnCardRevealed, &About::Card { card: revealed.clone(), install: None, installed: false }, Some(*by))]
        }
        GameEvent::CardsSetAside { .. } => Vec::new(),
        // The Runner breached, but the cards are the Corp's: whoever
        // listens hears it, and none of it is a card to be "this".
        GameEvent::ArchivesTurnedFaceup { count } => vec![moment(Trigger::OnArchivesTurnedFaceup, &About::Cards(*count), None)],
        // Only the ordinary conclusions: a flatline or an agenda win
        // mid-access ends the game, and nothing resolves after that.
        GameEvent::RunCompleted { server } | GameEvent::RunJackedOut { server } | GameEvent::RunEndedByEffect { server } => {
            vec![moment(Trigger::OnRunEnded, &About::Server(*server), Some(Side::Runner))]
        }

        // "Whenever you do damage" is the responsible player's (CR 10.4.1),
        // and damage nobody is responsible for is nobody's to hear.
        // And suffering it is the Runner's — the only player damage is
        // dealt to (CR 10.4.1) — whoever was responsible, a cost included.
        GameEvent::DamageTaken { responsible, damage_type, amount } => {
            let mut heard = Vec::new();
            if let Some(side) = responsible {
                heard.push(moment(Trigger::OnDamageDealt, &About::Nothing, Some(*side)));
            }
            if *amount > 0 {
                heard.push(moment(Trigger::OnDamageSuffered, &About::Damage(*damage_type), Some(Side::Runner)));
            }
            heard
        }
        // One occurrence per batch, not per card: "trash 1 **or more**".
        GameEvent::CardsTrashedFromHq { .. } => vec![moment(Trigger::OnCardsTrashedFromHq, &About::Nothing, Some(Side::Corp))],
        GameEvent::CardsTrashedFromRnD { by, .. } => vec![moment(Trigger::OnCardsTrashedFromRnD, &About::Nothing, *by)],
        GameEvent::CardsTrashedFromGripOrStack { by, .. } => vec![moment(Trigger::OnCardsTrashedFromGripOrStack, &About::Nothing, *by)],
        GameEvent::FinishedResolving { side, card: resolved } => vec![moment(Trigger::OnFinishedResolving, &card(resolved, None), Some(*side))],
        // Which action is the event's to say, for a requirement to read;
        // the moment is the actor's and about nothing.
        GameEvent::ActionFinished { side, .. } => vec![moment(Trigger::OnActionFinished, &About::Nothing, Some(*side))],
        // Only the Runner's tags are anyone's trigger, and removing none is
        // not removing one.
        // The moment is the remover's (CR 1.14.3a: either player can remove
        // the Runner's tags), so "whenever **you** remove" can be told from
        // Synapse Global's cost, the Corp removing the Runner's tag.
        GameEvent::TagsRemoved { side: Side::Runner, amount: 0, .. } => Vec::new(),
        GameEvent::TagRemoved { side: Side::Runner, by }
        | GameEvent::TagsRemoved { side: Side::Runner, by, .. }
        | GameEvent::TagsCleared { side: Side::Runner, by } => {
            vec![moment(Trigger::OnTagRemoved, &About::Nothing, Some(*by))]
        }
        GameEvent::TagsGiven { side: Side::Runner, .. } => vec![moment(Trigger::OnTagsGiven, &About::Nothing, Some(Side::Runner))],
        // Bad publicity is only ever the Corp's (CR 1.14.2e), and taking
        // none is not taking some.
        GameEvent::BadPublicityGiven { amount: 0 } => Vec::new(),
        GameEvent::BadPublicityGiven { .. } => vec![moment(Trigger::OnBadPublicityTaken, &About::Nothing, Some(Side::Corp))],
        GameEvent::TagRemoved { side: Side::Corp, .. }
        | GameEvent::TagsRemoved { side: Side::Corp, .. }
        | GameEvent::TagsCleared { side: Side::Corp, .. }
        | GameEvent::TagsGiven { side: Side::Corp, .. } => Vec::new(),

        // "When you would suffer damage" is the one "would" a card in the
        // pool is printed about; a tag or a trash about to happen is an
        // occurrence of nothing until a card listens for one.
        GameEvent::AboutToResolve { what: WouldHappen::Damage { kind, .. } } => vec![moment(Trigger::OnDamageAboutToResolve, &About::Damage(*kind), None)],
        // "You would draw" — only the Runner's draws are announced
        // (`ability::draw`), so only theirs is a moment.
        GameEvent::AboutToResolve { what: WouldHappen::Draw { side, .. } } => vec![moment(Trigger::OnDrawAboutToResolve, &About::Nothing, Some(*side))],
        GameEvent::AboutToResolve { what: WouldHappen::Tags { .. } | WouldHappen::Trash { .. } | WouldHappen::EncounterAbility { .. } | WouldHappen::RunEnds { .. } | WouldHappen::Trace { .. } } => Vec::new(),
        // Only the card itself prints it ("when this asset would be
        // uninstalled"), so the moment is the card's.
        GameEvent::AboutToBeUninstalled { card: card_id, install } => {
            vec![moment(Trigger::OnWouldBeUninstalled, &card(card_id, Some(*install)), None)]
        }

        GameEvent::ClickSpent { .. }
        | GameEvent::CreditsGained { .. }
        | GameEvent::CardDrawn { .. }
        | GameEvent::SubroutineGained { .. }
        | GameEvent::RunNotDeclaredSuccessful { .. }
        | GameEvent::IceStrengthModified { .. }
        | GameEvent::IceSwapped { .. }
        | GameEvent::TurnEnded { .. }
        | GameEvent::DiscardPending { .. }
        | GameEvent::CardDiscarded { .. }
        | GameEvent::CardAddedToDeck { .. }
        | GameEvent::CardHosted { .. }
        | GameEvent::RunEndPrevented { .. }
        | GameEvent::RunRedirected { .. }
        | GameEvent::RunnerFlatlined
        | GameEvent::WonByCardText { .. }
        | GameEvent::CreditsSpent { .. }
        | GameEvent::CardRemovedFromGame { .. }
        | GameEvent::GameOver { .. }
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
        | GameEvent::BadPublicityCreditsSpent { .. }
        | GameEvent::PaymentChoiceOffered { .. }
        | GameEvent::BonusRunCreditsSpent { .. }
        | GameEvent::CardsSelected { revealed: false, .. }
        | GameEvent::PendingCardSelectionOffered { .. }
        | GameEvent::MemoryLimitExceeded { .. }
        | GameEvent::PendingServerChoiceOffered { .. }
        | GameEvent::BadPublicityRemoved { .. }
        | GameEvent::HandKept { .. }
        | GameEvent::MulliganTaken { .. }
        | GameEvent::AdditionalAccessGranted { .. }
        | GameEvent::AccessReplacementSet { .. }
        | GameEvent::AccessReplaced { .. }
        | GameEvent::CreditsLost { .. }
        | GameEvent::ClicksLost { .. }
        | GameEvent::ClicksGained { .. }
        | GameEvent::Prevented { .. }
        | GameEvent::CountersAdded { .. }
        | GameEvent::CountersRemoved { .. }
        | GameEvent::PendingChoicePresented { .. }
        | GameEvent::PendingChoiceResolved { .. }
        | GameEvent::NumberChoiceOffered { .. }
        | GameEvent::NumberChosen { .. }
        | GameEvent::PsiBidsRevealed { .. }
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
pub(crate) fn plan_for(state: &GameState, registry: &CardRegistry, event: &GameEvent, as_of: &AsOf) -> Vec<(Side, DeferredTrigger)> {
    let moments = moments(state, event);
    if moments.is_empty() {
        return Vec::new();
    }
    let mut plan = Vec::new();
    // A card that has lost its abilities hears nothing (CR 9.1.9a): its
    // triggers are among what is lost.
    let lost = active::installs_without_abilities(state, registry);
    for listener in listeners(state, registry, &moments) {
        if !listener.in_heap && listener.install.is_some_and(|install| lost.contains(&install)) {
            continue;
        }
        let Some(definition) = registry.get(&listener.card) else { continue };
        // "The first time each turn": one verdict a card, because its
        // first-time entries share a count, judged against the log as the
        // event was recorded rather than as it stands now.
        let later = (definition.triggers.iter().any(|triggered| triggered.first_each_turn) && !is_first(state, definition, &listener, as_of))
            || (definition.triggers.iter().any(|triggered| triggered.first_each_encounter) && !is_first_this_encounter(state));
        for moment in &moments {
            let mut hearing = definition.triggers.iter().filter(|triggered| hears(state, registry, triggered, &listener, moment, later)).peekable();
            if hearing.peek().is_none() {
                continue;
            }
            // Who the moment is about travels with the trigger for a card
            // whose effects act on "it" (`TriggeredEffect::acts_on_subject`,
            // Cookbook) — on the queued trigger because the choice it parks
            // outlives this resolution. Which of the card's effects use it
            // is decided per effect when they fire.
            let (target, target_install) = match &moment.about {
                About::Card { card, install, .. } if hearing.any(|triggered| triggered.acts_on_subject) => (Some(card.clone()), *install),
                _ => (None, None),
            };
            let heard = match (listener.active, is_this(&listener, moment)) {
                _ if listener.in_heap => Heard::FromHeap,
                _ if listener.in_runner_score_area => Heard::FromRunnerScoreArea,
                (true, true) => Heard::AsBoth,
                (true, false) => Heard::AsBystander,
                (false, _) => Heard::AsSubject,
            };
            plan.push((
                listener.side,
                DeferredTrigger { announce: None,
                    card: listener.card.clone(),
                    install: listener.install,
                    trigger: moment.trigger,
                    target,
                    target_install,
                    event: Some(event.clone()),
                    continuation: None,
                    heard,
                    not_the_first_this_turn: later,
                    fired: 0,
                },
            ));
        }
    }
    // A delayed conditional ability waiting for this moment is one of its
    // reactions (CR 9.6.13), its controller's to order with the rest: it
    // goes in beside its side's entries, and resolves as the card that
    // made it. `dispatcher::dispatch_event` takes it off the list.
    for moment in &moments {
        for delayed in state.delayed.iter().filter(|delayed| delayed_hears(state, delayed, moment.trigger) && delayed_admits(state, registry, delayed, event)) {
            let side = registry.get(&delayed.card).map_or(Side::Corp, |card| card.side);
            let due = DeferredTrigger { announce: None,
                card: delayed.card.clone(),
                install: delayed.install,
                trigger: moment.trigger,
                target: None,
                target_install: None,
                event: Some(event.clone()),
                continuation: Some(delayed.effect.clone()),
                heard: Default::default(),
                not_the_first_this_turn: false,
                fired: 0,
            };
            let at = plan.iter().rposition(|(planned, _)| *planned == side).map_or(plan.len(), |last| last + 1);
            plan.insert(at, (side, due));
        }
    }
    plan
}

/// Whether the delayed ability `delayed` waits for a moment of `trigger`
/// now: its moment, on the turn it was made for.
pub(crate) fn delayed_hears(state: &GameState, delayed: &crate::rules::lingering::DelayedAbility, trigger: Trigger) -> bool {
    delayed_hears_on(delayed, trigger, state.turn)
}

pub(crate) fn delayed_hears_on(delayed: &crate::rules::lingering::DelayedAbility, trigger: Trigger, turn: u32) -> bool {
    delayed.when == trigger && delayed.turn == turn
}

/// Whether `event` is one of the occurrences the delayed ability `delayed`
/// waits for, by its filter: judged as a card's trigger condition is
/// (`when_admits`), the card that made it hearing as its controller —
/// In the Groove's "whenever **you** install".
pub(crate) fn delayed_admits(state: &GameState, registry: &CardRegistry, delayed: &crate::rules::lingering::DelayedAbility, event: &GameEvent) -> bool {
    if delayed.filter.is_none() {
        return true;
    }
    let controller = registry.get(&delayed.card).map_or(Side::Corp, |card| card.side);
    let triggered = TriggeredEffect {
        trigger: delayed.when,
        subject: None,
        when: delayed.filter.clone(),
        acts_on_subject: false,
        first_each_turn: false,
        first_each_encounter: false,
        granted: false,
        from_heap: false,
        from_runner_score_area: false,
        text: None,
        effects: Vec::new(),
        requirement: None,
    };
    when_admits(state, registry, &triggered, controller, &delayed.card, delayed.install, Some(event))
}

/// Whether the delayed ability `delayed` is done with once `event` has
/// been heard: one from an earlier turn, or one heard once (CR 9.6.13c).
pub(crate) fn delayed_spent_by(state: &GameState, registry: &CardRegistry, delayed: &crate::rules::lingering::DelayedAbility, event: &GameEvent) -> bool {
    delayed.turn < state.turn
        || (!delayed.every_time
            && moments(state, event).iter().any(|moment| delayed_hears(state, delayed, moment.trigger))
            && delayed_admits(state, registry, delayed, event))
}

/// Whether the occurrence `as_of` counted is the first this turn of what
/// `definition`'s "first time each turn" entries listen for. A card whose
/// first time is about itself ("you advance **this agenda**") or done by
/// itself ("**this program** fully breaks", `EventFilter::ByThis`) is judged
/// on its copy, the rest on the turn; `validate` keeps a card to one or the
/// other.
fn is_first(state: &GameState, definition: &crate::dsl::CardDefinition, listener: &Listener, as_of: &AsOf) -> bool {
    let first_time = definition.triggers.iter().filter(|triggered| triggered.first_each_turn);
    // Installs into a root are counted on the copies in it, the listener
    // among them (`turn_log::record`), as of this install.
    if first_time.clone().any(|triggered| triggered.when == Some(EventFilter::InRootOfThisServer)) {
        return listener
            .install
            .and_then(|install| state.find_corp_install(install))
            .is_some_and(|installed| installed.this_turn.count(state.turn, Trigger::OnInstall) == 1);
    }
    if first_time.clone().any(|triggered| triggered.subject == Some(Subject::This) || triggered.when == Some(EventFilter::ByThis)) {
        let triggers: Vec<Trigger> = first_time.map(|triggered| triggered.trigger).collect();
        as_of.is_first_on(listener.install, state.turn, &triggers)
    } else {
        as_of.is_first(&turn_log::first_time_on(definition, state))
    }
}

/// Whether the break being heard is the encounter's first
/// (`TriggeredEffect::first_each_encounter`). The count is read as the scan
/// runs, which is as of the break: each `SubroutineBroken` is dispatched
/// the moment its break is counted (`ability::break_pending`), and
/// `validate` holds the word to that one trigger.
fn is_first_this_encounter(state: &GameState) -> bool {
    state.active_run.as_ref().is_some_and(|run| run.this_encounter.subroutines_broken == 1)
}

/// Whether `moment` is about `listener` itself — or, for a moment about a
/// server, about the server it is in.
fn is_this(listener: &Listener, moment: &Moment) -> bool {
    match &moment.about {
        About::Nothing | About::Damage(_) | About::Cards(_) => false,
        About::Card { install: Some(install), .. } => listener.install == Some(*install),
        // A card with no handle left is "this" only to itself, and it is
        // listening only because it is the subject — inactive, or active
        // only for the instant it left (`Moment::was_active`). Never an
        // active copy still on the table, which keeps its handle.
        About::Card { install: None, card, .. } => (!listener.active || moment.was_active) && listener.install.is_none() && &listener.card == card,
        About::Server(server) => listener.server == Some(*server),
    }
}

/// Whether what a moment is about passes a card's `when`.
/// `here` is the server the listening card is in or protecting — or was,
/// for a card trashed out of it that hears its own trash.
fn passes(state: &GameState, registry: &CardRegistry, filter: &EventFilter, moment: &Moment, listener_card: &CardId, install: Option<InstallId>, here: Option<ServerId>) -> bool {
    if let EventFilter::All(parts) = filter {
        return parts.iter().all(|part| passes(state, registry, part, moment, listener_card, install, here));
    }
    // "From the root of this server or protecting it, except during
    // installation."
    if let EventFilter::TrashedFromThisServer = filter {
        return moment.trashed_install.is_some_and(|trashed| !trashed.installing && Some(trashed.server) == here);
    }
    // "A rezzed card, except during installation."
    if let EventFilter::TrashedRezzed = filter {
        return moment.trashed_install.is_some_and(|trashed| trashed.rezzed && !trashed.installing);
    }
    // "Whenever **it** fully breaks": the object the moment names is the
    // listening install.
    if let EventFilter::ByThis = filter {
        return moment.by.is_some() && moment.by == install;
    }
    // "Whenever **host ice** is …": the moment is about the listening
    // install's host.
    if let EventFilter::Host = filter {
        let About::Card { install: Some(about), .. } = moment.about else { return false };
        return install.is_some_and(|install| {
            state.runner.rig.iter().any(|card| card.install_id == install && (card.hosted_on_ice == Some(about) || card.hosted_on_rig_card == Some(about)))
        });
    }
    // "The Corp installs a card in the root of a server": every Corp
    // install but a piece of ice (CR 3.4.2).
    if let EventFilter::InRoot = filter {
        return moment.of == Some(Side::Corp)
            && matches!(&moment.about, About::Card { card, .. } if registry.get(card).is_some_and(|definition| !matches!(definition.card_type, crate::dsl::CardType::Ice(_))));
    }
    // "You install a card in the root of **this server**": the same, into
    // the listener's own server.
    if let EventFilter::InRootOfThisServer = filter {
        return moment.of == Some(Side::Corp)
            && here.is_some()
            && moment.installed_in == here
            && matches!(&moment.about, About::Card { card, .. } if registry.get(card).is_some_and(|definition| !matches!(definition.card_type, crate::dsl::CardType::Ice(_))));
    }
    if let EventFilter::Ice(required) = filter {
        return moment.ice.is_some_and(|facts| required.admits(facts));
    }
    if let EventFilter::Whose(side) = filter {
        return moment.of == Some(*side);
    }
    if let EventFilter::Anyone = filter {
        return true;
    }
    if let EventFilter::OwnedBy { owner, whose } = filter {
        return moment.of == Some(*whose)
            && matches!(&moment.about, About::Card { card, .. } if registry.get(card).is_some_and(|definition| definition.side == *owner));
    }
    if let EventFilter::InstalledFromHq(from_hq) = filter {
        return moment.from_hq == Some(*from_hq);
    }
    if let EventFilter::InstalledIn(kind) = filter {
        return moment.installed_in.is_some_and(|server| kind.admits(server));
    }
    if let EventFilter::TrashedFrom(places) = filter {
        return moment.trashed_from.is_some_and(|from| places.contains(&from));
    }
    match (filter, &moment.about) {
        (EventFilter::Card(filter), About::Card { card, install, .. }) => {
            registry.get(card).is_some_and(|definition| crate::dsl::card_matches_filter(definition, filter))
                && crate::rules::pending_choice::copy_matches(state, &placed(state, filter, listener_card, here), *install)
        }
        (EventFilter::InstalledCard(filter), About::Card { card, installed: true, .. }) => {
            registry.get(card).is_some_and(|definition| crate::dsl::card_matches_filter(definition, filter))
        }
        (EventFilter::Server(servers), About::Server(server)) => servers.contains(server),
        (EventFilter::Mark, About::Server(server)) => crate::rules::lingering::mark(state) == Some(*server),
        (EventFilter::ChosenServer, About::Server(server)) => crate::rules::lingering::chosen_server(state, listener_card) == Some(*server),
        (EventFilter::ProtectedByIce, About::Server(server)) => {
            state.corp.installed.iter().any(|card| card.server == *server && card.slot == InstallSlot::Ice)
        }
        (EventFilter::Damage(kind), About::Damage(dealt)) => kind == dealt,
        (EventFilter::AtLeast(least), About::Cards(count)) => count >= least,
        // `CardDefinition::validate` refuses the mismatch in a card file.
        _ => false,
    }
}

/// A card filter in a `when` with the listener's own places written in:
/// "protecting **this server**" (ZATO City Grid, `CardFilter::InThisServer`)
/// and "protecting **the chosen server**" (Tsakhia, `InChosenServer`). A
/// filter that names neither is returned borrowed, so the scan copies
/// nothing for the cards that do not ask.
fn placed<'f>(state: &GameState, filter: &'f crate::dsl::CardFilter, card: &CardId, here: Option<ServerId>) -> std::borrow::Cow<'f, crate::dsl::CardFilter> {
    use crate::dsl::CardFilter;
    fn names_a_place(filter: &CardFilter) -> bool {
        match filter {
            CardFilter::InThisServer | CardFilter::InChosenServer => true,
            CardFilter::All(parts) | CardFilter::AnyOf(parts) => parts.iter().any(names_a_place),
            _ => false,
        }
    }
    if !names_a_place(filter) {
        return std::borrow::Cow::Borrowed(filter);
    }
    std::borrow::Cow::Owned(filter.clone().with_this_server(here).with_chosen_server(crate::rules::lingering::chosen_server(state, card)))
}

/// Whether `triggered`'s `when` admits the event a queued trigger carries —
/// the scan's own check, asked again where the trigger fires, because one
/// queued trigger stands for every `TriggeredEffect` of its card that names
/// the trigger and two of them can mean different occurrences (a card
/// printed "on HQ, … / on R&D, …"). What a moment is about is in the event,
/// so the answer cannot have changed since the scan. A filter with no event
/// to read admits nothing.
///
/// Whose moment it was is asked again too (`whose_admits`), for the same
/// reason: Méliès U hears the discard phase ending on its own "your" and
/// on "the Runner's", and the queued trigger stands for both entries.
pub(crate) fn when_admits(state: &GameState, registry: &CardRegistry, triggered: &TriggeredEffect, controller: Side, card: &CardId, install: Option<InstallId>, event: Option<&GameEvent>) -> bool {
    let Some(event) = event else { return triggered.when.is_none() };
    let mut meant = moments(state, event).into_iter().filter(|moment| moment.trigger == triggered.trigger).peekable();
    // A trigger queued with an event that is no occurrence of it (a
    // continuation, a fixture) had nothing to narrow before, and has not now.
    if triggered.when.is_none() && meant.peek().is_none() {
        return true;
    }
    meant.any(|moment| {
        // Where the card is — or, for the card the moment is about and no
        // longer on the table, where it was.
        let here = install.and_then(|install| state.find_corp_install(install)).map(|installed| installed.server).or_else(|| match &moment.about {
            About::Card { card: about, .. } if about == card => moment.trashed_install.map(|trashed| trashed.server),
            _ => None,
        });
        whose_admits(triggered, controller, &moment) && triggered.when.as_ref().is_none_or(|filter| passes(state, registry, filter, &moment, card, install, here))
    })
}

/// Whether `moment` is one of the side's `triggered` hears: for a trigger
/// phrased about "you" (`Hears::OwnSide`), the controller's own — or, where
/// the card names another's (`EventFilter::Whose`, "when the Runner's
/// discard phase ends"), that one's, which `passes` then checks.
///
/// A trigger about the card itself (`Subject::This`) is no "you": Strike
/// Fund's "when this event is trashed" is heard whoever trashed it — the
/// Corp, for a card net damage the Corp did took out of the grip (CR
/// 10.4.2a). Every other `Subject::This` on a trigger phrased about its
/// controller is about something only the controller does to their own
/// card (installs it, plays it, advances it), so the answer there was
/// always the controller's.
fn whose_admits(triggered: &TriggeredEffect, controller: Side, moment: &Moment) -> bool {
    if triggered.when.as_ref().is_some_and(EventFilter::names_whose)
        || triggered.subject == Some(Subject::This)
        || triggered.trigger.hears() != Hears::OwnSide
    {
        return true;
    }
    moment.of.is_none_or(|side| side == controller)
}

/// Whether one `TriggeredEffect` on `listener` hears `moment`.
fn hears(state: &GameState, registry: &CardRegistry, triggered: &TriggeredEffect, listener: &Listener, moment: &Moment, later: bool) -> bool {
    if triggered.trigger != moment.trigger {
        return false;
    }
    // A second occurrence was never a listener — `first_each_turn` is part
    // of the condition, like `when` below.
    if (triggered.first_each_turn || triggered.first_each_encounter) && later {
        return false;
    }
    if triggered.when.as_ref().is_some_and(|filter| !passes(state, registry, filter, moment, &listener.card, listener.install, listener.server)) {
        return false;
    }
    if !whose_admits(triggered, listener.side, moment) {
        return false;
    }
    // A heap listener hears its heap triggers, and nothing else does.
    if triggered.from_heap != listener.in_heap {
        return false;
    }
    // And a stolen agenda's, which nothing else hears.
    if triggered.from_runner_score_area != listener.in_runner_score_area {
        return false;
    }
    // An ability this card gives the subject is the subject's (ZATO City
    // Grid's): a subject that cannot gain abilities, or has lost them
    // (Hush's host), does not have it.
    if triggered.granted && !matches!(moment.about, About::Card { install: Some(install), .. } if active::may_have_granted(state, registry, install)) {
        return false;
    }
    if listener.in_heap || listener.in_runner_score_area {
        return true;
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

    // `rules::active` says which cards are active, in the order this scan
    // always asked them: the score area before the table — the order
    // `DiscardPhaseEnded` always asked in, and the only one the old
    // audiences agreed on.
    let listening = |card: active::ActiveCard<'_>| Listener { side: card.side, card: card.card.clone(), install: card.install, server: card.server, active: true, in_heap: false, in_runner_score_area: false };
    corp.extend(active::corp(state, registry).map(listening));
    runner.extend(active::runner(state, registry).map(listening));

    // The subject leads its side: "when you score this agenda" before the
    // identity's "whenever you score an agenda". It is already listening
    // if it is active; where it is not — face down, in a hand or a deck,
    // gone from the table — it listens for what is about itself.
    for moment in moments {
        let About::Card { card, install, .. } = &moment.about else { continue };
        // A pass, a break and a bypass are none of CR 9.1.8's exceptions,
        // so ice hears them only while it is active (9.1.7): an unrezzed
        // Vertigo passed does not fire.
        if moment.trigger.is_about_ice_in_a_run() {
            continue;
        }
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
                let server = install
                    .and_then(|install| state.corp.installed.iter().find(|c| c.install_id == install))
                    .map(|c| c.server)
                    .or(moment.trashed_install.map(|trashed| trashed.server));
                Listener { side, card: card.clone(), install: *install, server, active: moment.was_active, in_heap: false, in_runner_score_area: false }
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
            in_heap: false,
            in_runner_score_area: false,
        }));
    }

    // …and while the run lasts, a persisting ability hears what it listens
    // for as any active one does: it "never becomes inactive" (CR 9.12.5b).
    // Tucana's "whenever an agenda is … stolen from the root of this
    // server" is heard after the Runner trashed Tucana and then stole the
    // agenda beside it. Its "this server" is the attacked one (the
    // listener's `server`, and `EffectRequirement::
    // AgendaCameFromThisCardsServer`'s fallback).
    if let Some(run) = &state.active_run {
        corp.extend(run.persistent_trashed_upgrades.iter().map(|card| Listener {
            side: Side::Corp,
            card: card.clone(),
            install: None,
            server: Some(run.server),
            active: true,
            in_heap: false,
            in_runner_score_area: false,
        }));
    }

    // A card in the Runner's heap whose text is active there (CR 9.1.8b:
    // Jeitinho's "install this hardware from your heap"). One listener a
    // card, since copies in a heap are the same card to both players;
    // after the table, as the heap is no part of it.
    let mut in_heap: Vec<&CardId> = Vec::new();
    for card in &state.runner.heap {
        if !in_heap.contains(&card) && registry.get(card).is_some_and(|definition| definition.triggers.iter().any(|triggered| triggered.from_heap)) {
            in_heap.push(card);
        }
    }
    runner.extend(in_heap.into_iter().map(|card| Listener { side: Side::Runner, card: card.clone(), install: None, server: None, active: false, in_heap: true, in_runner_score_area: false }));

    // An agenda in the Runner's score area whose text says it is active
    // there (CR 4.5.4: Project Vacheron). The Corp's ability (CR 1.14.4a),
    // so it is asked with the Corp's cards; one listener a copy, since its
    // counters are its own.
    corp.extend(
        state
            .runner
            .scored_agendas
            .iter()
            .filter(|scored| scored.as_agenda.is_none())
            .filter(|scored| registry.get(&scored.card).is_some_and(|definition| definition.triggers.iter().any(|triggered| triggered.from_runner_score_area)))
            .map(|scored| Listener { side: Side::Corp, card: scored.card.clone(), install: Some(scored.install_id), server: None, active: false, in_heap: false, in_runner_score_area: true }),
    );

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

    /// The scan, for an event these tests never recorded: nothing here
    /// prints "the first time each turn", so the count is not read.
    fn plan_for(state: &GameState, registry: &CardRegistry, event: &GameEvent) -> Vec<(Side, DeferredTrigger)> {
        super::plan_for(state, registry, event, &AsOf::unrecorded(state))
    }
    use crate::dsl::{CardDefinition, CardType, Effect};
    use crate::rules::state::{InstalledCard, InstalledRunnerCard, ScoredAgenda};

    fn listens(id: &str, side: Side, card_type: CardType, trigger: Trigger, subject: Option<Subject>) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type,
            triggers: vec![TriggeredEffect { subject, when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_heap: false, from_runner_score_area: false, text: None, trigger, effects: vec![Effect::GainCredits(side, 1)], requirement: None }],
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
            listens("press_office", Side::Corp, CardType::Asset, Trigger::OnCardPlayed, Some(Subject::Any)),
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

    /// "Whenever you make a successful run **on HQ**" is not pending after a
    /// run on R&D: the filter is part of the trigger condition, so a card it
    /// refuses was never a listener — nothing queued, nothing to order.
    #[test]
    fn a_trigger_that_means_some_occurrences_is_not_a_listener_for_the_others() {
        let mut on_hq = listens("docklands_pass", Side::Runner, CardType::Hardware, Trigger::OnSuccessfulRun, Some(Subject::Any));
        on_hq.triggers[0].when = Some(EventFilter::Server(vec![ServerId::Hq]));
        let mut on_a_virus = listens("cookbook", Side::Runner, CardType::Resource, Trigger::OnCardInstalled, Some(Subject::Any));
        on_a_virus.triggers[0].when = Some(EventFilter::Card(crate::dsl::CardFilter::HasSubtype(crate::dsl::CardSubtype::Virus)));
        on_a_virus.triggers[0].acts_on_subject = true;
        let virus = CardDefinition { subtypes: vec![crate::dsl::CardSubtype::Virus], ..listens("leech", Side::Runner, CardType::Program, Trigger::OnTurnStart, None) };
        let plain = listens("mayfly", Side::Runner, CardType::Program, Trigger::OnTurnStart, None);
        let registry = registry(vec![on_hq, on_a_virus, virus, plain]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.runner.rig = vec![in_the_rig("docklands_pass", 1), in_the_rig("cookbook", 2), in_the_rig("leech", 3), in_the_rig("mayfly", 4)];

        assert_eq!(who(&plan_for(&state, &registry, &GameEvent::RunSucceeded { server: ServerId::Hq })), vec![("docklands_pass", Heard::AsBystander)]);
        assert!(plan_for(&state, &registry, &GameEvent::RunSucceeded { server: ServerId::RnD }).is_empty());

        let installed = |card: &str| GameEvent::ProgramInstalled { side: Side::Runner, card: CardId(card.to_string()), memory_cost: 1, credits_paid: 0 };
        assert!(plan_for(&state, &registry, &installed("mayfly")).is_empty());
        // "…place 1 virus counter on **it**": the virus travels with the
        // trigger, because the card said its effects act on it.
        let plan = plan_for(&state, &registry, &installed("leech"));
        assert_eq!(who(&plan), vec![("cookbook", Heard::AsBystander)]);
        assert_eq!((plan[0].1.target.clone(), plan[0].1.target_install), (Some(CardId("leech".to_string())), Some(InstallId(3))));
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

    /// "The first time each turn" is part of what a card listens for: the
    /// turn's second occurrence is not planned at all, and what decides is
    /// the count as each event was recorded.
    #[test]
    fn the_second_occurrence_in_a_turn_is_never_a_listener() {
        let mut reality_plus = listens("nbn_reality_plus", Side::Corp, CardType::Identity, Trigger::OnTagsGiven, None);
        reality_plus.triggers[0].first_each_turn = true;
        let registry = registry(vec![reality_plus]);
        let mut state = GameState::default();
        state.corp.identity = Some(CardId("nbn_reality_plus".to_string()));
        let tagged = GameEvent::TagsGiven { side: Side::Runner, amount: 1, had: 0 };

        let first = turn_log::record(&mut state, &registry, &tagged);
        assert_eq!(who(&super::plan_for(&state, &registry, &tagged, &first)), vec![("nbn_reality_plus", Heard::AsBystander)]);
        let second = turn_log::record(&mut state, &registry, &tagged);
        assert!(super::plan_for(&state, &registry, &tagged, &second).is_empty());
        // Judged as of its own recording: the first is still the first
        // after the turn has counted a second.
        assert_eq!(super::plan_for(&state, &registry, &tagged, &first).len(), 1);

        turn_log::rotate(&mut state);
        let next_turn = turn_log::record(&mut state, &registry, &tagged);
        assert_eq!(super::plan_for(&state, &registry, &tagged, &next_turn).len(), 1);
    }

    /// Whose occurrence it was is part of the count where the trigger is
    /// phrased about its controller: the Runner's install is not the
    /// Corp's first. And the flag was reset only when the *Corp's* turn
    /// began, so an install by a card's text on the Runner's turn (Brân
    /// 1.0) paid Engineering the Future nothing.
    #[test]
    fn the_first_time_you_install_counts_your_installs_in_either_players_turn() {
        let mut the_future = listens("haas_bioroid_engineering_the_future", Side::Corp, CardType::Identity, Trigger::OnInstall, Some(Subject::Any));
        the_future.triggers[0].first_each_turn = true;
        let registry = registry(vec![the_future]);
        let mut state = GameState::default();
        state.corp.identity = Some(CardId("haas_bioroid_engineering_the_future".to_string()));
        let runners = GameEvent::ProgramInstalled { side: Side::Runner, card: CardId("leech".to_string()), memory_cost: 1, credits_paid: 0 };
        let corps = GameEvent::CardInstalled { side: Side::Corp, install: InstallId(1), card: Some(CardId("pad_campaign".to_string())), server: ServerId::Remote(0), from_hq: true };

        let as_of = turn_log::record(&mut state, &registry, &runners);
        assert!(super::plan_for(&state, &registry, &runners, &as_of).is_empty(), "\"you\" is the Corp");
        let as_of = turn_log::record(&mut state, &registry, &corps);
        assert_eq!(who(&super::plan_for(&state, &registry, &corps, &as_of)), vec![("haas_bioroid_engineering_the_future", Heard::AsBystander)], "the Corp's first, after the Runner's");
        let as_of = turn_log::record(&mut state, &registry, &corps);
        assert!(super::plan_for(&state, &registry, &corps, &as_of).is_empty());
    }

    /// One printed ability in two entries shares one count: "the first
    /// time each turn you steal **or** trash a Corp card" (Cacophony). With
    /// a once-per-turn apiece they shared a use; with a count apiece a
    /// steal after a trash would be a second first time.
    #[test]
    fn a_cards_first_time_entries_share_one_count() {
        let mut cacophony = listens("cacophony", Side::Runner, CardType::Resource, Trigger::OnTrashedFromAccess, Some(Subject::Any));
        let stolen = TriggeredEffect { trigger: Trigger::OnAgendaStolen, ..cacophony.triggers[0].clone() };
        cacophony.triggers.push(stolen);
        cacophony.triggers.iter_mut().for_each(|triggered| triggered.first_each_turn = true);
        let registry = registry(vec![cacophony]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.runner.rig = vec![in_the_rig("cacophony", 1)];

        let trashed = GameEvent::CardTrashedFromAccess { card: CardId("pad_campaign".to_string()), cost_paid: 4, install: None };
        let stole = GameEvent::AgendaStolen { card: CardId("offworld_office".to_string()), agenda_points: 2 };
        let as_of = turn_log::record(&mut state, &registry, &trashed);
        assert_eq!(super::plan_for(&state, &registry, &trashed, &as_of).len(), 1);
        let as_of = turn_log::record(&mut state, &registry, &stole);
        assert!(super::plan_for(&state, &registry, &stole, &as_of).is_empty(), "a steal after a trash is the second time");
    }

    /// "Trashes an **installed** Corp card" is in the trigger condition
    /// (`EventFilter::InstalledCard`), so a trash out of HQ is not the
    /// turn's first of what Aggressive Trendsetting counts. As an
    /// intervening if it would have been, and the installed trash after it
    /// would have found the first time gone.
    #[test]
    fn a_trash_out_of_hq_is_not_the_first_installed_card_trashed() {
        let mut trendsetting = listens("aggressive_trendsetting", Side::Corp, CardType::Agenda, Trigger::OnTrashedFromAccess, Some(Subject::Any));
        trendsetting.triggers[0].when = Some(EventFilter::InstalledCard(crate::dsl::CardFilter::Any));
        trendsetting.triggers[0].first_each_turn = true;
        let registry = registry(vec![trendsetting, listens("pad_campaign", Side::Corp, CardType::Asset, Trigger::OnTurnStart, None)]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.corp.scored_agendas = vec![crate::rules::state::ScoredAgenda { card: CardId("aggressive_trendsetting".to_string()), install_id: InstallId(7), agenda_counters: 0, scored_on_turn: 0, installed_on_scoring_turn: false, advanced_on_scoring_turn: false, as_agenda: None }];

        let trash = |install| GameEvent::CardTrashedFromAccess { card: CardId("pad_campaign".to_string()), cost_paid: 4, install };
        let as_of = turn_log::record(&mut state, &registry, &trash(None));
        assert!(super::plan_for(&state, &registry, &trash(None), &as_of).is_empty(), "out of HQ");
        let as_of = turn_log::record(&mut state, &registry, &trash(Some(InstallId(3))));
        assert_eq!(who(&super::plan_for(&state, &registry, &trash(Some(InstallId(3))), &as_of)), vec![("aggressive_trendsetting", Heard::AsBystander)]);
        let as_of = turn_log::record(&mut state, &registry, &trash(Some(InstallId(4))));
        assert!(super::plan_for(&state, &registry, &trash(Some(InstallId(4))), &as_of).is_empty(), "the second installed card");
    }
}
