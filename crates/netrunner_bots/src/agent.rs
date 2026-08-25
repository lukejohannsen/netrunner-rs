use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{GameEvent, PlayerAction};
use netrunner_core::view::ClientView;

/// Unified interface for an automated player. Given the current `view` —
/// everything this agent's side is entitled to know, plus `view.
/// legal_actions` (only the actions it may actually submit) — choose
/// exactly one to play.
///
/// `view.legal_actions` is always non-empty — callers only invoke this once
/// it's non-empty (an empty result means the game is over or no decision is
/// pending for this side right now). `: Send` since a `MatchSession` owns
/// agents across `.await` points.
pub trait BotAgent: Send {
    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction;

    /// Optional hook for a stateful agent to observe events as they occur
    /// (e.g. a future opponent-modeling agent). No-op by default — none of
    /// this crate's baseline agents need it, since `select_action` already
    /// receives the full current `view` on every call.
    fn observe(&mut self, _event: &GameEvent) {}
}
