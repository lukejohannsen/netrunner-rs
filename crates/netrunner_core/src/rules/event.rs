use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::run::ServerId;
use crate::rules::state::Side;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEvent {
    ClickSpent { side: Side },
    CreditsGained { side: Side, amount: u32 },
    CardDrawn { side: Side },
    IceApproached { server: ServerId, position: u32 },
    IceEncountered { server: ServerId, position: u32 },
    SubroutineResolved { server: ServerId, position: u32, remaining: u32 },
    SubroutineBroken { server: ServerId, position: u32, remaining: u32 },
    IcePassed { server: ServerId, position: u32 },
    RunSucceeded { server: ServerId },
    RunJackedOut { server: ServerId },
    RunCompleted { server: ServerId },
    CardInstalled { side: Side, card: CardId, server: ServerId },
    IceRezzed { card: CardId, server: ServerId },
    RunInitiated { server: ServerId },
    EventPlayed { side: Side, card: CardId },
    HardwareInstalled { side: Side, card: CardId },
    ProgramInstalled { side: Side, card: CardId, memory_cost: u8 },
}
