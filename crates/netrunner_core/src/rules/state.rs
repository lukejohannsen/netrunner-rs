use serde::{Deserialize, Serialize};

use crate::dsl::{CardId, Effect};
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
    /// Corp's identity card, set once by `GameState::setup`. `None` before
    /// a real game is set up — `GameState::new()`'s bare/empty state (used
    /// directly by many unit tests) has no real identity to put here, and
    /// `CardId` has no `Default` impl, so `Option` is the natural "no game
    /// set up yet" value.
    pub identity: Option<CardId>,
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
    /// Corp's persistent Bad Publicity counter. Public information — never
    /// masked, same treatment as `scored_agendas`. Seeds the Runner's
    /// temporary per-run credit pool (`run::RunState::bad_publicity_credits`)
    /// at `engine::initiate_run`.
    pub bad_publicity: u32,
}

/// A Runner card installed in the Rig (Hardware or Program), with the
/// per-instance runtime state needed for icebreaker strength: Corp's
/// `InstalledCard` already carries per-instance state (`advancement_tokens`)
/// alongside its `CardId` lookup key, but the Runner side had nothing
/// analogous — mutable strength buffs can't live on `dsl::Card` itself,
/// since that's a single shared/immutable definition in `CardRegistry`, not
/// a per-instance object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledRunnerCard {
    pub card: CardId,
    /// Printed strength, seeded once at install time from
    /// `registry.get(card).strength.unwrap_or(0)` — mirrors
    /// `RunIce::current_strength`'s seeding at `build_run_ice` exactly. `0`
    /// for Hardware and non-strength Programs.
    pub base_strength: i32,
    /// Sum of active `Effect::BoostStrength { duration: Encounter, .. }`
    /// amounts. Reset to `0` whenever the current ICE encounter ends (see
    /// `reset_encounter_strength_buffs`).
    pub encounter_strength_buff: i32,
    /// Sum of active `Effect::BoostStrength { duration: Turn, .. }` amounts.
    /// Reset to `0` at the end of the Runner's turn (see
    /// `reset_turn_strength_buffs`). Tracked separately from
    /// `encounter_strength_buff` rather than as one combined mutable total
    /// (unlike `RunIce::current_strength`) because an `Encounter` buff and a
    /// `Turn` buff can be live simultaneously and must expire independently.
    pub turn_strength_buff: i32,
}

impl InstalledRunnerCard {
    /// Base strength plus every currently-active buff.
    pub fn effective_strength(&self) -> i32 {
        self.base_strength + self.encounter_strength_buff + self.turn_strength_buff
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerState {
    /// Runner's identity card, set once by `GameState::setup`. `None`
    /// before a real game is set up — see `CorpState::identity`'s doc
    /// comment for the same rationale.
    pub identity: Option<CardId>,
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
    pub rig: Vec<InstalledRunnerCard>,
    /// Runner's discard pile. Like Corp's `archives`, this is fully public —
    /// never masked in the masked view.
    pub heap: Vec<CardId>,
    /// Agendas the Runner has stolen, in steal order. Fully public — never
    /// masked. See `CorpState::scored_agendas`'s doc comment.
    pub scored_agendas: Vec<CardId>,
    /// Static link strength, added to the Runner's bid when resolving a
    /// trace (see `TraceState`). No Identity-card mechanic exists in this
    /// engine yet and no `Effect` variant currently raises this — it starts
    /// (and normally stays) at `0` until a future identity/hardware system
    /// lands. Public information, same treatment as `tags`.
    pub link_strength: u32,
}

impl RunnerState {
    /// Clears every rig card's `Encounter`-duration strength buff. Called
    /// when the current ICE encounter ends (see
    /// `run::engine::continue_run`).
    pub fn reset_encounter_strength_buffs(&mut self) {
        for card in &mut self.rig {
            card.encounter_strength_buff = 0;
        }
    }

    /// Clears every rig card's `Turn`-duration strength buff. Called at the
    /// end of the Runner's turn (see `turn::end_turn`).
    pub fn reset_turn_strength_buffs(&mut self) {
        for card in &mut self.rig {
            card.turn_strength_buff = 0;
        }
    }

    /// Whether the Runner currently has at least one tag.
    pub fn is_tagged(&self) -> bool {
        self.tags > 0
    }
}

/// Which sub-phase of a turn is currently active. `StartOfTurn`/`Action`/
/// `Discard` all carry the `Side` whose turn it is; `GameOver` carries the
/// winning `Side` instead (there's no "active side" once the game has ended).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamePhase {
    /// Sequential opening-hand mulligan decision, entered once by
    /// `GameState::setup` (`Mulligan(Side::Corp)`). Corp's decision
    /// (`PlayerAction::KeepHand`/`TakeMulligan`) advances to
    /// `Mulligan(Side::Runner)`; the Runner's decision advances straight
    /// into Corp's first turn via `turn::enter_start_of_turn`. No
    /// `PlayerAction` other than `KeepHand`/`TakeMulligan` is legal here —
    /// every other handler gates on `Action(_)`/`Discard { .. }` via
    /// `engine::require_phase`, so it falls through to the existing
    /// `RulesError::WrongPhase` for free.
    Mulligan(Side),
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

/// A Paid Ability Window (PAW) — a priority-passing sub-loop that pauses the
/// run flow so both sides get a chance to fire paid abilities (rez ICE,
/// activate a `Trigger::Paid` ability, break a subroutine) before the engine
/// auto-advances past a checkpoint (ICE approach, ICE encounter, pre-access,
/// or a pending per-card access decision). Lives as a sibling field on
/// `GameState`, not folded into `GamePhase` — mirrors `RunPhase`'s existing
/// precedent of never changing `state.phase` mid-run (see this file's
/// `GamePhase` doc comment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaidAbilityWindow {
    pub active_priority: Side,
    pub consecutive_passes: u8,
    /// Snapshot of `state.phase` at the moment the window opened. Not
    /// currently read anywhere — `GamePhase` never changes mid-run, so this
    /// is always `Action(Side::Runner)` for every window today (all four
    /// checkpoints live inside a run). Kept for forward compatibility with a
    /// hypothetical future non-run window, where restoring `state.phase` on
    /// close would actually matter.
    pub return_phase: Box<GamePhase>,
}

/// What to do once a trace resolves (avoided or not), set by whichever
/// caller of `evaluate_effect` actually knows the answer. `evaluate_effect`
/// itself has no such context (it doesn't know if it's resolving a
/// subroutine, an on-play trigger, or anything else), so `TraceState`
/// starts with `None` and `ability::resolve_unbroken_subroutines` upgrades
/// it to `ResumeSubroutines` immediately after firing a subroutine whose
/// effect turned out to be a trace. No continuation stack is needed:
/// resuming just means calling `resolve_unbroken_subroutines` again, which
/// re-scans `RunIce::subroutines` fresh and picks up wherever it left off —
/// the same "re-derive from existing state" idiom `close_window` already
/// uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceResume {
    None,
    ResumeSubroutines,
}

