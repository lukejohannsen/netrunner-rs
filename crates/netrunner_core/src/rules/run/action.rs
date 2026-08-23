use serde::{Deserialize, Serialize};

/// The verbs that drive a run through `advance_run`. Only the Runner ever
/// makes a run, so unlike `PlayerAction` there is no `side` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunAction {
    /// Initiation -> ApproachIce (or -> Success if no ICE); ApproachIce ->
    /// EncounterIce (Rez Window collapses into this transition — rez
    /// cost/timing isn't modeled at this abstraction); EncounterIce -> next
    /// ICE's ApproachIce or Success once no subroutines remain pending
    /// (this is "Pass ICE").
    Continue,
    /// Resolve (fire) the next pending subroutine on the ICE being encountered.
    ResolveSubroutine,
    /// Break the next pending subroutine on the ICE being encountered.
    BreakSubroutine,
    /// Voluntarily end the run early ("Jack Out").
    JackOut,
}
