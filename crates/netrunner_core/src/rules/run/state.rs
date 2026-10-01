use serde::{Deserialize, Serialize};

use crate::rules::state::InstallId;

use crate::dsl::{CardId, CardSubtype, Cost, Effect, IceType, SubroutineDef};
use crate::rules::state::Side;

/// Which Corp zone/server a run targets. Central servers are singletons;
/// Remote servers are numbered since multiple can exist simultaneously.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerId {
    Hq,
    RnD,
    Archives,
    Remote(u32),
}

/// Where a run stands: the Comprehensive Rules' phases (6.9), with the
/// approach of the server kept as the entry to `Success` and the breach as
/// `AccessingCard`. The steps inside a phase — the rez window, resolving
/// subroutines, the paid-ability windows — are `RunAction`-driven
/// transitions and `PaidAbilityWindow`s within it, not further variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunPhase {
    Initiation,
    ApproachIce,
    EncounterIce,
    /// CR 6.9.4, between one piece of ice and whatever is next: entered from
    /// every pass (`run::engine::pass_current_ice`), from an initiation with
    /// no ice to approach, and when the ice being approached or encountered
    /// leaves the table. `position` already names the next ice inward, or
    /// `ice.len()` when the server is next.
    ///
    /// It has two moments and `RunState::jack_out_permitted` says which,
    /// so nothing new is stored: while it is `true` the Runner owes the
    /// decision to jack out or go on (6.9.4c), after a paid-abilities-only
    /// window if the Corp has a paid ability to use (6.9.4b); `ContinueRun`
    /// shuts it and opens the paid-ability window in which the Corp may rez
    /// what is not ice (6.9.4e); and when that window closes the Runner
    /// approaches the next ice or the server (6.9.4f–g). Without this phase the pass *was*
    /// the next approach, which put the jack-out decision after the rez it
    /// exists to precede, and a server with no ice was approached in the
    /// action that began the run — before its upgrades could be rezzed.
    Movement,
    /// Resolving accessed cards one at a time via `PlayerAction::
    /// StealAgenda`/`TrashAccessedCard`/`PassAccessedCard`. Entered from
    /// `Success` once `PlayerAction::CompleteRun` finds a non-empty access
    /// list (see `run::access_server`); `RunState::access_state` is `Some`
    /// throughout. Treated the same as `Success`/`Ended` by `advance_run`'s
    /// "already concluded" guard — none of `ContinueRun`/`JackOut`/
    /// `BreakSubroutine`/`ResolveSubroutine` apply here.
    AccessingCard,
    Success,
    Ended,
}

/// Where one `EncounteredSubroutine` sits in its handling lifecycle.
/// `Pending` blocks `continue_run` from passing this ICE — see
/// `RunPhase::EncounterIce`'s gate in `run::engine::continue_run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubroutineStatus {
    Pending,
    Broken,
    Resolved,
}

/// One subroutine on the ICE currently being encountered, individually
/// addressable by `id` (its index within `RunIce::subroutines`, kept equal
/// to it when a subroutine is gained ahead of the rest). `status` tracks
/// whether the Runner broke it, let it fire (`Resolved`), or hasn't
/// handled it yet (`Pending`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncounteredSubroutine {
    pub id: usize,
    pub definition: SubroutineDef,
    pub status: SubroutineStatus,
    /// Gained for this encounter (`Effect::GainSubroutine`), not printed:
    /// it goes when the encounter does (`run::engine::enter_movement`).
    /// Public, like the rest of the list: the Runner is facing it.
    pub gained: bool,
}

/// One entry of `RunState::gained_for_the_run`: which ice, what it
/// gained, and on which side of its other subroutines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GainedForTheRun {
    pub ice: InstallId,
    pub subroutine: SubroutineDef,
    /// After the ice's other subroutines (CR 9.8.3e) rather than before
    /// them (9.8.3a).
    pub after: bool,
}

