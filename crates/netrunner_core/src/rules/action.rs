use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::run::ServerId;
use crate::rules::state::{InstallSlot, Side};

/// Which Corp zone/server an installed card is placed into. Alias of
/// `ServerId` — see its doc comment.
pub type TargetZone = ServerId;

/// Which Corp server a run targets. Alias of `ServerId`.
pub type ServerTarget = ServerId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerAction {
    /// Spend 1 click, gain 1 credit. Symmetric: either side can do this.
    GainCreditClick { side: Side },
    /// Spend 1 click, draw 1 card from the Stack into the Grip. Runner-only for now.
    DrawCardClick,
    /// Spend 1 click, move `card_id` from HQ onto `zone` as a newly installed,
    /// unrezzed card occupying `slot`. Corp-only (Runner grip/rig aren't
    /// modeled with card identity yet). The caller declares `slot` explicitly
    /// (rather than the engine deriving it from the card's `dsl::CardType`,
    /// which it can't look up — no `CardRegistry` is wired in) so that
    /// `run::access_server` can correctly exclude ICE from what a run
    /// accesses on a remote server.
    InstallCard { card_id: CardId, zone: TargetZone, slot: InstallSlot },
    /// Flip an already-installed card face-up. Corp-only. No click cost (rez is
    /// not a click action). Pays `ice_id`'s registry `cost` in credits via
    /// `ability::pay_cost` — `RulesError::CardNotFoundInRegistry` if `ice_id`
    /// isn't in the registry, `RulesError::NotEnoughCredits` if the Corp can't
    /// afford it. Permitted either during the Corp's own `GamePhase::Action`,
    /// or — regardless of whose turn it is — while any `PaidAbilityWindow` is
    /// open, since `phase` never changes mid-run/mid-window.
    RezIce { ice_id: CardId },
    /// Spend 1 click, start a run on `server`. Runner-only. The resulting
    /// `RunState::ice` is left empty — populating real ICE requires a
    /// `CardRegistry` lookup from `corp.installed` that doesn't exist yet.
    InitiateRun { server: ServerTarget },
    /// Advance the active run to its next phase (Initiation -> ApproachIce,
    /// ApproachIce -> EncounterIce, EncounterIce -> next ICE's ApproachIce or
    /// Success). Runner-only. No click cost — like `JackOut`/`BreakSubroutine`,
    /// this is a run-flow sub-action, not a basic click action. Delegates to
    /// `run::advance_run`'s `RunAction::Continue`; requires `active_run` to be
    /// `Some` (`RulesError::NoActiveRun` otherwise), and propagates
    /// `RulesError::SubroutinesStillPending` when subroutines remain on the
    /// ICE currently being encountered.
    ContinueRun,
    /// Voluntarily end the active run. Runner-only, no click cost. Delegates
    /// to `run::advance_run`'s `RunAction::JackOut`, legal only while
    /// `RunState::jack_out_permitted` is `true` — Netrunner/Null Signal
    /// Games-style jack-out windows: closed while initially approaching the
    /// outermost ICE, closed
    /// while committed to an encounter/subroutine resolution, and open once
    /// an ICE has been passed (even an unrezzed one) or the server approach
    /// step is reached (`RunPhase::Success`) with no ICE remaining.
    /// `RulesError::IllegalJackOutWindow` otherwise.
    JackOut,
    /// Close out a run that has already reached `RunPhase::Success`, clearing
    /// `active_run` so a new run can be initiated. Runner-only, no click cost —
    /// like `JackOut`/`ContinueRun`, this is a run-flow sub-action, not a basic
    /// click action. Deliberately does NOT delegate to `run::advance_run` (whose
    /// top-of-function guard exists specifically to reject action on an
    /// already-concluded run — the opposite of what's needed here); the engine
    /// manipulates `GameState.active_run` directly instead. Requires `active_run`
    /// to be `Some` (`RulesError::NoActiveRun` otherwise) with
    /// `phase == RunPhase::Success` (`RulesError::RunNotConcluded` otherwise).
    /// `JackOut` is legal at `Success` too (the "approach server" jack-out
    /// window) — the Runner can still bail right up until `CompleteRun` is
    /// actually called; once it is, the run has moved on to
    /// `RunPhase::AccessingCard`, where `JackOut` is rejected via
    /// `RulesError::RunAlreadyConcluded`.
    CompleteRun,
    /// Spend 1 click, move `card_id` out of the Grip and resolve it. Runner-only.
    /// No credit cost yet — like `RezIce`, cost is data-driven per-card and no
    /// `CardRegistry` is wired into the engine yet.
    PlayEvent { card_id: CardId },
    /// Spend 1 click and `card_id`'s registry `cost` in credits, move
    /// `card_id` out of HQ into Archives, then resolve its `OnPlay`
    /// triggers. Corp-only mirror of `PlayEvent`. `card_id`'s `CardType`
    /// must be `Operation` (`RulesError::CardNotOperation` otherwise).
    PlayOperation { card_id: CardId },
    /// Spend 1 click, move `card_id` from the Grip into the Rig. Runner-only.
    /// No credit cost yet, for the same reason as `PlayEvent`.
    InstallHardware { card_id: CardId },
    /// Spend 1 click, move `card_id` from the Grip into the Rig, reserving
    /// `memory_cost` memory units. Runner-only. No credit cost yet, for the
    /// same reason as `PlayEvent`.
    InstallProgram { card_id: CardId, memory_cost: u8 },
    /// Spend 1 click and `card_id`'s registry `cost` in credits, move
    /// `card_id` from the Grip into the Rig. Runner-only. Mirrors
    /// `InstallHardware` exactly (no memory-unit reservation, unlike
    /// `InstallProgram`) — added because no action previously existed for
    /// installing a `CardType::Resource` card at all (`Resource`s could
    /// only ever leave the Rig via `TrashResource`, never enter it).
    InstallResource { card_id: CardId },
    /// Spend 1 click and `card_id`'s registry cost in credits, move
    /// `card_id` from the Grip onto the Corp ICE named by `host_ice_id`
    /// (`state::InstalledRunnerCard::hosted_on_ice`) rather than into the
    /// ordinary Rig-install flow. Runner-only. `card_id` must be a Trojan
    /// Program (`dsl::CardDefinition::installs_on_ice == true`) and
    /// `host_ice_id` must be a real Corp `InstalledCard` with `slot ==
    /// InstallSlot::Ice` — rezzed or not, real rules allow hosting on
    /// unrezzed ICE. Mirrors `InstallProgram` otherwise (same memory-unit
    /// reservation/cost computation) — e.g. Botulus, Tranquilizer.
    InstallProgramOnIce { card_id: CardId, host_ice_id: CardId, memory_cost: u8 },
    /// Break the next pending subroutine on the ICE currently being
    /// encountered. Runner-only; delegates to `run::advance_run`'s
    /// `RunAction::BreakSubroutine`. `ice_id` is cross-checked against
    /// `RunState::ice[position].card_id` before delegating —
    /// `RulesError::MismatchedIceId` if it doesn't match the ICE actually
    /// being encountered, since `transition_subroutine` itself identifies the
    /// right `RunIce` positionally (`run.position`), not by `ice_id`, and so
    /// can't catch a caller-supplied mismatch on its own. `subroutine_index`
    /// addresses one specific `EncounteredSubroutine` by its `id`/index
    /// within `RunIce::subroutines`, bounds/status-checked by
    /// `transition_subroutine`.
    BreakSubroutine { ice_id: CardId, subroutine_index: usize },
    /// Breaks a subroutine by spending a Runner click instead of matching a
    /// breaker to it — Bioroid-style ICE only (`dsl::CardDefinition::
    /// click_breakable == true`, e.g. Ansel 1.0, Brân 1.0). A dedicated
    /// action rather than a `Cost`/`AbilityDef`, since the existing
    /// `ActivateAbility` legality model only ever offers a card's own
    /// abilities to its own controller, and this needs the *other* side to
    /// act on a card it doesn't own. Same `ice_id`/`subroutine_index`
    /// cross-checking as `BreakSubroutine`, plus `RulesError::
    /// IceNotClickBreakable` if the ICE isn't flagged, plus the ordinary
    /// `RulesError::NotEnoughClicks` if the Runner can't pay.
    BreakSubroutineWithClick { ice_id: CardId, subroutine_index: usize },
    /// End the active side's turn, handing control to the other side and
    /// refilling their clicks to the fixed per-turn allotment (Corp 3 / Runner
    /// 4). Symmetric — no `side` field; the acting side is whichever side
    /// `GameState::phase` is currently `Action(_)` for. Errors with
    /// `RulesError::CannotEndTurnWhileRunActive` if a run is in progress.
    /// If the ending side's hand is over its max hand size, transitions to
    /// `GamePhase::Discard { side, required }` instead of handing control
    /// over immediately — see `turn::end_turn`'s doc comment. Otherwise,
    /// control passes to the Corp, `turn::end_turn` also performs their
    /// mandatory start-of-turn draw from R&D into HQ automatically.
    EndTurn,
    /// Discard `card_id` from hand to satisfy a pending mandatory discard.
    /// Symmetric — no `side` field; the acting side is whichever side
    /// `GameState::phase` is currently `Discard { side, .. }` for. Errors if
    /// the phase isn't `Discard` (`RulesError::NotInDiscardPhase`) or the
    /// card isn't in that side's hand (`RulesError::CardNotInHand`). Once the
    /// phase's `required` count reaches zero, transitions to the other
    /// side's `GamePhase::StartOfTurn` — see `turn::discard_card`'s doc
    /// comment.
    DiscardCard { card_id: CardId },
    /// Keep the current opening hand. Symmetric — no `side` field; the
    /// acting side is whichever side `GameState::phase` is currently
    /// `Mulligan(side)` for, same convention as `EndTurn`/`DiscardCard`.
    /// `RulesError::NotInMulliganPhase` outside `GamePhase::Mulligan`.
    /// Corp's decision advances to `Mulligan(Side::Runner)`; the Runner's
    /// decision hands off into Corp's first turn — see
    /// `rules::setup::keep_hand`.
    KeepHand,
    /// Return the current opening hand, reshuffle it back into the deck,
    /// and draw a fresh 5-card hand. Same phase-inference convention as
    /// `KeepHand`. See `rules::setup::take_mulligan`.
    TakeMulligan,
    /// Pay and resolve the `ability_index`-th ability (a `dsl::AbilityDef`,
    /// looked up in the `CardRegistry`) on `card_id`. Symmetric — no `side`
    /// field; the acting side is whichever side `GameState::phase` is
    /// currently `Action(side)` for, same as `EndTurn`/`DiscardCard`. No
    /// implicit click cost — a paid ability's `AbilityDef::cost` is whatever
    /// the card prints, which may itself include `Cost::Clicks`. `card_id`
    /// must be in an active zone for the acting side (Corp: installed *and*
    /// rezzed; Runner: in the Rig) or this errors with
    /// `RulesError::CardNotActive`. `ability_index` must address a
    /// `Trigger::Paid` ability on that card's definition, or this errors with
    /// `RulesError::InvalidAbilityIndex`/`RulesError::AbilityNotManuallyActivatable`
    /// respectively.
    ActivateAbility { card_id: CardId, ability_index: usize },
    /// Place one advancement token on `card_id`, a Corp-installed card.
    /// Corp-only. Costs 1 click + 1 credit (`pay_cost(state, side,
    /// &Cost::Credits(1))`, in addition to the click). `card_id` must be
    /// installed (`RulesError::CardNotInstalled` otherwise) — no rez
    /// requirement, matching the real game (advancement doesn't require
    /// rez). Its `CardRegistry` definition must have `advancement_requirement:
    /// Some(_)` (`RulesError::CardNotAdvanceable` otherwise); this doesn't
    /// score the card even if the requirement is met — scoring is a
    /// separate, not-yet-modeled action.
    AdvanceCard { card_id: CardId },
    /// Score `card_id`, an installed Corp Agenda whose `advancement_tokens`
    /// already meet its registry `advancement_requirement`. Corp-only,
    /// costs 1 click (no credit cost — matches `AdvanceCard`'s "just a
    /// click" shape). `RulesError::CardNotInstalled` if `card_id` isn't in
    /// `CorpState::installed`; `RulesError::CardNotAgenda` if its registry
    /// `card_type` isn't `Agenda`; `RulesError::AdvancementRequirementNotMet`
    /// if it hasn't been advanced enough yet. No rez requirement — scoring
    /// doesn't depend on `InstalledCard::rezzed`, matching real Netrunner/Null
    /// Signal Games rules. On success, moves the card into `CorpState::
    /// scored_agendas`, fires its own `Trigger::OnAgendaScored` triggers
    /// (e.g. Hostile Takeover), then the Corp identity's (e.g. Jinteki:
    /// Personal Evolution), then checks win conditions.
    ScoreAgenda { card_id: CardId },
    /// Spend 1 click + 2 credits to remove 1 of the Runner's tags.
    /// Runner-only. `RulesError::RunnerNotTagged` if `RunnerState::tags == 0`.
    RemoveTag,
    /// Spend 3 clicks to remove every virus counter in play. Corp-only, and
    /// the Corp's whole turn — `CORP_CLICKS_PER_TURN` is exactly 3.
    ///
    /// Targets every installed/rigged card whose registry `counter_kind` is
    /// `CounterKind::Virus`, on **both** sides: the rule is about the kind
    /// of counter, not who owns the card. Only Runner Programs qualify in
    /// the current card pool (*Botulus*, *Leech*, *Fermenter*, *Conduit*,
    /// *Tranquilizer*), but a Corp card holding virus counters would be
    /// purged too.
    ///
    /// Deliberately has **no** "nothing to purge" error, unlike
    /// `RemoveTag`'s `RulesError::RunnerNotTagged`: you genuinely cannot
    /// remove a tag you don't have, but purging an empty board is a legal
    /// (if pointless) way for the Corp to spend a turn, so `legal_actions`
    /// offers it whenever the clicks are there.
    PurgeVirusCounters,
    /// Picks which of your own simultaneous triggers resolves next,
    /// answering a parked `PendingDecision::ChooseTriggerOrder`. Either
    /// side, whichever the decision names.
    ///
    /// Only ever legal while such a decision is parked, which only happens
    /// when 2 or more of one player's own cards react to the same event —
    /// the rules give that ordering to their controller. Cross-side order
    /// is fixed by rule (`dispatcher::order_active_first`) and is never
    /// offered here.
    ///
    /// `RulesError::CardNotActive` if `card_id` isn't one of the pending
    /// triggers.
    ChooseTriggerToResolve { card_id: CardId },
    /// Spend 1 click + 2 credits to trash `card_id`, an installed Runner
    /// Resource, off the Rig into the Heap. Corp-only, legal only while the
    /// Runner is tagged (`RulesError::RunnerNotTagged` otherwise). `card_id`'s
    /// `CardRegistry` definition must be `CardType::Resource`
    /// (`RulesError::CardNotResource` otherwise) and must be installed in the
    /// Runner's rig (`RulesError::CardNotInRig` otherwise).
    TrashResource { card_id: CardId },
    /// Choose which of the currently offered cards to resolve next, when
    /// more than one card was accessed from a single server. Runner-only.
    /// Legal only while a run is in `RunPhase::AccessingCard` and its
    /// `AccessPhase` is `SelectNextCard` (`RulesError::NotInAccessPhase`
    /// otherwise — including if it's already at `PendingChoice` for a
    /// single remaining/bypassed card); `card_id` must be among
    /// `selectable_cards` or this errors with
    /// `RulesError::InvalidAccessSelection`. Moves the card out of
    /// `AccessState::unaccessed_cards` and presents it via
    /// `AccessPhase::PendingChoice`, ready for `StealAgenda`/
    /// `TrashAccessedCard`/`PassAccessedCard` — see
    /// `run::access::resolve_select_card`. Blocked while a Paid Ability
    /// Window is open (`RulesError::BlockedByPaidAbilityWindow`) — both
    /// sides must pass priority first via `PassPriority`. Resolving this
    /// action may itself open a fresh window if it presents a card's
    /// `PendingChoice`/`PendingInteractiveTrigger` — see
    /// `rules::paid_ability::open_window_if_at_checkpoint`.
    SelectCardToAccess { card_id: CardId },
    /// Steal the currently pending accessed card. Runner-only. Legal only
    /// while a run is in `RunPhase::AccessingCard` and `card_id` matches
    /// the `AccessPhase::PendingChoice` card, and that card is actually a
    /// stealable Agenda (`mandatory_steal` or `steal_cost` is set) —
    /// `RulesError::NotInAccessPhase` otherwise. If the card has a
    /// `steal_cost`, it's paid here (`RulesError::CannotAffordStealCost` if
    /// unaffordable). Moves the card into `RunnerState::scored_agendas`,
    /// checks win conditions, and advances to the next accessed card (or
    /// finalizes the run) — see `run::access::resolve_steal`. Blocked while
    /// a Paid Ability Window is open (`RulesError::BlockedByPaidAbilityWindow`)
    /// — both sides must pass priority first via `PassPriority`. Resolving
    /// this action may itself open a fresh window if it presents another
    /// card's `PendingChoice`/`PendingInteractiveTrigger` — see
    /// `rules::paid_ability::open_window_if_at_checkpoint`.
    StealAgenda { card_id: CardId },
    /// Pay to trash the currently pending accessed card off the table into
    /// `CorpState::archives`. Runner-only. Legal only while a run is in
    /// `RunPhase::AccessingCard`, `card_id` matches the pending card, and
    /// that card has a `trash_cost` (`RulesError::NotInAccessPhase`
    /// otherwise); `RulesError::CannotAffordTrashCost` if the cost can't be
    /// paid. Advances to the next accessed card (or finalizes the run) —
    /// see `run::access::resolve_trash`. Blocked while a Paid Ability
    /// Window is open (`RulesError::BlockedByPaidAbilityWindow`) — both
    /// sides must pass priority first via `PassPriority`. Resolving this
    /// action may itself open a fresh window if it presents another card's
    /// `PendingChoice`/`PendingInteractiveTrigger` — see
    /// `rules::paid_ability::open_window_if_at_checkpoint`.
    TrashAccessedCard { card_id: CardId },
    /// Decline to steal/trash the currently pending accessed card and move
    /// on. Runner-only. Legal only while a run is in `RunPhase::
    /// AccessingCard` and `card_id` matches the pending card
    /// (`RulesError::NotInAccessPhase` otherwise); illegal
    /// (`RulesError::MandatoryStealViolation`) if that card is a
    /// mandatory-steal Agenda. Advances to the next accessed card (or
    /// finalizes the run) — see `run::access::resolve_pass`. Blocked while
    /// a Paid Ability Window is open (`RulesError::BlockedByPaidAbilityWindow`)
    /// — both sides must pass priority first via `PassPriority`. Resolving
    /// this action may itself open a fresh window if it presents another
    /// card's `PendingChoice`/`PendingInteractiveTrigger` — see
    /// `rules::paid_ability::open_window_if_at_checkpoint`.
    PassAccessedCard { card_id: CardId },
    /// Pay the pending `AccessPhase::PendingInteractiveTrigger`'s `cost` to
    /// prevent its `effects`. Runner-only. Legal only while a run is in
    /// `RunPhase::AccessingCard` and `card_id` matches the pending
    /// interactive trigger (`RulesError::NotInAccessPhase` otherwise);
    /// `RulesError::CannotAffordAvoidanceCost` if the cost can't be paid.
    /// Transitions straight to that card's normal `AccessPhase::
    /// PendingChoice` afterward — see `run::access::resolve_pay_to_avoid`.
    /// Blocked while a Paid Ability Window is open
    /// (`RulesError::BlockedByPaidAbilityWindow`) — both sides must pass
    /// priority first via `PassPriority`. Resolving this action may itself
    /// open a fresh window for the card's `PendingChoice` — see
    /// `rules::paid_ability::open_window_if_at_checkpoint`.
    PayToAvoidAccessTrigger { card_id: CardId },
    /// Decline to pay the pending `AccessPhase::PendingInteractiveTrigger`'s
    /// `cost`, letting its `effects` resolve instead. Runner-only. Legal
    /// only while a run is in `RunPhase::AccessingCard` and `card_id`
    /// matches the pending interactive trigger (`RulesError::
    /// NotInAccessPhase` otherwise). Transitions to that card's normal
    /// `AccessPhase::PendingChoice` afterward, unless the effects ended the
    /// game — see `run::access::resolve_decline_to_avoid`. Blocked while a
    /// Paid Ability Window is open (`RulesError::BlockedByPaidAbilityWindow`)
    /// — both sides must pass priority first via `PassPriority`. Resolving
    /// this action may itself open a fresh window for the card's
    /// `PendingChoice` — see `rules::paid_ability::open_window_if_at_checkpoint`.
    DeclineAccessTrigger { card_id: CardId },
    /// Pass priority in the currently open Paid Ability Window. Carries an
    /// explicit `side` — unlike `EndTurn`/`DiscardCard`/`ActivateAbility`,
    /// there's no card/zone/phase to infer it from: `GameState::phase` stays
    /// `Action(Side::Runner)` throughout a run, so it can't tell whose
    /// priority it is. Errors with `RulesError::NotInPaidAbilityWindow` if no
    /// window is open, or `RulesError::NotYourPriority` if it isn't `side`'s
    /// priority. Once both sides pass consecutively, the window closes and
    /// the engine auto-advances whatever run step was paused — a window can
    /// also open at an access-time checkpoint (`AccessPhase::PendingChoice`/
    /// `PendingInteractiveTrigger`, not `SelectNextCard`) as well as the run
    /// checkpoints described above — see
    /// `rules::paid_ability`.
    PassPriority { side: Side },
    /// Corp commits `amount` credits on top of the base trace strength that
    /// an `Effect::Trace` parked in `GameState::active_trace`. Corp-only, no
    /// click cost — a bidding step, not a basic click action, same class as
    /// `RezIce`/`BreakSubroutine`. Legal only while `active_trace` is `Some`
    /// with its `corp_bid` still unset (`RulesError::TraceNotAwaitingCorpBid`
    /// otherwise); `RulesError::NotEnoughCredits` if unaffordable. While a
    /// trace is active, every other `PlayerAction` is rejected with
    /// `RulesError::ActionBlockedByActiveTrace` — see `engine::apply_action`.
    /// See `rules::trace::submit_corp_bid`.
    SubmitCorpTraceBid { amount: u32 },
    /// Runner commits `amount` credits (added to `RunnerState::link_strength`)
    /// against the Corp's already-submitted trace strength. Runner-only.
    /// Legal only while `active_trace` is `Some` with `corp_bid` set
    /// (`RulesError::TraceNotAwaitingRunnerBid` otherwise);
    /// `RulesError::NotEnoughCredits` if unaffordable. Resolves the trace
    /// immediately: if the Runner's total meets or beats the Corp's, the
    /// trace is avoided; otherwise its `effect_on_success` fires. If this
    /// trace was one of the ICE's own subroutines, resumes firing any
    /// remaining pending subroutines and re-advancing the run afterward —
    /// see `rules::trace::submit_runner_bid`.
    SubmitRunnerTraceBid { amount: u32 },
    /// Pays a pending `Effect::OfferPaidChoice`'s cost, resolving its
    /// `if_paid` effect. Whichever side `state.pending_paid_choice::side`
    /// names — no explicit `side` field, same phase-inference convention as
    /// `SubmitCorpTraceBid`/`SubmitRunnerTraceBid`. `cost_option_index`
    /// selects which alternative to pay when the pending cost is
    /// `Cost::AnyOf`; ignored (and may be omitted) otherwise.
    /// `RulesError::NoPendingPaidChoice` if none is parked. While a
    /// `PendingPaidChoice` is parked, every other `PlayerAction` except
    /// this and `DeclinePendingPaidChoice` is rejected with
    /// `RulesError::ActionBlockedByPendingPaidChoice` — see
    /// `engine::apply_action`.
    AcceptPendingPaidChoice { cost_option_index: Option<usize> },
    /// Declines a pending `Effect::OfferPaidChoice`, resolving its
    /// `if_declined` effect instead — no cost is paid.
    /// `RulesError::NoPendingPaidChoice` if none is parked.
    DeclinePendingPaidChoice,
    /// Resolves a pending `Effect::PresentChoice` by picking
    /// `options[option_index]`. `RulesError::NoPendingDecision` if none is
    /// parked, `RulesError::InvalidChoiceIndex` if out of range. While a
    /// `PendingDecision` is parked, every other `PlayerAction` is rejected
    /// with `RulesError::ActionBlockedByPendingDecision` — see
    /// `engine::apply_action`.
    ResolvePendingChoice { option_index: usize },
    /// Adds `card_id` to (or removes it from, if already present) the
    /// in-progress selection of a pending `PendingDecision::ChooseCards`.
    /// `RulesError::NoPendingDecision` if none is parked (or a different
    /// variant is); `RulesError::CardNotEligibleForSelection` if `card_id`
    /// isn't currently a legal candidate (not present in the pending
    /// decision's `source` zone, or doesn't match its `filter`).
    ToggleCardSelection { card_id: CardId },
    /// Commits the in-progress selection of a pending `PendingDecision::
    /// ChooseCards`. `RulesError::NoPendingDecision` if none is parked (or a
    /// different variant is); `RulesError::CardSelectionOutOfRange` if the
    /// current selection's size falls outside `min..=max`.
    ConfirmCardSelection,
    /// Picks `server` for a pending `PendingDecision::ChooseServer`,
    /// initiating a run against it. `RulesError::NoPendingDecision` if none
    /// is parked (or a different variant is).
    ChooseServerForPendingDecision { server: ServerId },
}
