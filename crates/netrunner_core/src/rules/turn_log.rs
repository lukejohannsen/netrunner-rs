//! What has happened this turn: one count per kind of moment, kept in one
//! place.
//!
//! A card that asks about the turn — "if you made a successful run this
//! turn", "play only if the Runner made a successful run during their last
//! turn", "if you played an operation this turn" — used to be answered by a
//! field of its own on `GameState`, written by whichever handler produced
//! the fact and reset by a line of its own in `turn::enter_start_of_turn`.
//! Seven fields, each with its own `EffectRequirement`, and each reset on
//! whichever side's turn its first card happened to care about: the
//! Runner's successful-run flag stood through the whole of the Corp's next
//! turn, where the view and the bots' evaluator both read it as true.
//!
//! Here the turn is counted where it is heard. `dispatcher::dispatch_event`
//! is the one door every event a card can hear goes through (a debug build
//! holds the engine to that — `dispatcher::audit`), and
//! `listeners::moments` already says what each event is an occurrence of,
//! so `record` bumps one cell per moment and nothing else in the engine
//! writes a count. `rotate` is the one reset: every turn start, both sides.
//!
//! **Constant size, no heap.** jinteki keeps the turn's events and filters
//! them (`first-event?`, `no-event?`); a `Vec` of events here would be
//! cloned with the `GameState` on every action and thousands of times per
//! search. The log is a flat array of `u8`, `Copy`, a few hundred bytes
//! beside the hundred-odd `CardId` strings a clone already allocates.
//!
//! **The key is the moment, not a list of named facts.** A `TurnFact::
//! SuccessfulRunOnHq` would be `Trigger::OnSuccessfulRunOnHq` coming back
//! one enum over — the variant `TriggeredEffect::when` deleted — and every
//! new kind of occurrence would be a Rust edit for a card with no new
//! mechanic in it. A cell is a `Trigger`, whose moment it was, and a
//! `Class`, the coarse thing the moment was about.
//!
//! **"The first time each turn" is asked here, and only by the scan.** A
//! card says it with one word beside its trigger and its `when`
//! (`TriggeredEffect::first_each_turn`), and what it counts is what it
//! listens for (`Occurrences`). `record` hands back the log as it stood
//! with the event just counted (`AsOf`), and `listeners::plan_for` cannot
//! be called without one, so a trigger is never judged before its own
//! occurrence is counted nor after a nested event has counted a second.
//! It was `requirement: OncePerTurn`, a use limit on the card, which a
//! card arriving mid-turn — or one whose first trigger stood down — found
//! unspent.
//!
//! **A class holds only what both players saw.** The Corp installs
//! facedown and the Runner is not told what an advanced card is, so a
//! count keyed by the type of *that* card would tell the Runner "an agenda
//! was installed this turn". `concealed` names the moments whose card one
//! player did not see, exhaustively, and those are counted as
//! `Kind::Unseen` — so the log needs no masking and rides in a view
//! whole.

use serde::{Deserialize, Serialize};

use crate::cards::CardRegistry;
use crate::dsl::{CardDefinition, CardFilter, CardSubtype, CardType, EventFilter, Hears, IceFacts, Trigger};
use crate::rules::event::GameEvent;
use crate::rules::listeners::{self, About, Moment};
use crate::rules::run::ServerId;
use crate::rules::state::{ArchivedCard, GameState, InstallId, Side};

const TRIGGERS: usize = Trigger::ALL.len();
/// The widest of the four column sets: a card's `Kind`, once for a card
/// that was on the table when it happened and once for one that was not.
const CLASSES: usize = Kind::COUNT * 2;
// The ice's facts are a column set of their own, and must fit the widest.
const _: () = assert!(IceFacts::ALL.len() <= CLASSES);

/// What a counted moment was about, as coarsely as a card in the pool
/// distinguishes — and no finer than both players saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// `installed`: the card was on the table when it happened — a rez,
    /// an install, a card trashed out of a root rather than out of HQ.
    /// Public wherever the moment is: both players see where an access is.
    Card { kind: Kind, installed: bool },
    Server(ServerClass),
    /// A kind of damage, which both players see dealt.
    Damage(crate::dsl::DamageType),
    /// A moment about a piece of ice in a run, counted by what was true of
    /// the ice (`IceFacts`) rather than by its type, which is always ice:
    /// Sipa's "the first time each turn you pass the outermost piece of ice
    /// … after fully breaking it" and The Tungsten Tailor's "the first time
    /// each turn you break a subroutine on a piece of ice with 0 or less
    /// strength" count only those. Public, like the rest: both players saw
    /// the position, the breaks and the strength.
    Ice(IceFacts),
    /// A moment about nothing a card could point at — a phase, a tag.
    Nothing,
}

/// A card's type, or `Unseen` where a player was not shown the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Unseen,
    Agenda,
    Asset,
    Operation,
    Ice,
    Hardware,
    Resource,
    Program,
    Event,
    Identity,
    Upgrade,
    /// A **double** operation or event, apart from the others of its type:
    /// Synchrocyclotron's "the first double operation you play each turn".
    /// Double is a subtype, and the log counts nothing finer than a type,
    /// so this is the one subtype given a column — both players see a card
    /// played, so a double is as public as an operation.
    DoubleOperation,
    DoubleEvent,
    /// A **mandate** operation, the second subtype given a column, for
    /// Sudden Commandment's "if this operation is the first mandate you
    /// played this turn" — as public as a double. No card in the catalog
    /// is both a double and a mandate, so a card has one column
    /// (`of_card` reads double first).
    MandateOperation,
    /// A **virus** program, the third subtype given a column, for
    /// Avgustina Ivanovskaya's "the first time each turn you install a
    /// virus program" — the Runner installs faceup, so as public as any
    /// program. A virus is always a program, so a card has one column.
    VirusProgram,
    /// A **companion** resource, the fourth subtype given a column, for
    /// Keiko's "the first time each turn you install a companion card or
    /// spend credits from an installed companion card" — faceup on the
    /// table, so as public as any resource. The pool's one companion that
    /// is not a resource is Keiko itself, which its own clause does not
    /// count (`docs/roadmap/nsg-card-pool.md`, Known limits).
    CompanionResource,
    /// A **run** event, the fifth subtype given a column, for Swift's "the
    /// first time each turn you play a run event" — played in the open, so
    /// as public as any event. The seventeenth kind, which widened
    /// `Occurrences::columns` from a `u32` to a `u64`.
    RunEvent,
    /// A run event that is also a double (Maintenance Access), counted
    /// apart from both so a card has one column and each of "a double
    /// event" and "a run event" still reads it (`all_of`, `kinds`).
    DoubleRunEvent,
}

impl Kind {
    const COUNT: usize = 18;
    const ALL: [Kind; Kind::COUNT] = [
        Kind::Unseen,
        Kind::Agenda,
        Kind::Asset,
        Kind::Operation,
        Kind::Ice,
        Kind::Hardware,
        Kind::Resource,
        Kind::Program,
        Kind::Event,
        Kind::Identity,
        Kind::Upgrade,
        Kind::DoubleOperation,
        Kind::DoubleEvent,
        Kind::MandateOperation,
        Kind::VirusProgram,
        Kind::CompanionResource,
        Kind::RunEvent,
        Kind::DoubleRunEvent,
    ];

