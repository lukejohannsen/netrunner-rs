//! Ratings for Netrunner players and bots: Glicko-2, kept as one rating
//! per *track*, *participant* and *role*.
//!
//! **Pure and engine-free.** This crate knows nothing about `GameState`,
//! decks or transports — a match reaches it as "this participant sat as
//! the Corp, that one as the Runner, and here is who won" — so the same
//! `RatingBook` rates a human-vs-human ladder on the server, a human's
//! progress against the bots, and the bot benchmark `netrunner_cli bench`
//! runs offline. It performs no I/O: `RatingBook::to_json`/`from_json`
//! are the whole persistence contract, and whoever owns a file (the CLI,
//! the server) owns reading and writing it, the same split
//! `netrunner_core::decks` keeps with `netrunner_cli::deck_store`.
//!
//! **Glicko-2 rather than Elo**, because every consumer here has few
//! games per participant — a bot benchmark of a few dozen games a
//! seating, a human's first evenings — and Elo has no way to say "this
//! 1600 is a guess": a rating deviation is what lets a ladder be printed
//! honestly as `1612 ± 120`, and what lets a new participant move fast
//! without a hand-tuned K-factor. See `glicko2` for the update.
//!
//! **Corp and Runner are rated separately** (`Role`), on every track.
//! Netrunner is asymmetric enough that one number would average two
//! different skills; a participant's Corp rating is updated only against
//! the opponent's Runner rating, and vice versa, so the two never mix.

pub mod book;
pub mod glicko2;

pub use book::{Outcome, RatingBook, Role, Standing, Track};
pub use glicko2::{Glicko2, Rating, Score};
