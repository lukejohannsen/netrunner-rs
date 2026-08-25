//! Bridges `config::BotKind` selection to `netrunner_bots::BotAgent`
//! instances. Whose decision is pending right now
//! (`netrunner_core::rules::current_actor`) now lives in `netrunner_core`
//! itself — both `netrunner_server::MatchSession` and this crate need it,
//! so it no longer belongs only here.

use netrunner_bots::{BotAgent, HeuristicAgent, MctsAgent, RandomAgent};
use netrunner_core::rules::Side;

use crate::config::BotKind;

/// `Human => None` — no agent drives that side; the CLI hosts it as the
/// human seat instead (see `config::Config::corp`'s doc comment).
pub fn make_agent(kind: BotKind, side: Side, seed: u64) -> Option<Box<dyn BotAgent>> {
    match kind {
        BotKind::Human => None,
        BotKind::Random => Some(Box::new(RandomAgent::new(seed))),
        BotKind::Heuristic => Some(Box::new(HeuristicAgent::new(side, seed))),
        BotKind::Mcts => Some(Box::new(MctsAgent::new(side, seed))),
    }
}
