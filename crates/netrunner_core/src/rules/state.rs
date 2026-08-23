use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::run::{RunState, ServerId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Corp,
    Runner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct Clicks(pub u32);

impl Clicks {
    /// Returns `None` (never goes negative) if `amount` exceeds what's available.
    pub fn spend(self, amount: u32) -> Option<Self> {
        self.0.checked_sub(amount).map(Clicks)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct Credits(pub u32);

impl Credits {
    /// Gains never fail in the rules; saturate rather than ever panicking on overflow.
    pub fn gain(self, amount: u32) -> Self {
        Credits(self.0.saturating_add(amount))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct AgendaPoints(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct MemoryUnits(pub u32);

impl MemoryUnits {
    /// Returns `None` (never goes negative) if `amount` exceeds what's available.
    pub fn spend(self, amount: u32) -> Option<Self> {
        self.0.checked_sub(amount).map(MemoryUnits)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerResources {
    pub credits: Credits,
    pub clicks: Clicks,
    pub agenda_points: AgendaPoints,
}

/// A Corp card installed on a server (ICE or a non-ICE install like an
/// Asset/Agenda). `rezzed` gates card-identity visibility in the masked view:
/// an unrezzed card's identity is hidden from the Runner, but its presence
/// (server + rezzed flag) is public.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledCard {
    pub card: CardId,
    pub server: ServerId,
    pub rezzed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpState {
    pub resources: PlayerResources,
    /// Corp's hand — hidden from the Runner in the masked view.
    pub hq: Vec<CardId>,
    /// Corp's deck — hidden from the Runner in the masked view.
    pub r_and_d: Vec<CardId>,
    /// Corp's discard pile. Unlike `hq`/`r_and_d`, Archives is fully public —
    /// never masked in the masked view (see `RunnerState::rig`'s doc comment
    /// for the same pattern). Nothing currently populates this (no
    /// discard/trash mechanic exists yet); starts empty.
    pub archives: Vec<CardId>,
    pub installed: Vec<InstalledCard>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerState {
    pub resources: PlayerResources,
    /// Unspent memory units available for installing programs.
    pub memory_units: MemoryUnits,
    /// Runner's hand.
    pub grip: Vec<CardId>,
    /// Runner's deck — ordered outermost-to-innermost; drawing pops the end.
    pub stack: Vec<CardId>,
    /// Installed Hardware/Programs. Unlike Corp's `installed`, Rig cards have
    /// no hidden/unrezzed state — they're always face-up once installed.
    pub rig: Vec<CardId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    pub corp: CorpState,
    pub runner: RunnerState,
    pub active_turn: Side,
    pub active_run: Option<RunState>,
}

impl GameState {
    pub fn resources(&self, side: Side) -> &PlayerResources {
        match side {
            Side::Corp => &self.corp.resources,
            Side::Runner => &self.runner.resources,
        }
    }

    pub fn resources_mut(&mut self, side: Side) -> &mut PlayerResources {
        match side {
            Side::Corp => &mut self.corp.resources,
            Side::Runner => &mut self.runner.resources,
        }
    }
}