/// Counts for one encounter, about the ice being encountered — Hammer's
/// "cannot break more than 1 of its printed subroutines" and Sorocaban
/// Blade's "cannot trash more than 1 installed Runner card with this ice".
/// Public: both players watched each break and each trash.
///
/// Not `Copy` since Slap Vandal: the abilities used this encounter are a
/// set of keys, as the run's are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncounterTally {
    /// Printed subroutines broken by a breaker the ice's `BreakLimit` does
    /// not except.
    pub limited_breaks: u32,
    /// Installed Runner cards trashed by this ice's text.
    pub runner_cards_trashed: u32,
    /// Whose abilities broke this encounter's subroutines — what CR
    /// 6.5.7b asks when the ice is fully broken: "if all its subroutines
    /// were broken using abilities on a single object, that object also
    /// fully breaks the ice" (Lobisomem's "whenever it fully breaks a code
    /// gate"). A tally rather than a mark on each subroutine, because the
    /// one question is whether every break was one install's.
    #[serde(default, skip_serializing_if = "BrokenBy::is_nothing")]
    pub broken_by: BrokenBy,
    /// Which kinds of icebreaker broke one of the ice's *printed*
    /// subroutines this encounter — Virtual Service Agent's "if they did
    /// not break its printed subroutine with a **decoder** during that
    /// encounter". Carried out of the encounter on the pass that ends it
    /// (`GameEvent::IcePassed::printed_broken_with`), because the tally is
    /// reset as the movement phase begins and the pass's triggers resolve
    /// after that.
    #[serde(default, skip_serializing_if = "BrokenWith::is_empty")]
    pub printed_broken_with: BrokenWith,
    /// The abilities printed "Use this ability only once per encounter"
    /// already used this encounter (`EffectRequirement::OncePerEncounter`)
    /// — Slap Vandal's — keyed by card and copy as `RunState::
    /// once_per_run_used` is. On the tally because the tally's reset is the
    /// encounter's end, so there is no clearing site of its own.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub once_per_encounter_used: std::collections::BTreeSet<crate::rules::state::OncePerTurnKey>,
    /// Subroutines broken this encounter, by anything — what Flux
    /// Capacitor's "the **first time** you break a subroutine during each
    /// encounter with host ice" counts (`TriggeredEffect::
    /// first_each_encounter`). Counted as each break happens, and its
    /// `SubroutineBroken` is dispatched at once, so the listener scan reads
    /// it as of that break: an ability that breaks two has counted one when
    /// the first is heard. `subroutine_broken` on the run reaches back to
    /// the run's earlier ice.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub subroutines_broken: u32,
}


/// A set of the icebreaker subtypes (CR 2.16.7i: AI, decoder, fracter,
/// killer) — `EncounterTally::printed_broken_with`. Bits rather than a
/// list, because the tally is `Copy` and is cloned with every search
/// state; only the four words an icebreaker is typed by, since those are
/// what "with a decoder" can name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokenWith(u8);

impl BrokenWith {
    /// The subtypes the set can hold, each at its own bit.
    pub const KINDS: [CardSubtype; 4] = [CardSubtype::Ai, CardSubtype::Decoder, CardSubtype::Fracter, CardSubtype::Killer];

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// The set with every icebreaker subtype among `subtypes` added.
    pub fn with(self, subtypes: &[CardSubtype]) -> BrokenWith {
        let bits = Self::KINDS.iter().enumerate().filter(|(_, kind)| subtypes.contains(kind)).fold(0, |bits, (bit, _)| bits | 1 << bit);
        BrokenWith(self.0 | bits)
    }

    /// Whether `subtype` is in the set; never, for a word that is not an
    /// icebreaker's.
    pub fn includes(self, subtype: CardSubtype) -> bool {
        Self::KINDS.iter().position(|kind| *kind == subtype).is_some_and(|bit| self.0 & 1 << bit != 0)
    }
}

/// `EncounterTally::broken_by`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrokenBy {
    /// Nothing broken yet this encounter.
    #[default]
    Nothing,
    /// Every break so far by abilities on this install.
    Only(InstallId),
    /// By more than one object, or by something that is none (a click).
    Mixed,
}

impl BrokenBy {
    fn is_nothing(&self) -> bool {
        *self == BrokenBy::Nothing
    }

    /// The tally after one more break, by `by`.
    pub fn and(self, by: Option<InstallId>) -> BrokenBy {
        match (self, by) {
            (BrokenBy::Nothing, Some(install)) => BrokenBy::Only(install),
            (BrokenBy::Only(so_far), Some(install)) if so_far == install => self,
            _ => BrokenBy::Mixed,
        }
    }

    /// The object that fully broke the ice, if one did.
    pub fn object(self) -> Option<InstallId> {
        match self {
            BrokenBy::Only(install) => Some(install),
            BrokenBy::Nothing | BrokenBy::Mixed => None,
        }
    }
}

