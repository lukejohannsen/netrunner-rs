use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::run::ServerId;
use crate::rules::state::{InstallId, InstallSlot, Side};

/// Which Corp zone/server an installed card is placed into. Alias of
/// `ServerId` — see its doc comment.
pub type TargetZone = ServerId;

/// Which Corp server a run targets. Alias of `ServerId`.
pub type ServerTarget = ServerId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerAction {
    /// Spend 1 click, gain 1 credit. Symmetric: either side can do this.
    GainCreditClick { side: Side },
    /// Spend 1 click, draw 1 card — the Runner from the Stack into the
    /// Grip, the Corp from R&D into HQ. Both sides' basic action; this was
    /// Runner-only for a long time ("for now", said the comment), which
    /// left the Corp with no way to dig for an agenda or ICE beyond the
    /// mandatory draw (ROADMAP Rules Audit T4). The Corp cannot take it
    /// with an empty R&D (`RulesError::EmptyZone`): a click that would
    /// have to draw from nothing is not offered rather than resolved as a
    /// self-inflicted deck-out. The Runner's empty-Stack draw stays a
    /// harmless no-op, as before.
    DrawCardClick { side: Side },
    /// Spend 1 click, move `card_id` from HQ onto `zone` as a newly installed,
    /// unrezzed card occupying `slot`. Corp-only; the Runner's installs are
    /// `InstallHardware`/`InstallProgram`/`InstallResource`. Costs no
    /// credits for a Root install and 1[c] per piece of ICE already
    /// protecting `zone` for an ICE install — the printed cost is paid at
    /// `RezIce`, not here. The caller declares `slot` explicitly
    /// (rather than the engine deriving it from the card's `dsl::CardType`,
    /// which it can't look up — no `CardRegistry` is wired in) so that
    /// `run::access_server` can correctly exclude ICE from what a run
    /// accesses on a remote server.
    InstallCard { card_id: CardId, zone: TargetZone, slot: InstallSlot },
    /// Flip an already-installed card face-up. Corp-only. No click cost (rez is
    /// not a click action). Pays the card's registry `cost` in credits via
    /// `ability::pay_cost` — `RulesError::CardNotFoundInRegistry` if it
    /// isn't in the registry, `RulesError::NotEnoughCredits` if the Corp can't
    /// afford it. **When** depends on the card: ICE only while the Runner is
    /// approaching that very install (`RulesError::IceNotBeingApproached`
    /// otherwise — not on the Corp's own turn, not at another window);
    /// assets and upgrades during the Corp's own `GamePhase::Action` or,
    /// regardless of whose turn it is, while any `PaidAbilityWindow` is
    /// open. The name predates the split and is kept: it is the action
    /// every client and policy already knows.
    ///
    /// `ice` names the install, not the card: with two copies of the same
    /// ICE installed, this rezzes the one actually chosen. See
    /// `state::InstallId`. `RulesError::InstallNotFound` if it names
    /// nothing installed.
    RezIce { ice: InstallId },
    /// Spend 1 click, start a run on `server`. Runner-only. `run::start_run`
    /// builds the run's ICE list from `corp.installed` through the registry
    /// (`build_run_ice`), outermost first. (An earlier comment here said the
    /// list was left empty for want of a registry — true when written,
    /// false for a long time after; the shape the Rules Audit warns about.)
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
    /// Spend 1 click and `card_id`'s registry `cost` in credits, move
    /// `card_id` out of the Grip, resolve its `OnPlay` triggers, and trash it
    /// to the Heap. Runner-only.
    PlayEvent { card_id: CardId },
    /// Spend 1 click and `card_id`'s registry `cost` in credits, move
    /// `card_id` out of HQ into Archives, then resolve its `OnPlay`
    /// triggers. Corp-only mirror of `PlayEvent`. `card_id`'s `CardType`
    /// must be `Operation` (`RulesError::CardNotOperation` otherwise).
    PlayOperation { card_id: CardId },
    /// Spend 1 click and `card_id`'s registry `cost` in credits (less any
    /// install discount), move `card_id` from the Grip into the Rig.
    /// Runner-only. One console at a time (`RulesError::ConsoleLimitExceeded`);
    /// a second copy of a ◆ card trashes the first.
    InstallHardware { card_id: CardId },
    /// Spend 1 click and `card_id`'s registry `cost` in credits (less any
    /// install discount), move `card_id` from the Grip into the Rig.
    /// Runner-only. Its memory cost is read from the registry and must fit
    /// `rules::memory::available_memory`; a second copy of a ◆ card trashes
    /// the first.
    ///
    /// **Carries no memory cost, deliberately.** It used to, and the caller
    /// had to name a value matching the card's registry `memory_cost`
    /// exactly — which `legal_actions` never did (it always offered `0`),
    /// so no program with a declared cost was ever a legal action. The
    /// registry is the only authority and every handler already holds one,
    /// so the field was pure duplication with a way to be wrong. It also
    /// could not survive the `ActionSpace` round trip: `action_at` takes no
    /// `CardRegistry` and could only ever synthesise `0`.
    ///
    /// How many memory units this reserves is therefore not a property of
    /// the action at all — see `runner::available_memory`, which derives
    /// the Runner's free memory from what is on the board.
    InstallProgram { card_id: CardId },
    /// Spend 1 click and `card_id`'s registry `cost` in credits, move
    /// `card_id` from the Grip into the Rig. Runner-only. Mirrors
    /// `InstallHardware` exactly (no memory-unit reservation, unlike
    /// `InstallProgram`) — added because no action previously existed for
    /// installing a `CardType::Resource` card at all (`Resource`s could
    /// only ever leave the Rig via `TrashResource`, never enter it).
    InstallResource { card_id: CardId },
    /// Spend 1 click and `card_id`'s registry cost in credits, move
    /// `card_id` from the Grip onto the Corp ICE named by `host`
    /// (`state::InstalledRunnerCard::hosted_on_ice`) rather than into the
    /// ordinary Rig-install flow. Runner-only. `card_id` must be a Trojan
    /// Program (`dsl::CardDefinition::installs_on_ice == true`) and
    /// `host` must name a real Corp `InstalledCard` with `slot ==
    /// InstallSlot::Ice` — rezzed or not, real rules allow hosting on
    /// unrezzed ICE. Mirrors `InstallProgram` otherwise, including carrying
    /// no memory cost (see there) — e.g. Botulus, Tranquilizer.
    ///
    /// **`host` is an `InstallId` precisely because unrezzed ICE is a legal
    /// host.** Naming it by `CardId` handed the Runner the identity their
    /// own `ClientView` masks to `None` — a fog-of-war leak straight
    /// through `legal_actions_for`. See `state::InstallId`.
    InstallProgramOnIce { card_id: CardId, host: InstallId },
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
    /// looked up in the `CardRegistry`) on the install named by `target`.
    /// Symmetric — no `side` field; the acting side is whichever side owns
    /// `target`, which an `InstallId` answers directly (it was previously
    /// derived by scanning both zones for a `CardId`). No implicit click
    /// cost — a paid ability's `AbilityDef::cost` is whatever the card
    /// prints, which may itself include `Cost::Clicks`. `target` must be in
    /// an active zone for the acting side (Corp: installed *and* rezzed;
    /// Runner: in the Rig) or this errors with
    /// `RulesError::CardNotActive`. `ability_index` must address a
    /// `Trigger::Paid` ability on that card's definition, or this errors with
    /// `RulesError::InvalidAbilityIndex`/`RulesError::AbilityNotManuallyActivatable`
    /// respectively.
    ActivateAbility { target: InstallId, ability_index: usize },
    /// Place one advancement token on `target`, a Corp-installed card.
    /// Corp-only. Costs 1 click + 1 credit (`pay_cost(state, side,
    /// &Cost::Credits(1))`, in addition to the click). `target` must name a
    /// live install (`RulesError::InstallNotFound` otherwise) — no rez
    /// requirement, matching the real game (advancement doesn't require
    /// rez). Its `CardRegistry` definition must have `advancement_requirement:
    /// Some(_)` (`RulesError::CardNotAdvanceable` otherwise). Advancing never
    /// scores: that is `ScoreAgenda`, a separate — and free — action.
    ///
    /// An `InstallId` because advancing "a *Tithe*" is ambiguous with two
    /// installed and the Corp advances one of them, not both.
    AdvanceCard { target: InstallId },
    /// Score `target`, an installed Corp Agenda whose `advancement_tokens`
    /// already meet its registry `advancement_requirement`. Corp-only, and
    /// **free**: scoring is not an action, costs no click and no credit,
    /// and is legal on zero clicks before the turn ends (it used to spend a
    /// click — ROADMAP Rules Audit T6). `RulesError::InstallNotFound` if `target` isn't in
    /// `CorpState::installed`; `RulesError::CardNotAgenda` if its registry
    /// `card_type` isn't `Agenda`; `RulesError::AdvancementRequirementNotMet`
    /// if it hasn't been advanced enough yet. No rez requirement — scoring
    /// doesn't depend on `InstalledCard::rezzed`, matching real Netrunner/Null
    /// Signal Games rules. On success, moves the card into `CorpState::
    /// scored_agendas`, fires its own `Trigger::OnAgendaScored` triggers
    /// (e.g. Hostile Takeover), then the Corp identity's (e.g. Jinteki:
    /// Personal Evolution), then checks win conditions.
    ///
    /// An `InstallId` for the same reason as `AdvanceCard`: the Corp scores
    /// the copy it advanced, which a `CardId` cannot distinguish.
    ScoreAgenda { target: InstallId },
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
    /// Spend 1 click + 2 credits to trash `target`, an installed Runner
    /// Resource, off the Rig into the Heap. Corp-only, legal only while the
    /// Runner is tagged (`RulesError::RunnerNotTagged` otherwise). Its
    /// `CardRegistry` definition must be `CardType::Resource`
    /// (`RulesError::CardNotResource` otherwise) and it must be installed in
    /// the Runner's rig (`RulesError::InstallNotFound` otherwise).
    ///
    /// An `InstallId` so the Corp trashes one specific copy of a Resource
    /// the Runner installed twice, rather than always the first.
    TrashResource { target: InstallId },
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
    /// unaffordable). Moves the card out of the Corp zone it was accessed
    /// in — HQ, the top of R&D, the run's remote, or Archives — and into
    /// `RunnerState::scored_agendas`, checks win conditions, and advances
    /// to the next accessed card (or finalizes the run) — see
    /// `run::access::resolve_steal`. Blocked while
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
    /// `RulesError::CannotAffordAccessTriggerCost` if the cost can't be paid.
    /// Transitions straight to that card's normal `AccessPhase::
    /// PendingChoice` afterward — see `run::access::resolve_pay_access_trigger`.
    /// Blocked while a Paid Ability Window is open
    /// (`RulesError::BlockedByPaidAbilityWindow`) — both sides must pass
    /// priority first via `PassPriority`. Resolving this action may itself
    /// open a fresh window for the card's `PendingChoice` — see
    /// `rules::paid_ability::open_window_if_at_checkpoint`.
    PayAccessTrigger { card_id: CardId },
    /// Decline to pay the pending `AccessPhase::PendingInteractiveTrigger`'s
    /// `cost`, letting its `effects` resolve instead. Runner-only. Legal
    /// only while a run is in `RunPhase::AccessingCard` and `card_id`
    /// matches the pending interactive trigger (`RulesError::
    /// NotInAccessPhase` otherwise). Transitions to that card's normal
    /// `AccessPhase::PendingChoice` afterward, unless the effects ended the
    /// game — see `run::access::resolve_decline_access_trigger`. Blocked while a
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
    /// Adds `position` to (or removes it from, if already present) the
    /// in-progress selection of a pending `PendingDecision::ChooseCards`.
    /// `RulesError::NoPendingDecision` if none is parked (or a different
    /// variant is); `RulesError::CardNotEligibleForSelection` if `position`
    /// isn't currently a legal candidate (past the end of the pending
    /// decision's `source` zone, or the card there doesn't match its
    /// `filter`).
    ///
    /// **A position into `pending_choice::zone_card_ids`, not a `CardId`.**
    /// Two reasons, and the first is the load-bearing one:
    ///
    /// - The `source` zone may hold cards the chooser cannot see. *Tāo
    ///   Salonga* selects two installed Barriers over `OpponentInstalled`,
    ///   and real Netrunner lets the Runner swap ICE they cannot identify —
    ///   so naming the candidate by `CardId` leaked the identity their own
    ///   `ClientView` had just masked to `None`.
    /// - A `CardId` cannot name the second of two identical cards in a
    ///   zone. That previously needed a copy-count cycling scheme inside
    ///   `pending_choice::resolve_toggle_card_selection` to work at all;
    ///   distinct positions are distinct with no such machinery.
    ///
    /// This is also what `ActionSpace` has always encoded (see
    /// `action_mask`'s `TOGGLE_CARD_SELECTION_START`), so the wire format
    /// is unchanged — the payload merely caught up with it.
    ToggleCardSelection { position: usize },
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

impl PlayerAction {
    /// Every variant's name, in declaration order.
    ///
    /// Exists for the rules-coverage gate in `netrunner_session`, which asks
    /// "was every kind of action applied at least once across the sweep?" —
    /// the question that would have caught `InstallProgram` being silently
    /// unreachable for months (ROADMAP Phase 1 §3). A `const` rather than a
    /// derive so the engine keeps its three dependencies; kept honest by
    /// `variant_names_match_the_enum` below, which fails the moment a
    /// variant is added here or in the enum but not both.
    pub const VARIANT_NAMES: &'static [&'static str] = &[
        "GainCreditClick",
        "DrawCardClick",
        "InstallCard",
        "RezIce",
        "InitiateRun",
        "ContinueRun",
        "JackOut",
        "CompleteRun",
        "PlayEvent",
        "PlayOperation",
        "InstallHardware",
        "InstallProgram",
        "InstallResource",
        "InstallProgramOnIce",
        "BreakSubroutineWithClick",
        "EndTurn",
        "DiscardCard",
        "KeepHand",
        "TakeMulligan",
        "ActivateAbility",
        "AdvanceCard",
        "ScoreAgenda",
        "RemoveTag",
        "PurgeVirusCounters",
        "ChooseTriggerToResolve",
        "TrashResource",
        "SelectCardToAccess",
        "StealAgenda",
        "TrashAccessedCard",
        "PassAccessedCard",
        "PayAccessTrigger",
        "DeclineAccessTrigger",
        "PassPriority",
        "SubmitCorpTraceBid",
        "SubmitRunnerTraceBid",
        "AcceptPendingPaidChoice",
        "DeclinePendingPaidChoice",
        "ResolvePendingChoice",
        "ToggleCardSelection",
        "ConfirmCardSelection",
        "ChooseServerForPendingDecision",
    ];

    /// This action's variant name — `"InstallProgram"`, never the payload.
    /// Read off the `Debug` rendering so a new variant needs no arm here;
    /// the same trick `netrunner_session`'s fog-of-war sweep uses to cover
    /// every variant without a 40-arm match.
    pub fn variant_name(&self) -> String {
        let rendered = format!("{self:?}");
        rendered.split(['(', '{', ' ']).next().unwrap_or(&rendered).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::InstallId;

    /// One instance per variant. Adding a variant to the enum without
    /// adding it here is a non-exhaustive-match compile error via the
    /// `match` below; adding it here without adding it to `VARIANT_NAMES`
    /// fails the assertion. Either way the const cannot silently drift.
    fn one_of_each() -> Vec<PlayerAction> {
        let card = || CardId("card".to_string());
        let install = InstallId::PLACEHOLDER;
        let all = vec![
            PlayerAction::GainCreditClick { side: Side::Corp },
            PlayerAction::DrawCardClick { side: Side::Corp },
            PlayerAction::InstallCard { card_id: card(), zone: ServerId::Hq, slot: InstallSlot::Ice },
            PlayerAction::RezIce { ice: install },
            PlayerAction::InitiateRun { server: ServerId::Hq },
            PlayerAction::ContinueRun,
            PlayerAction::JackOut,
            PlayerAction::CompleteRun,
            PlayerAction::PlayEvent { card_id: card() },
            PlayerAction::PlayOperation { card_id: card() },
            PlayerAction::InstallHardware { card_id: card() },
            PlayerAction::InstallProgram { card_id: card() },
            PlayerAction::InstallResource { card_id: card() },
            PlayerAction::InstallProgramOnIce { card_id: card(), host: install },
            PlayerAction::BreakSubroutineWithClick { ice_id: card(), subroutine_index: 0 },
            PlayerAction::EndTurn,
            PlayerAction::DiscardCard { card_id: card() },
            PlayerAction::KeepHand,
            PlayerAction::TakeMulligan,
            PlayerAction::ActivateAbility { target: install, ability_index: 0 },
            PlayerAction::AdvanceCard { target: install },
            PlayerAction::ScoreAgenda { target: install },
            PlayerAction::RemoveTag,
            PlayerAction::PurgeVirusCounters,
            PlayerAction::ChooseTriggerToResolve { card_id: card() },
            PlayerAction::TrashResource { target: install },
            PlayerAction::SelectCardToAccess { card_id: card() },
            PlayerAction::StealAgenda { card_id: card() },
            PlayerAction::TrashAccessedCard { card_id: card() },
            PlayerAction::PassAccessedCard { card_id: card() },
            PlayerAction::PayAccessTrigger { card_id: card() },
            PlayerAction::DeclineAccessTrigger { card_id: card() },
            PlayerAction::PassPriority { side: Side::Corp },
            PlayerAction::SubmitCorpTraceBid { amount: 0 },
            PlayerAction::SubmitRunnerTraceBid { amount: 0 },
            PlayerAction::AcceptPendingPaidChoice { cost_option_index: None },
            PlayerAction::DeclinePendingPaidChoice,
            PlayerAction::ResolvePendingChoice { option_index: 0 },
            PlayerAction::ToggleCardSelection { position: 0 },
            PlayerAction::ConfirmCardSelection,
            PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq },
        ];
        // The exhaustiveness pressure: a new variant fails to compile here.
        for action in &all {
            match action {
                PlayerAction::GainCreditClick { .. }
                | PlayerAction::DrawCardClick { .. }
                | PlayerAction::InstallCard { .. }
                | PlayerAction::RezIce { .. }
                | PlayerAction::InitiateRun { .. }
                | PlayerAction::ContinueRun
                | PlayerAction::JackOut
                | PlayerAction::CompleteRun
                | PlayerAction::PlayEvent { .. }
                | PlayerAction::PlayOperation { .. }
                | PlayerAction::InstallHardware { .. }
                | PlayerAction::InstallProgram { .. }
                | PlayerAction::InstallResource { .. }
                | PlayerAction::InstallProgramOnIce { .. }
                | PlayerAction::BreakSubroutineWithClick { .. }
                | PlayerAction::EndTurn
                | PlayerAction::DiscardCard { .. }
                | PlayerAction::KeepHand
                | PlayerAction::TakeMulligan
                | PlayerAction::ActivateAbility { .. }
                | PlayerAction::AdvanceCard { .. }
                | PlayerAction::ScoreAgenda { .. }
                | PlayerAction::RemoveTag
                | PlayerAction::PurgeVirusCounters
                | PlayerAction::ChooseTriggerToResolve { .. }
                | PlayerAction::TrashResource { .. }
                | PlayerAction::SelectCardToAccess { .. }
                | PlayerAction::StealAgenda { .. }
                | PlayerAction::TrashAccessedCard { .. }
                | PlayerAction::PassAccessedCard { .. }
                | PlayerAction::PayAccessTrigger { .. }
                | PlayerAction::DeclineAccessTrigger { .. }
                | PlayerAction::PassPriority { .. }
                | PlayerAction::SubmitCorpTraceBid { .. }
                | PlayerAction::SubmitRunnerTraceBid { .. }
                | PlayerAction::AcceptPendingPaidChoice { .. }
                | PlayerAction::DeclinePendingPaidChoice
                | PlayerAction::ResolvePendingChoice { .. }
                | PlayerAction::ToggleCardSelection { .. }
                | PlayerAction::ConfirmCardSelection
                | PlayerAction::ChooseServerForPendingDecision { .. } => {}
            }
        }
        all
    }

    #[test]
    fn variant_names_match_the_enum() {
        let derived: Vec<String> = one_of_each().iter().map(PlayerAction::variant_name).collect();
        assert_eq!(derived, PlayerAction::VARIANT_NAMES, "VARIANT_NAMES must list every variant, in declaration order");
    }
}
