//! Why a match ended, and how to work that out from a final `GameState`.
//!
//! Moved here from `netrunner_server::protocol` — it was never a transport
//! concern. `netrunner_cli` depended on the whole server crate purely to
//! call `classify_end_reason` on its *local* (offline, no-socket) path,
//! which is the tell that this belongs beside the session driver rather
//! than beside the wire messages. `netrunner_server` re-exports both, so
//! the protocol's public surface is unchanged.

use serde::{Deserialize, Serialize};

use netrunner_core::rules::{GameEvent, GameState, Side};

/// Why a match ended. Not tracked as a distinct concept anywhere in
/// `netrunner_core` (`GameEvent::GameOver { winner }` doesn't say why), so
/// `classify_end_reason` derives this heuristically from the trailing
/// `GameEvent`s of whichever `apply_action` call produced `GameOver` —
/// presentation logic, not a core engine capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEndReason {
    AgendaThreshold,
    Flatline,
    Deckout,
    Surrender,
    /// The side that had to act dropped its connection and did not come
    /// back within the host's grace period. Like `Surrender`, a transport
    /// outcome the engine never produces: `netrunner_server::MatchSession`
    /// is the only thing that can observe it, and it reports the *other*
    /// side as the winner.
    Disconnected,
}

/// Best-effort classification — see `GameEndReason`'s doc comment. A
/// `RunnerFlatlined` event in this action's trailing events means Flatline;
/// an empty Corp R&D at a Runner win means the Corp decked out attempting
/// their mandatory draw (the one other unprompted win path — see
/// `turn::enter_start_of_turn`'s doc comment); anything else defaults to
/// the ordinary agenda-point threshold.
///
/// `Surrender` and `Disconnected` are never produced here: neither is a
/// rules outcome at all, so only the transport that received the concession
/// (or lost the client) can report them.
pub fn classify_end_reason(events: &[GameEvent], winner: Side, state: &GameState) -> GameEndReason {
    if events.iter().any(|event| matches!(event, GameEvent::RunnerFlatlined)) {
        return GameEndReason::Flatline;
    }
    if winner == Side::Runner && state.corp.r_and_d.is_empty() {
        return GameEndReason::Deckout;
    }
    GameEndReason::AgendaThreshold
}
