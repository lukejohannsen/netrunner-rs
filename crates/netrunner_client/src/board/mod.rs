//! What a board needs that is neither a rule nor a pixel: which action a
//! click on a card or a server means (`action_map`), and what changed
//! between one view and the next (`diff`). Both read only the masked
//! `ClientView` and the masked `PublicHistoryEntry` a seat receives, so
//! nothing here can show a card the mask withheld.

pub mod action_map;
pub mod diff;

pub use action_map::{ActionEntry, ActionMap, Prompt, Target};
pub use diff::{transitions, Transition, Zone};
