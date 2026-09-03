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
    /// The Runner bypassed the ice being encountered
    /// (`Effect::BypassEncounteredIce`, Fransofia Ward): its remaining
    /// subroutines will not fire and its own "when encountered" reactions
    /// do not resolve. The `IcePassed` follows on the next `Continue`.
    IceBypassed { card_id: CardId, position: u32 },
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
    /// `Effect::MoveThisCardToRoot` carried a root-slot Corp card from one
    /// server's root to another's (Mercia B4LL4RD following the ice it
    /// installed). Not an install — no `CardInstalled` accompanies it.
    CardMoved { card: CardId, from: ServerId, to: ServerId },
    RunInitiated { server: ServerId },
    EventPlayed { side: Side, card: CardId },
    /// `from_archives`: the card was played out of Archives rather than
    /// HQ (`CardDefinition::playable_from_archives`, Petty Cash) — read by
    /// `EffectRequirement::PlayedFromArchives` off the triggering event.
    OperationPlayed {
        side: Side,
        card: CardId,
        #[serde(default)]
        from_archives: bool,
    },
    /// `credits_paid` is what the install actually cost after every
    /// discount — Bling's "whenever you install a card without spending
    /// credits" reads it through
    /// `EffectRequirement::InstalledWithoutSpendingCredits`. `serde(default)`
    /// for histories recorded before the field existed.
    HardwareInstalled {
        side: Side,
        card: CardId,
        #[serde(default)]
        credits_paid: u32,
    },
    ProgramInstalled {
        side: Side,
        card: CardId,
        memory_cost: u8,
        #[serde(default)]
        credits_paid: u32,
    },
    ResourceInstalled {
        side: Side,
        card: CardId,
        #[serde(default)]
        credits_paid: u32,
    },
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
    /// `Effect::AddToBottomOfStack` moved `card` under the Runner's stack.
    CardAddedToBottomOfStack { card: CardId },
    /// `Effect::HostRigCardOnInstall` hosted the rig card `card` on the rig
    /// card `host` (GAMEDRAGON™ Pro on an icebreaker).
    CardHosted { card: CardId, host: CardId },
    /// `Effect::FlipIdentity` turned `side`'s identity over.
    IdentityFlipped { side: Side },
    /// `side`'s action phase ended (`turn::end_turn`) — drives
    /// `Trigger::OnActionPhaseEnd`.
    ActionPhaseEnded { side: Side },
    /// An armed `RunState::end_run_prevention` intercepted an
    /// `Effect::EndTheRun`; the Corp's paid choice decides the run's fate.
    RunEndPrevented { server: ServerId },
    /// A run that would have approached `from` was redirected to `to`
    /// (`Effect::RedirectRunOnApproach`).
    RunRedirected { from: ServerId, to: ServerId },
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
    /// One or more cards left HQ for Archives in a single selection —
    /// emitted once per batch by `pending_choice::resolve_confirm_card_selection`,
    /// beside the per-card `CardTrashed`. AU Co.'s "trash 1 or more cards
    /// from HQ" reads the batch, and `CardTrashed` cannot answer it: it
    /// names no zone, and fires for every Corp card trashed anywhere.
    CardsTrashedFromHq { count: u32 },
    /// An agenda left the Corp's score area as a forfeit (Biawak's rez,
    /// Plutus's). Paired with `CardRemovedFromGame`, which says where it
    /// went; this one says *why*, which is what `Trigger::OnForfeit` keys
    /// off.
    AgendaForfeited { card: CardId },
    /// Credits gained by a resolving card's ability, naming the card —
    /// emitted with `CreditsGained` whenever the resolution has an acting
    /// card. The Zwicky Group: Invisible Hands draws off it. Carries no
    /// amount: no reader needs one, and `CreditsGained` has it.
    AbilityGainedCredits { side: Side, card: CardId },
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
    /// a public Runner rig card; should a Corp card ever hold them,
    /// `masking::mask_event_for_player` already strips an unrezzed one
    /// from the Runner's copy of this list.
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