/// A single piece of ICE within a run's ice stack, as seen by the run state
/// machine. Built by `run::start_run` from the Corp's `InstalledCard`s on
/// the targeted server (`CardRegistry`-looked-up for its subtype and
/// `subroutines` — **not its strength**, which is a question put to the
/// table at every read, `continuous::ice_strength`, because a number stored
/// here outlived the counters it was computed from) and **kept in step with `corp.installed` by
/// `run::reconcile_ice`** — ICE installed, trashed, rezzed, derezzed or
/// swapped on the server mid-run shows up here at the run's next step; it
/// is not a snapshot. Ordered outermost-to-innermost matching install
/// order (index 0 is the first ICE approached; `position` indexes into this
/// for whichever ICE is currently being approached/encountered). Not `Copy`
/// — owns a `Vec` and a `String`-backed `CardId`, unlike the bare counter
/// this replaced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunIce {
    pub card_id: CardId,
    /// Which install this is — the handle `PlayerAction::RezIce` names.
    /// `card_id` cannot tell two copies of one ICE on a server apart, and
    /// "is this the ICE being approached" is exactly the question the rez
    /// rule asks (Rules Audit T10). `serde(default)` so a history recorded
    /// before the field existed still deserializes, to the placeholder.
    #[serde(default)]
    pub install_id: InstallId,
    /// This ICE's subtype, seeded from `CardDefinition::card_type`'s `CardType::Ice(_)`
    /// at `engine::build_run_ice` — the data `Effect::BreakSubroutines`'s
    /// `restrict_to` gate compares against. A card that is not ICE never
    /// becomes a `RunIce` at all (it used to default to `Barrier`).
    pub ice_type: IceType,
    pub subroutines: Vec<EncounteredSubroutine>,
    /// Mirrors `InstalledCard::rezzed` — flipped by `rez_ice` when rezzing
    /// during this ICE's `ApproachIce` window, and re-read from the install
    /// by `run::reconcile_ice` at every step, so a derez is seen too. Gates
    /// `run::engine::continue_run`'s `ApproachIce` transition: unrezzed
    /// ICE presents no subroutines and has no effect on the run, per
    /// Netrunner/Null Signal Games rules, so it auto-passes straight
    /// through instead of entering
    /// `EncounterIce`.
    pub rezzed: bool,
}

/// What the Runner chooses to access next during a breach: one of the
/// *candidates* (CR 7.3.4, "the Runner is presented with the current
/// candidates. They choose 1 of those cards and access it").
///
/// **A candidate is named the way the Runner can point at it, never by
/// what it is.** Choices used to be `CardId`s, and the breach picked every
/// random HQ card and the top N of R&D up front, so the Runner was offered
/// "Hedge Fund or Project Atlas?" about cards not yet accessed, and the name
/// of every unrezzed card in the root (Rules Conformance A1). A card in a
/// root is the install it is; a card in HQ or R&D is the zone, because the
/// rules offer "a random candidate from among the ones in the Corp's hand"
/// (CR 7.3.4a) and "1 candidate from the Corp's deck at a time in turn,
/// working down from the top of the deck" (CR 7.4.7); a card in Archives is
/// its name, since the breach has turned it faceup (CR 7.3.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccessCandidate {
    /// A card in the root of the breached server.
    Root(InstallId),
    /// The next card out of HQ or R&D, which neither player knows until it
    /// is accessed: a random card from HQ, the top card of R&D not yet
    /// accessed. One entry however many the breach may still access
    /// (`AccessState::from_zone`), because they are one choice.
    Zone,
    /// A card in Archives, faceup since the breach began.
    Archived(CardId),
}

/// One card the Runner is currently being asked to make a choice about,
/// mid-access, or a choice of which candidate to access next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessPhase {
    /// Offered when 2+ candidates remain. The Runner picks one via
    /// `PlayerAction::SelectCardToAccess`, which moves it into
    /// `PendingChoice`. With one candidate left there is nothing to choose,
    /// and it is presented unasked — which is how R&D's cards come one at a
    /// time, top down, with nothing else in the server.
    SelectNextCard { selectable_cards: Vec<AccessCandidate> },
    /// Offered instead of `PendingChoice` when the just-accessed card's
    /// registry definition has an `InteractiveOnAccess` trigger (Fetal AI's
    /// "pay 2c to avoid 2 net damage", Snare!'s "you may pay 4c" to inflict
    /// its damage) — resolved first, via `PlayerAction::PayAccessTrigger`/
    /// `DeclineAccessTrigger` (`run::access::resolve_pay_access_trigger`/
    /// `resolve_decline_access_trigger`), before the card's normal
    /// `PendingChoice` is presented.
    PendingInteractiveTrigger {
        card_id: CardId,
        cost: Cost,
        /// Which side chooses whether to pay — the card's
        /// `AccessInteraction::payer`, denormalized onto the parked state
        /// because `legal_actions::current_actor` takes no `CardRegistry`
        /// and must still name the right player. Same reason `cost` and
        /// `can_pay` live here rather than being re-read from the registry.
        decider: Side,
        /// Whether `decider` can currently afford `cost` (for
        /// `Cost::Credits`; `true` otherwise) — a precomputed hint, same
        /// role as `PendingChoice::can_trash`. Resolution re-checks
        /// affordability regardless.
        can_pay: bool,
    },
    PendingChoice {
        card_id: CardId,
        /// The printed trash cost, if the card has one. Whether the Runner
        /// can *afford* it is not stored: a `can_trash` hint used to sit
        /// here, computed once when the card was presented, and
        /// `legal_actions` trusted it — so a Runner who gained credits in
        /// the paid-ability window before deciding was never offered the
        /// trash. `run::access::resolve_trash` reads live credits and is
        /// the one authority (ROADMAP Rules Audit, Tier 2).
        trash_cost: Option<u32>,
        /// `true` for a "free" Agenda (an Agenda with no `steal_cost`) —
        /// `PlayerAction::PassAccessedCard` is illegal while this is set.
        mandatory_steal: bool,
        steal_cost: Option<Cost>,
        /// What the trash costs beside `trash_cost`'s credits
        /// (`ContinuousKind::AdditionalTrashCost`, Daniela Jorge Inácio's
        /// "add 2 cards from the grip at random to the bottom of the
        /// stack"), paid with them; the trash is not offered while it
        /// cannot be paid.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trash_also: Option<Cost>,
    },
}

