use serde::{Deserialize, Serialize};

use crate::dsl::ability::EffectRequirement;
use crate::dsl::card::{CardId, IceType};
use crate::dsl::cost::Cost;
use crate::dsl::zone::{CardFilter, CardZoneRef};
use crate::rules::{InstallId, ServerId, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageType {
    Net,
    Meat,
    Brain,
}

/// Which ordered deck zone a `TrashCard(CardTarget::TopOfStack)` effect
/// mills from — the only two zones in `GameState` that have a meaningful
/// "top."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackZone {
    RAndD,
    Stack,
}

/// What an `Effect::TrashCard` targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardTarget {
    /// The card this ability/subroutine/trigger is itself printed on. Must
    /// be resolved to a concrete target by the dispatch layer before
    /// reaching `evaluate_effect` — that function has no "which card is
    /// resolving" context on its own.
    ThisCard,
    /// A Corp card installed on a server, identified the same way
    /// `state::InstalledCard` already identifies one (`CardId` +
    /// `ServerId`).
    CorpInstalled { card: CardId, server: ServerId },
    /// A Runner card in the Rig — no server/slot component, since
    /// `RunnerState::rig` is a flat `Vec<CardId>` with no per-card
    /// location metadata.
    RunnerRig(CardId),
    /// The top card of an ordered deck zone, without needing to name it —
    /// covers "mill" effects (trash without revealing).
    TopOfStack { side: Side, zone: StackZone },
    /// The Corp ICE that `acting_card` (a Trojan Program hosted via
    /// `PlayerAction::InstallProgramOnIce`) is currently hosted on —
    /// resolved via `InstalledRunnerCard::hosted_on_ice`, then treated
    /// exactly like `CorpInstalled` (including cascade-trash) once found.
    /// `RulesError::UnresolvedCardTarget` if `acting_card` isn't a hosted
    /// card. e.g. Tranquilizer's "derez host ice" once counters reach 3.
    HostIce,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    /// `Side` is explicit — even though most cards only ever grant
    /// credits to their own controller (and `CardDefinition::side` already implies
    /// that), an explicit target lets a card affect the opponent instead.
    GainCredits(Side, u32),
    /// `GainCredits` with a computed amount — *Jinteki: Restoring Humanity*'s
    /// "gain 1[c] for each facedown card in Archives". A variant rather than
    /// composition because no existing primitive counts anything: `EffectIf`
    /// branches, it does not multiply. Same shape as `DealDamageAmount`.
    GainCreditsAmount(Side, Amount),
    /// Renamed from `InflictDamage`. `usize` (not `u32`) matches
    /// `damage::apply_damage`'s existing signature exactly. No `Side`
    /// param: damage in this engine's model always targets the Runner,
    /// same as `apply_damage` itself.
    DealDamage(DamageType, usize),
    ModifyStrength(i32),
    /// `Side`-explicit for the same reason as `GainCredits`.
    DrawCards(Side, u32),
    /// `DrawCards` with a computed amount — Ritual's "Draw 1 card for each
    /// click you have remaining." Same shape and same reason as
    /// `GainCreditsAmount`: nothing composable multiplies.
    DrawCardsAmount(Side, Amount),
    /// Ends whatever run is in `GameState::active_run`. No payload — there
    /// is exactly one active run at a time.
    EndTheRun,
    /// Deliberately no `Side` param, unlike `GainCredits`/`DrawCards` —
    /// tags exist solely on `RunnerState` in this data model, so
    /// `Side::Corp` would never be a legal target.
    GiveTags(u32),
    /// Deliberately no `Side` param, same rationale as `GiveTags`.
    RemoveTags(u32),
    /// Deliberately no `Side` param — Bad Publicity exists solely on
    /// `CorpState` in this data model, same rationale as `GiveTags`.
    GiveBadPublicity(u32),
    /// Deliberately no `Side` param, same rationale as `GiveBadPublicity`.
    RemoveBadPublicity(u32),
    TrashCard(CardTarget),
    /// Boosts a Runner rig card's own strength — unlike `ModifyStrength`,
    /// which always targets whatever ICE is currently being encountered,
    /// this always targets whichever rig card activated the ability (see
    /// `evaluate_effect`'s `acting_card` parameter). `Encounter`-duration
    /// boosts are cleared when the encounter ends
    /// (`RunnerState::reset_encounter_strength_buffs`); `Turn`-duration
    /// boosts are cleared at the end of the Runner's turn
    /// (`RunnerState::reset_turn_strength_buffs`).
    BoostStrength { amount: u32, duration: BoostDuration },
    /// Breaks pending subroutines on the ICE currently being encountered,
    /// gated on the acting rig card's `effective_strength()` meeting the
    /// ICE's `current_strength` (`RulesError::BreakerStrengthTooLow`
    /// otherwise). `restrict_to`, if set, further gates this on the ICE's
    /// subtype matching (`RulesError::InvalidBreakerSubtype` otherwise) —
    /// e.g. Corroder's `Some(IceType::Barrier)`. `None` is a universal
    /// breaker: no subtype restriction.
    BreakSubroutines {
        count: SubroutineBreakCount,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        restrict_to: Option<IceType>,
    },
    /// Breaks up to `count` pending subroutines on the ICE currently being
    /// encountered — identical to `BreakSubroutines` except it skips the
    /// breaker-strength-vs-ICE-strength contest (and the subtype
    /// restriction) entirely. For hosted-counter-cost break abilities that
    /// have no printed strength stat at all and never contest it — e.g.
    /// Botulus's "hosted virus counter: break 1 subroutine on host ice."
    /// Deliberately a separate variant rather than an `ignore_strength`
    /// flag on `BreakSubroutines` itself, to avoid touching that effect's
    /// ~25 existing construction sites for one card's exception.
    BreakSubroutinesUnconditionally { count: SubroutineBreakCount },
    /// Establishes a trace of strength `base` (plus whatever the Corp
    /// commits on top once bidding begins). Does not resolve `on_success`
    /// synchronously — unlike every other variant, this effect alone cannot
    /// complete within one `evaluate_effect` call, since it spans two future
    /// `PlayerAction`s (the Corp's bid, then the Runner's). `evaluate_effect`
    /// instead parks the pending state in `GameState::active_trace` and
    /// returns immediately; `rules::trace::submit_runner_bid` is what
    /// eventually evaluates `on_success`, if the trace succeeds. `Box`ed
    /// since this is the first `Effect` variant that nests another `Effect`.
    Trace { base: u32, on_success: Box<Effect> },
    /// Grants `count` additional cards accessed from `server` on top of the
    /// normal single-card access, for the remainder of the current run —
    /// e.g. a Runner program's "access 1 additional card from HQ" ability.
    /// Requires an active run (`RulesError::NoActiveRun` otherwise). A
    /// no-op, emitting nothing, for `ServerId::Archives`/`ServerId::
    /// Remote(_)`: a breach of either already accesses every card there,
    /// so an additional access is meaningless by the rules, not merely
    /// unmodelled — which is why only `RunState::additional_hq_access`/
    /// `additional_rd_access` exist, for the two servers whose access is
    /// naturally capped at one card.
    AddAdditionalAccess { server: ServerId, count: u32 },
    /// Replaces this run's normal access of `server` with `effect` instead
    /// — e.g. Account Siphon's "gain 8 credits instead of accessing HQ".
    /// Consumed (and the run concluded) the moment `run::access_server` is
    /// next called against `server`; see `run::access::try_replace_access`.
    /// Requires an active run (`RulesError::NoActiveRun` otherwise).
    /// `Box`ed for the same reason as `Trace::on_success` — the first two
    /// other variants that nest another `Effect`.
    SetAccessReplacement {
        server: ServerId,
        effect: Box<Effect>,
        /// Printed "you **may** … instead" (Account Siphon): the breach's
        /// owner is offered the replacement rather than bound by it — see
        /// `run::access::try_replace_access`, which parks the choice.
        /// Declining consumes the replacement and the next `CompleteRun`
        /// breaches normally. `false` (the default, and the old wire
        /// format) replaces unconditionally.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        optional: bool,
    },
    /// Resolves every `Effect` in order, collecting all of their events —
    /// e.g. Account Siphon's "Corp loses 5, Runner gains 10, Runner gains 2
    /// tags" bundled into the single `Effect` `SetAccessReplacement`
    /// requires. Stops and propagates immediately if any inner `Effect`
    /// errors, same "no rollback of already-applied effects" convention as
    /// `resolve_unbroken_subroutines`/`process_card_triggers`.
    ///
    /// Also stops (without erroring) the instant an inner `Effect` parks
    /// something spanning future `PlayerAction`s (a trace, a prevention
    /// window, `OfferPaidChoice`, `PresentChoice`, or `PromptChooseCards`/
    /// `PromptChooseServer`) — there is no mechanism to resume a `Sequence`
    /// partway through once control returns from one of those, so any
    /// remaining effects after the parking one are simply never reached.
    /// **Don't chain two independently-parking effects in one `Sequence`**
    /// (e.g. two `PromptChooseCards` back to back) — the second would
    /// silently never run. Chain through the parking effect's own `then`
    /// field instead (see Longevity Serum's card JSON for the pattern).
    Sequence(Vec<Effect>),
    /// Symmetric opposite of `GainCredits` — saturating, never errors even
    /// if `side` can't actually afford it (mirrors `GainCredits`'s own
    /// "gains/losses never fail in the rules" precedent, not a `pay_cost`
    /// credit *cost* subject to affordability checks).
    LoseCredits(Side, u32),
    /// Removes `amount` clicks from the Runner (this engine has no card that
    /// costs the Corp a click this way yet, so — like `GiveTags`/
    /// `GiveBadPublicity` — deliberately no `Side` param). Saturating.
    LoseClicks(u32),
    /// Grants `side` extra clicks — e.g. Luminal Transubstantiation's "gain
    /// [click][click][click]" on score. Takes a `Side` (unlike
    /// `LoseClicks`) because the only card needing it is Corp-side.
    GainClicks(Side, u32),
    /// Initiates a run on `server`, exactly like `PlayerAction::InitiateRun`
    /// (same `RunState` shape, `RulesError::RunAlreadyInProgress` guard) but
    /// without spending a click — the enclosing `PlayEvent`/`PlayOperation`
    /// already spent the one click this whole card costs. Lets a single
    /// card's `OnPlay` effect list read as "make a run on X, then [modify
    /// that run's access]" in one resolution (e.g. The Maker's Eye, Account
    /// Siphon) by simply listing `InitiateRun` before the access-modifying
    /// effect(s) that follow it.
    InitiateRun(ServerId),
    /// Saturating-reduces the `amount` of a `PendingPreventionKind::Damage`
    /// currently parked in `GameState::pending_prevention` (a card's
    /// `Trigger::Paid` ability activated during the resulting
    /// `WindowCheckpoint::Prevention` window — e.g. a future "Feedback
    /// Filter"-style card). `RulesError::NoPendingPrevention` if nothing is
    /// parked, `RulesError::PreventionKindMismatch` if what's parked isn't
    /// `Damage`.
    PreventDamage(usize),
    /// Marks a parked `PendingPreventionKind::Trash` as prevented outright —
    /// trash prevention is binary (all-or-nothing per instance), unlike
    /// damage's incremental `amount`. Same error conditions as
    /// `PreventDamage`, mismatched on `Trash` instead of `Damage`.
    PreventTrash,
    /// Saturating-adds `amount` generic counters (see `dsl::card::
    /// CounterKind`) to whichever card activated this effect — always
    /// `acting_card`, the same target `BoostStrength` uses, since a
    /// counter-placing effect is always a card's own ability/trigger
    /// putting counters on itself. `RulesError::UnresolvedCardTarget` if
    /// `acting_card` is `None`, `RulesError::CardNotActive` if it names a
    /// card that's neither a rezzed Corp install nor a Runner rig card.
    AddCounters(u32),
    /// Saturating-removes `amount` generic counters from `acting_card`. Same
    /// target/error rules as `AddCounters`.
    RemoveCounters(u32),
    /// Raises `acting_card`'s generic counters *to* `u32` if they are below
    /// it, and leaves them alone otherwise — a **recurring credit** pool's
    /// "refill to N" (Azimat: "when you install this program and before
    /// your turn begins, refill to 2 hosted credits"). Not composable from
    /// `AddCounters`, which adds: reaching "refill to 2" that way needs one
    /// `EffectIf` per possible starting count. Same target/error rules as
    /// `AddCounters`.
    RefillCountersTo(u32),
    /// Evaluates `effect` only if `condition` holds; otherwise silently
    /// no-ops (`Ok(Vec::new())`) — the same soft-gate convention
    /// `dsl::card::TriggeredEffect::requirement` already uses, but usable
    /// inline inside an effect list/`Sequence` rather than only at a
    /// trigger's top level. Which side's context (e.g. `EffectRequirement::
    /// OncePerTurn`) `condition` is checked against is resolved from
    /// `acting_card`'s own registry `side` — see `evaluate_effect`'s
    /// `EffectIf` arm.
    EffectIf { condition: EffectRequirement, effect: Box<Effect> },
    /// Offers `side` a choice: pay `cost` (resolving `if_paid`), or don't
    /// (resolving `if_declined`) — e.g. Funhouse's subroutine ("give the
    /// Runner 1 tag unless they pay 4 credits") or Anoetic Void ("the Corp
    /// may pay 2 credits and trash 2 HQ cards to end the run"). Doesn't
    /// resolve synchronously: parks a `state::PendingPaidChoice` and
    /// returns immediately, mirroring `Effect::Trace`'s "spans future
    /// `PlayerAction`s" shape. Resolved via `PlayerAction::
    /// AcceptPendingPaidChoice`/`DeclinePendingPaidChoice`
    /// (`rules::pending_choice`).
    ///
    /// Deliberately a second, non-run-scoped mechanism alongside
    /// `dsl::ability::InteractiveOnAccess` rather than a generalization of
    /// it: `InteractiveOnAccess` is intrinsically tied to `RunState::
    /// access_state`/`AccessPhase::PendingInteractiveTrigger` and can't
    /// represent a choice with no active run at all (a standalone
    /// Operation, an on-rez/on-approach trigger before access begins). Both
    /// converge on the same "park state, block unrelated actions, resume
    /// via dedicated `PlayerAction`s" idiom `TraceState`/`PendingPrevention`
    /// already established.
    OfferPaidChoice { side: Side, cost: Cost, if_paid: Box<Effect>, if_declined: Box<Effect> },
    /// Presents `chooser` with a choice of which one `Effect` among
    /// `options` resolves — e.g. Wildcat Strike ("resolve 1 of the
    /// following of the Corp's choice"), NBN: Reality Plus ("gain 2
    /// credits or draw 2 cards"). Parks a `state::PendingDecision::
    /// ChooseEffect` and returns immediately, resolved via `PlayerAction::
    /// ResolvePendingChoice`.
    PresentChoice { chooser: Side, options: Vec<Effect> },
    /// Grants `side` 1 credit for each card accessed during the just-ended
    /// run (`GameState::last_completed_run`) — e.g. Zahya Sadeghi's "gain 1
    /// credit for each time you accessed a card during that run." A
    /// narrowly-scoped one-off (rather than a general dynamic-amount
    /// system, which no card needs yet) — see this variant's tracking note
    /// in `ROADMAP.md` for the planned future generalization.
    GainCreditsPerCardAccessedThisRun(Side),
    /// Offers `side` a choice of up to `max` (at least `min`) cards from
    /// `source` matching `filter`, optionally moving the chosen cards to
    /// `destination` (shuffling it afterward if `shuffle_after`), then
    /// evaluating `then` (if present) with the *first* selected card as
    /// `acting_card` context — e.g. Sprint's "shuffle 2 cards from HQ into
    /// R&D", Mutual Favor's "search your stack for 1 icebreaker", Above the
    /// Law's "you may trash 1 installed resource", Send a Message's "rez 1
    /// installed ICE, ignoring costs" (`destination: None`, `then: Some(
    /// RezInstalledIgnoringCost(..))` — the placeholder `CardId` inside
    /// `then` is ignored; `ConfirmCardSelection`'s resolution substitutes
    /// the actual selected card via `acting_card`, the same substitution
    /// convention `Effect::TrashCard(CardTarget::ThisCard)` already uses).
    ///
    /// Silently no-ops (`Ok(Vec::new())`) without parking anything if fewer
    /// than `min` cards are actually available in `source` — the same
    /// "nothing to do" leniency `Effect::DrawCards`/`TrashCard`'s "already
    /// trashed" case already establish — e.g. Hansei Review's "if there are
    /// any cards in HQ, trash 1 of them" needs no separate `EffectIf` gate
    /// because of this.
    ///
    /// Doesn't resolve synchronously: parks a `state::PendingDecision::
    /// ChooseCards` and returns immediately, resolved via `PlayerAction::
    /// ToggleCardSelection`/`ConfirmCardSelection`.
    PromptChooseCards {
        side: Side,
        source: CardZoneRef,
        filter: CardFilter,
        min: u32,
        max: u32,
        /// Whether the chosen cards' identities are revealed to the
        /// opponent — recorded on the resulting `GameEvent::CardsSelected`
        /// for now; doesn't yet integrate with `masking`'s per-card hidden-
        /// identity rules (no consumer needs that distinction enforced
        /// yet).
        reveal: bool,
        shuffle_after: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        destination: Option<CardZoneRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        then: Option<Box<Effect>>,
    },
    /// Lets `chooser` pick any server to run, then initiates a run against
    /// it — e.g. Tread Lightly ("run any server; during that run, ICE rez
    /// cost is increased by 3"), Overclock ("run any server; you can spend
    /// 5 hosted credits during that run"). `rez_cost_delta`/
    /// `bonus_run_credits` seed the resulting `RunState`'s matching fields
    /// (`0`/`0` for a plain "run any server" with no further modifier).
    /// Doesn't resolve synchronously: parks a `state::PendingDecision::
    /// ChooseServer` and returns immediately, resolved via `PlayerAction::
    /// ChooseServerForPendingDecision`.
    PromptChooseServer {
        chooser: Side,
        rez_cost_delta: i32,
        bonus_run_credits: u32,
        /// Restricts the offer to these servers — e.g. Jailbreak's "Run HQ
        /// or R&D". `None` (the default, and the shape every pre-Jailbreak
        /// card authored) means any server, including a fresh remote.
        /// Honored by `legal_actions` so an excluded server is never even
        /// offered, keeping the action mask and the resolver in agreement.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        allowed_servers: Option<Vec<ServerId>>,
        /// Evaluated if and when the resulting run succeeds, via
        /// `run::RunState::on_success_effect` — e.g. Jailbreak's "If
        /// successful, draw 1 card and ... access 1 additional card". An
        /// `AddAdditionalAccess` inside it has its `server` treated as an
        /// ignored placeholder and rewritten to the server actually chosen,
        /// the same substitution convention `PromptChooseCards::then` uses
        /// for `RezInstalledIgnoringCost`. `RunSucceeded` fires before
        /// access is computed, so an access bonus granted here still
        /// applies to that same breach.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on_success: Option<Box<Effect>>,
        /// Evaluated as the parking card the moment the chosen run starts —
        /// Shred's "The first time the Corp would end that run, ..." arms
        /// itself here. The general form of `bonus_run_credits`: a rider on
        /// the run itself, which a `Sequence` after this effect cannot be,
        /// since the prompt parks and the rest of the sequence never runs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on_start: Option<Box<Effect>>,
        /// Drop from the offer every server the Runner has already run
        /// this turn (`RunnerState::servers_run_this_turn`) — Red Team's
        /// "Run a central server you have not run this turn". A field
        /// rather than a variant or an `EffectRequirement`: a requirement
        /// can only gate the whole ability, and nothing composable narrows
        /// a server *offer*. Applied when the decision is parked, and if
        /// nothing is left to offer the effect fails instead of parking,
        /// so `legal_actions`' probe withholds the ability rather than
        /// offering a click that parks an unresolvable decision.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        exclude_servers_run_this_turn: bool,
    },
    /// Rezzes `card_id`, an already-installed Corp card, skipping the
    /// credit-cost payment `PlayerAction::RezIce` would otherwise require —
    /// e.g. Send a Message's "rez 1 installed ICE, ignoring all costs".
    /// Mirrors `engine::rez_ice`'s state transition (flips `InstalledCard::
    /// rezzed`, syncs the matching `RunIce::rezzed` if mid-`ApproachIce`,
    /// dispatches `Trigger::OnRez`) minus the payment step —
    /// `RulesError::CardNotInstalled`/`RulesError::AlreadyRezzed` under the
    /// same conditions `rez_ice` itself would reject. Deliberately
    /// duplicates (rather than shares) `rez_ice`'s state-mutation lines: the
    /// two live in different modules (`ability`/`engine`) and the paid path
    /// additionally needs `paid_ability::note_window_action`, which this
    /// free variant has no priority-window context to call.
    ///
    /// An `InstallId` for the same reason as `SwapInstalledIce`: *Send a
    /// Message* rezzes the installed ICE the Corp chose, which with two
    /// unrezzed copies of one card a `CardId` could not name.
    RezInstalledIgnoringCost(InstallId),
    /// Removes *every* counter currently on `acting_card` and grants `side`
    /// that many credits — e.g. Pennyshaver's "place 1 credit on this
    /// hardware, then take all credits from it." A narrowly-scoped one-off
    /// (like `GainCreditsPerCardAccessedThisRun`) rather than a general
    /// dynamic-amount system, which no card needs yet — the M4 plan
    /// section claimed hosted-credit-pool cards would need no new `Effect`
    /// variants at all, but "take a variable amount, not a fixed N" is a
    /// genuine gap the fixed-`u32` `RemoveCounters`/`GainCredits` pair
    /// can't express. Same `RulesError::UnresolvedCardTarget`/
    /// `CardNotEligibleForCounters` error conditions as `AddCounters`/
    /// `RemoveCounters` (delegates to the same `modify_counters` helper).
    TakeAllCountersAsCredits(Side),
    /// Permanently grants `side` `amount` additional max hand size —
    /// e.g. Superconducting Hub's "you get +2 maximum hand size" (fired
    /// from its own `Trigger::OnAgendaScored`). Adds to `CorpState`/
    /// `RunnerState::max_hand_size_bonus`; never decremented (see that
    /// field's doc comment).
    GainMaxHandSize(Side, u32),
    /// Trashes the card currently pending in `run::AccessPhase::
    /// PendingChoice` — the card the Runner is actively accessing — for
    /// free, skipping its `trash_cost` entirely (unlike `PlayerAction::
    /// TrashAccessedCard`/`run::access::resolve_trash`, which charges it).
    /// e.g. Carnivore's "trash 2 cards from your grip: trash the card you
    /// are accessing." `RulesError::NotInAccessPhase` if the Runner isn't
    /// actually mid-access of a specific card right now. See
    /// `run::access::trash_currently_accessed_card_without_cost`.
    TrashCurrentlyAccessedCard,
    /// Flips a rezzed Corp installed card back face-down — the only
    /// player/effect-driven derez path (rez itself is otherwise one-way
    /// via `PlayerAction::RezIce`). `target` is almost always
    /// `CardTarget::HostIce` in practice (e.g. Tranquilizer's "derez host
    /// ice" once 3+ virus counters accumulate) but composes with any
    /// `CardTarget` that resolves to a Corp installed card.
    DerezCard(CardTarget),
    /// Gains `credits_per_counter` credits for each of `acting_card`'s own
    /// hosted counters — a narrow, proportional one-off distinct from
    /// `TakeAllCountersAsCredits`'s flat 1-per-counter payout and from
    /// `GainCreditsPerCardAccessedThisRun`'s unrelated read, kept
    /// deliberately separate from removing the counters (the caller pairs
    /// it with its own cost/cleanup, e.g. Fermenter's "[click], [trash]:
    /// gain 2 credits for each hosted virus counter" — the `TrashSelf`
    /// cost already disposes of the card and its counters together, so
    /// this effect only needs to read the count once). To be folded into a
    /// general `Amount` vocabulary alongside `TakeAllCountersAsCredits`/
    /// `GainCreditsPerCardAccessedThisRun` in a later milestone.
    GainCreditsPerCounter { side: Side, credits_per_counter: u32 },
    /// Exchanges two Corp ICE's `server`/`slot` positions in place — e.g.
    /// Tāo Salonga's "you may swap 2 installed pieces of ice." Both
    /// `CardId`s are authored as unused placeholders in JSON (the real
    /// targets aren't known until the Runner picks them) and substituted
    /// at `PendingDecision::ChooseCards` resolution time, the same
    /// "acting-context substitution" convention `Effect::
    /// RezInstalledIgnoringCost` already established for a single target —
    /// extended here to two. `RulesError::CardNotInstalled` if either
    /// doesn't resolve to a currently-installed ICE. Legal mid-run: the
    /// run's `RunState::ice` follows `CorpState::installed` through
    /// `run::reconcile_ice`, so a swap involving the attacked server is
    /// reflected at the run's next step (it used to be refused with a
    /// `CannotSwapIceDuringActiveRun` error, because that list was a
    /// snapshot).
    ///
    /// Takes `InstallId`s rather than `CardId`s so a swap of two copies of
    /// the *same* ICE resolves. Under `CardId` both placeholders were
    /// substituted with the same value, both position lookups found the
    /// first copy, and the swap silently no-opped — reachable with two
    /// Barriers of one title against *Tāo Salonga*.
    SwapInstalledIce(InstallId, InstallId),
    /// Installs `card_id` (a placeholder substituted at `PendingDecision::
    /// ChooseCards` resolution time, same convention as `SwapInstalledIce`)
    /// from `origin_zone` (fixed at authoring — `OwnHq` or `OwnArchives`)
    /// into `into` (a placeholder substituted with `source_card`'s own
    /// currently-installed server), skipping its install cost entirely.
    /// `slot`: `None` infers `Ice` for `CardType::Ice(_)` and `Root`
    /// otherwise; `Some`
    /// pins it explicitly (Brân 1.0's subroutine only ever offers ICE, but
    /// authors it explicitly for clarity). `insert_after`: `None` appends
    /// to the end of `CorpState::installed`; `Some(host_card_id)` (substituted from
    /// `source_card`, same as `into`) inserts immediately after that
    /// card's own index instead, which — since `CorpState::installed`'s
    /// vec order is install order and `run::engine::build_run_ice` derives
    /// a server's ICE sequence positionally from it — is exactly what
    /// Brân 1.0's "directly inward from this ice" means structurally.
    InstallFromZoneIgnoringCost {
        card_id: CardId,
        origin_zone: CardZoneRef,
        into: ServerId,
        slot: Option<crate::rules::InstallSlot>,
        /// The install the new card goes directly after in
        /// `corp.installed` (directly inward of it), or `None` to append.
        /// Authored as the placeholder `0` (`InstallId::PLACEHOLDER`) meaning
        /// "this ice"; `pending_choice` substitutes the encountered ICE's
        /// real install at resolution. An install, not a `CardId`: two
        /// copies of Brân 1.0 on two servers must each insert inward of
        /// themselves, and a first-match-by-title lookup picked the first.
        insert_after: Option<crate::rules::InstallId>,
    },
    /// Parks the Corp's choice of *destination server* for installing the
    /// resolving card — `acting_card`, a card sitting in `origin_zone`
    /// (`OwnHq` or `OwnArchives`) — then installs it there **paying** the
    /// normal install cost: Ansel 1.0's "You may install 1 card from HQ or
    /// Archives", whose printed text neither fixes the server nor waives
    /// the cost. Parks a `PendingDecision::ChooseServer` carrying a
    /// `state::PendingInstallFromZone` (see its doc for why the card is a
    /// position, not an id), with `allowed_servers` precomputed by
    /// `engine::corp_install_destinations` — agendas/assets to remotes
    /// only, ICE only where the per-protecting-ICE tax is affordable.
    /// Resolved by the same `PlayerAction::ChooseServerForPendingDecision`
    /// a run-target choice uses. Contrast `InstallFromZoneIgnoringCost`
    /// (Brân 1.0): a *positional* install — "directly inward from this
    /// ice" — that ignores costs and offers no server choice.
    PromptInstallCorpCard { origin_zone: CardZoneRef },
    /// Installs the resolving card — `acting_card`, a card sitting in the
    /// Runner's grip — into the rig, **paying** its install cost (with the
    /// usual discounts) and respecting the memory budget, the console limit
    /// and the unique rule: Pantograph's "you may install 1 card from your
    /// grip", Mutual Favor's "you may install that program" (the search has
    /// already moved the found icebreaker to the grip, so both install from
    /// the one zone). A Trojan is out of scope — its host is a choice no
    /// parked effect models yet — and is never offered; see `CardFilter::
    /// InstallableRunnerCard`, whose eligibility this effect re-checks,
    /// silently no-oping (the card stays in the grip) if the pick has
    /// become uninstallable since it was offered. Contrast
    /// `InstallFromZoneIgnoringCost`, the Corp-side subroutine install
    /// that pays nothing.
    InstallRunnerCardFromGrip,
    /// `InstallRunnerCardFromGrip` for a card sitting in the **heap** —
    /// Scrounge's "Install 1 program from your heap", Magdalene
    /// Keino-Chemutai's install from among the cards just discarded. A
    /// sibling rather than a zone parameter on the grip variant so every
    /// existing card JSON keeps its bare `"InstallRunnerCardFromGrip"`
    /// string; both share one pricing and eligibility path
    /// (`engine::can_install_runner_card_from_zone`). Same Trojan exclusion
    /// and same silent no-op when the pick is no longer installable.
    InstallRunnerCardFromHeap,
    /// `InstallRunnerCardFromGrip` paying `u32` less — Illumination's
    /// "install up to 3 cards from your grip, paying 1[c] less for each".
    /// Paired with `CardFilter::InstallableRunnerCardWithDiscount` so the
    /// offer and the price agree. A discount parameter rather than a
    /// separate pricing effect: nothing composable subtracts from a cost.
    InstallRunnerCardFromGripWithDiscount(u32),
    /// Installs the resolving card from the cards **hosted on the acting
    /// install** (`InstalledRunnerCard::hosted_cards`, the parking card's
    /// own — `ResolutionContext::acting_install`), paying its cost —
    /// Madani's "Install 1 hosted program". Same eligibility as the grip
    /// variant, over `CardZoneRef::HostedOnSource`.
    InstallRunnerCardFromHost,
    /// Marks the active run so that, when it would approach its server
    /// after passing every piece of ice, it approaches `ServerId` instead
    /// (`RunState::redirect_on_approach`) — Maintenance Access's "instead
    /// change the attacked server to HQ and approach HQ". The new server's
    /// ice is not encountered: the rules say *approach* HQ, and the run's
    /// ice list becomes HQ's, all passed. `RulesError::NoActiveRun` if no
    /// run is active. A run-state flag rather than an immediate change
    /// because the redirect happens at a later step of the same run.
    RedirectRunOnApproach(ServerId),
    /// Registers `Effect` to resolve as the parking card when the active
    /// run ends, however it ends (`RunState::on_end_effect`, evaluated by
    /// the `OnRunEnded` dispatch) — Charm Offensive's "When that run ends,
    /// you may trash 1 rezzed copy of a card you accessed". The run-end
    /// twin of `PromptChooseServer::on_success`, for an Event that is in
    /// the heap by then and cannot carry a `Trigger::OnRunEnded` of its
    /// own. `RulesError::NoActiveRun` if no run is active.
    SetRunEndedEffect(Box<Effect>),
    /// Arms the active run's `RunState::end_run_prevention` — Shred. See
    /// `EndRunPrevention` for what `Effect::EndTheRun` does with it.
    ArmRunEndPrevention(EndRunPrevention),
    /// Sabotage `u32`: the Corp trashes that many cards of their choice
    /// from HQ and/or the top of R&D (Cacophony). Parks a Corp
    /// `PendingDecision::ChooseCards` over HQ whose bounds are computed
    /// from the two zones' sizes — at least what R&D cannot cover, at most
    /// what HQ holds — with a `MillRnDAmount(RemainingAfterSelection)`
    /// `then` for the rest; with nothing in HQ to choose from it mills R&D
    /// directly. An engine variant because the bounds and the "rest" are
    /// decided by state a card author cannot see.
    Sabotage(u32),
    /// Trashes `Amount` cards from the top of R&D, facedown — the R&D half
    /// of a sabotage.
    MillRnDAmount(Amount),
    /// Moves one card from HQ, chosen at random with the state's PRNG,
    /// onto the acting rig card's `hosted_cards`, faceup — Detente's "host
    /// 1 card from HQ at random faceup on this hardware". A no-op with an
    /// empty HQ.
    HostRandomHqCardOnThisCard,
    /// Flips the Runner's identity to its other side
    /// (`RunnerState::identity_flipped`) — Dewi Subrotoputri. A flag rather
    /// than swapping the identity card: one card, two sides, and every
    /// side's text gated by `EffectRequirement::IdentityFlipped`.
    FlipIdentity,
    /// Moves `acting_card` — a card in the Runner's grip or heap — to the
    /// **bottom** of the stack: Scrounge's "You may add 1 program from your
    /// heap to the bottom of your stack." `PromptChooseCards::destination`
    /// cannot express it: a destination zone receives cards at its *top*
    /// (the end of the `Vec` a draw pops from), and no existing primitive
    /// addresses the bottom of a deck. A no-op when the card is in neither
    /// zone, per the `TrashCard` "already gone" precedent.
    AddToBottomOfStack,
    /// Hosts the rig card `card` on the rig card `host` —
    /// `state::InstalledRunnerCard::hosted_on_program` — GAMEDRAGON™ Pro's
    /// "you may host this hardware on an installed non-AI icebreaker". A
    /// relation between two installs, which nothing composable can name:
    /// both are authored as `InstallId::PLACEHOLDER` and substituted when
    /// the parking `PromptChooseCards` resolves, the way `SwapInstalledIce`
    /// is — `card` becomes the parking card's own install, `host` the
    /// selected one. `RulesError::InstallNotFound` if either has left the
    /// rig, `RulesError::HostIsNotIce`-style rejection is not needed:
    /// eligibility is the prompt's `CardFilter::Icebreaker`. Re-hosting an
    /// already-hosted card simply moves it.
    HostRigCardOnInstall { card: crate::rules::InstallId, host: crate::rules::InstallId },
    /// Sets `RunState::runner_cannot_steal_or_trash`, blocking `PlayerAction::
    /// StealAgenda`/`TrashAccessedCard` for the remainder of the current
    /// run — e.g. Ansel 1.0's third subroutine. `RulesError::NoActiveRun`
    /// if there's no run to apply it to. Cleared automatically when the run
    /// ends (`RunState` isn't carried between runs), never persists past it.
    PreventStealAndTrashForRemainderOfRun,
    /// Sets `CorpState::cannot_score_agendas_this_turn`, blocking any
    /// further `PlayerAction::ScoreAgenda` for the remainder of the Corp's
    /// turn — e.g. Luminal Transubstantiation's "You cannot score agendas
    /// for the remainder of the turn". Unlike
    /// `PreventStealAndTrashForRemainderOfRun` this needs no active-run
    /// guard: it's turn-scoped, cleared by `turn::enter_start_of_turn`.
    PreventScoringForRemainderOfTurn,
    /// Places `0` advancement counters on `acting_card` — e.g. Seamless
    /// Launch's "place 2 advancement counters on 1 installed card". Distinct
    /// from `AddCounters`, which targets the generic `counters` field;
    /// advancement tokens are their own thing (`InstalledCard::
    /// advancement_tokens`, what `ScoreAgenda` reads). Authored as the
    /// `then` of a `PromptChooseCards`, so `acting_card` is the card the
    /// Corp just selected. `RulesError::CardNotInstalled` if that card isn't
    /// a Corp install (only Corp cards can be advanced).
    AddAdvancementTokens(u32),
    /// `Effect::DealDamage` with `amount` resolved dynamically via
    /// `Amount` instead of authored as a flat `usize` — e.g. Neurospike's
    /// "X net damage, X = agenda points scored this turn." Delegates to the
    /// exact same `damage::apply_damage`/prevention-parking logic
    /// `DealDamage` itself uses once the amount is resolved.
    DealDamageAmount(DamageType, Amount),
    /// `Effect::AddAdditionalAccess` with `count` resolved dynamically via
    /// `Amount` — e.g. Conduit's "access X additional cards, X = hosted
    /// virus counters." Same `Hq`/`RnD`-only, silent-no-op-elsewhere
    /// semantics as the fixed-count variant.
    AddAdditionalAccessAmount { server: ServerId, amount: Amount },
    /// `Effect::BoostStrength` with `amount` resolved dynamically via
    /// `Amount` — e.g. Unity's "+X strength, X = installed icebreakers."
    BoostStrengthAmount { amount: Amount, duration: BoostDuration },
}

