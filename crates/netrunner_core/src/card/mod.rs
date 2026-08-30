mod dto;
mod error;
mod id;
mod pack;

use serde::{Deserialize, Serialize};

pub use dto::NetrunnerDbCardDto;
pub use error::CardConversionError;
pub use id::CardId;
pub use pack::{NetrunnerDbPackDto, PackInfo};

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
