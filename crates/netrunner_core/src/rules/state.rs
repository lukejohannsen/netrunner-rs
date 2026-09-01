use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::dsl::{CardFilter, CardId, CardTarget, CardZoneRef, Cost, DamageType, Effect, Trigger};
use crate::rules::event::GameEvent;
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

/// A handle on one installed *instance*, unique for the life of a game and
/// allocated by `GameState::allocate_install_id`.
///
/// **Public information, never masked.** It names the physical card sitting
/// on the table, not what that card is: install order is something both
/// players watch happen, exactly like the already-public
/// `InstalledCard::advancement_tokens` and `installed_this_turn`. That is
/// what lets a `PlayerAction` refer to an unrezzed Corp card — which real
/// Netrunner requires, since the Runner may host a Trojan on unrezzed ICE
/// and may swap ICE they cannot identify — without naming its `CardId` and
/// leaking the identity the masking layer is there to hide.
///
/// It names the instance, not the position: an `InstallId` travels with its
/// card through a `SwapInstalledIce`, and two copies of the same card are
/// two different ids. That is what distinguishes it from the
/// first-match-by-`CardId` lookup it replaced, which aliased every copy of a
/// card onto the first one's `ActionSpace` slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct InstallId(pub u32);

impl InstallId {
    /// The id no real install ever gets. `allocate_install_id` counts from
    /// `1`, so `0` is free to act as the "not a real install" placeholder —
    /// the same role the empty `CardId` plays in `InstalledCard::default`,
    /// and what a test fixture built with `..Default::default()` receives.
    /// Two fixtures that both take the placeholder are therefore
    /// indistinguishable by id; any test that cares must set it.
    pub const PLACEHOLDER: InstallId = InstallId(0);
}

impl Default for InstallId {
    fn default() -> Self {
        InstallId::PLACEHOLDER
    }
}

/// A Corp card installed on a server (ICE or a non-ICE install like an
/// Asset/Agenda). `rezzed` gates card-identity visibility in the masked view:
/// an unrezzed card's identity is hidden from the Runner, but its presence
/// (server + rezzed flag + `install_id`) is public.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledCard {
    pub card: CardId,
    /// This install's public handle — see [`InstallId`]. Every
    /// `PlayerAction` naming this card names it by this, not by `card`.
    #[serde(default)]
    pub install_id: InstallId,
    pub server: ServerId,
    pub slot: InstallSlot,
    pub rezzed: bool,
    /// Advancement tokens placed via `PlayerAction::AdvanceCard`. Public
    /// information even on an unrezzed card — never masked (see
    /// `masking::PublicInstalledCard`).
    pub advancement_tokens: u32,
    /// Generic counters (virus/power/credit — see `dsl::card::CounterKind`)
    /// placed by `Effect::AddCounters`/removed by `Effect::RemoveCounters`.
    /// Exposed through `masking::PublicInstalledCard::counters` under the
    /// same visibility rule as the card's identity: visible to the owner
    /// always, to the opponent once rezzed. Hidden on an unrezzed card,
    /// where a credit total would betray what the card is.
    #[serde(default)]
    pub counters: u32,
    /// Whether this card was installed during the Corp's current turn —
    /// read by `dsl::zone::CardFilter::NotInstalledThisTurn` (Seamless
    /// Launch's "1 installed card that you did not install this turn").
    /// Set at `engine::install_card`, cleared for every installed card at
    /// the start of each Corp turn (`turn::enter_start_of_turn`). Public
    /// information — install timing is visible to both players even for an
    /// unrezzed card, same as `advancement_tokens`.
    #[serde(default)]
    pub installed_this_turn: bool,
}

/// Every field at its neutral value, so test fixtures can spell out only the
/// fields they care about via `..Default::default()` instead of restating all
/// of them and breaking every time one is added (see `dsl::CardDefinition`'s
/// `Default` for the same rationale and the M9 precedent).
///
/// `card`, `server`, and `slot` have no meaningful neutral value — an empty
/// `CardId` names no card, and every real install picks a server and slot
/// deliberately. The placeholders exist only so `Default` can be implemented
/// at all; any caller that cares must override them. Production construction
/// sites deliberately do **not** use this — they stay exhaustive so the
/// compiler forces a decision about each new field.
impl Default for InstalledCard {
    fn default() -> Self {
        Self {
            card: CardId(String::new()),
            install_id: InstallId::PLACEHOLDER,
            server: ServerId::Hq,
            slot: InstallSlot::Root,
            rezzed: false,
            advancement_tokens: 0,
            counters: 0,
            installed_this_turn: false,
        }
    }
}

/// One card in the Corp's Archives, plus which way up it is.
///
/// A card is faceup exactly when the Runner has already seen it — trashed
/// from play while rezzed, a resolved Operation, or a card the Runner
/// accessed and trashed. It is facedown when the Runner never saw it: the
/// Corp's own discards from HQ, cards milled off R&D, and unrezzed installs
/// trashed off the table. Only the orientation is public to the Runner; a
/// facedown card's identity is masked out of their view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedCard {
    pub card: CardId,
    pub facedown: bool,
}

