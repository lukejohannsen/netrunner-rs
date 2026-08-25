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
    /// not a click action) and no credit cost yet — rez cost is data-driven
    /// per-card via `dsl::Card`, and no `CardRegistry` is wired into the engine
    /// yet. Permitted either during the Corp's own `GamePhase::Action`, or —
    /// regardless of whose turn it is — while a run is active and in
    /// `RunPhase::ApproachIce` (the rez window), since `phase` never changes
    /// mid-run.
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
    /// Break the next pending subroutine on the ICE currently being
    /// encountered. Runner-only; delegates to `run::advance_run`'s
    /// `RunAction::BreakSubroutine`. `ice_id` isn't cross-checked against
    /// `RunState::ice[position].card_id` — it's accepted as caller-provided
    /// context only, since `transition_subroutine` already identifies the
    /// right `RunIce` positionally (`run.position`), not by `ice_id`; a
    /// mismatched `ice_id` here silently breaks whatever's actually being
    /// encountered rather than erroring (a real, separate, pre-existing
    /// gap). `subroutine_index` addresses one specific
    /// `EncounteredSubroutine` by its `id`/index within
    /// `RunIce::subroutines`, bounds/status-checked by
    /// `transition_subroutine`.
    BreakSubroutine { ice_id: CardId, subroutine_index: usize },
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
    /// Spend 1 click + 2 credits to remove 1 of the Runner's tags.
    /// Runner-only. `RulesError::RunnerNotTagged` if `RunnerState::tags == 0`.
    RemoveTag,
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
}
