use serde::{Deserialize, Serialize};

use crate::dsl::{CardId, Cost, SubroutineDef};

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
/// machine. `card_id` identifies which installed card this is (not yet
/// cross-checked against `CorpState::installed` — populating real ICE from
/// there via a `CardRegistry` lookup is still future work; `initiate_run`
/// still builds `ice: Vec::new()`). Not `Copy` — owns a `Vec` and a
/// `String`-backed `CardId`, unlike the bare counter this replaced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunIce {
    pub card_id: CardId,
    pub current_strength: i32,
    pub subroutines: Vec<EncounteredSubroutine>,
}

/// One card the Runner is currently being asked to make a choice about,
/// mid-access. Currently a single-variant enum (room for future variants —
/// e.g. an "on access" trigger prompt — without reshaping `AccessState`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessPhase {
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
/// one at a time, via `PlayerAction::StealAgenda`/`TrashAccessedCard`/
/// `PassAccessedCard`. Lives in `RunState::access_state` while
/// `RunState::phase == RunPhase::AccessingCard`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessState {
    pub server: ServerId,
    pub accessed_cards: Vec<CardId>,
    pub current_index: usize,
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
}