    /// The column a card is counted in: its type, or its type's double,
    /// mandate, virus, companion or run (and a double run event's own).
    fn of_card(definition: &CardDefinition) -> Kind {
        let double = definition.subtypes.contains(&CardSubtype::Double);
        let mandate = definition.subtypes.contains(&CardSubtype::Mandate);
        let virus = definition.subtypes.contains(&CardSubtype::Virus);
        let companion = definition.subtypes.contains(&CardSubtype::Companion);
        let run = definition.subtypes.contains(&CardSubtype::Run);
        match Kind::of(&definition.card_type) {
            Kind::Operation if double => Kind::DoubleOperation,
            Kind::Operation if mandate => Kind::MandateOperation,
            Kind::Event if double && run => Kind::DoubleRunEvent,
            Kind::Event if double => Kind::DoubleEvent,
            Kind::Event if run => Kind::RunEvent,
            Kind::Program if virus => Kind::VirusProgram,
            Kind::Resource if companion => Kind::CompanionResource,
            kind => kind,
        }
    }

    /// Every column a card of `card_type` may be counted in.
    fn all_of(card_type: &CardType) -> Vec<Kind> {
        match Kind::of(card_type) {
            Kind::Operation => vec![Kind::Operation, Kind::DoubleOperation, Kind::MandateOperation],
            Kind::Event => vec![Kind::Event, Kind::DoubleEvent, Kind::RunEvent, Kind::DoubleRunEvent],
            Kind::Program => vec![Kind::Program, Kind::VirusProgram],
            Kind::Resource => vec![Kind::Resource, Kind::CompanionResource],
            kind => vec![kind],
        }
    }

    /// A Runner card's type: every Runner card trashed goes faceup to the
    /// heap, from wherever it was, so both players see what it was.
    fn is_runners(self) -> bool {
        matches!(self, Kind::Hardware | Kind::Resource | Kind::CompanionResource | Kind::Program | Kind::VirusProgram | Kind::Event | Kind::DoubleEvent | Kind::RunEvent | Kind::DoubleRunEvent)
    }

    fn of(card_type: &CardType) -> Kind {
        match card_type {
            CardType::Agenda => Kind::Agenda,
            CardType::Asset => Kind::Asset,
            CardType::Operation => Kind::Operation,
            CardType::Ice(_) => Kind::Ice,
            CardType::Hardware => Kind::Hardware,
            CardType::Resource => Kind::Resource,
            CardType::Program => Kind::Program,
            CardType::Event => Kind::Event,
            CardType::Identity => Kind::Identity,
            CardType::Upgrade => Kind::Upgrade,
        }
    }
}

/// Which server, with the remotes as one: "a remote server" is as fine as
/// a card prints, and a remote's number is not a fixed-size key.
/// `RunnerState::servers_run_this_turn` is the list for a card that needs
/// the server itself (Red Team).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerClass {
    Archives,
    RnD,
    Hq,
    Remote,
}

impl Class {
    fn column(self) -> usize {
        match self {
            Class::Card { kind, installed } => kind as usize + if installed { Kind::COUNT } else { 0 },
            Class::Server(server) => server as usize,
            Class::Damage(kind) => kind as usize,
            Class::Ice(facts) => facts.bits(),
            Class::Nothing => 0,
        }
    }
}

/// Whose moment it was (`listeners::Moment::of`): the third part of a
/// cell's key, because a card counts *its controller's* occurrences — "the
/// first time each turn **you** install" — and a card's type does not say
/// whose it was once the type is `Unseen`. Public like the rest: who
/// installed, who drew, whose turn began.
const WHOSE: usize = 3;

fn whose(of: Option<Side>) -> usize {
    match of {
        None => 0,
        Some(Side::Corp) => 1,
        Some(Side::Runner) => 2,
    }
}

/// Whether the card a `trigger`'s moment is about was hidden from one of
/// the players when it happened, so its type may not be counted.
///
/// Exhaustive, because the answer is a masking decision: a new `Trigger`
/// about a card does not compile until someone says who saw the card.
fn concealed(trigger: Trigger, of: Option<Side>) -> bool {
    match trigger {
        // The Corp installs facedown; the Runner's installs are faceup.
        Trigger::OnInstall => of == Some(Side::Corp),
        // `mask_event_for_player` strikes the advanced card's identity, and
        // the Corp is not shown what is accessed out of R&D.
        Trigger::OnAdvance | Trigger::OnAccessed => true,
        // Played, rezzed, encountered, scored, stolen, forfeited, trashed
        // faceup by the Runner, or a faceup card's ability: on the table.
        Trigger::OnPlay
        | Trigger::OnCardPlayed
        | Trigger::OnCardInstalled
        | Trigger::OnTrashedFromAccess
        | Trigger::OnAgendaScored
        | Trigger::OnAgendaStolen
        | Trigger::OnForfeit
        | Trigger::OnRez
        // Only a rezzed card is derezzed, and it was seen as it was rezzed.
        | Trigger::OnDerez
        | Trigger::OnEncounter
        | Trigger::OnAbilityGainedCredits
        // Broken or bypassed only in an encounter, which only a rezzed
        // piece of ice has.
        | Trigger::OnSubroutineBroken
        | Trigger::OnSubroutineResolved
        | Trigger::OnIceFullyBroken
        | Trigger::OnIceBypassed
        | Trigger::OnEncounterEnded
        // Announced only for an active card, which is rezzed.
        | Trigger::OnWouldBeUninstalled
        // An ability is used on a faceup card, or on one its cost reveals
        // (Tocsin, from HQ).
        | Trigger::OnActionTaken
        | Trigger::OnAbilityUsed => false,
        // An ambush asks for credits face down (Cerebral Overwriter, Esca),
        // seen by the Runner who accessed it and by no spectator.
        Trigger::OnAbilityTookCredits => true,
        // An unrezzed piece of ice is passed without being seen. The log
        // counts a pass by its `IceFacts`, which do not name the card, but
        // a filter on the card itself is refused.
        Trigger::OnIcePassed => true,
        // A root card moves with its rez state, so a facedown one moves
        // unseen. None in the pool is moved facedown — every mover names a
        // rezzed card or moves itself, being active — but nothing in the
        // move says so.
        Trigger::OnCardMoved => true,
        // An operation resolves in the open, and an expendable card is
        // revealed as it is used from HQ.
        Trigger::OnFinishedResolving => false,
        // Not about a card: which action it was is `SameAction`, public.
        Trigger::OnActionFinished => false,
        // A Corp card trashed out of HQ or R&D goes facedown, unseen by
        // the Runner, so the log counts a Corp card's trash without its
        // type. A Runner card's is seen wherever it came from, and counted
        // with it (`seen_anyway`).
        Trigger::OnCardTrashed => true,
        // Not about a card.
        Trigger::OnRunStart
        | Trigger::OnIceApproached
        | Trigger::OnApproachServer
        | Trigger::OnSuccessfulRun
        | Trigger::OnBreach
        | Trigger::OnRunEnded
        | Trigger::OnCreditsSpentOutsidePool
        | Trigger::OnArchivesTurnedFaceup
        | Trigger::OnTurnStart
        | Trigger::OnActionPhaseEnd
        | Trigger::OnDiscardPhaseEnd
        | Trigger::OnBasicDrawAction
        | Trigger::OnTagsGiven
        | Trigger::OnTagRemoved
        | Trigger::OnDamageDealt
        | Trigger::OnCardsTrashedFromHq
        | Trigger::OnCardsTrashedFromRnD
        | Trigger::OnCardsTrashedFromGripOrStack
        | Trigger::OnBadPublicityTaken
        | Trigger::OnDamageAboutToResolve
        | Trigger::OnDamageSuffered
        | Trigger::OnCreditsSpentFromInstalledCard
        | Trigger::OnIdentityFlipped
        | Trigger::OnVirusCountersPurged
        | Trigger::Paid => false,
    }
}

