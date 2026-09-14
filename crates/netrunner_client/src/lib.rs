//! The toolkit-agnostic client core.
//!
//! Two clients play this engine — the ratatui terminal (`netrunner_cli`)
//! and the Bevy desktop (`netrunner_desktop`) — and a player expects them
//! to be one game: the name they set in either, the decks they build in
//! either and the rating they earn in either are the same files. That is
//! only true if one crate owns those files. This crate is that owner.
//!
//! **Why lifted rather than copied.** The settings file has no
//! `deny_unknown_fields` (a player's hand-edited file should not be
//! refused over a stray key), so a field one client did not know about
//! would be dropped the next time that client saved. Two `Settings`
//! structs would silently erase each other's preferences; one struct
//! cannot.
//!
//! **What does not live here.** Nothing that renders and nothing that
//! decides a rule. The client contract (AGENTS.md §3) is that a client
//! renders a `ClientView` and submits from `legal_actions`; this crate
//! carries the parts of a client that are neither — file paths,
//! persistence, and the pool a format allows — and, as later phases land,
//! the one match-driving interface every screen consumes.

pub mod cards;
pub mod deck_store;
pub mod decks;
pub mod ratings;
pub mod settings;
