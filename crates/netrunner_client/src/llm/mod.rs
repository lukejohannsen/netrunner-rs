//! An opponent played by a language model the person configured
//! (Phase 7 §11): their own key, their own provider, or a server on their
//! own machine, seated where the planner sits.
//!
//! **Why here and not in `netrunner_bots`.** The bots crate depends on
//! `netrunner_core` alone, and a model has to be shown the board in
//! words — the words this crate already has for an action
//! (`actions::describe_action`), a prompt (`prose::decision_prompt`), a
//! card (`card_text`) and a server (`board::table_servers`). A second set
//! of words in the bots crate would drift from the ones a person reads,
//! so the agent lives beside them and implements the bots' `BotAgent`,
//! which `netrunner_session::Seat::Agent` takes from anywhere.
//!
//! **One request per decision, never a transcript.** A conversation that
//! re-sends every earlier turn grows with the square of the turn count; a
//! fresh request stays flat at a few thousand tokens. The system prompt
//! is built once per game (`prompt::system_prompt`) — the rules in this
//! project's words and the bot's own deck's card texts — so a provider
//! that caches a repeated prefix serves it at its cache rate, and the
//! per-decision message names own cards by title alone. Continuity is the
//! model's own last few moves, listed back to it.
//!
//! **The model is asked only when there is something to decide**, and it
//! answers with a number off a menu of the legal actions. A menu of one
//! is taken unasked; under `profile::Asks::Clicks` a priority window is
//! the planner's. Whatever the model says, the action played is one of
//! `view.legal_actions`, so the session (which panics on an illegal one)
//! never sees what the model wrote.
//!
//! **When the model fails, the planner plays that one decision** and the
//! person is told in a line on the board (`agent::LlmAgent`): a bad key, a
//! server that is down, a refusal, or an answer that names no action
//! after one retry. The game goes on, and the model is asked again at the
//! next decision. Stopping the game was rejected: a person who set a
//! model up to play a game should get a game.
//!
//! **A key is never in the settings file.** `settings.toml` is the file a
//! person pastes into a bug report; a profile's key is in `secrets.toml`
//! beside the identity key, created readable by its owner alone
//! (`secrets`), and the settings hold the profile's name, protocol, URL
//! and model (`profile`).

pub mod agent;
pub mod profile;
pub mod prompt;
pub mod protocol;
pub mod secrets;
pub mod transport;

#[cfg(test)]
pub(crate) mod fixture;

pub use agent::LlmAgent;
pub use protocol::Request;
pub use protocol::{ProviderError, Usage};
pub use profile::{profile_problem, Asks, LlmProfile, Preset, Protocol};
pub use secrets::{ApiKey, Secrets};
pub use transport::{HttpTransport, ScriptedTransport, Transport};

/// The one trivial request a Test button sends: whether the URL, the
/// key and the model name reach a model at all. Built the way a decision
/// is, so what passes here is what a game will send.
pub fn probe(profile: &LlmProfile, key: Option<&ApiKey>) -> Result<Request, ProviderError> {
    protocol::build(profile, key, "You are being tested by a card game's settings screen.", "Reply with the single word OK.")
}

/// A probe's answer in a line for the screen: the model's word, or why
/// none came.
pub fn probe_answer(profile: &LlmProfile, response: Result<transport::Response, transport::TransportError>) -> Result<String, String> {
    let response = response.map_err(|error| error.to_string())?;
    let reply = protocol::read(profile.protocol, response.status, &response.body).map_err(|error| error.to_string())?;
    let word: String = reply.text.split_whitespace().take(8).collect::<Vec<_>>().join(" ");
    Ok(format!("The model answered: {word}"))
}
