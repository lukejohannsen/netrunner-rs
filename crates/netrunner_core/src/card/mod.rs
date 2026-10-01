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
}
