//! The deck editor's state is `netrunner_client::deck_editor`, shared
//! with the terminal's builder since Phase 7 §16 stage (c); the screen
//! (`screens::deck_editor`) drives it with the same intents.

pub use netrunner_client::deck_editor::{Editor, Intent, Outcome};