/// A dynamically-resolved quantity, computed at effect-evaluation time via
/// `rules::ability::resolve_amount` rather than authored as a flat literal —
/// the counterpart to plain `u32` fields on effects like `DealDamage`/
/// `AddAdditionalAccess`/`BoostStrength` for the handful of cards whose text
/// scales with some other piece of state. Deliberately a small, closed set
/// (not a general expression language) — extend only when a real card needs
/// a new formula. `TakeAllCountersAsCredits`/`GainCreditsPerCounter`/
/// `GainCreditsPerCardAccessedThisRun` predate this enum and aren't folded
/// into it (no behavior change, no card needs the refactor yet) — see
/// ROADMAP.md's tracking note for the planned future consolidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Amount {
    /// A plain literal — lets an amount-typed effect field be authored with
    /// an ordinary fixed number when no dynamic formula is needed.
    Fixed(u32),
    /// Sum of printed agenda points on agendas the Corp has scored this
    /// turn (`CorpState::agenda_points_scored_this_turn`) — e.g. Neurospike.
    AgendaPointsScoredThisTurn,
    /// `acting_card`'s own hosted generic counter count — e.g. Conduit's
    /// R&D-access bonus.
    HostedCounters,
    /// `acting_card`'s own hosted advancement token count (Corp installed
    /// cards only) — e.g. Clearinghouse, Urtica Cipher.
    HostedAdvancementTokens,
    /// Count of Runner-installed icebreakers (`dsl::zone::CardFilter::
    /// Icebreaker`'s heuristic), including `acting_card` itself if it
    /// qualifies — e.g. Unity's pump ability.
    InstalledIcebreakerCount,
    /// Facedown cards currently in Archives — *Jinteki: Restoring Humanity*.
    FacedownCardsInArchives,
    /// Credits actually removed by the most recent `Effect::LoseCredits`
    /// **in this same resolution** (`ResolutionContext::credits_lost` —
    /// the printed amount capped by what the side had), the same
    /// within-one-resolution contract `damage_discarded` follows. Account
    /// Siphon's "gain 2[c] for each credit lost" is authored as two
    /// `GainCreditsAmount`s of this — composition instead of a multiplier
    /// field no second card needs.
    CreditsLostThisResolution,
    /// Clicks the side whose action phase it is still has — Ritual's "Draw
    /// 1 card for each click you have remaining", counted *after* the click
    /// that played it was spent. Resolves to 0 outside an action phase.
    ClicksRemaining,
    /// The printed install cost of `acting_card` — "Knickknack" O'Brian's
    /// "gain credits equal to its printed install cost", where the acting
    /// card is the one the prompt selected. Read from the registry, so a
    /// discount the card was installed with does not count.
    PrintedInstallCost,
    /// `u32` minus the cards the resolving `PromptChooseCards` selected
    /// (`ResolutionContext::selected_count`) — the R&D half of a sabotage
    /// of `u32`, resolved in the HQ selection's `then`. Saturating.
    RemainingAfterSelection(u32),
}