impl ArchivedCard {
    /// The Runner has seen this card — a rezzed install trashed off the
    /// table, a resolved Operation, or a card they accessed and trashed.
    pub fn faceup(card: CardId) -> Self {
        ArchivedCard { card, facedown: false }
    }

    /// The Runner never saw this card — a Corp discard from HQ, an R&D
    /// mill, or an unrezzed install trashed off the table.
    pub fn facedown(card: CardId) -> Self {
        ArchivedCard { card, facedown: true }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Corp's discard pile. Unlike `hq`/`r_and_d`, the *shape* of Archives
    /// is public — both players always see how many cards are here and
    /// which way up each one is — but a facedown card's identity is hidden
    /// from the Runner (see `masking::PublicArchivedCard`). The Corp always
    /// sees its own zone in full.
    pub archives: Vec<ArchivedCard>,
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
    /// Whether the Corp's "first install this turn" bonus
    /// (`Trigger::OnInstall` gated by `EffectRequirement::
    /// FirstInstallThisTurn`, e.g. Haas-Bioroid: Engineering the Future) has
    /// already fired this turn. Reset to `false` at the start of every Corp
    /// turn (`turn::enter_start_of_turn`); consumed (flipped to `true`) by
    /// `ability::process_card_triggers` the moment a gated `TriggeredEffect`
    /// actually fires.
    pub first_install_used_this_turn: bool,
    /// Corp's current recurring-credit pool, spendable on trace bids before
    /// the Corp's own wallet (`ability::pay_cost`'s `Cost::Credits` arm,
    /// mirroring the Runner's `RunState::bad_publicity_credits`-before-wallet
    /// precedent but keyed on `GameState::active_trace` instead of an active
    /// run). Refilled to `recurring_credits_max` unconditionally at the
    /// start of every Corp turn.
    pub recurring_credits: u32,
    /// The size `recurring_credits` refills to each Corp turn, set once at
    /// `GameState::setup` from the Corp identity's registry `CardDefinition::
    /// recurring_credits` (`0` for an identity with no such pool, e.g.
    /// every identity but NBN: Making News in the baseline set).
    pub recurring_credits_max: u32,
    /// Sum of printed agenda points on agendas scored this Corp turn — read
    /// by `dsl::effect::Amount::AgendaPointsScoredThisTurn` (e.g.
    /// Neurospike). Incremented in `engine::score_agenda`, reset to `0` at
    /// the start of every Corp turn (`turn::enter_start_of_turn`).
    #[serde(default)]
    pub agenda_points_scored_this_turn: u32,
    /// Tags consumed by `EffectRequirement::OncePerTurn(tag)` gates the Corp
    /// has already fired this turn — the generalized replacement for adding
    /// another bespoke per-effect bool alongside `first_install_used_this_turn`.
    /// Cleared at the start of every Corp turn (`turn::enter_start_of_turn`).
    #[serde(default)]
    pub once_per_turn_used: HashSet<String>,
    /// Permanent additive bonus to the Corp's max hand size (`turn::
    /// max_hand_size`), set once at `GameState::setup` from the Corp
    /// identity's registry `CardDefinition::max_hand_size_bonus` — e.g.
    /// Haas-Bioroid: Precision Design's "+1 maximum hand size". `0` for the
    /// common case (no such identity trait). Unlike `recurring_credits_max`,
    /// this never refills/resets — it's a one-time, permanent addition, the
    /// same treatment `RunnerState::brain_damage` gives the Runner's side
    /// (just additive instead of subtractive).
    #[serde(default)]
    pub max_hand_size_bonus: u32,
    /// Whether the Corp is barred from scoring any further agenda for the
    /// remainder of this turn — set by Luminal Transubstantiation's own
    /// score trigger ("You cannot score agendas for the remainder of the
    /// turn"), reset at the start of every Corp turn
    /// (`turn::enter_start_of_turn`). Enforced in
    /// `legal_actions::advance_score_trash_candidates` so `ScoreAgenda` is
    /// never even offered, keeping the action mask and `engine::score_agenda`'s
    /// own guard in agreement.
    #[serde(default)]
    pub cannot_score_agendas_this_turn: bool,
    /// Cards removed from the game entirely — Spin Doctor's "Remove this
    /// asset from the game" cost. Deliberately *not* Archives: a removed
    /// card must never be recurrable, accessible, or counted by anything
    /// reading the discard pile (e.g. Jinteki: Restoring Humanity's
    /// facedown-in-Archives check). Public information, never masked; write
    /// only, since nothing in this card pool ever reads a card back out.
    #[serde(default)]
    pub removed_from_game: Vec<CardId>,
}

impl CorpState {
    /// Whether `card_id` is in Archives at all, regardless of orientation.
    pub fn archives_contains(&self, card_id: &CardId) -> bool {
        self.archives.iter().any(|a| &a.card == card_id)
    }