/// The exception to `concealed`, which is decided by the trigger alone:
/// a trashed card of a kind only the Runner has (`Kind::is_runners`) went
/// faceup to the heap, whoever trashed it and from wherever — Boi-tatá's
/// "if you trashed any of your installed cards this turn" is a count by
/// type and place, and the Corp saw both. A piece of ice the Corp
/// installs is seen to be ice, and a Runner card whose ability took
/// credits was faceup.
fn seen_anyway(trigger: Trigger, kind: Kind) -> bool {
    match trigger {
        Trigger::OnCardTrashed => kind.is_runners(),
        // A facedown piece of ice is installed protecting a server, where
        // both players see it is ice (CR 3.4.2) — what it is stays hidden.
        // So the log can tell the Corp's root installs from its ice
        // (`EventFilter::InRoot`).
        Trigger::OnInstall => kind == Kind::Ice,
        // Only a Corp card asks for credits face down; a Runner card that
        // did was on the table faceup.
        Trigger::OnAbilityTookCredits => kind.is_runners(),
        _ => false,
    }
}

fn class_of(registry: &CardRegistry, moment: &Moment) -> Class {
    if let Some(facts) = moment.ice {
        return Class::Ice(facts);
    }
    match &moment.about {
        About::Nothing => Class::Nothing,
        About::Damage(kind) => Class::Damage(*kind),
        // How many is a number, which a fixed table cannot hold.
        About::Cards(_) => Class::Nothing,
        About::Server(ServerId::Archives) => Class::Server(ServerClass::Archives),
        About::Server(ServerId::RnD) => Class::Server(ServerClass::RnD),
        About::Server(ServerId::Hq) => Class::Server(ServerClass::Hq),
        About::Server(ServerId::Remote(_)) => Class::Server(ServerClass::Remote),
        About::Card { card, installed, .. } => {
            let kind = registry.get(card).map_or(Kind::Unseen, Kind::of_card);
            let kind = if concealed(moment.trigger, moment.of) && !seen_anyway(moment.trigger, kind) { Kind::Unseen } else { kind };
            Class::Card { kind, installed: *installed }
        }
    }
}

/// The occurrences a card means when it prints "the first time each turn":
/// a trigger's row, its controller's moments where the trigger is phrased
/// about its controller, and the columns its `when` admits.
///
/// Built from the card's own trigger and filter rather than named a second
/// time in the card file, so "the first time each turn you make a
/// successful run on HQ" is `OnSuccessfulRun`, `when: Server[Hq]` and one
/// word. The price is that a `when` has to be no finer than a `Class`:
/// `meant_by` refuses a subtype, an ice type, or any filter at all on a
/// moment whose card was concealed, and `CardDefinition::validate` turns
/// that refusal into a card file that does not load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Occurrences {
    trigger: Trigger,
    /// `None`: anyone's.
    of: Option<Side>,
    /// A bit per column; `None`: every column.
    columns: Option<u64>,
}

impl Occurrences {
    /// The installs a `Scope::Installing(filter)` effect is about: its
    /// controller's `OnInstall` moments of the filter's types.
    pub(crate) fn installs(filter: &CardFilter, controller: Side) -> Result<Occurrences, String> {
        Occurrences::meant_by(Trigger::OnInstall, Some(&EventFilter::Card(filter.clone())), controller)
    }

    /// The plays a `Scope::Playing(filter)` effect is about: its
    /// controller's `OnPlay` moments of the filter's kinds.
    pub(crate) fn plays(filter: &CardFilter, controller: Side) -> Result<Occurrences, String> {
        Occurrences::meant_by(Trigger::OnPlay, Some(&EventFilter::Card(filter.clone())), controller)
    }