/// What `Effect::EndTheRun` does the first time it would end a run whose
/// `RunState::end_run_prevention` is armed — Shred's "The first time the
/// Corp would end that run, prevent the run from ending unless ...". A
/// closed enum with one clause, extended when a card prints another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndRunPrevention {
    /// The run ends only if the Corp pays `Cost::TrashRandomFromHq(X)`,
    /// X being the number of cards in the attacked server's root — parked
    /// as a Corp `OfferPaidChoice` whose acceptance ends the run. With an
    /// empty root there is nothing to pay and the run simply ends; with
    /// fewer than X cards in HQ the Corp cannot pay and the run goes on.
    UnlessCorpTrashesRootCountFromHq,
}

/// How long an `Effect::BoostStrength` buff lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoostDuration {
    /// Cleared when the current ICE encounter ends.
    Encounter,
    /// Cleared when the run ends — Gordian Blade's "+1 strength for the
    /// remainder of this run": one pump carries across every encounter of
    /// the run it was bought in, and no further.
    Run,
    /// Cleared at the end of the Runner's turn.
    Turn,
}

/// How many pending subroutines an `Effect::BreakSubroutines` breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubroutineBreakCount {
    /// Breaks up to this many pending subroutines, lowest-id first —
    /// breaks fewer (not an error) if fewer are pending, mirroring
    /// `Effect::DrawCards`'s "stop silently on empty" precedent.
    Fixed(u32),
    /// Breaks every currently-pending subroutine.
    All,
}