    /// Whether Archives holds at least one facedown card — backs
    /// `EffectRequirement::ArchivesHasFacedownCard` (Jinteki: Restoring
    /// Humanity).
    pub fn has_facedown_in_archives(&self) -> bool {
        self.archives.iter().any(|a| a.facedown)
    }
}

/// A Runner card installed in the Rig (Hardware or Program), with the
/// per-instance runtime state needed for icebreaker strength: Corp's
/// `InstalledCard` already carries per-instance state (`advancement_tokens`)
/// alongside its `CardId` lookup key, but the Runner side had nothing
/// analogous — mutable strength buffs can't live on `dsl::CardDefinition` itself,
/// since that's a single shared/immutable definition in `CardRegistry`, not
/// a per-instance object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledRunnerCard {
    pub card: CardId,
    /// This install's public handle — see [`InstallId`]. The rig is never
    /// masked, so unlike the Corp's side there is no identity here to
    /// protect; it exists so `TrashResource`/`ActivateAbility` can name one
    /// specific copy of a card the Runner has installed twice.
    #[serde(default)]
    pub install_id: InstallId,
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
    /// Generic counters (virus/power/credit — see `dsl::card::CounterKind`)
    /// placed by `Effect::AddCounters`/removed by `Effect::RemoveCounters`.
    /// Exposed through `masking::PublicInstalledRunnerCard::counters`
    /// unmasked — a rig card is always face-up, so unlike the Corp's
    /// `InstalledCard::counters` there is nothing here to hide.
    #[serde(default)]
    pub counters: u32,
    /// The Corp installed ICE this card is hosted on, if it was installed
    /// via `PlayerAction::InstallProgramOnIce` (a Trojan Program, `dsl::
    /// CardDefinition::installs_on_ice`) — e.g. Botulus, Tranquilizer.
    /// `None` for every ordinary Rig card. A hosted card otherwise behaves
    /// exactly like any other Rig entry (same strength/counter machinery)
    /// — this field only records *where* it's attached, for cascade-trash
    /// (see `ability::cascade_trash_hosted_programs`) and for abilities
    /// that need to reference their own host (e.g. `EffectRequirement::
    /// EncounteringHostIce`, `CardTarget::HostIce`).
    #[serde(default)]
    pub hosted_on_ice: Option<CardId>,
}

impl InstalledRunnerCard {
    /// Base strength plus every currently-active buff.
    pub fn effective_strength(&self) -> i32 {
        self.base_strength + self.encounter_strength_buff + self.turn_strength_buff
    }
}

