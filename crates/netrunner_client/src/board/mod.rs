//! What a board needs that is neither a rule nor a pixel: which action a
//! click on a card or a server means (`action_map`), and what changed
//! between one view and the next (`diff`), a run as a trail of steps
//! read off the entry's own events, one beat at a time (`trail`), and
//! the words for an install's state — rezzed, advanced, hosting — as a
//! tile carries them and as its sheet lists them, and the state of an
//! encounter as marks on its subroutines (`facts`), and the
//! numbers a HUD keeps in fixed places (`hud`), and the rig's three rows
//! in the order the chair sees them (`rig`), and every card's way through
//! the encountered ICE with its price (`breaks`), and what a card will ask
//! before it is played (`preview`). All of
//! it reads only the masked `ClientView` and the masked
//! `PublicHistoryEntry` a seat receives, so nothing here can show a card
//! the mask withheld.

pub mod action_map;
pub mod affordance;
pub mod breaks;
pub mod diff;
pub mod facts;
pub mod hud;
pub mod onward;
pub mod phase;
pub mod preview;
pub mod rez;
pub mod rig;
pub mod timing;
pub mod trail;

pub use action_map::{table_servers, ActionEntry, ActionMap, Control, Pile, Prompt, Target};
pub use affordance::Affordance;
pub use breaks::{routes, AutoBreak, Next, Route};
pub use diff::{transitions, Transition, Zone};
pub use onward::{offered_label, onward, Onward};
pub use facts::{encounter_subroutines, install_facts, strength_words, subroutine_word, tile_label, tile_title, tile_tokens, Encounter, Subroutine, Token, TokenKind};
pub use preview::Asks;
pub use rig::RigRow;
pub use trail::{IceState, IceStep, Outcome, RunTrail, Stage};