    pub(crate) fn meant_by(trigger: Trigger, when: Option<&EventFilter>, controller: Side) -> Result<Occurrences, String> {
        // A conjunction counts what every part admits: the columns all of
        // them name, of the one player any of them names.
        if let Some(EventFilter::All(parts)) = when {
            let mut all = Occurrences { trigger, of: (trigger.hears() == Hears::OwnSide).then_some(controller), columns: None };
            for part in parts {
                let one = Occurrences::meant_by(trigger, Some(part), controller)?;
                if part.names_whose() {
                    all.of = one.of;
                }
                all.columns = match (all.columns, one.columns) {
                    (Some(a), Some(b)) => Some(a & b),
                    (a, b) => a.or(b),
                };
            }
            return Ok(all);
        }
        // Whose moments: the controller's for a trigger about "you",
        // unless the card names the other player's (`EventFilter::Whose`).
        let of = match when {
            Some(EventFilter::Whose(side) | EventFilter::OwnedBy { whose: side, .. }) => Some(*side),
            Some(EventFilter::InRoot) => Some(Side::Corp),
            // "A player trashes", and "is trashed".
            Some(EventFilter::TrashedFromThisServer | EventFilter::Anyone) => None,
            _ => (trigger.hears() == Hears::OwnSide).then_some(controller),
        };
        let bit = |class: Class| 1u64 << class.column();
        let columns = match when {
            None | Some(EventFilter::Whose(_) | EventFilter::Anyone) => None,
            Some(EventFilter::Server(servers)) => Some(
                servers
                    .iter()
                    .map(|server| match server {
                        ServerId::Archives => bit(Class::Server(ServerClass::Archives)),
                        ServerId::RnD => bit(Class::Server(ServerClass::RnD)),
                        ServerId::Hq => bit(Class::Server(ServerClass::Hq)),
                        ServerId::Remote(_) => bit(Class::Server(ServerClass::Remote)),
                    })
                    .fold(0, |mask, column| mask | column),
            ),
            // Always a central server (CR 10.11.2), and which one is the
            // state's: `first_time_on` writes the turn's mark in before a
            // count is read, so this is only what `validate` checks.
            Some(EventFilter::Mark) => Some(bit(Class::Server(ServerClass::Hq)) | bit(Class::Server(ServerClass::RnD)) | bit(Class::Server(ServerClass::Archives))),
            Some(EventFilter::Damage(kind)) => Some(bit(Class::Damage(*kind))),
            // By owner: a card counted `Unseen` is always a Corp card —
            // installed facedown, advanced, accessed, trashed out of HQ or
            // R&D — so the Corp's are those and every Corp type, and the
            // Runner's the Runner types, which are never concealed.
            Some(EventFilter::OwnedBy { owner, .. }) => Some(
                Kind::ALL
                    .iter()
                    .filter(|kind| match owner {
                        Side::Runner => kind.is_runners(),
                        Side::Corp => !kind.is_runners() && **kind != Kind::Identity,
                    })
                    .map(|kind| bit(Class::Card { kind: *kind, installed: false }) | bit(Class::Card { kind: *kind, installed: true }))
                    .fold(0, |mask, column| mask | column),
            ),
            // A Corp install of ice is counted as ice (`seen_anyway`), so
            // every install it counts unseen went into a root.
            Some(EventFilter::InRoot) => Some(bit(Class::Card { kind: Kind::Unseen, installed: false }) | bit(Class::Card { kind: Kind::Unseen, installed: true })),
            Some(EventFilter::Host) => {
                return Err(format!("the turn counts a {trigger:?} without which card hosted what, so \"the first\" cannot be narrowed to this card's host"));
            }
            Some(EventFilter::InRootOfThisServer) => {
                return Err(format!("the turn counts a {trigger:?} without which server it went into; the copies in a root count those"));
            }
            Some(EventFilter::ByThis) => {
                return Err(format!("the turn counts a {trigger:?} without which object did it, so \"the first\" cannot be narrowed to this card's"));
            }
            // Whether the card was installed is a column; which pile it
            // left otherwise is not.
            Some(EventFilter::TrashedFrom(places)) if places.as_slice() == [crate::dsl::TrashedFrom::Installed] => {
                Some(Kind::ALL.iter().map(|kind| bit(Class::Card { kind: *kind, installed: true })).fold(0, |mask, column| mask | column))
            }
            Some(EventFilter::TrashedFrom(_)) => {
                return Err(format!("the turn counts a {trigger:?} by the card's type and whether it was installed, not which pile it left, so \"the first\" cannot be narrowed to one"));
            }
            Some(EventFilter::All(_)) => unreachable!("a conjunction is read part by part above"),
            Some(EventFilter::InstalledFromHq(_)) => {
                return Err(format!("the turn counts a {trigger:?} without where the card came from, so \"the first\" cannot be narrowed by it"));
            }
            Some(EventFilter::TrashedFromThisServer) => {
                return Err(format!("the turn counts a {trigger:?} without the server the card left, so \"the first\" cannot be narrowed by it"));
            }
            Some(EventFilter::TrashedRezzed) => {
                return Err(format!("the turn counts a {trigger:?} without whether the card was rezzed, so \"the first\" cannot be narrowed by it"));
            }
            Some(EventFilter::InstalledIn(_)) => {
                return Err(format!("the turn counts a {trigger:?} without the server it went into, so \"the first\" cannot be narrowed by it"));
            }
            Some(EventFilter::ChosenServer | EventFilter::ProtectedByIce) => {
                return Err(format!("the turn counts a {trigger:?} by the kind of server, not which one or what protects it, so \"the first\" cannot be narrowed by it"));
            }
            Some(EventFilter::AtLeast(_)) => {
                return Err(format!("the turn counts a {trigger:?} without how many cards it was about, so \"the first\" cannot be narrowed by a number"));
            }
            Some(EventFilter::Ice(required)) => {
                Some(IceFacts::ALL.iter().filter(|facts| required.admits(**facts)).map(|facts| bit(Class::Ice(*facts))).fold(0, |mask, column| mask | column))
            }
            // A moment about ice is counted by its facts, not its type.
            Some(EventFilter::Card(_) | EventFilter::InstalledCard(_)) if trigger.is_about_ice_in_a_run() => {
                return Err(format!("the turn counts a {trigger:?} by what was true of the ice, not by the card, so \"the first\" is narrowed with `Ice`"));
            }
            Some(EventFilter::Card(filter) | EventFilter::InstalledCard(filter))
                if concealed(trigger, of) && !kinds(filter)?.iter().all(|kind| seen_anyway(trigger, *kind)) =>
            {
                return Err(format!("the card a {trigger:?} is about is hidden from a player, so the turn counts it without its type and \"the first\" cannot be narrowed by one"));
            }
            Some(EventFilter::Card(filter)) => Some(kinds(filter)?.iter().map(|kind| bit(Class::Card { kind: *kind, installed: false }) | bit(Class::Card { kind: *kind, installed: true })).fold(0, |mask, column| mask | column)),
            Some(EventFilter::InstalledCard(filter)) => Some(kinds(filter)?.iter().map(|kind| bit(Class::Card { kind: *kind, installed: true })).fold(0, |mask, column| mask | column)),
        };
        Ok(Occurrences { trigger, of, columns })
    }
}

/// What `definition`'s "first time each turn" entries count, together: one
/// printed ability in as many entries as it has triggers. An entry
/// `Occurrences::meant_by` refuses counts nothing — `validate` has already
/// refused the card file. Read on `state`: "on your mark" is the turn's
/// mark (CR 10.11.1a), and no server at all while there is none, so nothing
/// counted is a first time of it.
pub(crate) fn first_time_on(definition: &CardDefinition, state: &crate::rules::GameState) -> Vec<Occurrences> {
    let mark = crate::rules::lingering::mark(state);
    definition
        .triggers
        .iter()
        .filter(|triggered| triggered.first_each_turn)
        .filter_map(|triggered| {
            let when = triggered.when.as_ref().map(|filter| filter.with_mark(mark));
            Occurrences::meant_by(triggered.trigger, when.as_ref(), definition.side).ok()
        })
        .collect()
}

/// The `Kind`s a card filter admits, where it is no finer than one.
fn kinds(filter: &CardFilter) -> Result<Vec<Kind>, String> {
    // "A double operation", "a mandate", "a virus program", "a companion
    // card" (a resource but for Keiko itself): the subtypes
    // the log counts apart.
    if let CardFilter::All(parts) = filter
        && let [first, second] = parts.as_slice()
    {
        let subtype_of = match (first, second) {
            (CardFilter::CardType(card_type), CardFilter::HasSubtype(subtype))
            | (CardFilter::HasSubtype(subtype), CardFilter::CardType(card_type)) => Some((card_type, subtype)),
            _ => None,
        };
        match subtype_of {
            Some((CardType::Operation, CardSubtype::Double)) => return Ok(vec![Kind::DoubleOperation]),
            Some((CardType::Event, CardSubtype::Double)) => return Ok(vec![Kind::DoubleEvent, Kind::DoubleRunEvent]),
            Some((CardType::Event, CardSubtype::Run)) => return Ok(vec![Kind::RunEvent, Kind::DoubleRunEvent]),
            Some((CardType::Operation, CardSubtype::Mandate)) => return Ok(vec![Kind::MandateOperation]),
            Some((CardType::Program, CardSubtype::Virus)) => return Ok(vec![Kind::VirusProgram]),
            Some((CardType::Resource, CardSubtype::Companion)) => return Ok(vec![Kind::CompanionResource]),
            _ => {}
        }
    }
    let card_types = match filter {
        CardFilter::Any => return Ok(Kind::ALL.to_vec()),
        CardFilter::Ice => return Ok(vec![Kind::Ice]),
        CardFilter::CardType(card_type) => std::slice::from_ref(card_type),
        CardFilter::CardTypeOneOf(card_types) => card_types.as_slice(),
        _ => return Err(format!("the turn counts a card by its type and nothing finer, so \"the first\" cannot be narrowed by {filter:?}")),
    };
    if card_types.iter().any(|card_type| matches!(card_type, CardType::Ice(_))) {
        return Err("the turn counts ice as ice, whatever its type".to_string());
    }
    Ok(card_types.iter().flat_map(Kind::all_of).collect())
}