/// Every field at its neutral value, for test fixtures — see
/// `InstalledCard`'s `Default` for the full rationale. `card` is the only
/// field with no meaningful neutral value; every caller that cares must
/// override it. Production sites stay exhaustive.
impl Default for InstalledRunnerCard {
    fn default() -> Self {
        Self {
            card: CardId(String::new()),
            install_id: InstallId::PLACEHOLDER,
            base_strength: 0,
            encounter_strength_buff: 0,
            turn_strength_buff: 0,
            counters: 0,
            hosted_on_ice: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerState {
    /// Runner's identity card, set once by `GameState::setup`. `None`
    /// before a real game is set up — see `CorpState::identity`'s doc
    /// comment for the same rationale.
    pub identity: Option<CardId>,
    pub resources: PlayerResources,
    /// Memory units free for installing programs.
    ///
    /// **A cached report, not the authority.** The number is derived from
    /// what is installed — `memory::available_memory` — and refreshed at a
    /// single choke point in `engine::apply_action` after every handler.
    /// Do not decrement it on install or restore it on trash; change the
    /// rig and the report follows.
    ///
    /// It was a spent resource until memory was derived, which meant a
    /// program's cost was never given back when it left play. See
    /// `rules::memory` for why deriving beats refunding here, and
    /// `GamePhase::Discard`'s `required` for the same "stored value became
    /// a report" move made earlier for the discard count.
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
    /// trace (see `TraceState`). **Structurally always `0` today:** no
    /// `CardDefinition` field carries printed link and no `Effect` raises
    /// it, so identities and hardware with link are silently linkless
    /// (ROADMAP Rules Audit, Tier 2). Nothing in the current pool has a
    /// Trace, which is why this is recorded rather than fixed. Public
    /// information, same treatment as `tags`.
    pub link_strength: u32,
    /// Whether the Runner's "first successful HQ run this turn" bonus
    /// (`Trigger::OnSuccessfulRunOnHq` gated by `EffectRequirement::
    /// FirstSuccessfulHqRunThisTurn`, e.g. Gabriel Santiago) has already
    /// fired this turn. Reset to `false` at the start of every Runner turn;
    /// consumed the same way as `CorpState::first_install_used_this_turn`.
    pub first_hq_run_used_this_turn: bool,
    /// Whether the Runner's identity install-cost discount (`CardDefinition::
    /// first_install_discount`, e.g. Kate "Mac" McCaffrey) has already been
    /// applied to a Program/Hardware install this turn. Reset to `false` at
    /// the start of every Runner turn; consumed directly by
    /// `engine::install_hardware`/`install_program` (not a `Trigger`/
    /// `Effect` — see `CardDefinition::first_install_discount`'s doc comment for why).
    pub first_install_discount_used_this_turn: bool,
    /// Tags consumed by `EffectRequirement::OncePerTurn(tag)` gates the
    /// Runner has already fired this turn — see `CorpState::
    /// once_per_turn_used`'s doc comment for the full rationale. Cleared at
    /// the start of every Runner turn.
    #[serde(default)]
    pub once_per_turn_used: HashSet<String>,
    /// Whether the Runner has made at least one successful run this turn
    /// (any server) — set by `dispatcher::dispatch_event`'s `RunSucceeded`
    /// arm, reset to `false` at the start of every Runner turn. Backs
    /// `EffectRequirement::RunnerMadeSuccessfulRunLastTurn` via
    /// `made_successful_run_last_turn` below, and (from M5 on) install-cost
    /// discounts like Carmen's.
    #[serde(default)]
    pub made_successful_run_this_turn: bool,
    /// Snapshot of `made_successful_run_this_turn` taken when the Runner's
    /// turn ends (`turn::end_turn`), read by `EffectRequirement::
    /// RunnerMadeSuccessfulRunLastTurn` — e.g. Public Trail's play
    /// requirement ("play only if the Runner made a successful run during
    /// their last turn").
    #[serde(default)]
    pub made_successful_run_last_turn: bool,
    /// Permanent additive bonus to the Runner's max hand size (`turn::
    /// max_hand_size`) — the sum of every installed Hardware's registry
    /// `CardDefinition::max_hand_size_bonus` (e.g. T400 Memory Diamond's
    /// "+1 maximum hand size") plus any Agenda-scored `Effect::
    /// GainMaxHandSize` (e.g. Superconducting Hub) and identity-level bonus
    /// read once at `GameState::setup`. Deliberately one-way/permanent: it
    /// is **not** board-derived the way `memory_units` now is, because
    /// Agendas and identities contribute to it as well as Hardware, so
    /// summing the rig would not reproduce it. That is the whole difference
    /// between the two — see `rules::memory`.
    #[serde(default)]
    pub max_hand_size_bonus: u32,
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

/// What a `PaidAbilityWindow` is pausing, and therefore what closing it must
/// resume. `Run` re-derives its continuation purely from `state.active_run`'s
/// current `RunPhase`, exactly as `close_window` always has — this variant
/// exists so non-run checkpoints have an explicit alternative to encode,
/// since `active_run` is `None` for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowCheckpoint {
    Run,
    /// Opened once `side`'s mandatory start-of-turn steps (click refill,
    /// mandatory draw, `Trigger::OnTurnStart` reactions) have already
    /// resolved. Closing sets `state.phase = GamePhase::Action(side)`.
    StartOfTurn { side: Side },
    /// Opened once `side`'s end-of-turn cleanup (turn-duration strength-buff
    /// reset) has resolved, before the mandatory hand-size check. Closing
    /// resumes exactly where `turn::end_turn` paused: `turn::finish_end_turn`.
    EndOfTurn { side: Side },
    /// Opened the instant a `DealDamage`/`TrashCard` effect is parked in
    /// `GameState::pending_prevention` (only when at least one installed/
    /// rigged card actually has a matching `Effect::PreventDamage`/
    /// `PreventTrash` `Paid` ability — see `ability::evaluate_effect`'s
    /// `DealDamage`/`TrashCard` arms). Closing applies whatever's left
    /// unprevented via `paid_ability::close_window`'s `Prevention` arm.
    Prevention,
    /// Opened after `side` takes an ordinary basic click action, so their
    /// **opponent** gets a chance to use a paid ability before play
    /// continues. Closing just restores `GamePhase::Action(side)` — there
    /// is nothing to resume, unlike every other checkpoint.
    ///
    /// `side` is the *acting* player, not the one this window exists for.
    /// The active player needs no window of their own: `activate_ability`
    /// already permits their paid abilities throughout `Action(side)`. So
    /// this checkpoint's whole purpose is the opponent's opportunity, and
    /// it is opened **only when they actually have a usable paid ability**
    /// (`paid_ability::has_usable_paid_ability`) — otherwise every click
    /// action would cost both players a `PassPriority` for nothing.
    PostAction { side: Side },
}

/// A Paid Ability Window (PAW) — a priority-passing sub-loop that pauses the
/// run flow so both sides get a chance to fire paid abilities (rez ICE,
/// activate a `Trigger::Paid` ability, break a subroutine) before the engine
/// auto-advances past a checkpoint (ICE approach, ICE encounter, pre-access,
/// a pending per-card access decision, or a turn boundary). Lives as a
/// sibling field on `GameState`, not folded into `GamePhase` — mirrors
/// `RunPhase`'s existing precedent of never changing `state.phase` mid-run
/// (see this file's `GamePhase` doc comment); the `StartOfTurn`/`EndOfTurn`
/// checkpoints extend that same precedent to turn boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaidAbilityWindow {
    pub active_priority: Side,
    pub consecutive_passes: u8,
    /// What this window is pausing — determines how `close_window` resumes.
    pub checkpoint: WindowCheckpoint,
    /// Snapshot of `state.phase` at the moment the window opened. Not
    /// currently read anywhere — every checkpoint's `close_window` arm
    /// either re-derives its continuation from `active_run` (`Run`) or
    /// carries its own `side` (`StartOfTurn`/`EndOfTurn`). Kept for forward
    /// compatibility with a hypothetical future checkpoint that genuinely
    /// needs to restore an arbitrary prior phase.
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
    /// CardDefinition whose effect initiated this trace, threaded to `effect_on_success`'s
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

/// What to do once a `PendingPrevention` resolves, mirroring `TraceResume`'s
/// exact role/rationale — `ability::resolve_unbroken_subroutines` upgrades
/// this to `ResumeSubroutines` immediately after firing a subroutine whose
/// effect turned out to be a `DealDamage`/`TrashCard` that got parked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreventionResume {
    None,
    ResumeSubroutines,
}

/// Which kind of `PendingPrevention` is parked — used only to name the two
/// sides of a mismatch in `RulesError::PreventionKindMismatch`;
/// `PendingPreventionKind` itself carries the actual payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreventionKind {
    Damage,
    Trash,
}

/// What's parked, waiting on a `WindowCheckpoint::Prevention` window before
/// it actually applies. `prevented` tracks how much of it has been
/// prevented so far — incrementally for `Damage` (`Effect::PreventDamage`
/// saturating-reduces `amount`), all-or-nothing for `Trash`
/// (`Effect::PreventTrash` sets it outright, since real Netrunner trash
/// prevention isn't partial).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingPreventionKind {
    Damage { damage_type: DamageType, amount: usize, prevented: usize },
    Trash { target: CardTarget, prevented: bool },
}