impl AccessPhase {
    /// The card being accessed, when the access is at one.
    pub fn card(&self) -> Option<&CardId> {
        match self {
            AccessPhase::SelectNextCard { .. } => None,
            AccessPhase::PendingInteractiveTrigger { card_id, .. } | AccessPhase::PendingChoice { card_id, .. } => Some(card_id),
        }
    }
}

/// The in-progress state of resolving one server's worth of accessed cards,
/// one at a time, via `PlayerAction::SelectCardToAccess`/`StealAgenda`/
/// `TrashAccessedCard`/`PassAccessedCard`. Lives in `RunState::access_state`
/// while `RunState::phase == RunPhase::AccessingCard`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessState {
    pub server: ServerId,
    /// The candidates in the root, and in Archives, not yet chosen. Built
    /// at the breach and pruned of any install that has since left the
    /// server (CR 7.4.5). Archives cards are listed by name in name order,
    /// not the order they arrived in: "Discard piles are not ordered" (CR
    /// 4.4.2), and a list in arrival order let the Runner match each
    /// card the breach turned faceup to the moment it went facedown
    /// (Rules Conformance A2).
    pub candidates: Vec<AccessCandidate>,
    /// The cards of HQ or R&D this breach will still access, in the order
    /// it reaches them, **known to neither player** and never in a view.
    /// Drawn at the breach, when the random access limit is set (CR 7.3.5):
    /// HQ's in a random order, R&D's from the top down. Choosing
    /// `AccessCandidate::Zone` takes the first that is still there. Drawing
    /// them at the breach rather than at each choice is the same
    /// distribution while HQ and R&D hold still between accesses; a card
    /// that entered them mid-breach would be CR 7.4.6b and 7.4.7a, which the
    /// engine does not model, as it did not before.
    pub from_zone: Vec<CardId>,
    /// Cards already fully resolved (stolen/trashed/passed) this access.
    pub resolved_cards: Vec<CardId>,
    /// The card currently being presented to the Runner, set *before* its
    /// `Trigger::OnAccessed` reaction is dispatched and therefore before
    /// `phase` becomes the matching `PendingChoice`. Exists so a card that
    /// trashes itself out of that very trigger (an ambush like Shock!) is
    /// still known to have been seen by the Runner, and so lands faceup in
    /// Archives — `phase` alone can't answer that, since it's still the
    /// placeholder at dispatch time. `None` outside that window.
    #[serde(default)]
    pub currently_accessing: Option<CardId>,
    /// Which *installed instance* the card in `phase` is, when it is a
    /// root install (an upgrade in a central's root, or anything in a
    /// remote's root); `None` for a card accessed out of HQ, R&D or
    /// Archives. It is which instance leaves play and whose counters an
    /// `OnAccessed` trigger reads. Kept here rather than on the phase
    /// variants because there is only ever one pending card and the
    /// variants are built in dozens of fixtures. Set by
    /// `access::present_card_for_access` from the `AccessCandidate::Root`
    /// the Runner chose.
    #[serde(default)]
    pub pending_install: Option<InstallId>,
    /// Whether that install was **rezzed** when it was presented — read by
    /// `EffectRequirement::CurrentlyAccessingInstalledCard { rezzed_only }`.
    /// Recorded here rather than looked up on the table, because the one
    /// card that asks after the fact (Public Access Plaza's "when the
    /// Runner trashes this asset *while it is rezzed*") is dispatched from
    /// `Trigger::OnTrashedFromAccess`, by which point the install has
    /// already left the table.
    #[serde(default)]
    pub pending_install_rezzed: bool,
    pub phase: AccessPhase,
}

/// Every field at its neutral value, for test fixtures — see
/// `rules::state::InstalledCard`'s `Default` for the full rationale.
/// `server` and `phase` have no meaningful neutral value (every real access
/// targets a specific server in a specific phase); the placeholders exist
/// only so `Default` can be implemented, and any caller that cares must
/// override them. Production sites stay exhaustive.
impl Default for AccessState {
    fn default() -> Self {
        Self {
            server: ServerId::Hq,
            candidates: Vec::new(),
            from_zone: Vec::new(),
            resolved_cards: Vec::new(),
            currently_accessing: None,
            pending_install: None,
            pending_install_rezzed: false,
            phase: AccessPhase::SelectNextCard { selectable_cards: Vec::new() },
        }
    }
}