/// The log as it stood when one event had just been counted — what a
/// trigger's "the first time each turn" is judged against.
///
/// Only `record` makes one, and `listeners::plan_for` cannot be called
/// without one: a trigger cannot be judged before its own occurrence is in
/// the count, nor after a nested event (a rider's, the checkpoint's) has
/// added a second. It also keeps the two questions on two types. A trigger
/// asks `is_first` — exactly one, itself — and a price asks
/// `TurnLog::none_yet`, because an install is priced before it happens.
/// No card file writes a 0 or a 1, so none can write the wrong one.
pub(crate) struct AsOf(TurnLog, Option<(InstallId, CopyTurn)>);

impl AsOf {
    /// Whether the occurrence just counted is the first this turn of any
    /// of `triggers` about the copy `install` — "the first time each turn
    /// you advance **this agenda**" — or done by it ("the first time each
    /// turn **this program** fully breaks a piece of ice"). Read off the
    /// copy as it was counted, for the one install the event was about or
    /// whose abilities did it.
    pub(crate) fn is_first_on(&self, install: Option<InstallId>, turn: u32, triggers: &[Trigger]) -> bool {
        let Some((counted, copy)) = self.1 else { return false };
        install == Some(counted) && triggers.iter().map(|trigger| copy.count(turn, *trigger)).sum::<u32>() == 1
    }

    /// Whether the occurrence just counted is the turn's first of `meant`
    /// — several where one printed ability is two entries ("an agenda is
    /// scored **or** stolen").
    pub(crate) fn is_first(&self, meant: &[Occurrences]) -> bool {
        meant.iter().map(|occurrences| self.0.count(occurrences)).sum::<u32>() == 1
    }

    /// For a test that plans an event it never recorded.
    #[cfg(test)]
    pub(crate) fn unrecorded(state: &GameState) -> AsOf {
        AsOf(state.this_turn, None)
    }
}

/// What has happened to one Corp install this turn, or what one rig install
/// has done (`CopyTurn::counts_by`): for each trigger, whether a moment
/// about this copy — or by it — has been heard once, or more than once —
/// all "the first time each turn … **this** card" asks. The turn's log
/// counts classes (a card's kind, a server's) and has no column for one
/// copy, and a count per install on `GameState` would be a table to keep in
/// step with the installs; this rides on the install and leaves with it.
///
/// **Dated, never reset**: the counts are of `turn`, and read as none on
/// any other, as `ScoredAgenda::scored_on_turn` is — so no turn start has a
/// line for it. Public, as advancing and rezzing a card are: which copy was
/// advanced is on the table even when the card is not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyTurn {
    turn: u32,
    /// A bit per `Trigger::index`.
    once: u64,
    twice: u64,
}

// A bit per trigger.
const _: () = assert!(Trigger::ALL.len() <= u64::BITS as usize);

impl CopyTurn {
    /// Whether moments of `trigger` are counted on the copy: those a card
    /// asks of one copy, which is advancing it (Sacrifice Zone Expansion's
    /// "the first time each turn") and rezzing it (Cloud Eater's "if it was
    /// rezzed this turn", `Amount::TimesThisTurnOnThisCopy`) and, so far,
    /// nothing else. A Corp install's `OnInstall` is counted apart, and
    /// means the installs into the root it is in (`record`, for
    /// `EventFilter::InRootOfThisServer`), never its own alone. A card
    /// leaves the table when it is scored, stolen or
    /// trashed, so its copy could never count those; `validate` refuses the
    /// rest until a card prints one.
    pub(crate) fn counts(trigger: Trigger) -> bool {
        matches!(trigger, Trigger::OnAdvance | Trigger::OnRez)
    }

    /// Whether moments of `trigger` are counted on the copy that *did*
    /// them (`Moment::by`, a rig install): fully breaking a piece of ice
    /// (Abaasy's "the first time each turn this program fully breaks a
    /// piece of ice", CR 6.5.7b), and nothing else a card asks yet.
    pub(crate) fn counts_by(trigger: Trigger) -> bool {
        matches!(trigger, Trigger::OnIceFullyBroken)
    }

    /// Whether moments of `trigger` about a rig copy are counted on it:
    /// installing it, for Euler's "Use this ability only if this program
    /// was installed this turn" (`CardFilter::InstalledThisTurn` on the
    /// acting card, `pending_choice::copy_matches`). The Corp's installs
    /// keep `InstalledCard::installed_this_turn` instead, which a view
    /// carries; a rig copy's count is dated like the rest, and a card
    /// installed again is a new copy with a fresh one.
    pub(crate) fn counts_on_rig(trigger: Trigger) -> bool {
        matches!(trigger, Trigger::OnInstall)
    }

    fn bump(&mut self, turn: u32, trigger: Trigger) {
        if self.turn != turn {
            *self = CopyTurn { turn, ..CopyTurn::default() };
        }
        let bit = 1u64 << trigger.index();
        if self.once & bit != 0 {
            self.twice |= bit;
        }
        self.once |= bit;
    }

    /// 0, 1, or 2 for "more than once": `turn`'s moments of `trigger`
    /// about this copy. Public for the bots, which ask whether a ready
    /// agenda was advanced this turn as Issuaq Adaptics' scoring trigger
    /// will (Phase 5 §37).
    pub fn count(&self, turn: u32, trigger: Trigger) -> u32 {
        if self.turn != turn {
            return 0;
        }
        let bit = 1u64 << trigger.index();
        u32::from(self.once & bit != 0) + u32::from(self.twice & bit != 0)
    }
}

/// One turn's counts. See the module doc.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Sparse", into = "Sparse")]
pub struct TurnLog {
    counts: [[[u8; CLASSES]; WHOSE]; TRIGGERS],
    /// Actions the active side has *finished* this turn — every basic click
    /// action and run, not scoring (which is not an action) and not a click
    /// spent as a paid-ability cost. Petty Cash's "play only if you have
    /// not finished an action yet this turn". Finishing an action is not an
    /// event any card hears, so `engine::apply_action` records it; and it
    /// is not derivable from clicks, because Petty Cash itself refunds the
    /// click it cost when played from Archives, so "clicks still at the
    /// turn's starting value" would let a second copy follow the first.
    actions_finished: u8,
    /// The printed agenda points on agendas scored this turn — a sum, where
    /// every cell above is a count (Neurospike).
    agenda_points_scored: u8,
    /// The Corp's installs this turn of a card out of HQ — The Holo Man's
    /// "If you have not installed any cards from HQ this turn". A count
    /// beside the cells, not a column of them: where a card came from is
    /// no `Class`, and every Corp install from HQ is facedown, so its
    /// cells are `Unseen` anyway. Public all the same, as the card's
    /// leaving HQ is.
    installed_from_hq: u8,
    /// The Corp's installs this turn in the root of or protecting a remote
    /// server — A Teia: IP Recovery's "The first time each turn you install
    /// a card in the root of or protecting a remote server". A count beside
    /// the cells, as `installed_from_hq` is: which server a card went into
    /// is no `Class`. Public, as where the Corp installs is.
    installed_in_remotes: u8,
    /// The times the Runner gained [click] during a run this turn —
    /// Pichação's "If this is not the first time you gained [click] during
    /// a run this turn". A count beside the cells, as `installed_from_hq`
    /// is: gaining a click is a moment no card hears, and "during a run" is
    /// no `Class`. Public, as the gain is. Recorded where the click is
    /// gained (`Effect::GainClicks`), the one way a card gives one.
    click_gains_in_runs: u8,
    /// Corp cards added to Archives this turn, by any route — Regenesis's
    /// "if no Corp cards have been added to Archives this turn". A count
    /// beside the cells, as `installed_from_hq` is: no one event is common
    /// to every way into Archives (a trash, a discard at the end of the
    /// Corp's turn — which is dispatched to nobody — an operation filed
    /// after it resolves, a card a prompt sends there), so it is bumped by
    /// the one door every addition goes through, `file_in_archives`.
    /// Public, as a card's arriving in Archives is, faceup or not.
    added_to_archives: u8,
    /// The times each action was taken this turn, by what makes two
    /// actions the same (`SameAction`, CR 5.2.5a) — Wage Workers' "if you
    /// have taken that action exactly 3 times this turn". Beside the cells,
    /// as `actions_finished` is: an action is no `Class`, and a basic one
    /// was no moment at all. A fixed table so the log stays `Copy`; a
    /// turn of more than `SAME_ACTIONS` different actions stops counting
    /// the new ones, which no turn in the pool comes near. **Kept sorted**,
    /// so that two turns that took the same actions in another order are
    /// one log and one `GameState` — the table is a multiset, and a search
    /// that folds two orders of the same clicks into one position compares
    /// the states whole (`netrunner_bots::planner::prune`, Phase 5 §26).
    same_actions: [Option<(SameAction, u8)>; SAME_ACTIONS],
}

