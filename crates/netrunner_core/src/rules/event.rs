use serde::{Deserialize, Serialize};

use crate::dsl::{BoostDuration, CardId, CardTarget, DamageType, Effect};
use crate::rules::run::ServerId;
use crate::rules::state::Side;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEvent {
    ClickSpent { side: Side },
    CreditsGained { side: Side, amount: u32 },
    CardDrawn { side: Side },
    IceApproached { server: ServerId, position: u32 },
    IceEncountered { card_id: CardId, strength: i32, subroutine_count: usize },
    SubroutineBroken { card_id: CardId, index: usize },
    SubroutineFired { card_id: CardId, index: usize, effect: Effect },
    IceStrengthModified { card_id: CardId, new_strength: i32, delta: i32 },
    IcePassed { server: ServerId, position: u32 },
    /// The Runner has passed the last piece of ICE (or there was none) and
    /// is approaching the server itself — NSG's approach-server step, where
    /// jacking out is legal and "when the Runner approaches this server"
    /// abilities (Manegarm Skunkworks, Anoetic Void) fire. The run is
    /// **not yet successful**: that is `RunSucceeded`, which follows only
    /// if the Runner commits with `PlayerAction::CompleteRun` and nothing
    /// here ended the run. The two used to be one event, so a run Anoetic
    /// Void ended at approach had already paid out every "when your run is
    /// successful" trigger (ROADMAP Rules Audit T9).
    ServerApproached { server: ServerId },
    RunSucceeded { server: ServerId },
    RunJackedOut { server: ServerId },
    RunCompleted { server: ServerId },
    CardInstalled { side: Side, card: CardId, server: ServerId },
    /// `install` names which copy was rezzed, so `Trigger::OnRez` resolves
    /// on that copy — two Nico Campaigns used to load both sets of counters
    /// onto the first. `serde(default)` (the placeholder) for histories
    /// recorded before the field existed.
    IceRezzed {
        card: CardId,
        server: ServerId,
        #[serde(default)]
        install: crate::rules::state::InstallId,
    },
    /// A rezzed Corp installed card was flipped back face-down —
    /// `Effect::DerezCard`'s only emission site. No player-driven derez
    /// action exists (rez itself is otherwise one-way).
    CardDerezzed { card: CardId },
    /// `Effect::SwapInstalledIce` exchanged `a`'s and `b`'s server/slot
    /// positions.
    IceSwapped { a: CardId, b: CardId },
    RunInitiated { server: ServerId },
    EventPlayed { side: Side, card: CardId },
    OperationPlayed { side: Side, card: CardId },
    HardwareInstalled { side: Side, card: CardId },
    ProgramInstalled { side: Side, card: CardId, memory_cost: u8 },
    ResourceInstalled { side: Side, card: CardId },
    /// `install` names the instance when the accessed card is a root
    /// install (`AccessState::pending_install`), so `Trigger::OnAccessed`
    /// fires against *that* copy's counters rather than the first copy's.
    /// `serde(default)` so a history recorded before the field existed
    /// still deserializes; the dispatcher falls back to a by-`CardId`
    /// lookup for `None`.
    CardAccessed {
        card: CardId,
        server: ServerId,
        #[serde(default)]
        install: Option<crate::rules::state::InstallId>,
    },
    TurnEnded { side: Side },
    TurnStarted { side: Side, clicks: u32 },
    DiscardPending { side: Side, required: usize },
    /// `side`'s discard phase has ended — emitted whether they actually
    /// discarded or were already within hand size. Drives
    /// `Trigger::OnDiscardPhaseEnd`.
    DiscardPhaseEnded { side: Side },
    CardDiscarded { side: Side, card: CardId },
    AgendaStolen { card: CardId, agenda_points: u32 },
    DamageTaken { damage_type: DamageType, amount: usize },
    RunnerFlatlined,
    CreditsSpent { side: Side, amount: u32 },
    TagsGiven { side: Side, amount: u32 },
    /// `Cost::ClearTags` zeroed the Runner's tag count. Named for clearing,
    /// not purging — see `Cost::ClearTags`'s doc comment.
    TagsCleared { side: Side },
    CardTrashed { side: Side, card: CardId },
    /// A card left play permanently, bypassing the discard pile — Spin
    /// Doctor's `Cost::RemoveSelfFromGame`. Distinct from `CardTrashed`
    /// so a listener can tell "in Archives" from "gone".
    CardRemovedFromGame { side: Side, card: CardId },
    RunEndedByEffect { server: ServerId },
    GameOver { winner: Side },
    AbilityActivated { side: Side, card_id: CardId, ability_index: usize },
    CardAdvanced { card: CardId, advancement_tokens: u32 },
    CardTrashedFromAccess { card: CardId, cost_paid: u32 },
    AccessPassed { card: CardId },
    PaidAbilityWindowOpened { side: Side },
    PriorityPassed { side: Side },
    PaidAbilityWindowClosed,
    StrengthBoosted { card_id: CardId, new_strength: i32, delta: i32, duration: BoostDuration },
    TraceInitiated { base: u32, initiating_card: Option<CardId> },
    TraceCorpBidSubmitted { corp_bid: u32, total_strength: u32 },
    TraceRunnerBidSubmitted { runner_bid: u32, total_strength: u32 },
    TraceAvoided { corp_total: u32, runner_total: u32 },
    TraceSuccessful { corp_total: u32, runner_total: u32 },
    TagRemoved { side: Side },
    TagsRemoved { side: Side, amount: u32 },
    /// Two or more of `chooser`'s own cards react to the same event, so
    /// they get to pick the resolution order — a
    /// `PendingDecision::ChooseTriggerOrder` is now parked.
    TriggerOrderPending { chooser: Side },
    /// `chooser` picked `card`'s `trigger` as the next of their
    /// simultaneous triggers to resolve. `trigger` is named because one
    /// card can have several pending at once (a successful run on HQ
    /// offers `OnSuccessfulRun` and `OnSuccessfulRunOnHq` separately), and
    /// a log line reading "Docklands Pass, then Docklands Pass" says
    /// nothing.
    TriggerOrderChosen { chooser: Side, card: CardId, trigger: crate::dsl::Trigger },
    /// One of `card`'s `TriggeredEffect`s for `trigger` is firing: its
    /// requirement passed, and its effects' events follow this one.
    /// Emitted by `dispatcher::fire_one` for every trigger the game
    /// dispatches — the exact record the coverage harness counts as
    /// `triggers_fired`, replacing an inference from the event that
    /// would have offered the trigger, which could not see a failed
    /// requirement or a run that had ended (ROADMAP Rules Audit §0).
    TriggerFired { card: CardId, trigger: crate::dsl::Trigger },
    /// `PlayerAction::PurgeVirusCounters` zeroed the virus counters on
    /// `cards`. Empty when the Corp purged an empty board, which is legal —
    /// the event still fires, since the action still happened and still
    /// cost 3 clicks.
    ///
    /// Carries the affected card list so a game log can narrate which
    /// viruses were wiped. Every card that can hold virus counters today is
    /// a public Runner rig card, so this leaks nothing — but if a future
    /// set prints a Corp card holding virus counters, the tracked
    /// per-viewer event-masking work (ROADMAP Phase 4) must strip an
    /// unrezzed one's identity here.
    VirusCountersPurged { cards: Vec<CardId> },
    BadPublicityCreditsSpent { amount: u32 },
    BonusRunCreditsSpent { amount: u32 },
    /// A `PendingDecision::ChooseCards` was confirmed — `cards` is the
    /// committed selection, `revealed` mirrors the originating `Effect::
    /// PromptChooseCards::reveal`.
    CardsSelected { side: Side, cards: Vec<CardId>, revealed: bool },
    /// `Effect::PromptChooseCards` parked a `PendingDecision::ChooseCards`.
    PendingCardSelectionOffered { side: Side, min: u32, max: u32 },
    /// The Runner has `over_by` more memory units in use than available
    /// (a console left play under a full rig), so `rules::memory::
    /// enforce_limit` has parked a `ChooseCards` over their own programs:
    /// they must trash one, and the check repeats on the next action until
    /// the rig fits. Always followed by the `PendingCardSelectionOffered`
    /// that parked it.
    MemoryLimitExceeded { over_by: u32 },
    /// `Effect::PromptChooseServer` parked a `PendingDecision::ChooseServer`.
    PendingServerChoiceOffered { chooser: Side },
    BadPublicityGiven { amount: u32 },
    BadPublicityRemoved { amount: u32 },
    HandKept { side: Side },
    MulliganTaken { side: Side },
    AdditionalAccessGranted { server: ServerId, count: u32 },
    AccessReplacementSet { server: ServerId },
    AccessReplaced { server: ServerId },
    CreditsLost { side: Side, amount: u32 },
    ClicksLost { side: Side, amount: u32 },
    ClicksGained { side: Side, amount: u32 },
    RecurringCreditsSpent { amount: u32 },
    AgendaScored { card: CardId, agenda_points: u32, server: ServerId },
    DamageAboutToResolve { damage_type: DamageType, amount: usize },
    TrashAboutToResolve { target: CardTarget },
    DamagePrevented { amount: usize },
    TrashPrevented { target: CardTarget },
    CountersAdded { card: CardId, amount: u32 },
    CountersRemoved { card: CardId, amount: u32 },
    MaxHandSizeGained { side: Side, amount: u32 },
    /// Fired only by `engine::draw_card_click` — the *basic* click-to-draw
    /// action specifically, not `Effect::DrawCards` (e.g. Sure Gamble
    /// still only emits `CardDrawn`). Feeds `Trigger::OnBasicDrawAction`.
    BasicDrawActionTaken { side: Side },
    /// A player chose one of `Effect::PresentChoice`'s options
    /// (`state::PendingDecision::ChooseEffect`) is now awaiting
    /// `PlayerAction::ResolvePendingChoice`.
    PendingChoicePresented { chooser: Side, option_count: usize },
    /// `PlayerAction::ResolvePendingChoice` picked this option.
    PendingChoiceResolved { chooser: Side, option_index: usize },
    /// `Effect::OfferPaidChoice` parked a `state::PendingPaidChoice`,
    /// awaiting `PlayerAction::AcceptPendingPaidChoice`/
    /// `DeclinePendingPaidChoice`.
    PendingPaidChoiceOffered { side: Side },
    /// The pending paid choice was accepted (its cost paid) or declined.
    PendingPaidChoiceAccepted { side: Side },
    PendingPaidChoiceDeclined { side: Side },
}

impl GameEvent {
    /// This event's variant name — `"AgendaScored"`, never the payload.
    /// Read off the `Debug` rendering, exactly as `PlayerAction::
    /// variant_name` does, so a new variant needs no arm here. There is no
    /// `VARIANT_NAMES` table for events (ninety variants would be a large
    /// list to keep honest by hand), which is why `tutorial` cannot validate
    /// an `EventPredicate::Kind` by name and relies on the lesson gate
    /// instead: a misspelt kind is a step that never advances.
    pub fn variant_name(&self) -> String {
        let rendered = format!("{self:?}");
        rendered.split(['(', '{', ' ']).next().unwrap_or(&rendered).to_string()
    }
}