impl Effect {
    /// Calls `f` on this effect and then on every effect nested inside it,
    /// depth-first in authoring order.
    ///
    /// The nesting positions are the whole list of places one `Effect` can
    /// contain another — `Sequence`, `EffectIf`, `OfferPaidChoice` (both
    /// branches), `PresentChoice`, `PromptChooseCards::then`,
    /// `PromptChooseServer::on_success`, `Trace::on_success` and
    /// `SetAccessReplacement`. Kept as an exhaustive `match` with an
    /// explicit leaf arm rather than a `_ =>` so a new nesting variant is a
    /// compile error here, not a silently unwalked subtree.
    ///
    /// Exists because two consumers need the same walk and neither belongs
    /// in the other: the rules-coverage report infers which `Effect`
    /// variants a card's activated ability reached, and the `ActionSpace`
    /// cap gate checks every `PresentChoice`'s option count against
    /// `MAX_PENDING_CHOICE_OPTIONS` — the cap that was wrong for Ansel 1.0
    /// and Brân 1.0 because nothing walked the card JSON to check it.
    pub fn for_each_effect(&self, f: &mut impl FnMut(&Effect)) {
        f(self);
        match self {
            Effect::Sequence(effects) | Effect::PresentChoice { options: effects, .. } => {
                for effect in effects {
                    effect.for_each_effect(f);
                }
            }
            Effect::EffectIf { effect, .. }
            | Effect::Trace { on_success: effect, .. }
            | Effect::SetRunEndedEffect(effect)
            | Effect::SetAccessReplacement { effect, .. } => effect.for_each_effect(f),
            Effect::OfferPaidChoice { if_paid, if_declined, .. } => {
                if_paid.for_each_effect(f);
                if_declined.for_each_effect(f);
            }
            Effect::PromptChooseCards { then: Some(effect), .. } => effect.for_each_effect(f),
            Effect::PromptChooseServer { on_success, on_start, .. } => {
                for effect in [on_success, on_start].into_iter().flatten() {
                    effect.for_each_effect(f);
                }
            }
            Effect::PromptChooseCards { then: None, .. } => {}
            // Leaves: everything that holds no `Effect`.
            Effect::GainCredits(..)
            | Effect::DealDamage(..)
            | Effect::ModifyStrength(..)
            | Effect::DrawCards(..)
            | Effect::EndTheRun
            | Effect::GiveTags(..)
            | Effect::RemoveTags(..)
            | Effect::GiveBadPublicity(..)
            | Effect::RemoveBadPublicity(..)
            | Effect::TrashCard(..)
            | Effect::BoostStrength { .. }
            | Effect::BreakSubroutines { .. }
            | Effect::BreakSubroutinesUnconditionally { .. }
            | Effect::AddAdditionalAccess { .. }
            | Effect::LoseCredits(..)
            | Effect::LoseClicks(..)
            | Effect::GainClicks(..)
            | Effect::InitiateRun(..)
            | Effect::PreventDamage(..)
            | Effect::PreventTrash
            | Effect::AddCounters(..)
            | Effect::RemoveCounters(..)
            | Effect::GainCreditsPerCardAccessedThisRun(..)
            | Effect::RezInstalledIgnoringCost(..)
            | Effect::TakeAllCountersAsCredits(..)
            | Effect::GainMaxHandSize(..)
            | Effect::TrashCurrentlyAccessedCard
            | Effect::DerezCard(..)
            | Effect::GainCreditsPerCounter { .. }
            | Effect::SwapInstalledIce(..)
            | Effect::InstallFromZoneIgnoringCost { .. }
            | Effect::PromptInstallCorpCard { .. }
            | Effect::InstallRunnerCardFromGrip
            | Effect::InstallRunnerCardFromHeap
            | Effect::InstallRunnerCardFromGripWithDiscount(..)
            | Effect::InstallRunnerCardFromHost
            | Effect::RedirectRunOnApproach(..)
            | Effect::ArmRunEndPrevention(..)
            | Effect::Sabotage(..)
            | Effect::MillRnDAmount(..)
            | Effect::HostRandomHqCardOnThisCard
            | Effect::FlipIdentity
            | Effect::AddToBottomOfStack
            | Effect::HostRigCardOnInstall { .. }
            | Effect::RefillCountersTo(..)
            | Effect::DrawCardsAmount(..)
            | Effect::PreventStealAndTrashForRemainderOfRun
            | Effect::PreventScoringForRemainderOfTurn
            | Effect::AddAdvancementTokens(..)
            | Effect::DealDamageAmount(..)
            | Effect::AddAdditionalAccessAmount { .. }
            | Effect::BoostStrengthAmount { .. }
            | Effect::GainCreditsAmount(..) => {}
        }
    }