/// How many different actions one turn's log tells apart.
const SAME_ACTIONS: usize = 12;

/// Which action a player took, as the rules tell two apart: "the same if
/// they are all the same basic action or if all of those actions were
/// initiated by the same ability from the same card" (CR 5.2.5a). Every
/// install is the one basic action, whatever it installs, and every play
/// of an operation or event is another; two copies' abilities are
/// different actions (CR 5.2.5b), so an ability is its card's handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SameAction {
    GainCredit,
    Draw,
    Install,
    Play,
    Advance,
    TrashResource,
    Purge,
    RemoveTag,
    Run,
    /// A [click] ability of an installed card, an identity or a scored
    /// agenda, by its handle and which of its abilities.
    Ability { install: InstallId, index: u8 },
    /// A [click] ability used from HQ or the grip (Descent, Tocsin), by
    /// the printing its card file was built from (`built_from`, the same
    /// number for every copy): a card in hand has no handle, so two
    /// copies there are one card to this count. Each is revealed by its
    /// own cost, so the number is public.
    FromHand { card: u32, index: u8 },
}

impl Default for TurnLog {
    fn default() -> Self {
        TurnLog {
            counts: [[[0; CLASSES]; WHOSE]; TRIGGERS],
            actions_finished: 0,
            agenda_points_scored: 0,
            installed_from_hq: 0,
            installed_in_remotes: 0,
            click_gains_in_runs: 0,
            added_to_archives: 0,
            same_actions: [None; SAME_ACTIONS],
        }
    }
}

impl std::fmt::Debug for TurnLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The cells that are not zero: a derived `Debug` prints three
        // hundred of them into every failed `assert_eq!` on a state.
        std::fmt::Debug::fmt(&Sparse::from(*self), f)
    }
}

impl TurnLog {
    /// How many times `trigger`'s moment has happened, whatever it was
    /// about.
    pub fn times(&self, trigger: Trigger) -> u32 {
        self.counts[trigger.index()].iter().flatten().map(|count| u32::from(*count)).sum()
    }

    /// How many of those were about `class`.
    pub fn times_about(&self, trigger: Trigger, class: Class) -> u32 {
        self.counts[trigger.index()].iter().map(|row| u32::from(row[class.column()])).sum()
    }

    pub fn actions_finished(&self) -> u32 {
        u32::from(self.actions_finished)
    }

    /// How many times `action` was taken this turn (CR 5.2.5a).
    pub fn times_taken(&self, action: SameAction) -> u32 {
        self.same_actions.iter().flatten().find(|(taken, _)| *taken == action).map_or(0, |(_, count)| u32::from(*count))
    }

    fn take(&mut self, action: SameAction) {
        if let Some((_, count)) = self.same_actions.iter_mut().flatten().find(|(taken, _)| *taken == action) {
            *count = count.saturating_add(1);
        } else if let Some(free) = self.same_actions.iter_mut().find(|slot| slot.is_none()) {
            *free = Some((action, 1));
            // Canonical order (the field's doc): the entries sorted, the
            // empty slots after them — the order `Sparse` writes and reads
            // back, so a log equals its own round trip — and the free slot
            // is still the first `None`.
            self.same_actions.sort_unstable_by(|a, b| match (a, b) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });
        }
    }

    pub fn agenda_points_scored(&self) -> u32 {
        u32::from(self.agenda_points_scored)
    }

    pub fn installed_from_hq(&self) -> u32 {
        u32::from(self.installed_from_hq)
    }

    pub fn installed_in_remotes(&self) -> u32 {
        u32::from(self.installed_in_remotes)
    }

    pub fn click_gains_in_runs(&self) -> u32 {
        u32::from(self.click_gains_in_runs)
    }

    pub fn added_to_archives(&self) -> u32 {
        u32::from(self.added_to_archives)
    }

    /// How many of `trigger`'s moments this turn its `when` admits, as a
    /// card on `controller`'s side means them (`Occurrences::meant_by`);
    /// 0 for a filter finer than the log counts, which `validate` refuses.
    pub(crate) fn times_when(&self, trigger: Trigger, when: &EventFilter, controller: Side) -> u32 {
        Occurrences::meant_by(trigger, Some(when), controller).map_or(0, |occurrences| self.count(&occurrences))
    }

    fn count(&self, occurrences: &Occurrences) -> u32 {
        let rows = &self.counts[occurrences.trigger.index()];
        let mut total = 0;
        for (side, row) in rows.iter().enumerate() {
            // A moment that is nobody's counts for anyone who asks.
            if occurrences.of.is_some_and(|of| side != whose(None) && side != whose(Some(of))) {
                continue;
            }
            for (column, count) in row.iter().enumerate() {
                if occurrences.columns.is_none_or(|mask| mask & (1 << column) != 0) {
                    total += u32::from(*count);
                }
            }
        }
        total
    }

    /// Whether none of `occurrences` has happened yet this turn — the
    /// question a price asks ("the **first** program you install costs 1
    /// less"), before the install it is pricing is counted.
    pub(crate) fn none_yet(&self, occurrences: &Occurrences) -> bool {
        self.count(occurrences) == 0
    }

    fn bump(&mut self, trigger: Trigger, of: Option<Side>, class: Class) {
        let cell = &mut self.counts[trigger.index()][whose(of)][class.column()];
        *cell = cell.saturating_add(1);
    }
}

/// `TurnLog` on the wire and in a failed assertion: the cells that are not
/// zero. A `StateUpdate` would otherwise carry three hundred zeros, and
/// serde derives arrays only to 32.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sparse {
    /// `(trigger, whose, column, count)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    cells: Vec<(Trigger, u8, u8, u8)>,
    #[serde(default, skip_serializing_if = "is_zero")]
    actions_finished: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    agenda_points_scored: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    installed_from_hq: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    installed_in_remotes: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    click_gains_in_runs: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    added_to_archives: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    same_actions: Vec<(SameAction, u8)>,
}

fn is_zero(count: &u8) -> bool {
    *count == 0
}

