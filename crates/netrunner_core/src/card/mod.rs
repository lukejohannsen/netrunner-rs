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

impl Faction {
    /// The faction as the cards print it.
    pub fn name(self) -> &'static str {
        match self {
            Faction::Anarch => "Anarch",
            Faction::Criminal => "Criminal",
            Faction::Shaper => "Shaper",
            Faction::HaasBioroid => "Haas-Bioroid",
            Faction::Jinteki => "Jinteki",
            Faction::Nbn => "NBN",
            Faction::WeylandConsortium => "Weyland Consortium",
            Faction::NeutralCorp | Faction::NeutralRunner => "Neutral",
            Faction::Adam => "Adam",
            Faction::Apex => "Apex",
            Faction::SunnyLebeau => "Sunny Lebeau",
        }
    }

    /// The factions an identity may be from on each side, in the order
    /// the cards list them: the ones an event's rule can name (Phase 4
    /// §11). The neutral marks are not among them, since no identity is
    /// neutral.
    pub const CORP: [Faction; 4] = [Faction::HaasBioroid, Faction::Jinteki, Faction::Nbn, Faction::WeylandConsortium];
    pub const RUNNER: [Faction; 6] = [Faction::Anarch, Faction::Criminal, Faction::Shaper, Faction::Adam, Faction::Apex, Faction::SunnyLebeau];
}