/// A trace in progress. Lives as a sibling field on `GameState`, not nested
/// in `RunState` — a trace can be initiated by a standalone Operation with
/// no active run at all (Corp plays it during `GamePhase::Action(Side::
/// Corp)`), as well as by an ICE subroutine mid-`EncounterIce`, so `RunState`
/// can't be the only home for it. While `Some`, `engine::apply_action`
/// rejects every `PlayerAction` except `SubmitCorpTraceBid`/
/// `SubmitRunnerTraceBid` — unlike `PaidAbilityWindow`, a trace admits no
/// "stays legal during this" exceptions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceState {
    /// Card whose effect initiated this trace, threaded to `effect_on_success`'s
    /// `acting_card` context exactly like `evaluate_effect`'s own parameter.
    /// `None` for a subroutine-triggered trace, mirroring
    /// `resolve_unbroken_subroutines`'s existing `None` passed to
    /// `evaluate_effect` for every subroutine.
    pub initiating_card: Option<CardId>,
    pub base_strength: u32,
    /// `None` until `PlayerAction::SubmitCorpTraceBid` sets it — gates
    /// whether the pending action is the Corp's bid or the Runner's.
    pub corp_bid: Option<u32>,
    pub effect_on_success: Effect,
    pub resume: TraceResume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    pub corp: CorpState,
    pub runner: RunnerState,
    pub phase: GamePhase,
    pub active_run: Option<RunState>,
    pub paid_ability_window: Option<PaidAbilityWindow>,
    pub active_trace: Option<TraceState>,
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
            corp: CorpState { identity: None, bad_publicity: 0,
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
            runner: RunnerState { identity: None,
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
                link_strength: 0,
            },
            phase: GamePhase::Action(Side::Corp),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, base: i32, encounter_buff: i32, turn_buff: i32) -> InstalledRunnerCard {
        InstalledRunnerCard {
            card: CardId(id.to_string()),
            base_strength: base,
            encounter_strength_buff: encounter_buff,
            turn_strength_buff: turn_buff,
        }
    }

    #[test]
    fn effective_strength_sums_base_and_both_buffs() {
        assert_eq!(card("corroder", 2, 1, 3).effective_strength(), 6);
    }

    #[test]
    fn reset_encounter_strength_buffs_zeroes_only_encounter_buff() {
        let mut runner = RunnerState { identity: None,
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
            rig: vec![card("corroder", 2, 1, 3)],
            heap: Vec::new(),
            scored_agendas: Vec::new(),
            link_strength: 0,
        };

        runner.reset_encounter_strength_buffs();

        assert_eq!(runner.rig[0].encounter_strength_buff, 0);
        assert_eq!(runner.rig[0].turn_strength_buff, 3);
    }

    #[test]
    fn reset_turn_strength_buffs_zeroes_only_turn_buff() {
        let mut runner = RunnerState { identity: None,
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
            rig: vec![card("corroder", 2, 1, 3)],
            heap: Vec::new(),
            scored_agendas: Vec::new(),
            link_strength: 0,
        };

        runner.reset_turn_strength_buffs();

        assert_eq!(runner.rig[0].turn_strength_buff, 0);
        assert_eq!(runner.rig[0].encounter_strength_buff, 1);
    }
}