impl From<TurnLog> for Sparse {
    fn from(log: TurnLog) -> Self {
        let mut cells = Vec::new();
        for trigger in Trigger::ALL {
            for (whose, row) in log.counts[trigger.index()].iter().enumerate() {
                for (column, count) in row.iter().enumerate() {
                    if *count > 0 {
                        cells.push((trigger, whose as u8, column as u8, *count));
                    }
                }
            }
        }
        Sparse {
            cells,
            actions_finished: log.actions_finished,
            agenda_points_scored: log.agenda_points_scored,
            installed_from_hq: log.installed_from_hq,
            installed_in_remotes: log.installed_in_remotes,
            click_gains_in_runs: log.click_gains_in_runs,
            added_to_archives: log.added_to_archives,
            same_actions: log.same_actions.iter().flatten().copied().collect(),
        }
    }
}

impl From<Sparse> for TurnLog {
    fn from(sparse: Sparse) -> Self {
        let mut log = TurnLog {
            actions_finished: sparse.actions_finished,
            agenda_points_scored: sparse.agenda_points_scored,
            installed_from_hq: sparse.installed_from_hq,
            installed_in_remotes: sparse.installed_in_remotes,
            click_gains_in_runs: sparse.click_gains_in_runs,
            added_to_archives: sparse.added_to_archives,
            ..TurnLog::default()
        };
        for (slot, taken) in log.same_actions.iter_mut().zip(sparse.same_actions) {
            *slot = Some(taken);
        }
        for (trigger, whose, column, count) in sparse.cells {
            // A cell off the end is a log written by some other build;
            // dropping it beats a panic in a deserializer.
            if let Some(cell) = log.counts[trigger.index()].get_mut(usize::from(whose)).and_then(|row| row.get_mut(usize::from(column))) {
                *cell = count;
            }
        }
        log
    }
}

/// Counts `event`'s moments. Called by `dispatcher::dispatch_event` before
/// anything reacts, so a card asking about the turn while it reacts to an
/// occurrence finds that occurrence already counted.
pub(crate) fn record(state: &mut GameState, registry: &CardRegistry, event: &GameEvent) -> AsOf {
    let mut copy = None;
    for moment in listeners::moments(state, event) {
        let class = class_of(registry, &moment);
        state.this_turn.bump(moment.trigger, moment.of, class);
        // And on the Corp install the moment is about, if it is one.
        if let About::Card { install: Some(install), installed: true, .. } = moment.about
            && CopyTurn::counts(moment.trigger)
        {
            let turn = state.turn;
            if let Some(installed) = state.corp.installed.iter_mut().find(|installed| installed.install_id == install) {
                installed.this_turn.bump(turn, moment.trigger);
                copy = Some((install, installed.this_turn));
            }
        }
        // And on every card in the root a Corp card went into, the card
        // itself included: the installs into a root this turn, which
        // Tranquility Home Grid's first time reads off its own copy
        // (`EventFilter::InRootOfThisServer`). The copies in the root are
        // the ones that can hear it, and a card installed later saw its
        // own install, which is all "the first" needs of the ones before.
        if moment.trigger == Trigger::OnInstall
            && moment.of == Some(crate::rules::Side::Corp)
            && let Some(server) = moment.installed_in
            && let About::Card { card, .. } = &moment.about
            && registry.get(card).is_some_and(|definition| !matches!(definition.card_type, crate::dsl::CardType::Ice(_)))
        {
            let turn = state.turn;
            for installed in state.corp.installed.iter_mut().filter(|installed| installed.server == server && installed.slot == crate::rules::state::InstallSlot::Root) {
                installed.this_turn.bump(turn, Trigger::OnInstall);
            }
        }
        // And on the rig install it is about, for what a rig copy is
        // asked about itself.
        if let About::Card { install: Some(install), installed: true, .. } = moment.about
            && CopyTurn::counts_on_rig(moment.trigger)
        {
            let turn = state.turn;
            if let Some(installed) = state.runner.rig.iter_mut().find(|installed| installed.install_id == install) {
                installed.this_turn.bump(turn, moment.trigger);
            }
        }
        // And on the rig install that did it.
        if let Some(by) = moment.by
            && CopyTurn::counts_by(moment.trigger)
        {
            let turn = state.turn;
            if let Some(installed) = state.runner.rig.iter_mut().find(|installed| installed.install_id == by) {
                installed.this_turn.bump(turn, moment.trigger);
                copy = Some((by, installed.this_turn));
            }
        }
    }
    // The sums. The points are on the event, so this is still the one
    // door: an agenda scored by a card's text is counted like any other.
    if let GameEvent::AgendaScored { agenda_points, .. } = event {
        let points = u8::try_from(*agenda_points).unwrap_or(u8::MAX);
        state.this_turn.agenda_points_scored = state.this_turn.agenda_points_scored.saturating_add(points);
    }
    if let GameEvent::CardInstalled { side: Side::Corp, from_hq: true, .. } = event {
        state.this_turn.installed_from_hq = state.this_turn.installed_from_hq.saturating_add(1);
    }
    if let GameEvent::CardInstalled { side: Side::Corp, server: ServerId::Remote(_), .. } = event {
        state.this_turn.installed_in_remotes = state.this_turn.installed_in_remotes.saturating_add(1);
    }
    AsOf(state.this_turn, copy)
}

/// The Runner gained [click] during a run — see
/// `TurnLog::click_gains_in_runs`.
pub(crate) fn record_click_gain_in_a_run(state: &mut GameState) {
    state.this_turn.click_gains_in_runs = state.this_turn.click_gains_in_runs.saturating_add(1);
}

/// A Corp card is added to Archives — the one door into the zone, so
/// `TurnLog::added_to_archives` cannot miss a route. A source scan holds
/// every other push out of the engine (`tests::archives_has_one_door`).
pub(crate) fn file_in_archives(state: &mut GameState, card: ArchivedCard) {
    state.corp.archives.push(card);
    state.this_turn.added_to_archives = state.this_turn.added_to_archives.saturating_add(1);
}

/// The operation at `position` in Archives leaves for somewhere else as
/// its own text resolves (Backroom Machinations' "Add this operation to
/// your score area"). The engine files a played operation in Archives
/// before its text resolves (`engine::play_operation_card`), where the
/// rules trash it only once it has resolved (CR 8.2.7) — so this one was
/// never added to Archives, and the count gives it back.
pub(crate) fn unfile_resolving_operation(state: &mut GameState, position: usize) -> ArchivedCard {
    state.this_turn.added_to_archives = state.this_turn.added_to_archives.saturating_sub(1);
    state.corp.archives.remove(position)
}

/// An action was finished — see `TurnLog::actions_finished` and
/// `TurnLog::times_taken`.
pub(crate) fn record_action_finished(state: &mut GameState, action: SameAction) {
    state.this_turn.actions_finished = state.this_turn.actions_finished.saturating_add(1);
    state.this_turn.take(action);
}