/// An effect paused mid-resolution so both sides get a `PaidAbilityWindow`
/// to respond with a matching `Effect::PreventDamage`/`PreventTrash` `Paid`
/// ability before it actually applies — the same "park in `GameState`,
/// block unrelated actions via the window that's opened alongside it, and
/// resume on window close" idiom `TraceState` already established for
/// `Effect::Trace`. Lives as a sibling field on `GameState`, not nested in
/// `RunState`, for the same reason `TraceState` does: a standalone
/// Operation with no active run can deal damage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPrevention {
    pub kind: PendingPreventionKind,
    /// CardDefinition whose effect triggered this — same role as `TraceState::
    /// initiating_card`/`evaluate_effect`'s `acting_card` parameter.
    pub source_card: Option<CardId>,
    pub resume: PreventionResume,
}

/// What to do once a `PendingPaidChoice` resolves, mirroring `TraceResume`/
/// `PreventionResume`'s exact role — `ability::resolve_unbroken_subroutines`
/// upgrades this to `ResumeSubroutines` immediately after firing a
/// subroutine whose effect turned out to be an `Effect::OfferPaidChoice`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingPaidChoiceResume {
    None,
    ResumeSubroutines,
}

/// A parked `Effect::OfferPaidChoice`, awaiting `PlayerAction::
/// AcceptPendingPaidChoice`/`DeclinePendingPaidChoice`. Lives as a sibling
/// field on `GameState`, not nested in `RunState`, for the same reason
/// `TraceState`/`PendingPrevention` do: a standalone Operation with no
/// active run can offer one (e.g. Public Trail). While `Some`,
/// `engine::apply_action` rejects every `PlayerAction` except the two
/// above — see `Effect::OfferPaidChoice`'s doc comment for why this is a
/// deliberate second mechanism alongside `dsl::ability::
/// InteractiveOnAccess` rather than a generalization of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPaidChoice {
    pub side: Side,
    pub cost: Cost,
    pub if_paid: Effect,
    pub if_declined: Effect,
    /// CardDefinition whose effect offered this choice — same role as
    /// `TraceState::initiating_card`.
    pub source_card: Option<CardId>,
    pub resume: PendingPaidChoiceResume,
}

/// Mirrors `PendingPaidChoiceResume` for a parked `PendingDecision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingChoiceResume {
    None,
    ResumeSubroutines,
}

