use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::run::{RunState, ServerId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Corp,
    Runner,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Corp => Side::Runner,
            Side::Runner => Side::Corp,
        }
    }
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

impl AgendaPoints {
    /// Gains never fail in the rules; saturate rather than ever panicking on overflow.
    pub fn gain(self, amount: u32) -> Self {
        AgendaPoints(self.0.saturating_add(amount))
    }
}

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
    /// Advancement tokens placed via `PlayerAction::AdvanceCard`. Public
    /// information even on an unrezzed card — never masked (see
    /// `masking::PublicInstalledCard`).
    pub advancement_tokens: u32,
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
    /// Agendas the Corp has scored, in scoring order. Fully public — never
    /// masked, same treatment as `archives`. `win::check_win_conditions`
    /// sums each entry's registry-defined `agenda_points` to determine
    /// whether the Corp has won, rather than reading a running counter.
    pub scored_agendas: Vec<CardId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerState {
    pub resources: PlayerResources,
    /// Unspent memory units available for installing programs.
    pub memory_units: MemoryUnits,
    /// Cumulative Brain damage taken. Permanently reduces the Runner's max
    /// hand size (see `turn::max_hand_size`) — unlike Net/Meat damage, which
    /// only discards cards once, Brain damage never heals.
    pub brain_damage: usize,
    /// Runner's tag count. Public information in the real game (visibly
    /// affects Corp trace/meat-damage abilities) — never masked, same
    /// treatment as `brain_damage`.
    pub tags: u32,
    /// Runner's hand.
    pub grip: Vec<CardId>,
    /// Runner's deck — ordered outermost-to-innermost; drawing pops the end.
    pub stack: Vec<CardId>,
    /// Installed Hardware/Programs. Unlike Corp's `installed`, Rig cards have
    /// no hidden/unrezzed state — they're always face-up once installed.
    pub rig: Vec<CardId>,
    /// Runner's discard pile. Like Corp's `archives`, this is fully public —
    /// never masked in the masked view.
    pub heap: Vec<CardId>,
    /// Agendas the Runner has stolen, in steal order. Fully public — never
    /// masked. See `CorpState::scored_agendas`'s doc comment.
    pub scored_agendas: Vec<CardId>,
}

/// Which sub-phase of a turn is currently active. `StartOfTurn`/`Action`/
/// `Discard` all carry the `Side` whose turn it is; `GameOver` carries the
/// winning `Side` instead (there's no "active side" once the game has ended).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamePhase {
    /// Entered momentarily on a turn handoff; phase-entry triggers
    /// (mandatory Corp draw) resolve here, then the engine auto-advances to
    /// `Action(side)` before returning control to a `PlayerAction`. No
    /// `PlayerAction` ever targets `StartOfTurn` directly.
    StartOfTurn(Side),
    /// The bulk of a turn: clicks are spent here (`GainCreditClick`,
    /// `InstallCard`, `InitiateRun`, etc.). Ends via `PlayerAction::EndTurn`.
    Action(Side),
    /// Mandatory hand-size cleanup before control passes to the other side.
    /// `required` is how many more cards `side` must discard — set once on
    /// entry (`hand_size - max_hand_size`) and decremented by each
    /// `PlayerAction::DiscardCard`, rather than recomputed from hand size
    /// each time.
    Discard { side: Side, required: usize },
    /// Terminal phase; carries the winning side. Reachable via
    /// `win::check_win_conditions` (agenda-point threshold, checked from
    /// `run::access_server` after a steal), `turn::enter_start_of_turn`'s
    /// deck-out check, and `damage::apply_damage`'s flatline check.
    /// Included as its own phase (rather than a separate flag) so a
    /// win-condition check only needs to set `state.phase =
    /// GamePhase::GameOver(winner)`: no `PlayerAction` handler matches
    /// `Action(_)`/`Discard { .. }` once phase is `GameOver`, so every
    /// action is rejected automatically.
    GameOver(Side),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    pub corp: CorpState,
    pub runner: RunnerState,
    pub phase: GamePhase,
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
    /// callers populate `corp`/`runner` after construction. `phase` starts at
    /// `GamePhase::Action(Side::Corp)`, matching the real game's turn order.
    pub fn new(seed: u64) -> Self {
        GameState {
            corp: CorpState {
                scored_agendas: Vec::new(),
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
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
                heap: Vec::new(),
            },
            phase: GamePhase::Action(Side::Corp),
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