/// A run in progress (or just concluded) — the sub-state-machine embedded in
/// `GameState::active_run`. `ice` is ordered outermost-to-innermost (index 0
/// is the first ICE approached) and follows the attacked server's installs
/// through `run::reconcile_ice`; `position` indexes into `ice` for whichever
/// ICE is currently being approached/encountered.
///
/// Invariant (caller's responsibility when hand-building a `RunState`, same
/// as `GameState`'s own fields): while `phase` is `ApproachIce` or
/// `EncounterIce`, `position < ice.len()`; while it is `Movement`,
/// `position <= ice.len()`; while `phase` is `AccessingCard`,
/// `access_state` is `Some`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    pub server: ServerId,
    pub phase: RunPhase,
    pub ice: Vec<RunIce>,
    pub position: usize,
    pub access_state: Option<AccessState>,
    /// Whether `PlayerAction::JackOut` is currently legal. The Runner may
    /// jack out at one step of a run only, CR 6.9.4b in the movement phase,
    /// so this is `true` exactly while the run is in `RunPhase::Movement`
    /// and the Runner has not yet chosen to go on: set by every way into
    /// movement, and shut by the `ContinueRun` that opens movement's
    /// paid-ability window. It is `false` at every approach, ice or server
    /// — the jack-out decision comes *before* the Corp's chance to rez
    /// what lies ahead, never after it.
    pub jack_out_permitted: bool,
    /// The run has been declared successful (CR 6.9.5a) and the breach
    /// (6.9.5b) is next. Set by `CompleteRun`, which breaches at the end
    /// of its action unless a "when successful" ability parked a decision; then the
    /// breach waits for it, and `engine::resume_run` takes it when nothing is
    /// parked. There is no paid ability window between the two steps to
    /// wait in: the one the engine used to open there let the Corp rez an
    /// upgrade after the run had succeeded. Also what refuses a second
    /// `CompleteRun`.
    #[serde(default)]
    pub declared_successful: bool,
    /// This is a breach and no run (CR 7.3.1: "card abilities can also
    /// directly instruct the Runner to breach a server") — Cataloguer's
    /// "[click], hosted power counter: Breach R&D", begun by
    /// `Effect::Breach`. The access machinery is the run's, so a breach
    /// with no run stands in a `RunState` at its access, with no ice and
    /// nothing declared successful. What only a run has is refused on the
    /// flag: it is not a run in progress to a card (`GameState::
    /// run_in_progress`), its end is no `RunCompleted` a card hears, and it
    /// leaves `last_completed_run` the last *run*.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub breach_only: bool,
    /// The encounter under way is a forced one (CR 6.5.9a): the Runner
    /// was made to encounter a piece of ice again without moving —
    /// Sisyphus Protocol's "the Runner encounters that ice again", begun by
    /// `Effect::ForceEncounter` in the movement phase after the ice was
    /// passed. Its end returns the run to that movement phase without
    /// passing the ice a second time (`pass_current_ice`); an "end the
    /// run" ends both (6.5.9b), as it does any encounter. Public: both
    /// players saw what forced it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub forced_encounter: bool,
    /// Counters on the event that began this run, while it is in the play
    /// area (CR 8.6.5) — Spree's "place 3 power counters on this event",
    /// spent by its "hosted power counter:" ability. Kept on the run
    /// because the play area lasts as long as the run does: the engine puts
    /// a played event in the heap (`run::run_event`), where no counter is
    /// kept, and the counters go with the run. Public, as counters are.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub event_counters: u32,
    /// Subroutines pieces of ice gained for the rest of this run from an
    /// ability not their own — Thunderbolt Armaments' "that ice … gains
    /// “[subroutine] End the run unless…” after its other subroutines for
    /// the remainder of that run" (`Effect::GainSubroutine`, `duration:
    /// Run`), oldest first. Added to the ice's list at each encounter with
    /// it (`run::engine::add_gained_for_the_run`), because the encounter's
    /// list is rebuilt each time — the rest of what an encounter gained
    /// goes with it. On the run and not in `GameState::lingering`, which
    /// holds flat values a search clone copies in a few words: a subroutine
    /// is an effect tree. It goes with the run, so nothing resets it.
    /// Public: both players saw the rez and the ability.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gained_for_the_run: Vec<GainedForTheRun>,
    /// Temporary Runner credit pool for this run only, seeded from
    /// `state::CorpState::bad_publicity` at `engine::initiate_run`.
    /// Spendable via `ability::pay_cost`'s `Cost::Credits` arm — draws from
    /// here before the Runner's own wallet. Discarded for free whenever this
    /// `RunState` is dropped/replaced (every run-termination site already
    /// clears `GameState::active_run`); no separate cleanup is needed.
    pub bad_publicity_credits: u32,
    /// Extra R&D cards a successful run accesses beyond the top card —
    /// e.g. a Runner program's "access 1 additional card from R&D" ability
    /// (`Effect::AddAdditionalAccess`). Read (not decremented) by
    /// `run::access::compute_accessed_cards`; naturally discarded for free
    /// when this `RunState` is dropped/replaced, same lifecycle as
    /// `bad_publicity_credits`.
    pub additional_rd_access: u32,
    /// Extra HQ cards, mirroring `additional_rd_access`.
    pub additional_hq_access: u32,
    /// A pending "replace this server's normal access with `Effect`
    /// instead" grant (`Effect::SetAccessReplacement`), e.g. Account
    /// Siphon's "gain 8 credits instead of accessing HQ". Consumed the
    /// moment `run::access_server` is next called against the matching
    /// `ServerId` — see `run::access::try_replace_access`. `None` is the
    /// overwhelmingly common case. If set twice for the same server before
    /// being consumed, the second call overwrites the first (last write
    /// wins) — no error, since only ever one replacement can matter per
    /// access and this can only occur from malformed card authoring.
    /// The `bool` is `Effect::SetAccessReplacement::optional` — a "may"
    /// replacement parks the breach-owner's choice at access time instead
    /// of firing unconditionally.
    pub access_replacement: Option<(ServerId, Effect, bool)>,
    /// The card `access_replacement` resolves *as* — the one whose text set
    /// it, as `on_success_card` is for the run's rider. Without it the
    /// replacement resolved with no acting card, and a `Sequence` with no
    /// acting card cannot wait behind a decision (`ability::
    /// evaluate_sequence` pins a continuation to a card): when Account
    /// Siphon's "up to 5[credit]" became a number the Runner is asked for,
    /// the `EndTheRun` behind it was dropped and the run stood open after
    /// the siphon. `None` in a state recorded before the field existed.
    #[serde(default)]
    pub access_replacement_card: Option<CardId>,
    /// …and which copy of it, when it is installed: Cataloguer's "remove 1
    /// hosted power counter" is from the Cataloguer that set it, not the
    /// first of two.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_replacement_install: Option<InstallId>,
    /// How many cards this run's breach has accessed, counted by
    /// `run::access::present_card_for_access` as each is accessed (CR
    /// 7.3.6: only accesses actually performed; `0` if the run hasn't
    /// reached access yet, or accessed an empty zone).
    /// Read (not decremented) when the run concludes — see `GameState::
    /// last_completed_run`/`Effect::GainCreditsPerCardAccessedThisRun` —
    /// same "naturally discarded when this `RunState` is dropped/replaced"
    /// lifecycle as `additional_rd_access`.
    #[serde(default)]
    pub cards_accessed_count: u32,
    /// Where the run goes instead when it would approach its server after
    /// passing every piece of ice — `Effect::RedirectRunOnApproach`
    /// (Maintenance Access). Taken by `run::engine::apply_approach_redirect`
    /// the moment `ServerApproached` would fire. `None` for every ordinary
    /// run.
    #[serde(default)]
    pub redirect_on_approach: Option<ServerId>,
    /// An effect to evaluate if and when this run succeeds — e.g.
    /// Jailbreak's "If successful, draw 1 card and ... access 1 additional
    /// card". Seeded by `pending_choice::resolve_choose_server` (with any
    /// `AddAdditionalAccess` already rewritten to the chosen server) and
    /// evaluated once by `dispatcher`'s `GameEvent::RunSucceeded` arm,
    /// which fires *before* access is computed — so an access bonus granted
    /// here still applies to that same breach. Exists because an Event card
    /// is never installed and so can't carry a `Trigger::OnSuccessfulRun`
    /// of its own the way an installed card (e.g. Red Team) can. `None` for
    /// every ordinary run. Naturally discarded when this `RunState` is
    /// dropped/replaced, same lifecycle as `bad_publicity_credits`.
    #[serde(default)]
    pub on_success_effect: Option<Box<Effect>>,
    /// The card and install `on_success_effect` resolves *as* — the card
    /// whose `PromptChooseServer` started this run. Without them the rider
    /// resolved with no acting card, so a "this card" effect in it
    /// (*Red Team*'s "take 3[c] from this resource") had nothing to act on;
    /// Red Team was modelled as paying on *every* successful run instead.
    #[serde(default)]
    pub on_success_card: Option<CardId>,
    #[serde(default)]
    pub on_success_install: Option<InstallId>,
    /// `Effect::SetRunEndedEffect` — resolved as `on_end_card`/
    /// `on_end_install` when this run ends, carried over into
    /// `CompletedRun` for the `OnRunEnded` dispatch to take. Charm Offensive.
    #[serde(default)]
    pub on_end_effect: Option<Box<Effect>>,
    #[serde(default)]
    pub on_end_card: Option<CardId>,
    #[serde(default)]
    pub on_end_install: Option<InstallId>,
    /// Whether any subroutine has resolved during this run
    /// (`EffectRequirement::SubroutineResolvedThisRun`) — Ryō "Phoenix" Ōno.
    #[serde(default)]
    pub subroutine_resolved: bool,
    /// Whether a piece of ice has been derezzed during this run — Stegodon
    /// MK IV's "Each run, as long as a piece of ice has been derezzed during
    /// that run" (`EffectRequirement::IceDerezzedThisRun`). Set where a card
    /// is turned facedown (`ability::derez`), by a card's text or a cost
    /// alike; a new run is a new `RunState`, so nothing clears it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ice_derezzed: bool,
    /// Whether any subroutine has been broken during this run — Mercury:
    /// Chrome Libertador's "if you did not break any subroutines during
    /// that run" (`EffectRequirement::SubroutineBrokenThisRun`). Set by
    /// `run::break_subroutine`, the one place a subroutine is broken; a new
    /// run is a new `RunState`, so nothing clears it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub subroutine_broken: bool,
    /// Whether the run reached the success phase (CR 6.9.5) — set as the
    /// Runner completes the run past the approach to the server, whether
    /// or not it is then declared successful. A run that did not, against
    /// a server that still exists, is unsuccessful (CR 6.8.4:
    /// `CompletedRun::unsuccessful`, Hannah "Wheels" Pilintra). Not
    /// `declared_successful`: a run Crisium Grid keeps from being declared
    /// successful is not unsuccessful either (6.8.4a).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reached_success_phase: bool,
    /// The server this run's breach is of, from the moment it begins
    /// (`GameEvent::BreachBegun`, CR 6.9.5b) — the attacked server, or the
    /// one a card said to breach instead (Eru Ayase-Pessoa's "instead of
    /// breaching Archives, breach R&D", written by `Effect::Breach` from
    /// the replacement). `None` until then. State and not context, because
    /// the breach waits for what its beginning triggered ("access 1
    /// additional card" is applied before the random access limit is set,
    /// CR 7.3.5b), and that may be a decision of its own; it is also what
    /// tells `run::breach` the beginning was announced. Public: both
    /// players see which server is breached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breached: Option<ServerId>,
    /// How many times the Runner has encountered ice during this run,
    /// counting the encounter under way — S-Dobrado's "the first time"
    /// and "the second time you encounter a piece of ice during that run"
    /// (`Amount::EncountersThisRun`). Counted where an encounter begins,
    /// a forced one included (CR 6.5.9a: it is an encounter). Not the turn
    /// log's: that counts the turn, and a second run is a fresh count.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub encounters: u32,
    /// The action this run (or breach) is part of, which finishes when it
    /// ends: a run is an action in progress until it is over (CR 5.2.2a),
    /// and so is the event or ability that began one (Dirty Laundry,
    /// Cataloguer). Set by `engine::apply_action` when the action that
    /// began it returns, and announced by it as the run ends
    /// (`GameEvent::ActionFinished`); `None` for a run no action began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finishes: Option<crate::rules::turn_log::SameAction>,
    /// The card whose effect started this run (`Effect::InitiateRun`,
    /// `Effect::PromptChooseServer`) — `None` for a basic-action run.
    /// `EffectRequirement::RunEventActive` (Sang Kancil) asks whether it
    /// is a run event.
    #[serde(default)]
    pub initiated_by: Option<CardId>,
    /// Whether the ice at `position` was bypassed
    /// (`Effect::BypassEncounteredIce`) — cleared by `pass_current_ice`.
    /// Read by the dispatcher to withhold the ice's own `OnEncounter`
    /// reactions and stand down any already deferred.
    #[serde(default)]
    pub ice_bypassed: bool,
    /// Whether the Runner has fully broken the ice at `position` during
    /// this encounter (CR 6.5.7a: the first time every subroutine on it is
    /// broken) — set where a subroutine is broken
    /// (`run::break_subroutine`), cleared when an encounter begins and by
    /// `enter_movement`. Read when the ice is passed, which is what "after
    /// fully breaking it" asks (CR 6.1.3f: only the encounter just ended
    /// counts), and what keeps a second full break in one encounter from
    /// being a second occurrence (6.5.7d). State and not context, because
    /// the break and the pass are separate actions.
    #[serde(default)]
    pub fully_broken: bool,
    /// What the ice being encountered has limited so far this encounter
    /// (`ContinuousKind::BreakLimit`, `TrashLimit`), cleared where
    /// `fully_broken` is: when an encounter begins and by `enter_movement`.
    /// State and not context, because each break and each subroutine's
    /// trash is an action of its own.
    #[serde(default)]
    pub this_encounter: EncounterTally,
    /// A temporary Runner credit pool for this run only, set once at run
    /// start by whatever initiated it (e.g. Overclock's "place 5 credits on
    /// this event, then run any server — you can spend hosted credits
    /// during that run"), spendable via `ability::pay_cost`'s `Cost::
    /// Credits` arm the same way `bad_publicity_credits` already is.
    /// Naturally discarded when this `RunState` is dropped/replaced.
    /// Usually `0`.
    #[serde(default)]
    pub bonus_run_credits: u32,
    /// What `bonus_run_credits` may be spent on, when the event hosting
    /// them says — Bahia Bands' "Place 4[credit] on this event. You can
    /// spend hosted credits **to pay trash costs** for the remainder of
    /// this run" (`Effect::PlaceRunCredits::pays_for`). `None`, anything
    /// (Overclock, Trick Shot). One word for the whole pool: only the
    /// run's own event places credits in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_credits_pay_for: Option<crate::dsl::PaysFor>,
    /// Whether a card's text began this run as the Runner's turn began
    /// (Alarm Clock), so that its end puts the turn back at its start
    /// (`engine::start_run`, `engine::end_run`). Part of the run, so a new
    /// run is the reset.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub begun_as_the_turn_began: bool,
    /// How many agendas the Runner has stolen during this run
    /// (`run::access::resolve_steal`). Snapshotted into
    /// `state::CompletedRun::agendas_stolen` when the run concludes, since
    /// `Trigger::OnRunEnded` fires after this `RunState` is gone — backs
    /// AMAZE Amusements' "if the Runner stole any agendas during that run".
    #[serde(default)]
    pub agendas_stolen_this_run: u32,
    /// Root-slot Corp cards flagged `CardDefinition::persistent_after_trash`
    /// that were trashed during this run while it was running against their
    /// own server — e.g. AMAZE Amusements, whose ability explicitly still
    /// applies "for the remainder of this run" after the Runner trashes it
    /// on access. Snapshotted into `state::CompletedRun` at conclusion so
    /// `Trigger::OnRunEnded` can fire them from the registry even though
    /// they have left `CorpState::installed`. Naturally discarded when this
    /// `RunState` is dropped/replaced, so the persistence cannot leak into
    /// a later run.
    #[serde(default)]
    pub persistent_trashed_upgrades: Vec<CardId>,
    /// The abilities printed "Use this ability only once per run" already
    /// used during this run (`EffectRequirement::OncePerRun`) — Pressure
    /// Spike's threat pump — keyed by card and copy as `once_per_turn_used`
    /// is. On the run rather than beside the turn's set, because the run's
    /// end is the limit's end and a new `RunState` is the reset, with no
    /// clearing site to forget.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub once_per_run_used: std::collections::BTreeSet<crate::rules::state::OncePerTurnKey>,
}

