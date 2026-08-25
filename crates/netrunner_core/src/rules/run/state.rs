use serde::{Deserialize, Serialize};

use crate::dsl::{CardId, Cost, Effect, IceType, SubroutineDef};

/// Which Corp zone/server a run targets. Central servers are singletons;
/// Remote servers are numbered since multiple can exist simultaneously.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerId {
    Hq,
    RnD,
    Archives,
    Remote(u32),
}

/// The 5 states of a run. The doc's finer-grained steps (Rez Window,
/// Subroutine Resolution, Pass ICE, Jack Out/Continue) are modeled as
/// `RunAction`-driven transitions within `ApproachIce`/`EncounterIce`, not as
/// additional phase variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunPhase {
    Initiation,
    ApproachIce,
    EncounterIce,
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
/// addressable by `id` (its index within `RunIce::subroutines`). `status`
/// tracks whether the Runner broke it, let it fire (`Resolved`), or hasn't
/// handled it yet (`Pending`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncounteredSubroutine {
    pub id: usize,
    pub definition: SubroutineDef,
    pub status: SubroutineStatus,
}

/// A single piece of ICE within a run's ice stack, as seen by the run state
/// machine. Built by `engine::initiate_run` from the Corp's `InstalledCard`s
/// on the targeted server (`CardRegistry`-looked-up for
/// `strength`/`subroutines`, defaulting to a blank 0-strength/no-subroutines
/// ICE if unregistered), ordered outermost-to-innermost matching install
/// order (index 0 is the first ICE approached; `position` indexes into this
/// for whichever ICE is currently being approached/encountered). Not `Copy`
/// — owns a `Vec` and a `String`-backed `CardId`, unlike the bare counter
/// this replaced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunIce {
    pub card_id: CardId,
    pub current_strength: i32,
    /// This ICE's subtype, seeded from `Card::card_type`'s `CardType::Ice(_)`
    /// at `engine::build_run_ice` — the data `Effect::BreakSubroutines`'s
    /// `restrict_to` gate compares against. Defaults to `IceType::Barrier`
    /// for an unregistered card (same leniency as `current_strength`'s `0`
    /// default), which is harmless since such ICE has no strength/
    /// subroutines to break anyway.
    pub ice_type: IceType,
    pub subroutines: Vec<EncounteredSubroutine>,
    /// Mirrors `InstalledCard::rezzed` at the moment this `RunIce` was
    /// built (`initiate_run`) or later flipped (`rez_ice`, when rezzing
    /// during this ICE's `ApproachIce` window). Gates
    /// `run::engine::continue_run`'s `ApproachIce` transition: unrezzed
    /// ICE presents no subroutines and has no effect on the run, per
    /// Netrunner/Null Signal Games rules, so it auto-passes straight
    /// through instead of entering
    /// `EncounterIce`.
    pub rezzed: bool,
}

/// One card the Runner is currently being asked to make a choice about,
/// mid-access, or a choice of which accessed card to resolve next when more
/// than one remains unresolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessPhase {
    /// Offered when 2+ cards from this access remain unresolved. The Runner
    /// picks one via `PlayerAction::SelectCardToAccess`, which moves it into
    /// `PendingChoice`.
    SelectNextCard { selectable_cards: Vec<CardId> },
    /// Offered instead of `PendingChoice` when the just-accessed card's
    /// registry definition has an `InteractiveOnAccess` trigger (e.g. Fetal
    /// AI's "pay 2c to avoid 2 net damage") — resolved first, via
    /// `PlayerAction::PayToAvoidAccessTrigger`/`DeclineAccessTrigger`
    /// (`run::access::resolve_pay_to_avoid`/`resolve_decline_to_avoid`),
    /// before the card's normal `PendingChoice` is presented.
    PendingInteractiveTrigger {
        card_id: CardId,
        cost: Cost,
        /// Whether the Runner can currently afford `cost` (for
        /// `Cost::Credits`; `true` otherwise) — a precomputed hint, same
        /// role as `PendingChoice::can_trash`. Resolution re-checks
        /// affordability regardless.
        can_pay: bool,
    },
    PendingChoice {
        card_id: CardId,
        /// Whether the Runner can currently afford `trash_cost` (`false` if
        /// `trash_cost` is `None` — nothing to trash here at all). A
        /// precomputed hint; `run::access::resolve_trash` re-checks
        /// affordability itself regardless.
        can_trash: bool,
        trash_cost: Option<u32>,
        /// `true` for a "free" Agenda (an Agenda with no `steal_cost`) —
        /// `PlayerAction::PassAccessedCard` is illegal while this is set.
        mandatory_steal: bool,
        steal_cost: Option<Cost>,
    },
}

/// The in-progress state of resolving one server's worth of accessed cards,
/// one at a time, via `PlayerAction::SelectCardToAccess`/`StealAgenda`/
/// `TrashAccessedCard`/`PassAccessedCard`. Lives in `RunState::access_state`
/// while `RunState::phase == RunPhase::AccessingCard`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessState {
    pub server: ServerId,
    /// Cards from this access not yet chosen for resolution (order is
    /// access-determination order, not resolution order — the Runner picks
    /// freely among these via `SelectCardToAccess`).
    pub unaccessed_cards: Vec<CardId>,
    /// Cards already fully resolved (stolen/trashed/passed) this access.
    pub resolved_cards: Vec<CardId>,
    pub phase: AccessPhase,
}

/// A run in progress (or just concluded) — the sub-state-machine embedded in
/// `GameState::active_run`. `ice` is ordered outermost-to-innermost (index 0
/// is the first ICE approached); `position` indexes into `ice` for whichever
/// ICE is currently being approached/encountered.
///
/// Invariant (caller's responsibility when hand-building a `RunState`, same
/// as `GameState`'s own fields): while `phase` is `ApproachIce` or
/// `EncounterIce`, `position < ice.len()`; while `phase` is `AccessingCard`,
/// `access_state` is `Some`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    pub server: ServerId,
    pub phase: RunPhase,
    pub ice: Vec<RunIce>,
    pub position: usize,
    pub access_state: Option<AccessState>,
    /// Whether `PlayerAction::JackOut` is currently legal
    /// (Netrunner/Null Signal Games-style jack-out windows). `false` while
    /// initially approaching the outermost
    /// ICE (`initiate_run`'s starting value) or while committed to an
    /// encounter/subroutine resolution (`ApproachIce --Continue-->
    /// EncounterIce` closes it); `true` once an ICE has been passed —
    /// including an unrezzed one, which counts as "passed" — or once the
    /// server approach step is reached with no ICE remaining (both via
    /// `run::engine::pass_current_ice`, the single place an ICE gets left
    /// behind).
    pub jack_out_permitted: bool,
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
    pub access_replacement: Option<(ServerId, Effect)>,
}
