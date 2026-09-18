//! What a board needs that is neither a rule nor a pixel: which action a
//! click on a card or a server means (`action_map`), and what changed
//! between one view and the next (`diff`), a run as a trail of steps
//! read off the entry's own events, one beat at a time (`trail`), and
//! the words for an install's state — rezzed, advanced, hosting — as a
//! tile carries them and as its sheet lists them (`facts`), and the
//! numbers a HUD keeps in fixed places (`hud`). All of
//! it reads only the masked `ClientView` and the masked
//! `PublicHistoryEntry` a seat receives, so nothing here can show a card
//! the mask withheld.

pub mod action_map;
pub mod affordance;
pub mod diff;
pub mod facts;
pub mod hud;
pub mod phase;
pub mod trail;

pub use action_map::{table_servers, ActionEntry, ActionMap, Control, Pile, Prompt, Target};
pub use affordance::Affordance;
pub use diff::{transitions, Transition, Zone};
pub use facts::{install_facts, tile_label, tile_title, tile_tokens, Token, TokenKind};
pub use trail::{IceState, IceStep, Outcome, RunTrail, Stage};
