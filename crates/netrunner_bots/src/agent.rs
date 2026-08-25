use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{GameEvent, GameState, PlayerAction};

/// Unified interface for an automated player. Given the current `state` and
/// the `legal_actions` already computed for it (via
/// `netrunner_core::rules::legal_actions`), choose exactly one to play.
///
/// `legal_actions` is always non-empty — callers only invoke this once
/// `legal_actions` has returned at least one candidate; an empty result
/// means the game is over or stalled, neither of which calls for a
/// decision. See the crate-level doc comment for why `state` is the full,
/// unmasked `GameState` rather than a per-player masked view.
pub trait BotAgent {
    fn select_action(
        &mut self,
        state: &GameState,
        registry: &CardRegistry,
        legal_actions: &[PlayerAction],
    ) -> PlayerAction;

    /// Optional hook for a stateful agent to observe events as they occur
    /// (e.g. a future opponent-modeling agent). No-op by default — none of
    /// this crate's baseline agents need it, since `select_action` already
    /// receives the full current `state` on every call.
    fn observe(&mut self, _event: &GameEvent) {}
}