/// A decision parked by an `Effect`, awaiting a resolving `PlayerAction`.
/// Lives as a sibling field on `GameState`, same rationale as
/// `PendingPaidChoice`. Currently exactly one shape is needed; more
/// variants (e.g. choosing cards from a zone) are expected in later
/// milestones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingDecision {
    /// `Effect::PresentChoice` parked this — `chooser` picks one of
    /// `options` via `PlayerAction::ResolvePendingChoice`.
    ChooseEffect {
        chooser: Side,
        options: Vec<Effect>,
        source_card: Option<CardId>,
        resume: PendingChoiceResume,
    },
    /// `Effect::PromptChooseCards` parked this. `selected` is the
    /// in-progress selection, toggled via `PlayerAction::
    /// ToggleCardSelection` and committed via `PlayerAction::
    /// ConfirmCardSelection` (which validates `selected.len()` falls within
    /// `min..=max`).
    ChooseCards {
        side: Side,
        source: CardZoneRef,
        filter: CardFilter,
        min: u32,
        max: u32,
        reveal: bool,
        shuffle_after: bool,
        destination: Option<CardZoneRef>,
        then: Option<Box<Effect>>,
        /// Positions into `pending_choice::zone_card_ids(source)`, in the
        /// order the chooser picked them — **not** `CardId`s.
        ///
        /// This is what makes `PublicGameState::pending_decision`'s
        /// pass-through masking honest. Holding `CardId`s here published
        /// the identity of every card the chooser had selected, which for
        /// *Tāo Salonga*'s selection over `OpponentInstalled` meant
        /// publishing unrezzed Corp ICE to the Runner. A position carries
        /// nothing the chooser was not already shown.
        selected: Vec<usize>,
        source_card: Option<CardId>,
        resume: PendingChoiceResume,
    },
    /// `Effect::PromptChooseServer` parked this — `chooser` picks any
    /// `ServerId` via `PlayerAction::ChooseServerForPendingDecision`, which
    /// initiates a run against it (seeding the new `RunState`'s
    /// `ice_rez_cost_modifier`/`bonus_run_credits` from this variant's
    /// fields — `0`/`0` for a plain "run any server").
    /// Which of `pending` — several of one side's own triggers, all due
    /// simultaneously — that side wants to resolve next. Parked by
    /// `dispatcher::fire_plan` when two or more of a player's own cards
    /// react to the same event; the rules give the ordering choice to
    /// their controller.
    ///
    /// Resolved by `PlayerAction::ChooseTriggerToResolve`, which fires the
    /// chosen one and re-parks the remainder, until one is left and fires
    /// automatically. Cross-side order is **not** included: that is fixed
    /// by rule (active player first, `dispatcher::order_active_first`) and
    /// is nobody's choice, so `pending` only ever holds one side's cards.
    ChooseTriggerOrder {
        chooser: Side,
        /// The still-unresolved triggers, in their default (install) order.
        /// Always 2 or more — one reacting card fires directly with no
        /// decision parked at all.
        pending: Vec<DeferredTrigger>,
        /// Carried for the same reason as every other variant's: a
        /// subroutine's effect can dispatch an event that parks this (e.g.
        /// damage reaching two reacting cards), and losing the
        /// "resume subroutines afterwards" intent would strand the run
        /// mid-encounter.
        resume: PendingChoiceResume,
    },
    ChooseServer {
        chooser: Side,
        rez_cost_delta: i32,
        bonus_run_credits: u32,
        /// `None` means any server; otherwise only these are offered — see
        /// `Effect::PromptChooseServer::allowed_servers`.
        allowed_servers: Option<Vec<ServerId>>,
        /// Seeded onto the resulting `run::RunState::on_success_effect` —
        /// see `Effect::PromptChooseServer::on_success`.
        on_success: Option<Box<Effect>>,
        source_card: Option<CardId>,
        resume: PendingChoiceResume,
    },
}

/// A snapshot of a just-concluded run, taken immediately before
/// `GameState::active_run` is cleared so cards reacting to
/// `Trigger::OnRunEnded` can still see what happened during it (by which
/// point the `RunState` itself is gone). See `dispatcher::dispatch_event`'s
/// `Trigger::OnRunEnded` arm for which conclusions record one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedRun {
    pub server: ServerId,
    /// `RunState::cards_accessed_count` at conclusion — backs
    /// `Effect::GainCreditsPerCardAccessedThisRun` (Zahya Sadeghi).
    pub cards_accessed: u32,
    /// How many agendas the Runner stole during the run — backs
    /// `EffectRequirement::StoleAgendaDuringLastRun` (AMAZE Amusements).
    pub agendas_stolen: u32,
    /// `RunState::persistent_trashed_upgrades` at conclusion: Root-slot
    /// Corp cards flagged `persistent_after_trash` that the Runner trashed
    /// *during* this run. Carried here so `Trigger::OnRunEnded` can still
    /// fire them even though they are no longer in `CorpState::installed` —
    /// that outliving-its-own-trash behavior is the entire point.
    pub persistent_trashed_upgrades: Vec<CardId>,
}

impl CompletedRun {
    /// Captures the run-scoped facts `Trigger::OnRunEnded` consumers need,
    /// immediately before the caller clears `GameState::active_run`. Every
    /// site that concludes a run and dispatches `OnRunEnded` goes through
    /// here, so the three of them cannot drift apart.
    pub fn snapshot(run: &RunState) -> Self {
        CompletedRun {
            server: run.server,
            cards_accessed: run.cards_accessed_count,
            agendas_stolen: run.agendas_stolen_this_run,
            persistent_trashed_upgrades: run.persistent_trashed_upgrades.clone(),
        }
    }
}

