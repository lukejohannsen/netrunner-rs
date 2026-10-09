mod error;
mod id;

use serde::{Deserialize, Serialize};

pub use error::CardConversionError;
pub use id::PrintingId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Faction {
    Anarch,
    Criminal,
    Shaper,
    HaasBioroid,
    Jinteki,
    Nbn,
    WeylandConsortium,
    NeutralCorp,
    NeutralRunner,
    /// The three Runner mini-factions of the Fantasy Flight Games era,
    /// each an identity and a handful of cards (NetrunnerDB's `adam`,
    /// `apex`, `sunny_lebeau`). A faction like any other to deckbuilding:
    /// their cards are in-faction only for their own identity. Appended,
    /// so no serialized faction moves.
    Adam,
    Apex,
    SunnyLebeau,
}