/// A turn begins: this turn becomes last turn. The one reset, for both
/// sides at every turn start — five fields each had a line of their own in
/// `turn::enter_start_of_turn`, under whichever side's branch their first
/// card cared about.
///
/// **Whose turn "last turn" was is the rotation's:** it is the turn that
/// ended most recently, either side's, so on the Corp's turn it is the
/// Runner's — which is when an operation printed "during their last turn"
/// (Public Trail, Active Policing) can be played at all. The whole log is
/// kept: it was a row of totals, `LastTurn`, while no card asked a last
/// turn anything narrower than "made a successful run", and Active
/// Policing's "stole or trashed **a Corp card**" is a column of it — the
/// Runner's own trashes are in the same row.
pub(crate) fn rotate(state: &mut GameState) {
    state.last_turn = state.this_turn;
    state.this_turn = TurnLog::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardDefinition, CardId};
    use crate::rules::state::InstallId;

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::default();
        registry.insert(CardDefinition { id: CardId("an_agenda".into()), side: Side::Corp, card_type: CardType::Agenda, ..CardDefinition::default() });
        registry.insert(CardDefinition { id: CardId("a_program".into()), side: Side::Runner, card_type: CardType::Program, ..CardDefinition::default() });
        registry
    }

    #[test]
    fn a_moment_is_counted_by_what_it_was_about_and_a_turn_start_makes_it_last_turn() {
        let registry = registry();
        let mut state = GameState::default();
        record(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::Hq });
        record(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::Remote(3) });
        record(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::Remote(4) });
        assert_eq!(state.this_turn.times(Trigger::OnSuccessfulRun), 3);
        assert_eq!(state.this_turn.times_about(Trigger::OnSuccessfulRun, Class::Server(ServerClass::Hq)), 1);
        assert_eq!(state.this_turn.times_about(Trigger::OnSuccessfulRun, Class::Server(ServerClass::Remote)), 2);
        assert_eq!(state.this_turn.times(Trigger::OnRunStart), 0);
        // An event no card can hear is an occurrence of nothing.
        record(&mut state, &registry, &GameEvent::CardDrawn { side: Side::Runner });
        assert_eq!(Sparse::from(state.this_turn).cells.len(), 2);

        record_action_finished(&mut state, SameAction::GainCredit);
        record(&mut state, &registry, &GameEvent::AgendaScored { card: CardId("an_agenda".into()), agenda_points: 3, server: ServerId::Remote(0) });
        assert_eq!((state.this_turn.actions_finished(), state.this_turn.agenda_points_scored()), (1, 3));
        // The same action is the same basic action, or the same ability on
        // the same card (CR 5.2.5a).
        record_action_finished(&mut state, SameAction::GainCredit);
        record_action_finished(&mut state, SameAction::Ability { install: InstallId(7), index: 0 });
        record_action_finished(&mut state, SameAction::Ability { install: InstallId(8), index: 0 });
        assert_eq!(state.this_turn.times_taken(SameAction::GainCredit), 2);
        assert_eq!(state.this_turn.times_taken(SameAction::Ability { install: InstallId(7), index: 0 }), 1);
        assert_eq!(state.this_turn.times_taken(SameAction::Draw), 0);
        assert_eq!(state.this_turn.actions_finished(), 4);
        // Two turns that took the same actions in another order are one
        // log: the table is a multiset, and a search that folds two orders
        // of the same clicks into one position compares the states whole.
        let mut other = GameState::new(0);
        for action in [SameAction::Ability { install: InstallId(8), index: 0 }, SameAction::GainCredit, SameAction::Ability { install: InstallId(7), index: 0 }, SameAction::GainCredit] {
            record_action_finished(&mut other, action);
        }
        record(&mut other, &registry, &GameEvent::AgendaScored { card: CardId("an_agenda".into()), agenda_points: 3, server: ServerId::Remote(0) });
        assert_eq!(other.this_turn.same_actions, state.this_turn.same_actions);
        rotate(&mut state);
        assert_eq!(state.this_turn, TurnLog::default());
        assert_eq!(state.last_turn.times(Trigger::OnSuccessfulRun), 3);
        rotate(&mut state);
        assert_eq!(state.last_turn, TurnLog::default());
    }

    /// The fog rule of the module doc: the Runner's program is counted as a
    /// program, the Corp's facedown agenda as a card nobody saw.
    #[test]
    fn a_corp_install_is_counted_without_its_type() {
        let registry = registry();
        let mut state = GameState::default();
        let corp = GameEvent::CardInstalled {
            side: Side::Corp,
            install: InstallId(1),
            card: Some(CardId("an_agenda".into())),
            server: ServerId::Remote(0), from_hq: true,
        };
        record(&mut state, &registry, &corp);
        assert_eq!(state.this_turn.times_about(Trigger::OnInstall, Class::Card { kind: Kind::Unseen, installed: true }), 1);
        assert_eq!(state.this_turn.times_about(Trigger::OnInstall, Class::Card { kind: Kind::Agenda, installed: true }), 0);
    }

    #[test]
    fn a_log_survives_serde_and_an_empty_one_is_written_as_nothing() {
        let registry = registry();
        let mut state = GameState::default();
        assert_eq!(serde_json::to_string(&state.this_turn).unwrap(), "{}");
        record(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::Archives });
        record_action_finished(&mut state, SameAction::Run);
        record_action_finished(&mut state, SameAction::FromHand { card: 34125, index: 0 });
        let read: TurnLog = serde_json::from_str(&serde_json::to_string(&state.this_turn).unwrap()).unwrap();
        assert_eq!(read, state.this_turn);
        rotate(&mut state);
        let read: TurnLog = serde_json::from_str(&serde_json::to_string(&state.last_turn).unwrap()).unwrap();
        assert_eq!(read, state.last_turn);
    }

    #[test]
    fn a_card_filed_in_archives_is_counted_until_the_turn_ends() {
        let mut state = GameState::default();
        file_in_archives(&mut state, ArchivedCard::facedown(CardId("an_agenda".into())));
        file_in_archives(&mut state, ArchivedCard::faceup(CardId("an_agenda".into())));
        assert_eq!((state.corp.archives.len(), state.this_turn.added_to_archives()), (2, 2));
        let read: TurnLog = serde_json::from_str(&serde_json::to_string(&state.this_turn).unwrap()).unwrap();
        assert_eq!(read, state.this_turn);
        // An operation the engine filed early that leaves as it resolves
        // was never added (CR 8.2.7).
        unfile_resolving_operation(&mut state, 1);
        assert_eq!((state.corp.archives.len(), state.this_turn.added_to_archives()), (1, 1));
        rotate(&mut state);
        assert_eq!((state.this_turn.added_to_archives(), state.last_turn.added_to_archives()), (0, 1));
    }

    /// `file_in_archives` is the one door into Archives: a push anywhere
    /// else in the engine is a way in that `TurnLog::added_to_archives`
    /// would not count, and Regenesis would fire after it.
    #[test]
    fn archives_has_one_door() {
        fn scan(dir: &std::path::Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    scan(&path, found);
                // This file holds the door, and this test's own patterns.
                } else if path.extension().is_some_and(|extension| extension == "rs") && !path.ends_with("rules/turn_log.rs") {
                    let source = std::fs::read_to_string(&path).unwrap();
                    // Tests too: a fixture builds Archives by assignment.
                    for (number, line) in source.lines().enumerate() {
                        let pushes = ["corp.archives.push(", "corp.archives.extend(", "corp.archives.insert(", "corp.archives.append("].iter().any(|push| line.contains(push));
                        if pushes {
                            found.push(format!("{}:{}", path.display(), number + 1));
                        }
                    }
                }
            }
        }
        let mut found = Vec::new();
        scan(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
        assert!(found.is_empty(), "a way into Archives around `turn_log::file_in_archives`: {found:?}");
    }
}
