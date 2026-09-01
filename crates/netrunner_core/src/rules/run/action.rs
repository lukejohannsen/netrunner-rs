use serde::{Deserialize, Serialize};

/// The verbs that drive a run through `advance_run`. Only the Runner ever
/// makes a run, so unlike `PlayerAction` there is no `side` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunAction {
    /// Initiation -> ApproachIce (or -> Success if no ICE); ApproachIce ->
    /// EncounterIce if the approached ICE is rezzed, else straight past it
    /// (the rez window is the paid-ability window `paid_ability` opens at
    /// `ApproachIce`, where `engine::rez_ice` permits rezzing exactly the
    /// approached ICE for its printed cost); EncounterIce -> next ICE's
    /// ApproachIce or Success once every subroutine has been handled (no
    /// longer `Pending`) — this is "Pass ICE".
    Continue,
    /// Resolve (fire) the subroutine at this index on the ICE being
    /// encountered.
    ResolveSubroutine(usize),
    /// Break the subroutine at this index on the ICE being encountered.
    BreakSubroutine(usize),
    /// Voluntarily end the run early ("Jack Out").
    JackOut,
}
