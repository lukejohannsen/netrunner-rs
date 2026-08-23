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

/// Whether an installed card occupies a server's ICE-protection slot or its
/// "root" (content) slot. Lets `run::access_server` correctly exclude ICE
/// from what a successful run accesses without needing a full `CardRegistry`
/// lookup of the card's `dsl::CardType` — the installing action declares
/// this explicitly at install time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallSlot {
    Ice,
    Root,
}

/// A Corp card installed on a server (ICE or a non-ICE install like an
/// Asset/Agenda). `rezzed` gates card-identity visibility in the masked view:
/// an unrezzed card's identity is hidden from the Runner, but its presence
/// (server + rezzed flag) is public.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledCard {
    pub card: CardId,
    pub server: ServerId,
    pub slot: InstallSlot,
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
    /// Fixed seed for this game's deterministic pseudo-randomness (e.g.
    /// which HQ card a run accesses). Never mutated after construction —
    /// only `rng_step` advances — so replaying the same `(GameState,
    /// PlayerAction)` history always produces bit-identical results.
    pub seed: u64,
    /// How many pseudo-random values have been drawn so far. Advanced by
    /// `next_u64`; part of `GameState` (rather than living outside it, or
    /// being threaded through `PlayerAction`) so `apply_action` stays a pure
    /// function of its two explicit inputs even when it needs "randomness".
    pub rng_step: u64,
}

impl GameState {
    /// A fresh game state seeded for deterministic pseudo-randomness. Corp
    /// and Runner zones start empty and resources start at zero — real game
    /// setup (starting hands/decks/credits) isn't modeled by this engine yet;
    /// callers populate `corp`/`runner` after construction. Corp is `Side`'s
    /// `active_turn` value, matching the real game's turn order.
    pub fn new(seed: u64) -> Self {
        GameState {
            corp: CorpState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
            },
            active_turn: Side::Corp,
            active_run: None,
            seed,
            rng_step: 0,
        }
    }

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

    /// Deterministically advances `rng_step` and returns a pseudo-random
    /// `u64` derived purely from `(seed, rng_step)`. Uses a fixed SplitMix64
    /// finalizer rather than `std`'s `DefaultHasher` — `DefaultHasher`'s
    /// algorithm is explicitly unspecified and not guaranteed stable across
    /// Rust versions/platforms, whereas this needs to keep producing
    /// bit-identical results everywhere `netrunner_core` runs (client,
    /// server, gym) forever, not just within one process/build.
    pub fn next_u64(&mut self) -> u64 {
        self.rng_step = self.rng_step.wrapping_add(1);
        let mut z = self
            .seed
            .wrapping_add(self.rng_step.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}