/// Every field at its neutral value, for test fixtures — see
/// `rules::state::InstalledCard`'s `Default` for the full rationale. This is
/// the struct that motivated M10.5: it has absorbed a new field in five
/// consecutive milestones, and before this impl every one of those additions
/// broke ~97 test literals.
///
/// `server` and `phase` have no meaningful neutral value — every real run
/// targets a chosen server and starts at `Initiation`. The placeholders exist
/// only so `Default` can be implemented; any caller that cares must override
/// them. The real run constructors in `run::engine` deliberately stay
/// exhaustive so the compiler keeps forcing a decision about each new field.
impl Default for RunState {
    fn default() -> Self {
        Self {
            server: ServerId::Hq,
            phase: RunPhase::Initiation,
            ice: Vec::new(),
            position: 0,
            finishes: None,
            access_state: None,
            jack_out_permitted: false,
            declared_successful: false,
            breach_only: false,
            forced_encounter: false,
            event_counters: 0,
            gained_for_the_run: Vec::new(),
            bad_publicity_credits: 0,
            additional_rd_access: 0,
            additional_hq_access: 0,
            access_replacement: None,
            access_replacement_card: None,
            access_replacement_install: None,
            cards_accessed_count: 0,
            redirect_on_approach: None,
            bonus_run_credits: 0,
            run_credits_pay_for: None,
            begun_as_the_turn_began: false,
            initiated_by: None,
            ice_bypassed: false,
            fully_broken: false,
            this_encounter: EncounterTally::default(),
            once_per_run_used: Default::default(),
            agendas_stolen_this_run: 0,
            persistent_trashed_upgrades: Vec::new(),
            on_success_effect: None,
            on_end_effect: None,
            on_end_card: None,
            on_end_install: None,
            subroutine_resolved: false,
            ice_derezzed: false,
            subroutine_broken: false,
            reached_success_phase: false,
            breached: None,
            encounters: 0,
            on_success_card: None,
            on_success_install: None,
        }
    }
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}