    /// The variant name of this effect — `"Sequence"`, `"GainCredits"` —
    /// taken from the `Debug` rendering up to its first payload delimiter.
    /// Used wherever variants are counted by name; adding a variant needs no
    /// change here.
    pub fn variant_name(&self) -> String {
        let rendered = format!("{self:?}");
        rendered.split(['(', '{', ' ']).next().unwrap_or(&rendered).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_strength_and_break_subroutines_round_trip_through_json() {
        let boost = Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter };
        let boost_json = serde_json::to_string(&boost).unwrap();
        assert_eq!(boost_json, r#"{"BoostStrength":{"amount":1,"duration":"Encounter"}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&boost_json).unwrap(), boost);

        let turn_boost = Effect::BoostStrength { amount: 2, duration: BoostDuration::Turn };
        let turn_boost_json = serde_json::to_string(&turn_boost).unwrap();
        assert_eq!(turn_boost_json, r#"{"BoostStrength":{"amount":2,"duration":"Turn"}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&turn_boost_json).unwrap(), turn_boost);

        let fixed = Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None };
        let fixed_json = serde_json::to_string(&fixed).unwrap();
        assert_eq!(fixed_json, r#"{"BreakSubroutines":{"count":{"Fixed":1}}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&fixed_json).unwrap(), fixed);

        let all = Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None };
        let all_json = serde_json::to_string(&all).unwrap();
        assert_eq!(all_json, r#"{"BreakSubroutines":{"count":"All"}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&all_json).unwrap(), all);
    }

    #[test]
    fn break_subroutines_restrict_to_round_trips_through_json() {
        let restricted = Effect::BreakSubroutines {
            count: SubroutineBreakCount::Fixed(1),
            restrict_to: Some(crate::dsl::card::IceType::Barrier),
        };
        let restricted_json = serde_json::to_string(&restricted).unwrap();
        assert_eq!(
            restricted_json,
            r#"{"BreakSubroutines":{"count":{"Fixed":1},"restrict_to":"Barrier"}}"#
        );
        assert_eq!(serde_json::from_str::<Effect>(&restricted_json).unwrap(), restricted);

        // Absent restrict_to key still parses fine (backward-compatible with
        // older JSON that predates this field).
        let no_restrict_json = r#"{"BreakSubroutines":{"count":{"Fixed":1}}}"#;
        assert_eq!(
            serde_json::from_str::<Effect>(no_restrict_json).unwrap(),
            Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None }
        );
    }

    #[test]
    fn trace_round_trips_through_json() {
        let trace = Effect::Trace { base: 3, on_success: Box::new(Effect::GiveTags(1)) };
        let trace_json = serde_json::to_string(&trace).unwrap();
        assert_eq!(trace_json, r#"{"Trace":{"base":3,"on_success":{"GiveTags":1}}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&trace_json).unwrap(), trace);
    }

    #[test]
    fn add_additional_access_round_trips_through_json() {
        let effect = Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 };
        let json = serde_json::to_string(&effect).unwrap();
        assert_eq!(json, r#"{"AddAdditionalAccess":{"server":"Hq","count":1}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&json).unwrap(), effect);
    }

    #[test]
    fn set_access_replacement_round_trips_through_json() {
        let effect = Effect::SetAccessReplacement {
            server: ServerId::Hq,
            effect: Box::new(Effect::GainCredits(Side::Runner, 8)),
            optional: false,
        };
        let json = serde_json::to_string(&effect).unwrap();
        assert_eq!(
            json,
            r#"{"SetAccessReplacement":{"server":"Hq","effect":{"GainCredits":["Runner",8]}}}"#
        );
        assert_eq!(serde_json::from_str::<Effect>(&json).unwrap(), effect);
    }

    #[test]
    fn for_each_effect_reaches_every_nesting_position() {
        let effect = Effect::Sequence(vec![
            Effect::EffectIf {
                condition: crate::dsl::EffectRequirement::DuringEncounter,
                effect: Box::new(Effect::OfferPaidChoice {
                    side: Side::Runner,
                    cost: crate::dsl::Cost::Credits(1),
                    if_paid: Box::new(Effect::GainCredits(Side::Runner, 1)),
                    if_declined: Box::new(Effect::EndTheRun),
                }),
            },
            Effect::PresentChoice {
                chooser: Side::Corp,
                options: vec![
                    Effect::Trace { base: 2, on_success: Box::new(Effect::GiveTags(1)) },
                    Effect::SetAccessReplacement { server: ServerId::Hq, effect: Box::new(Effect::DrawCards(Side::Runner, 1)), optional: false },
                ],
            },
        ]);

        let mut names = Vec::new();
        effect.for_each_effect(&mut |e| names.push(e.variant_name()));
        assert_eq!(
            names,
            [
                "Sequence",
                "EffectIf",
                "OfferPaidChoice",
                "GainCredits",
                "EndTheRun",
                "PresentChoice",
                "Trace",
                "GiveTags",
                "SetAccessReplacement",
                "DrawCards",
            ]
        );
    }
}
