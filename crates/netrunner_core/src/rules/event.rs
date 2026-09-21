use serde::{Deserialize, Serialize};

use crate::dsl::{EffectDuration, CardId, DamageType, Effect};
use crate::rules::run::ServerId;
use crate::rules::state::{Side, WouldHappen};

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
    /// `install` names which copy landed, and is never masked — an install
    /// handle is public the moment the card hits the table
    /// (`PublicInstalledCard::install_id`). `card` is `None` exactly when
    /// `masking::mask_event_for_player` struck it out for a viewer who may
    /// not identify a face-down Corp install; **the engine always emits
    /// `Some`**. Before the handle existed the whole event was dropped for
    /// that viewer, so a Runner's log said nothing at all about an install
    /// driven by a card's own text (Ansel 1.0), which no action names
    /// either. `serde(default)` on both for histories recorded before.
    CardInstalled {
        side: Side,
        #[serde(default)]
        install: crate::rules::state::InstallId,
        #[serde(default)]
        card: Option<CardId>,
        server: ServerId,
    },
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
    /// `Effect::DerezCard`'s only emission site (Maglectric Rapid,
    /// Tranquilizer). No player-driven derez action exists (rez itself is
    /// otherwise one-way), so this event is the only record of it.
    ///
    /// Handle public, identity strikeable — see `CardInstalled`. The
    /// Runner *saw* the card while it was rezzed, but the view has no
    /// notion of "seen before" and renders the now-derezzed install as
    /// `card: None`; the log follows the view rather than a memory it
    /// cannot model. Keeping the handle is what lets it still say which
    /// install flipped.
    CardDerezzed {
        #[serde(default)]
        install: crate::rules::state::InstallId,
        #[serde(default)]
        card: Option<CardId>,
    },
    /// `Effect::SwapInstalledIce` exchanged `a`'s and `b`'s server/slot
    /// positions.
    ///
    /// The two handles are public and the two identities are struck
    /// independently — see `CardInstalled`. A swap is the case that needs
    /// this most: it comes from a card's text (Tāo Salonga, Brân 1.0) and
    /// there is no `PlayerAction` naming it, so when this event was dropped
    /// the Runner's log had no record that ice had moved at all.
    IceSwapped {
        #[serde(default)]
        a: crate::rules::state::InstallId,
        #[serde(default)]
        b: crate::rules::state::InstallId,
        #[serde(default)]
        a_card: Option<CardId>,
        #[serde(default)]
        b_card: Option<CardId>,
    },
    /// `Effect::MoveThisCardToRoot` carried a root-slot Corp card from one
    /// server's root to another's (Mercia B4LL4RD following the ice it
    /// installed). Not an install — no `CardInstalled` accompanies it.
    /// Handle public, identity strikeable — see `CardInstalled`. The card
    /// keeps its install id across the move (`Effect::MoveThisCardToRoot`),
    /// and both servers are public, so a viewer who cannot name the card
    /// still watched it leave one root and arrive in another.
    CardMoved {
        #[serde(default)]
        install: crate::rules::state::InstallId,
        #[serde(default)]
        card: Option<CardId>,
        from: ServerId,
        to: ServerId,
    },
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
    /// A standing prevention (`Lingering::PreventRunEnding`) intercepted an
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
    /// `advancement_tokens` is the count *after* this advancement, which is
    /// what `EffectRequirement::WasFirstAdvancementThisCard` reads off the
    /// resolution context rather than needing a flag of its own.
    ///
    /// Handle public, identity strikeable — see `CardInstalled`. Tokens on
    /// a face-down card are themselves public
    /// (`PublicInstalledCard::advancement_tokens` is never masked), so
    /// dropping this event told the Runner less than their own board
    /// already showed them.
    CardAdvanced {
        #[serde(default)]
        install: crate::rules::state::InstallId,
        #[serde(default)]
        card: Option<CardId>,
        advancement_tokens: u32,
    },
    /// A card put advancement counters on an installed card, which is
    /// **not** advancing it. Null Signal Games' Comprehensive Rules
    /// 1.18.2: "Resolving an instruction that directly places an
    /// advancement counter onto a card is not the same as advancing a
    /// card." Their own example is Mushin-no-Shin placing three counters
    /// on an Oaktown Renovation, which does *not* pay Oaktown's advance
    /// ability — so this event exists precisely so that it cannot fire
    /// `Trigger::OnAdvance`. `CardAdvanced` is the other one, emitted only
    /// by the basic action and by an ability that says "advance".
    ///
    /// Masked and counted exactly as `CardAdvanced` is: same handle, same
    /// identity strike, and the tokens themselves were never secret.
    AdvancementCountersPlaced {
        #[serde(default)]
        install: crate::rules::state::InstallId,
        #[serde(default)]
        card: Option<CardId>,
        advancement_tokens: u32,
    },
    /// `install`: the root install the card was, when the Runner trashed it
    /// off the table rather than out of HQ, R&D or Archives — public, like
    /// the access itself. It is what lets a card say "trashes an
    /// **installed** Corp card" in its trigger condition
    /// (`EventFilter::InstalledCard`); `serde(default)` for histories
    /// recorded before the field existed.
    CardTrashedFromAccess {
        card: CardId,
        cost_paid: u32,
        #[serde(default)]
        install: Option<crate::rules::state::InstallId>,
    },
    AccessPassed { card: CardId },
    PaidAbilityWindowOpened { side: Side },
    PriorityPassed { side: Side },
    PaidAbilityWindowClosed,
    StrengthBoosted { card_id: CardId, new_strength: i32, delta: i32, duration: EffectDuration },
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
    /// card can have several pending at once (an install offers `OnInstall`
    /// and `OnCardInstalled` separately), and a log line reading "Bling,
    /// then Bling" says nothing.
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
    /// A payment found pools its payer has to choose between, and the
    /// action that owes it is parked until they have (`PendingPayment`).
    /// The whole record of that step: nothing else has happened yet. What
    /// was chosen is said by the spend events of the step that follows.
    PaymentChoiceOffered { side: Side },
    BadPublicityCreditsSpent { amount: u32 },
    BonusRunCreditsSpent { amount: u32 },
    /// A `PendingDecision::ChooseCards` was confirmed — `cards` is the
    /// committed selection, `revealed` mirrors the originating `Effect::
    /// PromptChooseCards::reveal`.
    CardsSelected { side: Side, cards: Vec<CardId>, revealed: bool },
    /// `Effect::PromptChooseCards` parked a `PendingDecision::ChooseCards`.
    /// `source` is the card whose text asked (`PendingDecision::ChooseCards::
    /// prompting_card`), so a coverage report can charge the actions a
    /// prompt absorbs to the card that opened it — the measurement that
    /// makes a grinding prompt visible before it livelocks (ROADMAP Phase 2
    /// §5). Absent from records made before the field existed.
    PendingCardSelectionOffered {
        side: Side,
        min: u32,
        max: u32,
        #[serde(default)]
        source: Option<CardId>,
    },
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
    AgendaScored { card: CardId, agenda_points: u32, server: ServerId },
    /// Something a card may prevent was parked (`rules::prevention`), and
    /// the players are about to be asked. One event for every kind: what it
    /// is is the payload, as it is for the parked thing itself. Were four
    /// events, a pair per kind, with tags about to need a third pair.
    AboutToResolve { what: WouldHappen },
    /// `amount` of `what` was prevented; the rest, if any, follows as the
    /// event it always was.
    Prevented { what: WouldHappen, amount: u32 },
    /// **Deliberately no install handle, unlike `CardInstalled`,
    /// `CardAdvanced`, `IceSwapped`, `CardMoved` and `CardDerezzed`.**
    ///
    /// Those five keep a handle so `masking::mask_event_for_player` can
    /// strike the identity and still say *which* install; the obvious next
    /// step is to do the same here, and it would be a leak. A face-down
    /// Corp card's counters are concealed in their own right —
    /// `PublicInstalledCard::counters` is `None` for exactly this reason,
    /// "whose counters would otherwise leak what it is (a *Nico Campaign*
    /// draining credits is recognisable long before it is rezzed)" — so
    /// "the counters on install #7 changed" publishes the very fact the
    /// view withholds. These stay dropped whole for a viewer who cannot
    /// identify the card (ROADMAP Phase 4 §1).
    CountersAdded { card: CardId, amount: u32 },
    /// No install handle, for the reason on `CountersAdded`.
    CountersRemoved { card: CardId, amount: u32 },
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
    /// `Effect::ChooseNumber` parked a `state::PendingDecision::
    /// ChooseNumber`, awaiting `PlayerAction::ChooseNumber`.
    NumberChoiceOffered { chooser: Side, min: u32, max: u32 },
    /// `PlayerAction::ChooseNumber` named this number. Not emitted when the
    /// range held one number and nobody was asked.
    NumberChosen { chooser: Side, amount: u32 },
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
    /// Whether this event can have taught the player who caused it
    /// something they did not know, or handed the game to the other seat —
    /// what `netrunner_session::Session` reads to decide whether taking a
    /// move back is *free* (ROADMAP Phase 7 §8 item 4b).
    ///
    /// jinteki.net's `/undo-click` restores a whole state with no such
    /// test, so undoing a draw rewinds the draw. Here a take-back that
    /// crossed one of these stops being free: nothing against a bot, where
    /// every game is casual, and the line a rated game between two people
    /// is held to.
    /// A card leaving a hidden zone for the actor's eyes (a draw, an
    /// access, a rez seen from the other chair), a trace (the other seat
    /// bids), and a turn or the game ending all count. Paying, gaining,
    /// installing, and a decision being parked do not: the player knew
    /// everything those show before they acted.
    ///
    /// **Exhaustive on purpose**, unlike `variant_name`: a new event does
    /// not compile until someone decides, because the wrong default here
    /// is silent in both directions — `false` leaks a card to a rated
    /// game, `true` takes the Back button off a prompt that deserved it.
    /// `GameState::rng_step` moving is tested separately by the session,
    /// which is what covers a shuffle or a random discard.
    pub fn may_teach_the_actor(&self) -> bool {
        match self {
            GameEvent::CardDrawn { .. } | GameEvent::CardAccessed { .. } | GameEvent::CardTrashedFromAccess { .. }
            | GameEvent::AccessPassed { .. } | GameEvent::AgendaStolen { .. } | GameEvent::IceRezzed { .. }
            | GameEvent::IceEncountered { .. } | GameEvent::DamageTaken { .. } | GameEvent::CardsTrashedFromHq { .. }
            | GameEvent::MulliganTaken { .. } | GameEvent::HandKept { .. } | GameEvent::TraceInitiated { .. }
            | GameEvent::TraceCorpBidSubmitted { .. } | GameEvent::TraceRunnerBidSubmitted { .. }
            | GameEvent::TraceAvoided { .. } | GameEvent::TraceSuccessful { .. } | GameEvent::GameOver { .. }
            | GameEvent::RunnerFlatlined | GameEvent::TurnStarted { .. } | GameEvent::TurnEnded { .. } => true,
            GameEvent::ClickSpent { .. } | GameEvent::CreditsGained { .. } | GameEvent::IceApproached { .. }
            | GameEvent::SubroutineBroken { .. } | GameEvent::SubroutineFired { .. }
            | GameEvent::IceStrengthModified { .. } | GameEvent::IcePassed { .. } | GameEvent::IceBypassed { .. }
            | GameEvent::ServerApproached { .. } | GameEvent::RunSucceeded { .. } | GameEvent::RunJackedOut { .. }
            | GameEvent::RunCompleted { .. } | GameEvent::CardInstalled { .. } | GameEvent::CardDerezzed { .. }
            | GameEvent::IceSwapped { .. } | GameEvent::CardMoved { .. } | GameEvent::RunInitiated { .. }
            | GameEvent::EventPlayed { .. } | GameEvent::OperationPlayed { .. } | GameEvent::HardwareInstalled { .. }
            | GameEvent::ProgramInstalled { .. } | GameEvent::ResourceInstalled { .. }
            | GameEvent::DiscardPending { .. } | GameEvent::DiscardPhaseEnded { .. }
            | GameEvent::CardDiscarded { .. } | GameEvent::CardAddedToBottomOfStack { .. }
            | GameEvent::CardHosted { .. } | GameEvent::IdentityFlipped { .. } | GameEvent::ActionPhaseEnded { .. }
            | GameEvent::RunEndPrevented { .. } | GameEvent::RunRedirected { .. } | GameEvent::CreditsSpent { .. }
            | GameEvent::TagsGiven { .. } | GameEvent::TagsCleared { .. } | GameEvent::CardTrashed { .. }
            | GameEvent::CardRemovedFromGame { .. } | GameEvent::AgendaForfeited { .. }
            | GameEvent::AbilityGainedCredits { .. } | GameEvent::RunEndedByEffect { .. }
            | GameEvent::AbilityActivated { .. } | GameEvent::CardAdvanced { .. }
            | GameEvent::AdvancementCountersPlaced { .. } | GameEvent::PaidAbilityWindowOpened { .. }
            | GameEvent::PriorityPassed { .. } | GameEvent::PaidAbilityWindowClosed
            | GameEvent::StrengthBoosted { .. } | GameEvent::TagRemoved { .. } | GameEvent::TagsRemoved { .. }
            | GameEvent::TriggerOrderPending { .. } | GameEvent::TriggerOrderChosen { .. }
            | GameEvent::TriggerFired { .. } | GameEvent::VirusCountersPurged { .. }
            | GameEvent::PaymentChoiceOffered { .. }
            | GameEvent::BadPublicityCreditsSpent { .. } | GameEvent::BonusRunCreditsSpent { .. }
            | GameEvent::CardsSelected { .. } | GameEvent::PendingCardSelectionOffered { .. }
            | GameEvent::MemoryLimitExceeded { .. } | GameEvent::PendingServerChoiceOffered { .. }
            | GameEvent::BadPublicityGiven { .. } | GameEvent::BadPublicityRemoved { .. }
            | GameEvent::AdditionalAccessGranted { .. } | GameEvent::AccessReplacementSet { .. }
            | GameEvent::AccessReplaced { .. } | GameEvent::CreditsLost { .. } | GameEvent::ClicksLost { .. }
            | GameEvent::ClicksGained { .. }
            | GameEvent::AgendaScored { .. } | GameEvent::AboutToResolve { .. }
            | GameEvent::Prevented { .. } | GameEvent::CountersAdded { .. } | GameEvent::CountersRemoved { .. }
            | GameEvent::BasicDrawActionTaken { .. }
            | GameEvent::PendingChoicePresented { .. } | GameEvent::PendingChoiceResolved { .. }
            | GameEvent::NumberChoiceOffered { .. } | GameEvent::NumberChosen { .. }
            | GameEvent::PendingPaidChoiceOffered { .. } | GameEvent::PendingPaidChoiceAccepted { .. }
            | GameEvent::PendingPaidChoiceDeclined { .. } => false,
        }
    }
}
