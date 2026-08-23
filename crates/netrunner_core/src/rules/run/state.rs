use serde::{Deserialize, Serialize};

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
    Success,
    Ended,
}

/// A single piece of ICE within a run's ice stack, as seen by the run state
/// machine. Deliberately decoupled from `dsl::Card`/`dsl::IceType` — no
/// installed-ICE/server model exists in `GameState` yet, so this only carries
/// what `advance_run` needs to step through subroutines one at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunIce {
    pub subroutines_pending: u32,
}

/// A run in progress (or just concluded) — the sub-state-machine embedded in
/// `GameState::active_run`. `ice` is ordered outermost-to-innermost (index 0
/// is the first ICE approached); `position` indexes into `ice` for whichever
/// ICE is currently being approached/encountered.
///
/// Invariant (caller's responsibility when hand-building a `RunState`, same
/// as `GameState`'s own fields): while `phase` is `ApproachIce` or
/// `EncounterIce`, `position < ice.len()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    pub server: ServerId,
    pub phase: RunPhase,
    pub ice: Vec<RunIce>,
    pub position: usize,
}