/// One trigger that was due to fire but couldn't, because an earlier
/// trigger in the same dispatch parked something blocking (see
/// `GameState::is_resolution_blocked`).
///
/// Queued on `GameState::deferred_triggers` and fired by
/// `dispatcher::drain_deferred_triggers` once the blockage clears. This is
/// the engine's **only** continuation mechanism: without it, a dispatch
/// that hit a parked decision either kept firing underneath it (the bug
/// this fixes — *Clearinghouse* parks a `PresentChoice` from its
/// `OnTurnStart`, and every other Corp `OnTurnStart` card then resolved
/// during that pending choice) or would have had to drop the remainder,
/// which is what `Effect::Sequence` still does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeferredTrigger {
    /// The card whose trigger is owed.
    pub card: CardId,
    pub trigger: Trigger,
    /// Set only for the `ability::process_card_triggers_targeting` case,
    /// where the reacting card and the card its effect acts on differ —
    /// e.g. Cookbook reacting to a just-installed virus by placing a
    /// counter on *that* program. `None` is the ordinary "acts on itself"
    /// case.
    pub target: Option<CardId>,
    /// The event that fired this trigger, carried across the defer boundary
    /// so `dispatcher::fire_one` can rebuild the same
    /// `ability::ResolutionContext` the trigger would have had if it had
    /// resolved immediately.
    ///
    /// Without it, a deferred trigger would resolve with no triggering
    /// event and silently mis-answer any requirement that reads one — e.g.
    /// `WasFirstAdvancementThisCard` would report "not first" for an
    /// advancement that was. `None` only for a trigger with no originating
    /// event.
    #[serde(default)]
    pub event: Option<GameEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    pub corp: CorpState,
    pub runner: RunnerState,
    pub phase: GamePhase,
    /// Which turn is underway, counting **each side's turn separately** —
    /// `0` through both mulligans, `1` for the Corp's opening turn, `2` for
    /// the Runner's first, and so on. Not a round counter.
    ///
    /// Incremented at the single point a turn actually begins
    /// (`turn::enter_start_of_turn`, the only place `GameEvent::TurnStarted`
    /// is emitted), and deliberately *after* its Corp deck-out check: a Corp
    /// that cannot make its mandatory draw loses without the turn ever
    /// starting, so that turn is never counted.
    ///
    /// `netrunner_single_player::MatchHistory` reads this rather than
    /// reconstructing turn numbers by watching the event stream, which is
    /// what it used to do — the reconstruction was correct but was a second
    /// definition of the same fact, living outside the engine.
    #[serde(default)]
    pub turn: u32,
    pub active_run: Option<RunState>,
    pub paid_ability_window: Option<PaidAbilityWindow>,
    pub active_trace: Option<TraceState>,
    pub pending_prevention: Option<PendingPrevention>,
    /// A parked `Effect::OfferPaidChoice` awaiting resolution — see
    /// `PendingPaidChoice`'s doc comment.
    #[serde(default)]
    pub pending_paid_choice: Option<PendingPaidChoice>,
    /// A parked `Effect::PresentChoice` awaiting resolution — see
    /// `PendingDecision`'s doc comment.
    #[serde(default)]
    pub pending_decision: Option<PendingDecision>,
    /// A snapshot of the most recently concluded run (its normal
    /// `RunCompleted`/`RunJackedOut`/`RunEndedByEffect` conclusions only —
    /// see `dispatcher::dispatch_event`'s `Trigger::OnRunEnded` arm doc
    /// comment), set right before `active_run` is cleared. Backs
    /// `EffectRequirement::LastRunWasOnHqOrRnD`/`StoleAgendaDuringLastRun`
    /// and `Effect::GainCreditsPerCardAccessedThisRun` — e.g. Zahya
    /// Sadeghi, AMAZE Amusements. Overwritten each time a run concludes;
    /// `None` before any run has ever finished.
    ///
    /// **This is real state, not scratchpad — it belongs here.** Its two
    /// siblings (`last_discarded_cards`, `last_advancement_was_first`) were
    /// transient and have moved to `ability::ResolutionContext`; this one
    /// cannot follow them, for two independent reasons:
    ///
    /// - `Trigger::OnRunEnded` can be deferred into `deferred_triggers` and
    ///   fire on a **later `PlayerAction`**, by which point any resolution
    ///   context is long gone. Multiple rig cards reacting to one run is
    ///   ordinary, and that is exactly what parks a `ChooseTriggerOrder`.
    /// - It is the dispatcher's only handle on `persistent_after_trash`
    ///   cards the Runner trashed *during* the run, since `active_run` is
    ///   already cleared when `OnRunEnded` dispatches.
    #[serde(default)]
    pub last_completed_run: Option<CompletedRun>,
    /// Triggers owed but not yet fired, because an earlier trigger in the
    /// same dispatch parked something blocking. Drained by
    /// `dispatcher::drain_deferred_triggers` from `engine::apply_action`,
    /// the single choke point every action passes through. Empty in the
    /// overwhelmingly common case — a dispatch that hits no parked
    /// decision never touches this. See `DeferredTrigger`.
    #[serde(default)]
    pub deferred_triggers: Vec<DeferredTrigger>,
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
    /// The next [`InstallId`] `allocate_install_id` will hand out. Lives on
    /// `GameState` for the same reason `rng_step` does: installs happen in
    /// action order, so a counter here keeps `apply_action` a pure function
    /// of its inputs and keeps a replayed history bit-identical. Counts from
    /// `1`, leaving `InstallId::PLACEHOLDER` free.
    ///
    /// Not a scratchpad field under AGENTS.md's State Hygiene Rule: it is
    /// not cross-effect context but permanent identity, and it must survive
    /// arbitrarily many parked decisions.
    #[serde(default = "first_install_id")]
    pub next_install_id: u32,
}

/// `next_install_id`'s serde default. A state recorded before install ids
/// existed has installs carrying `InstallId::PLACEHOLDER`, so resuming it
/// must not hand a real card id `0` as well.
fn first_install_id() -> u32 {
    1
}

/// The empty starting state, every zone clear and every counter zero. This
/// is the **one** exhaustive `GameState` literal in the crate: `new` builds
/// on it, and test fixtures reach it via `..Default::default()`. Adding a
/// field to `GameState` therefore still fails to compile right here, forcing
/// a deliberate choice about its neutral value in exactly one place instead
/// of across ~43 test literals.
///
/// `phase` starts at `GamePhase::Action(Side::Corp)`, matching the real
/// game's turn order — the same value `GameState::new` has always used.
impl Default for GameState {
    fn default() -> Self {
        GameState {
            corp: CorpState::default(),
            runner: RunnerState::default(),
            phase: GamePhase::Action(Side::Corp),
            turn: 0,
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            pending_prevention: None,
            pending_paid_choice: None,
            pending_decision: None,
            last_completed_run: None,
            deferred_triggers: Vec::new(),
            seed: 0,
            rng_step: 0,
            next_install_id: first_install_id(),
        }
    }
}

