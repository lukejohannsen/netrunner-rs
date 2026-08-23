use serde::{Deserialize, Serialize};

use crate::dsl::{CardId, DamageType};
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
    CardAccessed { card: CardId, server: ServerId },
    TurnEnded { side: Side },
    TurnStarted { side: Side, clicks: u32 },
    DiscardPending { side: Side, required: usize },
    CardDiscarded { side: Side, card: CardId },
    AgendaStolen { card: CardId, agenda_points: u32 },
    DamageTaken { damage_type: DamageType, amount: usize },
    RunnerFlatlined,
    CreditsSpent { side: Side, amount: u32 },
    TagsGiven { side: Side, amount: u32 },
    TagsPurged { side: Side },
    CardTrashed { side: Side, card: CardId },
    RunEndedByEffect { server: ServerId },
    GameOver { winner: Side },
}