impl GameState {
    /// A fresh game state seeded for deterministic pseudo-randomness. Corp
    /// and Runner zones start empty and resources start at zero — real game
    /// setup (starting hands/decks/credits) isn't modeled by this engine yet;
    /// callers populate `corp`/`runner` after construction. `phase` starts at
    /// `GamePhase::Action(Side::Corp)`, matching the real game's turn order.
    pub fn new(seed: u64) -> Self {
        GameState { seed, ..GameState::default() }
    }

    /// Hands out the next [`InstallId`] and advances the counter. The
    /// **single** place a real install id is minted — every install site
    /// calls this rather than computing one from a length or a position,
    /// which is what makes an id stable across a trash, a swap, or any
    /// other reordering of `corp.installed`/`runner.rig`.
    pub fn allocate_install_id(&mut self) -> InstallId {
        let id = InstallId(self.next_install_id);
        self.next_install_id += 1;
        id
    }

    /// Where `id` is installed, if it still is — the lookup every handler
    /// uses to resolve a `PlayerAction`'s target. `None` once the card has
    /// left the table, which is the "already gone" case handlers must reject
    /// rather than act on a stale position.
    pub fn find_corp_install(&self, id: InstallId) -> Option<&InstalledCard> {
        self.corp.installed.iter().find(|c| c.install_id == id)
    }

    /// The rig-side mirror of `find_corp_install`.
    pub fn find_rig_install(&self, id: InstallId) -> Option<&InstalledRunnerCard> {
        self.runner.rig.iter().find(|c| c.install_id == id)
    }

    /// Whether the game has concluded. A pure query over `phase` — every
    /// existing call site that inlined `matches!(state.phase,
    /// GamePhase::GameOver(_))` itself (`legal_actions::current_actor`,
    /// `netrunner_bots::mcts::Node::is_terminal`, etc.) is left as-is;
    /// they're free to migrate to this later, but that's not this change's
    /// scope.
    pub fn is_over(&self) -> bool {
        matches!(self.phase, GamePhase::GameOver(_))
    }

    /// Whether something is parked that spans future `PlayerAction`s — a
    /// trace awaiting bids, a prevention window, a paid choice, or a
    /// decision. While any of these hold, no further effect or trigger may
    /// resolve: the parked thing must be answered first.
    ///
    /// The **single** definition of that predicate. It was previously
    /// spelled out inline in three places (`Effect::Sequence`,
    /// `resolve_unbroken_subroutines`, and — by omission, which was the bug
    /// — not at all in the trigger dispatch loops). Adding a new parked
    /// state means updating this one function, not hunting for copies.
    ///
    /// Note this is *not* the same question as `legal_actions::
    /// current_actor`, which resolves *who* may act next; this only asks
    /// whether automatic resolution must stop.
    pub fn is_resolution_blocked(&self) -> bool {
        self.active_trace.is_some()
            || self.pending_prevention.is_some()
            || self.pending_paid_choice.is_some()
            || self.pending_decision.is_some()
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
    use crate::rules::test_support::fixture_install_id;

    fn card(id: &str, base: i32, encounter_buff: i32, turn_buff: i32) -> InstalledRunnerCard {
        InstalledRunnerCard {
            install_id: fixture_install_id(id),
            card: CardId(id.to_string()),
            base_strength: base,
            encounter_strength_buff: encounter_buff,
            turn_strength_buff: turn_buff,
            ..Default::default()
        }
    }

    #[test]
    fn effective_strength_sums_base_and_both_buffs() {
        assert_eq!(card("corroder", 2, 1, 3).effective_strength(), 6);
    }

    #[test]
    fn reset_encounter_strength_buffs_zeroes_only_encounter_buff() {
        let mut runner = RunnerState {
            resources: PlayerResources {
                credits: Credits(0),
                clicks: Clicks(0),
                agenda_points: AgendaPoints(0),
            },
            memory_units: MemoryUnits(0),
            rig: vec![card("corroder", 2, 1, 3)],
            ..Default::default()
        };

        runner.reset_encounter_strength_buffs();

        assert_eq!(runner.rig[0].encounter_strength_buff, 0);
        assert_eq!(runner.rig[0].turn_strength_buff, 3);
    }

    #[test]
    fn reset_turn_strength_buffs_zeroes_only_turn_buff() {
        let mut runner = RunnerState {
            resources: PlayerResources {
                credits: Credits(0),
                clicks: Clicks(0),
                agenda_points: AgendaPoints(0),
            },
            memory_units: MemoryUnits(0),
            rig: vec![card("corroder", 2, 1, 3)],
            ..Default::default()
        };

        runner.reset_turn_strength_buffs();

        assert_eq!(runner.rig[0].turn_strength_buff, 0);
        assert_eq!(runner.rig[0].encounter_strength_buff, 1);
    }
}
