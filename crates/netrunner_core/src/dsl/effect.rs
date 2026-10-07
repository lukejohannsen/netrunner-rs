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

/// How much less a card's text installs a card for — `Effect::
/// InstallRunnerCardFromGripWithDiscount` and the offer that goes with it,
/// `CardFilter::InstallableRunnerCardWithDiscount`, so the two agree. Read
/// as credits by `ability::discount_credits`, at the offer and the install
/// alike.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Discount {
    /// "Paying 1[credit] less" (Illumination, Topan).
    Credits(u32),
    /// "Ignoring all costs" (Beta Build).
    AllCosts,
    /// As many credits less as the amount — Pauleʼs Café's "costs 1[credit]
    /// less to install for each unique (♦) connection resource you have
    /// installed", `Amount(RunnerInstalls(..))`, read as the install is
    /// offered and again as it is made. Composition didn't work: a
    /// discount was a fixed number, and a standing install cost
    /// (`ContinuousKind::InstallCost`) is about every install of a kind,
    /// never the one card a card's own text installs.
    Amount(Box<Amount>),
    /// "Paying 1[credit] **more**" — Masterwork (v37)'s "you may install 1
    /// piece of hardware from your grip, paying 1[credit] more". A discount
    /// below nothing, read through the same door as every other
    /// (`ability::discount_credits`, signed), so the offer and the install
    /// agree on the price. Composition didn't work: a discount was a
    /// number taken off, and an extra credit paid first as its own cost
    /// could be spent on an install the Runner then could not afford.
    Surcharge(u32),
}

/// Which end of a deck `Effect::AddToDeck` puts a card on. The top is the
/// end of the `Vec` a draw pops from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeckEnd {
    Top,
    Bottom,
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
    /// Every card hosted on `acting_card` (`InstalledRunnerCard::
    /// hosted_cards`) — Bling's "when your discard phase ends, trash all
    /// hosted cards". Only meaningful for `Effect::TrashCard`; each hosted
    /// card goes to its owner's discard pile.
    HostedOnThisCard,
    /// The piece of ice the Runner is encountering — Arruaceiras Crew's
    /// "trash the ice you are encountering". Resolved off the run
    /// (`RunState::ice` at its position, in `RunPhase::EncounterIce`),
    /// then treated like `CorpInstalled`; `RulesError::NotInEncounter`
    /// outside one. `ModifyStrength` needed no target, because it only
    /// ever meant this ice.
    EncounteredIce,
    /// A random card from that side's hand, one per `TrashCard`: from HQ
    /// facedown, as the Corp's other unseen trashes are — Heliamphora's
    /// "they trash 2 cards from HQ at random" — and from the grip to the
    /// heap faceup, as every Runner card goes — Mystic Maemi's "you must
    /// trash 1 card from your grip at random". Nothing when the hand is
    /// empty. The trash is the resolving card's controller's
    /// (`CardTrashed::by`), as every "the Corp trashes" a Runner card
    /// prints is (conformance ledger, 1.14). It was `RandomFromHq`;
    /// Maemi's is the same instruction about the other hand.
    RandomFromHand(crate::rules::Side),
    /// Every card in the root of the attacked server — Light the Fire!'s
    /// "When that run is successful, trash all cards in the root of the
    /// attacked server". Only meaningful for `Effect::TrashCard`; each card
    /// is trashed by the resolving card's controller, one after another,
    /// and nothing happens outside a run. Composition didn't work: a
    /// `PromptChooseCards` with a `count` of every root card parks a choice
    /// of everything, and trash prevention is offered only for a single
    /// selected card.
    AttackedServerRoot,
    /// Every card the acting card's controller has set aside (CR 4.8) —
    /// Gachapon's "Shuffle 3 of the remaining cards into your stack, then
    /// remove the rest from the game". Only meaningful for `Effect::
    /// RemoveFromGame`. Composition didn't work, for `AttackedServerRoot`'s
    /// reason: a `PromptChooseCards` with a `count` of every set-aside card
    /// parks a choice of everything, and no `Amount` counts the zone.
    SetAside,
    /// The install with this handle, whoever's it is — what
    /// `Effect::ForEach` writes over `InstallId::PLACEHOLDER` with, for
    /// each card it names (Game Over's "for each card that would be
    /// trashed this way"). Authored as the placeholder; nothing happens if
    /// the install has left the table by the time it resolves. Composition
    /// didn't work: `RunnerRig` names a card and takes its first copy, and
    /// `ThisCard` is the acting card, whose side is who carries out the
    /// trash.
    Install(crate::rules::InstallId),
}

/// Where `Effect::HostCardOnThisCard` takes the card from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostedCardOrigin {
    /// A random card from HQ — Detente's "host 1 random card from HQ".
    RandomFromHq,
    /// The top card of the Runner's stack — Bling's "host the top card of
    /// your stack faceup".
    TopOfStack,
    /// The card the Runner is accessing, from wherever it is — Cupellation's
    /// "Host the non-agenda card you are accessing faceup on this program.
    /// (If it was installed, it becomes uninstalled.)" and Heliamphora's
    /// "host it faceup on this program instead". Moving it ends the access
    /// (CR 7.1.7), which `run::host_currently_accessed_card` does as a
    /// trash does. `RulesError::NotInAccessPhase` with nothing accessed.
    AccessedCard,
}

/// What a card added to a score area "as an agenda" is (CR 10.1.3): it
/// loses every property it printed and has only these. A card file writes
/// the numbers the card prints — Myōshu's "worth 2 agenda points", Word on
/// the Street's "worth −1 agenda points with “You cannot forfeit this
/// agenda.”" — and the score area keeps them on the copy
/// (`ScoredAgenda::as_agenda`) for as long as it is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsAgenda {
    /// Signed: Word on the Street is worth −1 (CR 1.17.1 sums them).
    pub points: i32,
    /// "You cannot forfeit this agenda."
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cannot_forfeit: bool,
    /// "As an **assassination** agenda" (Jeitinho): the one subtype the
    /// converted card has, which `Amount::InScoreAreaWithSubtype` counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<crate::dsl::CardSubtype>,
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
    /// `LoseCredits` with a computed amount — Idiosyncresis's "the Runner
    /// loses 2[c] for each hosted advancement counter", authored as two of
    /// these over `Amount::HostedAdvancementTokens` the way Account Siphon
    /// doubles with two `GainCreditsAmount`s. Records what was actually
    /// lost like its fixed sibling.
    LoseCreditsAmount(Side, Amount),
    /// Renamed from `InflictDamage`. `usize` (not `u32`) matches
    /// `damage::apply_damage`'s existing signature exactly. No `Side`
    /// param: damage in this engine's model always targets the Runner,
    /// same as `apply_damage` itself.
    DealDamage(DamageType, usize),
    /// A piece of ice gets `delta` strength for a duration: which ice is
    /// `ice` (`StrengthOf`) — the encountered ice (Leech's "the ice you are
    /// encountering gets -1 strength for the remainder of this
    /// encounter"), every piece of ice wherever it is installed (ezaM's
    /// "each piece of ice gets +1 strength for the remainder of this run",
    /// a `Lingering::Strength` on `On::EachIce`), or the ice resolving
    /// (Brasília Government Grid's "the rezzed ice gets +3 strength for the
    /// remainder of that run", the rezzed ice being what its trigger acts
    /// on). One effect with a word rather than a second beside it: the
    /// sentences differ only in which ice and for how long, which is what
    /// a lingering effect already carries. The word was `each_ice: bool`
    /// until the third ice.
    ModifyStrength {
        delta: i32,
        #[serde(default, skip_serializing_if = "StrengthOf::is_encountered")]
        ice: StrengthOf,
        duration: EffectDuration,
    },
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
    /// `Side::Corp` would never be a legal target. An `Amount` since
    /// Vicsek's "give the Runner X tags. X is equal to the number of tags
    /// the Runner has", taken in place rather than as a `GiveTagsAmount`
    /// beside it, as `PlaceAdvancementCounters` was: a fixed number is
    /// `Fixed(n)`.
    GiveTags(Amount),
    /// Deliberately no `Side` param, same rationale as `GiveTags`. An
    /// `Amount` rather than a number since Bigger Picture's "remove any
    /// number of tags" became a number the Corp chooses
    /// (`Amount::ChosenNumber`) instead of every tag there is.
    RemoveTags(Amount),
    /// Deliberately no `Side` param — Bad Publicity exists solely on
    /// `CorpState` in this data model, same rationale as `GiveTags`. An
    /// `Amount` since Luana Campos's "take all hosted bad publicity" is
    /// the counters on the card, as `GiveTags` became one for Vicsek.
    GiveBadPublicity(Amount),
    /// Deliberately no `Side` param, same rationale as `GiveBadPublicity`.
    RemoveBadPublicity(u32),
    TrashCard(CardTarget),
    /// `TrashCard`'s removal from the game — Malandragem's "When it is
    /// empty, remove it from the game". Only `CardTarget::ThisCard` is
    /// printed; any other target is `RulesError::UnresolvedCardTarget`.
    /// The same move as `Cost::RemoveSelfFromGame`, made by a card's text
    /// rather than paid. `ThisCard` the Runner is accessing is taken from
    /// wherever it was accessed and its access ends (Nightmare Archive's
    /// "remove this asset from the game", `run::move_currently_accessed_card`).
    /// Composition didn't work: `TrashCard` sends a card to its owner's
    /// discard pile, where Scrounge finds it again.
    RemoveFromGame(CardTarget),
    /// Boosts a Runner rig card's own strength — unlike `ModifyStrength`,
    /// which always targets whatever ICE is currently being encountered,
    /// this always targets whichever rig card activated the ability (see
    /// `evaluate_effect`'s `acting_card` parameter). The boost is a
    /// `rules::lingering::LingeringEffect` on the card, which holds for as
    /// long as the state says its duration is still running.
    BoostStrength { amount: u32, duration: EffectDuration },
    /// Breaks pending subroutines on the ICE currently being encountered,
    /// gated on the acting rig card's `continuous::breaker_strength` meeting the
    /// ICE's strength (`RulesError::BreakerStrengthTooLow`
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
        /// The server whose breach is replaced; none for **the attacked
        /// server**, read as the replacement is made — Security Testing's
        /// "the first time each turn you make a successful run on the
        /// chosen server, instead of breaching it", where the chosen server
        /// is the state's and may be a remote. A run event that chooses
        /// its server writes it in (`pending_choice::
        /// substitute_chosen_server`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server: Option<ServerId>,
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
    /// When an inner `Effect` parks something spanning future
    /// `PlayerAction`s (a trace, a prevention window, `OfferPaidChoice`,
    /// `PresentChoice`, or `PromptChooseCards`/`PromptChooseServer`), the
    /// remaining effects are queued as a **continuation** on
    /// `GameState::deferred_triggers`, pinned to the acting card, and
    /// resolve once the decision does (`DeferredTrigger::continuation` —
    /// added for Key Performance Indicators' second pick). What the
    /// continuation gets back is the acting card, its install and the
    /// triggering event; the per-resolution scratch (`credits_lost`,
    /// `damage_discarded`, `selected_count`) does not survive the park.
    /// Before *Elevation* Stage 5 there was no continuation at all and the
    /// rest of the sequence was silently dropped, so cards chained
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
    /// Changes `side`'s click allotment for their *next* turn, not the
    /// current one — Aggressive Trendsetting's "you get +1 allotted
    /// [click] for your next turn", which resolves during the Runner's
    /// turn and so has no Corp click pool to add to (`turn::
    /// enter_start_of_turn` *assigns* the allotment, overwriting anything
    /// `GainClicks` had put there), and Caveat Emptor's "the Runner gets
    /// −1 allotted [click] for their next turn" (or +1). A lingering effect
    /// on the player until that turn begins (`Lingering::AllottedClicks`),
    /// taken as the allotment is assigned.
    ///
    /// Was `GainClicksNextTurn(Side, u32)`, banked on `CorpState::
    /// extra_clicks_next_turn` — a field no view carried, so every bot
    /// sample after Aggressive Trendsetting's ability planned the Corp's
    /// next turn a click short — and a no-op for the Runner, which had no
    /// field. Signed, because Caveat Emptor takes a click away.
    AllottedClicksNextTurn(Side, i32),
    /// Initiates a run on `server`, exactly like `PlayerAction::InitiateRun`
    /// (same `RunState` shape, `RulesError::RunAlreadyInProgress` guard) but
    /// without spending a click — the enclosing `PlayEvent`/`PlayOperation`
    /// already spent the one click this whole card costs. Lets a single
    /// card's `OnPlay` effect list read as "make a run on X, then [modify
    /// that run's access]" in one resolution (e.g. The Maker's Eye, Account
    /// Siphon) by simply listing `InitiateRun` before the access-modifying
    /// effect(s) that follow it.
    InitiateRun(ServerId),
    /// Prevents some of whatever is parked in `GameState::
    /// pending_prevention` — "Prevent 1 tag", "Prevent up to 3 meat
    /// damage", "Prevent a player from trashing 1 installed program". What
    /// is prevented is the payload, a `Preventable`, so a new kind of
    /// prevention is a word there and an arm in `rules::prevention`, never
    /// an effect of its own: `PreventDamage` and `PreventTrash` were two,
    /// and tags would have been the third. An ability whose effect holds
    /// one is an **interrupt** (`Effect::prevents`): the only thing either
    /// player may activate while a `WindowCheckpoint::Prevention` window is
    /// open, and refused everywhere else, because this arm errors
    /// (`RulesError::NothingToPrevent`) unless something it matches is
    /// parked — which is also what keeps it out of `legal_actions`.
    Prevent(Preventable),
    /// Increases a parked draw by `by` — Daily Business Show's "increase
    /// the number of cards you will draw by 1" — and, when `then` is given,
    /// resolves it as the cards are drawn: "When you draw those cards, add
    /// 1 of them to the bottom of R&D". `then` reads the number drawn as
    /// `Amount::ChosenNumber` (written in here, as `ChooseNumber`'s `then`
    /// has its number), so `CardFilter::TopOf(ChosenNumber)` over HQ is
    /// "those cards": drawn cards are the end of the hand. Waits in
    /// `PendingPrevention::waiting` and resolves after `prevention::happen`
    /// draws. Refused unless a draw is parked. Composition didn't work:
    /// `Prevent` only lowers what is parked, and nothing else runs after a
    /// parked thing has happened.
    IncreaseAboutToResolve {
        by: Amount,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        then: Option<Box<Effect>>,
    },
    /// Saturating-adds `amount` generic counters (see `dsl::card::
    /// CounterKind`) to whichever card activated this effect — always
    /// `acting_card`, the same target `BoostStrength` uses, since a
    /// counter-placing effect is always a card's own ability/trigger
    /// putting counters on itself. `RulesError::UnresolvedCardTarget` if
    /// `acting_card` is `None`, `RulesError::CardNotActive` if it names a
    /// card that's neither a rezzed Corp install nor a Runner rig card.
    AddCounters(u32),
    /// Saturating-removes `amount` generic counters from `acting_card`. Same
    /// target/error rules as `AddCounters`. An `Amount` since Business As
    /// Usual's "Remove **all** virus counters from 1 installed card"
    /// (`HostedCounters`, as the `then` of a selection of the card), which
    /// would otherwise have been a `RemoveAllCounters` beside it — the shape
    /// `PlaceAdvancementCounters` took.
    RemoveCounters(Amount),
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
    OfferPaidChoice {
        side: Side,
        cost: Cost,
        if_paid: Box<Effect>,
        if_declined: Box<Effect>,
        /// The printed clause this offer implements, quoted from the
        /// card's text — "you may pay 2[credit] and trash 2 cards from HQ.
        /// If you do, end the run." — so a client can put the card's own
        /// words on Accept/Decline instead of a rendering of this DSL,
        /// and so `printed_clauses_are_quoted_from_the_card` can check
        /// that the words still appear on the card after an erratum or a
        /// catalog update. Never read by the engine.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        /// The payment is not an offer but an obligation the player meets
        /// if able — Tollbooth's "they must pay 3[credit], if able. If they
        /// do not, end the run." Nobody is asked: the cost is paid when it
        /// can be (by the same accept a choice would take, so the payment's
        /// own questions still come), and `if_declined` resolves when it
        /// cannot. Composition didn't work: an `EffectIf` on the credit pool
        /// is an affordability sum the Payment Rule forbids, and a parked
        /// choice whose decline was refused would ask a question with one
        /// answer.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        if_able: bool,
    },
    /// Presents `chooser` with a choice of which one `Effect` among
    /// `options` resolves — e.g. Wildcat Strike ("resolve 1 of the
    /// following of the Corp's choice"), NBN: Reality Plus ("gain 2
    /// credits or draw 2 cards"). Parks a `state::PendingDecision::
    /// ChooseEffect` and returns immediately, resolved via `PlayerAction::
    /// ResolvePendingChoice`.
    PresentChoice {
        chooser: Side,
        options: Vec<Effect>,
        /// One printed clause per option, quoted from the card's text, for
        /// a client to label the choice with the card's own words; an empty
        /// string marks the option that is the card's "may" declined
        /// (always an empty `Sequence`). See `OfferPaidChoice::text`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        texts: Vec<String>,
    },
    /// Offers `side` a choice of up to `max` (at least `min`) cards from
    /// `source` matching `filter`, optionally moving the chosen cards to
    /// `destination` (shuffling it afterward if `shuffle_after`), then
    /// evaluating `then` (if present) with the *first* selected card as
    /// `acting_card` context — e.g. Sprint's "shuffle 2 cards from HQ into
    /// R&D", Mutual Favor's "search your stack for 1 icebreaker", Above the
    /// Law's "you may trash 1 installed resource", Send a Message's "rez 1
    /// installed ICE, ignoring costs" (`destination: None`, `then: Some(
    /// RezInstalled { .. })` — the placeholder install inside
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
        /// "That many": `min` and `max` both, read when the prompt parks —
        /// Simulation Reset's "Shuffle **that many** cards from Archives
        /// into R&D", `CardsSelected` inside the `then` of the selection
        /// that trashed them. Nothing is asked when it comes to 0, and
        /// `validate` wants `min` and `max` written 0 beside it.
        /// Composition didn't work: `min` and `max` are printed numbers,
        /// and the only other way to say "the number just chosen" was one
        /// `EffectIf` branch per count.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<Amount>,
        /// "Up to that many": `max`, read when the prompt parks, with
        /// `min` 0 — The Back's "For each hosted power counter, choose up to
        /// 2 cards", `Increased { HostedCounters, HostedCounters }`. Nothing
        /// is asked when it comes to 0, and `validate` wants `min` and `max`
        /// written 0 beside it, as beside `count`. Composition didn't work:
        /// `count` is exactly that many, and the prompt does nothing when
        /// fewer cards qualify.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        up_to: Option<Amount>,
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
        /// for `RezInstalled`. `RunSucceeded` fires before
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
        /// Offer only the servers at least one piece of ice protects,
        /// rezzed or not — Kompromat's "Run a server protected by ice". A
        /// field for the reason `exclude_servers_run_this_turn` is one:
        /// what narrows a server offer is the offer's, and a requirement
        /// could only withhold the whole card. Applied at the park with
        /// it, so a table with no ice anywhere withholds the event.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        only_protected_by_ice: bool,
        /// Offer only servers of this kind that exist — Aircheck's "you may
        /// run a remote server". A field for the reason the two above are:
        /// what narrows a server offer is the offer's. A remote with
        /// nothing in it is not a server to run, so the fresh one
        /// `legal_actions` offers "any server" is not among them, and with
        /// no remote on the table the effect fails rather than park.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        only_in: Option<crate::dsl::ServerKind>,
        /// Offer only the server the last run was on — Always Have a Backup
        /// Plan's "you may run the attacked server again", asked as that
        /// run ends (`GameState::last_completed_run`). A field for the
        /// reason the three above are: what narrows a server offer is the
        /// offer's, and no card file can name a server a run chose.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        last_run_server: bool,
        /// The run is made "ignoring any additional costs to run" (Always
        /// Have a Backup Plan): no run cost is paid as the server is
        /// announced (`run::start_run_ignoring_costs`), and none narrows
        /// the offer.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        ignore_additional_costs: bool,
    },
    /// Rezzes an already-installed Corp card, paying the same way
    /// `PlayerAction::RezIce` does but without its "ice only while it is
    /// being approached" restriction — Send a Message's "rez 1 installed
    /// piece of ice, ignoring all costs" (`pay_cost: false`), Mycoweb's
    /// "you may rez 1 installed piece of ice, paying 2[c] less"
    /// (`pay_cost: true, discount: 2`), and both branches of the forfeit
    /// choice `CardDefinition::rez_forfeit_discount` offers.
    ///
    /// It calls `engine::rez_install`, the routine `rez_ice` itself uses,
    /// rather than restating the state transition: this variant used to be
    /// `RezInstalledIgnoringCost` and hand-rolled the flip and the `OnRez`
    /// dispatch, under a comment saying the duplication was deliberate
    /// because only the paid path needed the payment and the priority
    /// window. Mycoweb prints a *discounted* rez, which needs the payment
    /// itself (a region's hosted rez credits among its sources), so the two
    /// paths had to converge; a second variant beside the first would have
    /// left three copies of one transition.
    ///
    /// Lenient about affordability: a `pay_cost` rez the Corp cannot afford
    /// resolves to nothing rather than erroring, the same "silently no-op"
    /// convention `PromptChooseCards` uses for an empty offer. That is what
    /// keeps declining the forfeit choice from failing (and so deadlocking)
    /// when the undiscounted cost is out of reach.
    ///
    /// An `InstallId` for the same reason as `SwapInstalledIce`: *Send a
    /// Message* rezzes the installed ice the Corp chose, which with two
    /// unrezzed copies of one card a `CardId` could not name. Authored as
    /// the placeholder `0` and substituted at `ConfirmCardSelection`.
    RezInstalled {
        install: InstallId,
        /// Whether the rez cost is paid at all. `false` is "ignoring all
        /// costs"; `true` pays the printed cost less `discount`.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        pay_cost: bool,
        /// Credits knocked off the rez cost when `pay_cost`. Never a
        /// refund — a discount larger than the cost makes the rez free.
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        discount: u32,
    },
    /// Removes *every* counter currently on `acting_card` and grants `side`
    /// that many credits — e.g. Pennyshaver's "place 1 credit on this
    /// hardware, then take all credits from it." A narrowly-scoped one-off
    /// rather than a general
    /// dynamic-amount system, which no card needs yet — the M4 plan
    /// section claimed hosted-credit-pool cards would need no new `Effect`
    /// variants at all, but "take a variable amount, not a fixed N" is a
    /// genuine gap the fixed-`u32` `RemoveCounters`/`GainCredits` pair
    /// can't express. Same `RulesError::UnresolvedCardTarget`/
    /// `CardNotEligibleForCounters` error conditions as `AddCounters`/
    /// `RemoveCounters` (delegates to the same `modify_counters` helper).
    TakeAllCountersAsCredits(Side),
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
    /// `TakeAllCountersAsCredits`'s flat 1-per-counter payout, kept
    /// deliberately separate from removing the counters (the caller pairs
    /// it with its own cost/cleanup, e.g. Fermenter's "[click], [trash]:
    /// gain 2 credits for each hosted virus counter" — the `TrashSelf`
    /// cost already disposes of the card and its counters together, so
    /// this effect only needs to read the count once). To be folded into a
    /// general `Amount` vocabulary alongside `TakeAllCountersAsCredits` in
    /// a later milestone.
    GainCreditsPerCounter { side: Side, credits_per_counter: u32 },
    /// Exchanges two Corp ICE's `server`/`slot` positions in place — e.g.
    /// Tāo Salonga's "you may swap 2 installed pieces of ice." Both
    /// `CardId`s are authored as unused placeholders in JSON (the real
    /// targets aren't known until the Runner picks them) and substituted
    /// at `PendingDecision::ChooseCards` resolution time, the same
    /// "acting-context substitution" convention `Effect::
    /// RezInstalled` already established for a single target —
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
    PromptInstallCorpCard {
        origin_zone: CardZoneRef,
        /// Waive the install cost and the per-protecting-ice tax — Key
        /// Performance Indicators' "install 1 piece of ice from HQ,
        /// ignoring all costs", Off the Books' "install that card,
        /// ignoring all costs". Every destination is then offered, since
        /// affordability no longer prunes any.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        ignore_costs: bool,
        /// Credits knocked off the install cost — Mercia B4LL4RD's "install
        /// 1 piece of ice from HQ, paying 1[c] less". Only the
        /// per-protecting-ice tax is ever paid here, so a discount larger
        /// than it makes the install free; never a refund. Meaningless
        /// with `ignore_costs`.
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        discount: u32,
        /// Resolves once the card has landed, as the card that offered the
        /// prompt (`acting_install` is the parking install, never the
        /// installed card) with the chosen server substituted into it —
        /// Mercia B4LL4RD's "If you do, move this upgrade to the root of
        /// the server that piece of ice is protecting". The "if you do" is
        /// structural: a declined offer parks nothing and the rider never
        /// runs. A `Sequence` continuation cannot express this — it resumes
        /// as the *selected* card and knows nothing of the server chosen.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        then: Option<Box<Effect>>,
        /// Offer remote servers only — Peer Review's "install 1 card from
        /// HQ **in the root of a remote server**". Agendas and assets are
        /// remote-only anyway; this is what keeps an *upgrade* out of a
        /// central's root, which the printed text forbids and
        /// `engine::corp_install_destinations` otherwise allows.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        remote_only: bool,
        /// Offer central servers only — Secure and Protect's "install that
        /// ice **protecting a central server**". `remote_only`'s other half,
        /// a field for the same reason.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        central_only: bool,
        /// Offer every server but the one the acting install is in —
        /// Tributary's "install 1 piece of ice from HQ **protecting another
        /// server**", from the ice being encountered. A field for the reason
        /// `remote_only` is one: what narrows an install's destinations is
        /// the install's.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        another_server: bool,
        /// Offer only a new remote server — Mitosis's "Install up to 2 cards
        /// from HQ, **creating a new remote server each time**": the one
        /// remote the Corp would create (`legal_actions::fresh_remote_id`),
        /// and nothing at all when the Corp may create no more. A field for
        /// the reason `remote_only` is one; ice goes protecting the new
        /// server, an agenda, asset or upgrade into its root.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        new_remote: bool,
        /// "Install **and rez**" — Reanimation Protocol's "Install and rez 1
        /// piece of ice from Archives, paying a total of 10[credit] less".
        /// The card is rezzed as it lands, paying its rez cost (unless
        /// `ignore_costs`) less whatever of `discount` the install did not
        /// use: CR 1.16.2f lets the Corp divide a "total" modifier between
        /// the two costs, and the division taken is install first, which
        /// is never more credits than another would be. A rez the Corp
        /// cannot afford leaves the card installed and unrezzed (CR
        /// 1.16.4b). A field rather than a `then`: `then` resolves as the
        /// card that offered the prompt, and the rez is of the card that
        /// landed, which only the resolution knows.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        rez: bool,
        /// Resolves as the card just rezzed, and only if `rez` rezzed it —
        /// Reanimation Protocol's "If you rezzed a piece of non-liability
        /// ice this way, take 1 bad publicity" (the "non-liability" is an
        /// `EffectRequirement::ActingCardMatches` inside it).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        if_rezzed: Option<Box<Effect>>,
        /// Resolves as the card that landed, once it has — Warm Reception's
        /// "You cannot score that card this turn". `then` resolves as the
        /// card that offered the install and `if_rezzed` only when the card
        /// was rezzed too; neither is the card installed, unrezzed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        if_installed: Option<Box<Effect>>,
        /// "…ignoring **credit** costs" (CR 1.16.5b) — Ob Superheavy
        /// Logistics' "Install and rez the card you found, ignoring credit
        /// costs": every credit of the install and of the rez is removed,
        /// and every other cost stays — an additional cost to rez (CR
        /// 8.5.13c's own example is Ob finding Archer), a card's way to pay
        /// for its rez (`rez_alternatives`). `ignore_costs` is "ignoring
        /// **all** costs" (1.16.5c), which removes those too. Paid as a
        /// discount of everything, so the rez's alternatives are still met.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        ignore_credit_costs: bool,
        /// "You cannot install that card **in the root of this server**" —
        /// Vaporframe Fabricator's, heard as it is trashed. A card that is
        /// not ice is not offered that root while the server still exists;
        /// ice may still protect it, and once the server is gone a new
        /// remote is another server. `This` is written as the server
        /// (`Effect::with_this_server`) when a selection ahead of the
        /// install parks, while the resolution still knows where the card
        /// was: a continuation keeps no triggering event. A field for the
        /// reason `another_server` is one, which excludes ice too and reads
        /// the acting install, gone by the time a trash is heard.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        not_in_root_of: Option<ThisServer>,
        /// "Install 1 piece of ice … **in any position** protecting a
        /// server" — Timely Public Release (CR 6.2.2d: outward from every
        /// position, inward from every one, or between two). Once the server
        /// is chosen and has ice, the Corp is asked a number — how many of
        /// that server's pieces of ice are outward of the new one, 0 being
        /// the outermost position and the count the innermost — and the
        /// card lands there (`PendingDecision::ChooseNumber::install`). The
        /// position is part of the destination (CR 6.2.2: "created when the
        /// Corp declares an install destination", step 8.5.16b), so it is
        /// asked before the card lands, never as a move after it. A field
        /// for the reason `remote_only` is one: what narrows an install's
        /// destinations is the install's.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        any_position: bool,
    },
    /// Installs the resolving card — `acting_card`, a card sitting in the
    /// Runner's grip — into the rig, **paying** its install cost (with the
    /// usual discounts) and trashing programs to the memory limit, a second
    /// console and the unique rule as any install does (CR 3.9.3b, 3.8.5b): Pantograph's "you may install 1 card from your
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
    /// Keino-Chemutai's install from among the cards just discarded — paying
    /// the `Discount` less: Privileged Access's "install 1 resource from
    /// your heap, paying 2[credit] less", with `CardFilter::
    /// InstallableRunnerCardWithDiscount` over `OwnHeap` as the offer. A
    /// sibling rather than a zone parameter on the grip variant so every
    /// existing card JSON keeps its bare `"InstallRunnerCardFromGrip"`
    /// string; both share one pricing and eligibility path
    /// (`engine::can_install_runner_card_from_zone_with_discount`). Same
    /// Trojan exclusion and same silent no-op when the pick is no longer
    /// installable. The discount was taken into this variant rather than a
    /// fourth beside it (`Credits(0)` for the two cards that pay in full),
    /// as `PlayOperation { from }` took Plutus's zone, and so was the zone
    /// when The Wizard's Chest's "You may install 1 of those 2 cards,
    /// ignoring all costs" installed out of the set-aside zone: it was
    /// `InstallRunnerCardFromHeap(Discount)`. `from` is `OwnHeap`,
    /// `OwnSetAside`, `OwnStack` (World Tree's found card) or
    /// `HostedOnSource` — the cards hosted on the acting install, the
    /// parking card's own (Madani's "Install 1 hosted program", Pauleʼs
    /// Café's "Install 1 hosted card"), which was `InstallRunnerCardFromHost`
    /// until the Café's discount wanted the zone install's; anything else is
    /// `RulesError::UnresolvedCardTarget`.
    InstallRunnerCardFromZone { from: crate::dsl::CardZoneRef, discount: Discount },
    /// Sets cards aside faceup from the top of the Runner's stack, one at a
    /// time, until `count` of them match `filter` or the stack is empty —
    /// The Wizard's Chest's "Set aside cards from the top of your stack
    /// faceup until you set aside 2 cards of the chosen type"
    /// (`RunnerState::set_aside`, CR 4.8). What is set aside is public
    /// (`GameEvent::CardsSetAside`). Composition didn't work: no effect
    /// reads down a deck until a condition holds, and a card left in the
    /// stack cannot wait for the install choice that follows.
    ///
    /// `deck` is whose deck is read and whose set-aside zone the cards go
    /// to: the Runner's by default, the Corp's for Deep Dive's "The Corp
    /// must set aside the top 8 cards of R&D faceup" (`filter: Any`, `count:
    /// 8`, `CorpState::set_aside`) — the top N being "until N of any card".
    SetAsideFromTopUntil {
        filter: crate::dsl::CardFilter,
        count: u32,
        #[serde(default = "crate::dsl::effect::the_runner", skip_serializing_if = "crate::dsl::effect::is_the_runner")]
        deck: crate::rules::Side,
    },
    /// `InstallRunnerCardFromGrip` paying `u32` less — Illumination's
    /// "install up to 3 cards from your grip, paying 1[c] less for each".
    /// Paired with `CardFilter::InstallableRunnerCardWithDiscount` so the
    /// offer and the price agree. A discount parameter rather than a
    /// separate pricing effect: nothing composable subtracts from a cost.
    ///
    /// Or ignoring all costs — Beta Build's "Install it, ignoring all
    /// costs" (`Discount::AllCosts`). The memory limit still applies: it
    /// is not a cost (CR 1.16).
    InstallRunnerCardFromGripWithDiscount(Discount),
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
    /// run ends, however it ends (`RunState::on_end`, evaluated by
    /// the `OnRunEnded` dispatch) — Charm Offensive's "When that run ends,
    /// you may trash 1 rezzed copy of a card you accessed". The run-end
    /// twin of `PromptChooseServer::on_success`, for an Event that is in
    /// the heap by then and cannot carry a `Trigger::OnRunEnded` of its
    /// own. `RulesError::NoActiveRun` if no run is active.
    SetRunEndedEffect(Box<Effect>),
    /// Arms a prevention that stands for the rest of the active run — Shred.
    /// An entry on `GameState::lingering` (`Lingering::PreventRunEnding`),
    /// as `Prohibit` makes one for a "cannot"; `rules::prevention::
    /// run_ending` is what `Effect::EndTheRun` asks. See `EndRunPrevention`.
    ArmRunEndPrevention(EndRunPrevention),
    /// Sabotage `u32`: the Corp trashes that many cards of their choice
    /// from HQ and/or the top of R&D (Cacophony). Parks a Corp
    /// `PendingDecision::ChooseCards` over HQ whose bounds are computed
    /// from the two zones' sizes — at least what R&D cannot cover, at most
    /// what HQ holds — with a `Mill` of `RemainingAfterSelection` from R&D as its
    /// `then` for the rest; with nothing in HQ to choose from it mills R&D
    /// directly. An engine variant because the bounds and the "rest" are
    /// decided by state a card author cannot see.
    Sabotage(u32),
    /// Trashes `amount` cards from the top of `deck`'s deck, one at a time
    /// — the R&D half of a sabotage (facedown), Nuvem SA's "trash the top
    /// card of R&D", The Price's "Trash the top 4 cards of your stack" —
    /// and then resolves `then` about **those cards**: every
    /// `CardFilter::TrashedThisWay` in it is written over with the cards
    /// trashed (`CardFilter::AmongCards`) before it resolves, or before it
    /// waits behind whatever a trashed card's own ability parked (Strike
    /// Fund's, between the instructions, CR 9.6.5a). A value the effect
    /// found, written into the effect that waits, as a chosen number is
    /// (the State Hygiene Rule): the heap has no order to read "the top 4"
    /// back from (CR 4.4.2), and the event being played lands on it in the
    /// middle of the resolution. It was `MillRnDAmount`, R&D's alone, with
    /// no `then`; The Price generalised it rather than add a variant.
    Mill {
        deck: Side,
        amount: Amount,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        then: Option<Box<Effect>>,
    },
    /// Moves one card from `HostedCardOrigin` onto the acting rig card's
    /// `hosted_cards`, faceup — Detente's "host 1 card from HQ at random
    /// faceup on this hardware", Bling's "host the top card of your stack
    /// faceup on this hardware". A no-op when the origin is empty. One
    /// variant with an origin rather than one per card: the hosting half
    /// is identical, only the pick differs.
    HostCardOnThisCard(HostedCardOrigin),
    /// Bypasses the ice being encountered — Fransofia Ward's "(Pass that
    /// ice. No subroutines or further "when encountered" abilities
    /// resolve.)". Marks every pending subroutine as handled and sets
    /// `run::RunState::ice_bypassed`, so the encounter's paid window closes
    /// with nothing to fire and the next `Continue` passes the ice; the
    /// dispatcher skips the ice's own `OnEncounter` reactions, and a
    /// deferred one stands down. Not composable: nothing else can mark
    /// subroutines handled without firing or breaking them. Fails outside
    /// an encounter (`RulesError::NotInEncounter`).
    BypassEncounteredIce,
    /// Removes every virus counter in play, exactly as the Corp's
    /// `PlayerAction::PurgeVirusCounters` does but as a card effect and
    /// without the three-click cost — Flyswatter's "when you rez this ice
    /// during a run against this server, purge virus counters". Not
    /// composable: no primitive sweeps counters off every card at once.
    PurgeVirusCounters,
    /// The acting card, facedown in Archives, is turned faceup — Cohort
    /// Guidance Program's "Turn 1 facedown card in Archives faceup", as the
    /// `then` of a selection of `Facedown` cards there. Heard as a breach's
    /// turning over is (`GameEvent::ArchivesTurnedFaceup`, one card). The
    /// copy turned is the first facedown one of that card: facedown copies
    /// of one card are the same card to both players. A no-op when none is
    /// facedown any more. Composition didn't work: nothing turned a card in
    /// Archives over but the breach (CR 7.3.2), which turns them all.
    TurnFaceupInArchives,
    /// Every card in Archives is turned facedown — Kakurenbo's "Turn all
    /// cards in Archives facedown". Composition didn't work: nothing turned
    /// a card in Archives facedown at all — a card goes there faceup or
    /// facedown as it is trashed (CR 4.4.6b), and `TurnFaceupInArchives` and
    /// the breach only turn them over the other way. Announced by nothing:
    /// no card hears a card turned facedown, and the view is the record.
    TurnArchivesFacedown,
    /// "Resolve `count` of the following in any order" — Key Performance
    /// Indicators. `chooser` picks one of `options`; it resolves, then the
    /// remaining options are offered again with `count - 1`, until the
    /// count is spent or the options run out. A composition primitive
    /// rather than nested `PresentChoice`s: written out, four options
    /// choose two is twelve leaves of duplicated text, and every card
    /// with this wording would repeat the expansion. Resolves by
    /// rewriting itself into a `PresentChoice`, so it parks and resumes
    /// exactly as one does.
    ResolveSomeOf {
        chooser: Side,
        count: u32,
        options: Vec<Effect>,
        /// See `PresentChoice::texts`; carried through the rewrite.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        texts: Vec<String>,
    },
    /// `effect`, `times` times over, each resolved in full before the next
    /// begins — Fully Operational's "Gain 2[credit] or draw 2 cards. Repeat
    /// this process for each remote server that has a card in its root and
    /// is protected by ice". `times` is read once, as it resolves, and the
    /// effect is rewritten into a `Sequence` of that many copies, so a
    /// choice inside one parks and the rest wait behind it
    /// (`evaluate_sequence`). Composition didn't work: a `Sequence` is as
    /// long as the card file writes it, and a number chosen up front
    /// (`ChooseNumber`) would decide every repetition before the first
    /// draw could inform the next.
    Repeat { times: Amount, effect: Box<Effect> },
    /// `effect` once for each card in `source` that `filter` admits as it
    /// resolves, the card named in it as `CardTarget::Install` of its
    /// handle — Game Over's "Trash all installed non-icebreaker cards of
    /// the chosen type. For each card that would be trashed this way, the
    /// Runner may pay 3[credit] to prevent that card from being trashed",
    /// an `OfferPaidChoice` whose decline trashes `Install(PLACEHOLDER)`.
    /// Rewritten into a `Sequence`, as `Repeat` is, so each choice parks
    /// and the rest wait behind it, resolving as the card whose text it is.
    /// `source` is an installed zone, from the controller's side. Composition
    /// didn't work: `Repeat` counts and names nothing, and a selection's
    /// `then` acts as the card chosen, whose side would be who trashed it.
    ForEach { source: crate::dsl::CardZoneRef, filter: crate::dsl::CardFilter, effect: Box<Effect> },
    /// `chooser` names a number from `min` to `max` and `then` resolves
    /// with it as `Amount::ChosenNumber` — "remove **any number of** tags"
    /// (Bigger Picture), "lose **up to 5** credits" (Account Siphon),
    /// "remove **up to 2** tags" (Lie Low). Composition didn't work because
    /// every existing decision picks from a list written in the card file
    /// (`PresentChoice`, capped at `MAX_PENDING_CHOICE_OPTIONS`) or from
    /// cards in a zone, and a number is neither: the three cards above each
    /// took the most their text allows, which for Bigger Picture gave away
    /// the one decision the card is — how tagged to leave the Runner.
    ///
    /// `max` is resolved when the decision is parked and capped at
    /// `action_mask::MAX_CHOSEN_NUMBER`; `of`, when present, caps it again
    /// — "up to 2" *of* the tags there are (`max: Fixed(2), of:
    /// RunnerTags`), so a number that could do nothing is never offered.
    /// **Nothing is asked when there is nothing to choose:** a range of one
    /// number resolves `then` with it at once.
    ///
    /// Not how an X *cost* is said — no card in the pool prints one, and a
    /// cost is chosen before it is paid, which is a question for the
    /// action that pays it, not for an effect.
    ChooseNumber {
        chooser: Side,
        #[serde(default)]
        min: u32,
        max: Amount,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        of: Option<Amount>,
        then: Box<Effect>,
        /// The printed clause the number is about (the Linked Clause
        /// Rule), shown as the prompt.
        text: String,
        /// "**Secretly** set your identity to any copy" (Méliès U, CR
        /// 1.5.2b): the other player is told a number was chosen and never
        /// which — the answer is concealed in the log
        /// (`masking::ConcealedAction::ChoosingSecretly`) and its
        /// `GameEvent::NumberChosen` is dropped for them. A flag on the
        /// decision rather than an effect of its own, because what is
        /// chosen is still a number in a range and `then` still reads it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        secret: bool,
    },
    /// Sets the controller's identity to copy `n` of itself (Méliès U's
    /// "secretly set your identity to any copy of Méliès U: Only the
    /// Brightest", CR 1.5.2b) — `CorpState::identity_copy`, read by
    /// `EffectRequirement::IdentityCopy` on the reverse side each copy
    /// prints. The copy entering play always does so front side up, so
    /// this never flips anything. Only ever the `then` of a secret
    /// `ChooseNumber` (`CardDefinition::validate`). Composition didn't
    /// work: which copy is in play is state no existing effect writes, and
    /// the one-bit flip (`FlipIdentity`) cannot tell three reverse sides
    /// apart.
    SetIdentityCopy(Amount),
    /// "Identify your mark" (CR 10.11.2): if no server is the Runner's mark,
    /// a random central server becomes it for the remainder of the turn —
    /// `lingering::Lingering::Mark` until the end of the turn, drawn from
    /// the state's own RNG with equal odds for HQ, R&D and Archives
    /// (10.11.2a); if one already is, nothing (10.11.3). Announced as
    /// `GameEvent::MarkIdentified`. Tunnel Vision and Info Bounty, and
    /// Midnight Sun's mark cards after them. Composition didn't work: no
    /// effect chooses a server at random, and the mark is a designation no
    /// effect wrote.
    IdentifyMark,
    /// Flips the Runner's identity to its other side
    /// (`RunnerState::identity_flipped`) — Dewi Subrotoputri. A flag rather
    /// than swapping the identity card: one card, two sides, and every
    /// side's text gated by `EffectRequirement::IdentityFlipped`.
    FlipIdentity,
    /// Moves `acting_card` to the top or the **bottom** of its owner's deck
    /// — from the Runner's grip or heap to the stack (Scrounge's "You may
    /// add 1 program from your heap to the bottom of your stack"), from HQ,
    /// Archives or R&D to R&D (Let Them Dream's "add that agenda to HQ or
    /// the bottom of R&D"), or, when the resolution names the card's
    /// install (`ResolutionContext::acting_install`), out of the rig (Beta
    /// Build's "when that run ends, if that program has not been
    /// uninstalled, add it to the top of your stack"). The install is the
    /// "if": a program uninstalled since has no handle, and a copy
    /// reinstalled has another, so either way nothing moves.
    /// `PromptChooseCards::destination` could not say the bottom, and it
    /// cannot say "this install" at all. It was `AddToBottomOfDeck`, and
    /// before that the Runner's alone as `AddToBottomOfStack`. A no-op when
    /// the card is in none of its owner's zones, per the `TrashCard`
    /// "already gone" precedent.
    AddToDeck(DeckEnd),
    /// Adds the acting install to its owner's grip — Pichação's "add this
    /// program to your grip". The install is the "if", as it is for
    /// `AddToDeck`: gone, or reinstalled under another handle, nothing
    /// moves. A Corp install goes to HQ — Wall to Wall's "Add this asset to
    /// HQ", one option of several, where Descent and Janaína add themselves
    /// as a cost (`Cost::AddSelfToHq`), which moves the card the same way.
    /// Composition didn't work:
    /// `PromptChooseCards` cannot say "this install", and `AddToDeck` moves
    /// only into a deck.
    AddToHand,
    /// Shuffles every card in each of these zones into the acting card's
    /// owner's deck — Read-Write Share's "[trash]: Shuffle all hosted cards
    /// into your stack" (`HostedOnSource`) and Ashen Epilogue's "Shuffle
    /// your grip and heap into your stack" (`OwnGrip`, `OwnHeap`). The
    /// Runner's zones only: no Corp card in the pool prints it, and
    /// Archives would need its faceup cards turned down; any other zone is
    /// `RulesError::UnresolvedCardTarget`. A hosting cost has uninstalled
    /// the host by the time this resolves, so the hosted cards are the ones
    /// set aside as it was paid (CR 9.5.5, `ResolutionContext::set_aside`),
    /// or, with no such cost, the ones still hosted. Composition didn't
    /// work: `PromptChooseCards` over a zone asks a question "all" does
    /// not, finds no host once the cost has trashed it, and `AddToDeck`
    /// moves the acting card. It was `ShuffleHostedIntoDeck`, Read-Write
    /// Share's alone, and took the zones when Ashen Epilogue needed two
    /// more. With no zones it shuffles the stack alone — Bring Them Home's
    /// "the Runner shuffles it into the stack", after an `AddToDeck`.
    ShuffleIntoDeck(Vec<crate::dsl::CardZoneRef>),
    /// Places credits on the run's event, spendable during the run as its
    /// others are (`RunState::bonus_run_credits`, Overclock's pool) —
    /// Trick Shot's "If successful, place 2[credit] on this event", and the
    /// credits it still hosts carried into the run its end starts
    /// (`Amount::RunCreditsLeftLastRun`). `RulesError::NoActiveRun` with no
    /// run. Composition didn't work: the pool is set only as a run begins
    /// (`PromptChooseServer::bonus_run_credits`), from a number the card
    /// file writes.
    ///
    /// `pays_for` is what the credits may be spent on, when the card says
    /// — Bahia Bands' "Place 4[credit] on this event. You can spend hosted
    /// credits to pay trash costs for the remainder of this run"
    /// (`RunState::run_credits_pay_for`); `None` is anything.
    PlaceRunCredits {
        amount: Amount,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pays_for: Option<crate::dsl::PaysFor>,
    },
    /// Installs the program this resolves as out of `from`, paying its cost,
    /// onto a host — Muse's "If that program is a trojan, install it on a
    /// piece of ice. Otherwise, install it on this program.", as the `then`
    /// of the search that found it: the program is the acting card and the
    /// install searching (Muse) the acting install. A Trojan's ice is the
    /// Runner's to choose, so it parks a selection of ice whose `then` is
    /// this effect again with the program written in (`card`: a value a
    /// player chose, written into the effect that waits — the State
    /// Hygiene Rule), resolving as the ice it installs on. A card file
    /// leaves `card` out. Nothing happens when the program cannot be
    /// installed that way (unaffordable, too big, no ice), as for every
    /// text install. `from` is the Runner's grip, heap or stack. Composition
    /// didn't work: every text install puts the card in the rig
    /// (`InstallRunnerCardFromGrip` and its siblings), and the trojan's
    /// host is a choice they cannot make.
    InstallProgramOnHost {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        card: Option<CardId>,
        from: crate::dsl::CardZoneRef,
    },
    /// Adds the acting card to its controller's score area "as an agenda"
    /// (CR 10.1.3) — Myōshu's "Add this operation to your score area as an
    /// agenda worth 2 agenda points", Jeitinho's "you may add this hardware
    /// to your score area as an assassination agenda worth 0 agenda
    /// points". It is not scored or stolen (CR 1.17.3f), so nothing that
    /// hears a score hears it, and it keeps nothing it printed
    /// (`ScoredAgenda::as_agenda`). An operation is filed in Archives before
    /// its text resolves (`engine::play_operation_card`), so that is where
    /// it is taken from; a Runner card is taken out of the rig, what it
    /// hosts trashed with it as `Cost::AddToScoreAreaAsAgenda` does. A Corp
    /// card the Runner is accessing goes to the *Runner's* score area and
    /// its access ends (CR 7.1.7) — Nightmare Archive's "they may add it to
    /// their score area as an agenda worth -1 agenda point", the Runner's
    /// choice out of the Corp's ability (`run::move_currently_accessed_card`).
    /// A no-op when it is not there. Composition didn't work: no effect
    /// moves a card into a score area without scoring or stealing it.
    AddToScoreAreaAsAgenda(AsAgenda),
    /// The acting card's controller wins the game — Jeitinho's "Then, if
    /// you have 3 assassination agendas in your score area, you win the
    /// game", under an `EffectIf`. A third way to win beside the two the
    /// rules give (CR 1.7.2: agenda points, and flatline or an empty R&D),
    /// through the one door into `GameOver` (`win::end_game`), announced
    /// first as `GameEvent::WonByCardText` so a driver can say why.
    /// Immediate, not a standing condition: the card says it as part of an
    /// ability's resolution, not as a rule checked at every checkpoint.
    /// Composition didn't work: nothing ends the game but the standing
    /// checks and the failed draw.
    WinTheGame,
    /// Every card hosted facedown on the acting rig card is turned faceup —
    /// Matryoshka's "When your turn begins, turn each hosted card faceup",
    /// undoing what `Cost::TurnHostedFacedown` paid. A no-op on a card
    /// hosting none. Composition didn't work: nothing turned a hosted card
    /// over.
    TurnHostedFaceup,
    /// The acting install — a piece of ice, reached through
    /// `TriggeredEffect::acts_on_subject` — gains `subroutine`, before or
    /// after its other subroutines, for `duration`:
    /// - Stick and Poke's "it gains “[subroutine] Do 1 net damage. The
    ///   Runner draws 1 card.”, before its other subroutines, for the
    ///   remainder of that encounter" — the ice being encountered, ordered
    ///   ahead of every subroutine it has, the newest such first (CR
    ///   9.8.3a). Written into the run's list for the encounter
    ///   (`EncounteredSubroutine::gained`), which is where a subroutine's
    ///   status already lives, and dropped at the one step every encounter
    ///   leaves by (`run::engine::enter_movement`); a no-op when that ice is
    ///   not being encountered.
    /// - Thunderbolt Armaments' "that ice gets +1 strength and gains
    ///   “[subroutine] End the run unless the Runner trashes 1 of their
    ///   installed cards.” after its other subroutines for the remainder of
    ///   that run" — the ice just rezzed, which need not be encountered
    ///   yet. Kept on the run (`RunState::gained_for_the_run`) and added to
    ///   the list at every encounter with that ice that run
    ///   (`run::engine::add_gained_for_the_run`), after the rest, oldest
    ///   first (9.8.3e); to the encounter in progress too, if it is that
    ///   ice's. A no-op outside a run.
    ///
    /// It is the ice's subroutine: it fires as the ice's and is broken
    /// like one, and ice already fully broken stays so (CR 6.5.7d).
    /// `Turn` is refused by `validate`: nothing is encountered outside a
    /// run. Composition didn't work: nothing adds to a subroutine list.
    GainSubroutine {
        subroutine: Box<crate::dsl::SubroutineDef>,
        /// "After its other subroutines" (9.8.3e) rather than before them
        /// (9.8.3a).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        after: bool,
        duration: EffectDuration,
        /// How many copies — Starlit Knight's "it gains X “[subroutine] End
        /// the run.” subroutines … X is equal to the number of tags the
        /// Runner has", read as it is gained. `None` is one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<Amount>,
    },
    /// The acting install — a piece of ice, as it is rezzed — gains
    /// `subtype` while it remains rezzed: Lycian Multi-Munition's "choose 1
    /// or more subtypes among barrier, code gate, and sentry. This ice
    /// gains the chosen subtypes while it remains rezzed", one of these per
    /// subtype under the `PresentChoice` that is the choice. Made as a
    /// `Lingering::GainSubtype` until `Until::WhileRezzed`, public like the
    /// choice, which the Corp makes with the ice faceup. A no-op on
    /// anything that is not a rezzed piece of ice. Composition didn't work:
    /// a gained subtype was only ever declared (`ContinuousKind::
    /// GainSubtype`), which holds while its source is active and has
    /// nowhere to keep which subtype was chosen.
    ///
    /// `ice` says which ice and so for how long: `This`, the default, is
    /// Lycian's own, while it remains rezzed; `Encountered` is Pelangi's
    /// "the ice you are encountering gains that subtype for the remainder
    /// of this encounter", said by a Runner program (`Until::Encounter` of
    /// the encountered ice). A word rather than a second variant, as
    /// `ModifyStrength`'s `ice` is: the sentences differ only in which ice
    /// and for how long. `EachIce` is refused by `validate`.
    ///
    /// `for_the_run` stretches the encountered ice's gain from the
    /// encounter to the run — Rielle "Kit" Peddler's "it gains **code
    /// gate** for the remainder of this run" (`Until::Run`). A word on the
    /// encountered ice's sentence, refused on the others by `validate`.
    GainIceSubtype {
        subtype: crate::dsl::IceType,
        #[serde(default = "StrengthOf::this", skip_serializing_if = "StrengthOf::is_this")]
        ice: StrengthOf,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        for_the_run: bool,
    },
    /// The controller looks at the top `count` cards of `deck`'s owner's
    /// deck and nobody else sees them (`GameEvent::CardsLookedAt`, masked
    /// for the other player) — Hiram "0mission" Svensson's "look at the
    /// top card of R&D". Nothing moves and nothing is chosen. Composition
    /// didn't work: `PromptChooseCards` over the deck shows its chooser the
    /// cards, but it is a choice — a prompt to answer and a card to move —
    /// and a look is neither.
    LookAtTopOfDeck { deck: Side, count: u32 },
    /// Hosts the rig card `card` on the rig card `host` —
    /// `state::InstalledRunnerCard::hosted_on_rig_card` — GAMEDRAGON™ Pro's
    /// "you may host this hardware on an installed non-AI icebreaker". A
    /// relation between two installs, which nothing composable can name:
    /// both are authored as `InstallId::PLACEHOLDER` and substituted when
    /// the parking `PromptChooseCards` resolves, the way `SwapInstalledIce`
    /// is — `card` becomes the parking card's own install, `host` the
    /// selected one. `RulesError::InstallNotFound` if either has left the
    /// rig, `RulesError::HostIsNotIce`-style rejection is not needed:
    /// eligibility is the prompt's `CardFilter::Icebreaker`. Re-hosting an
    /// already-hosted card simply moves it. The host may be a piece of ice
    /// (`hosted_on_ice`): Spree's "host 1 installed trojan program on a
    /// piece of ice protecting the attacked server", the trojan chosen
    /// first and the ice second, so the parking card is the trojan.
    HostRigCardOnInstall { card: crate::rules::InstallId, host: crate::rules::InstallId },
    /// "The Runner cannot steal or trash Corp cards for the remainder of
    /// this run" (Ansel 1.0), "You cannot score agendas for the remainder
    /// of the turn" (Luminal Transubstantiation): a `rules::lingering`
    /// entry about the player the prohibition names, holding for as long
    /// as the state says `until` is running, and asked through
    /// `continuous::cannot` by the guard and the action list alike.
    /// `RulesError::NoActiveRun` for a `Run` with no run to last, and
    /// `NotInEncounter` for an `Encounter` with none.
    ///
    /// Replaced `PreventStealAndTrashForRemainderOfRun` and
    /// `PreventScoringForRemainderOfTurn`, which each set a flag of their
    /// own with a reset of its own — and the score lock's was on a field
    /// no view carried, so no bot sample ever saw it. `validate` refuses
    /// `Encounter` but for Banner's, about the encountered ice: the guards
    /// every other prohibition answers are not asked during an encounter.
    Prohibit {
        what: Prohibition,
        until: EffectDuration,
        /// Only about copies of `acting_card` — Perfect Recall's "copies of
        /// that card", the card revealed out of HQ its `then` acts as. A
        /// flag rather than a card named in the file, which could not name
        /// the card revealed (`lingering::On::CopiesOf`).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        copies_of_it: bool,
        /// Only about the install resolving it — Warm Reception's "You
        /// cannot score that card this turn", said by the text install's
        /// `if_installed` rider as the card that landed
        /// (`lingering::On::Install`). The handle is public, as every
        /// install's is, so a facedown card is bound without being named.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        this_install: bool,
        /// Only about the ice being encountered — Banner's "Subroutines on
        /// the barrier you are encountering cannot end the run for the
        /// remainder of this encounter" (`lingering::On::Install` of that
        /// ice). The acting card is the breaker, so neither flag above
        /// could name it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        encountered_ice: bool,
    },
    /// Leaves `effect` waiting for a moment later this turn, then resolves
    /// it as the card that made it (a `lingering::DelayedAbility`, CR
    /// 9.6.13) — Lightning Laboratory's "When this turn ends, derez 2 pieces
    /// of ice protecting that server" (`when: OnDiscardPhaseEnd`), Climactic
    /// Showdown's "the first time this turn you breach either R&D or HQ,
    /// access 2 additional cards" (`OnBreach`, `filter: Server([RnD,
    /// Hq])`), and In the Groove's "For the remainder of this turn, whenever
    /// you install a card with a printed install cost of 1[credit] or
    /// greater" (`OnInstall`, a `Card` filter, `every_time`). `filter` is
    /// the trigger condition's `EventFilter`, judged as a card's `when` is
    /// (`listeners::when_admits`), and the moment is heard as the card's
    /// controller hears it. Once (CR 9.6.13c) unless `every_time`, which
    /// keeps it for the rest of the turn. "That server" is the attacked
    /// server, written over as the ability is made
    /// (`Effect::with_attacked_server`), since the run is over by the time
    /// Lightning Laboratory's resolves.
    ///
    /// Composition didn't work: nothing resolved later than the resolution
    /// that asked for it, but the run's own end (`SetRunEndedEffect`). It
    /// was `WhenThisTurnEnds`, the turn's end alone, until two cards waited
    /// for other moments.
    LaterThisTurn {
        when: crate::dsl::Trigger,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<crate::dsl::EventFilter>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        every_time: bool,
        effect: Box<Effect>,
        /// For the rest of the run only, not the turn — Whistleblower's
        /// "the next time **this run** you access an agenda with the chosen
        /// name" and Always Have a Backup Plan's "**during the second
        /// run**, whenever you encounter …": the ability is dropped as the
        /// run ends (`DelayedAbility::this_run`).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        this_run: bool,
    },
    /// "After you resolve this operation, your action phase ends" — a
    /// terminal card's line (Active Policing, Bring Them Home): the rest of
    /// the resolving card's controller's action phase is skipped and their
    /// discard phase begins, clicks unspent (CR 5.4.3,
    /// `turn::force_action_phase_end`). Last in the card's resolution,
    /// where "after" puts it, so a decision it parks earlier is answered
    /// first. Nothing outside that player's action phase (CR 5.4.4).
    /// Composition didn't work: nothing ended a phase but the basic pass
    /// with no clicks left, and losing the clicks is not the same — a
    /// Corp at 0 clicks still has the action phase's paid ability window,
    /// with its rezzes and scores, which 5.4.3a skips.
    EndActionPhase,
    /// The Corp scores the card this resolves as, if able — Big Deal's
    /// "You may score that card, if able", the card its selection chose
    /// (`engine::score_install`, which the basic action shares, so the costs,
    /// the dividends and the `AgendaScored` are the same). Not a card that
    /// cannot be scored now (`engine::scorable`): that resolves to nothing,
    /// "if able". Composition didn't work: scoring was only
    /// `PlayerAction::ScoreAgenda`, the Corp's with their own action phase's
    /// priority and no window open, and Big Deal scores in the middle of its
    /// resolution, just before it ends that phase.
    Score,
    /// `chooser` chooses a card name, among the playable cards `names`
    /// admits, and `then` resolves with the name written over
    /// `CardFilter::ChosenName` — Complete Image's "Choose a card name,
    /// then do 1 net damage" (the Runner's cards) and Whistleblower's "to
    /// choose a card name. The next time this run you access an agenda with
    /// the chosen name" (agendas). Parks `state::PendingDecision::
    /// ChooseCardName`, answered by `PlayerAction::ChooseCardName`. A name
    /// is a card's id: two cards never share a title, and a reprint is the
    /// same card.
    ///
    /// `again_if`: "If you trash a card with the chosen name this way,
    /// repeat this process" — after `then`, if the requirement holds (read
    /// with the name written in), the whole effect resolves again, a new
    /// name chosen. A field rather than a loop of its own because the
    /// process is this effect: the choice is the first step of every
    /// repetition.
    ///
    /// Composition didn't work: every decision chooses among options the
    /// card file writes out or among cards in a zone, and a name is
    /// neither — the card named need not be anywhere the chooser can see.
    ChooseCardName {
        chooser: Side,
        names: crate::dsl::CardFilter,
        then: Box<Effect>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        again_if: Option<crate::dsl::EffectRequirement>,
        text: String,
    },
    /// The Runner steals the agenda being accessed, ignoring all costs —
    /// Whistleblower's "steal it, ignoring all costs. (You are no longer
    /// accessing it.)", heard as the agenda is accessed. The steal is the
    /// access decision's own (`run::access::resolve_steal`), with the
    /// steal cost dropped; a "cannot steal" still forbids it. Composition
    /// didn't work: a steal was only ever `PlayerAction::StealAgenda`, the
    /// Runner's answer to the access, which pays what the agenda asks.
    StealAccessedCard,
    /// The Runner breaches this server, with no run — Cataloguer's
    /// "[click], hosted power counter: Breach R&D" (CR 7.3.1; `run::engine::
    /// start_breach`). Accessed as any breach is, and nothing a run owes
    /// follows: no successful run, no run ending, no new last run.
    /// Composition didn't work: every breach began at a run's success step
    /// (`CompleteRun`), which a card's text outside a run does not reach.
    ///
    /// **As a run's breach is replaced, it names the server breached
    /// instead** — Eru Ayase-Pessoa's "If successful, instead of breaching
    /// Archives, breach R&D", `SetAccessReplacement { server: Archives,
    /// effect: Breach(RnD) }` (`ResolutionContext::replacing_breach`,
    /// `RunState::breached`). The run goes on, on Archives, to a breach of
    /// R&D.
    Breach(crate::rules::ServerId),
    /// The Runner accesses `count` of the cards in `from` that match
    /// `filter`, one at a time, choosing each, and then `then` resolves —
    /// an access that is not a breach (CR 7.1.9, 7.1.10): Pinhole
    /// Threading's "instead of breaching the attacked server, access 1 card
    /// in the root of another server" (`from: OpponentInstalled`, a root
    /// card, `filter` saying which) and Deep Dive's "Access 1 of those
    /// cards" over what it set aside (`OpponentSetAside`). Each access
    /// follows a breach's steps (`run::access`, `OutsideBreach`), and "the
    /// procedure ends once the designated number of cards have been chosen
    /// for access" (7.1.10). Inside a run only as the run's breach is
    /// replaced, which the run then ends after; outside one it stands in a
    /// `RunState` flagged `breach_only`, as `Breach` does.
    ///
    /// Composition didn't work: every access began at a breach
    /// (`access::access_server`), whose candidates are a server's. **`then`
    /// is part of the effect** because an access in progress parks nothing
    /// a `Sequence` waits behind (`resolution_halted`), so "You may spend
    /// [click] to access another 1 of those cards. Then, the Corp shuffles
    /// the set-aside cards into R&D" would resolve under the first access.
    Access {
        from: crate::dsl::CardZoneRef,
        filter: crate::dsl::CardFilter,
        count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        then: Option<Box<Effect>>,
    },
    /// Reveals `count` cards at random from `side`'s hand — HQ or the grip
    /// — and, with `each`, resolves it as each card in turn: Bring Them
    /// Home's "Reveal and add 2 cards at random from the grip to the top of
    /// the stack" is `each: AddToDeck(Top)`, and its threat's "reveal 1
    /// card in the grip at random. The Runner shuffles it into the stack"
    /// adds a shuffle. The cards are drawn together, before any moves, with
    /// the state's own PRNG, so an `each` that moves one cannot be dealt it
    /// twice; fewer in the hand, fewer revealed. They stay revealed until
    /// they move or the ability has finished (`GameState::revealed`, CR
    /// 1.21.6), so a later step can choose among them: Burner's "reveal 3
    /// cards in HQ at random. Add 2 of the revealed cards to the top and/or
    /// bottom of R&D" reveals with no `each` and selects over
    /// `CardFilter::Revealed`. `each` must not park
    /// (`CardDefinition::validate`): the cards after it would be dropped.
    /// Composition didn't work: `TrashCard(RandomFromHand(Corp))` trashes what it
    /// draws and nothing else, and every other card an effect acts on is
    /// one a player chose or the acting card.
    RevealAtRandom {
        side: crate::rules::Side,
        count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        each: Option<Box<Effect>>,
    },
    /// "Play a Psi Game" (CR 10.14.6): each player secretly bids 0, 1 or
    /// 2[credit], no more than they could spend (10.14.3), then both bids
    /// are revealed and spent (10.14.4) and `on_match` or `on_differ`
    /// resolves (10.14.6d) — See How They Run's "If the bids differ, do 1
    /// core damage. If the bids match, do 1 net damage." Parks a
    /// `state::PendingDecision::PsiGame`, the Corp bidding first and the
    /// Runner told only that it has, each answered by
    /// `PlayerAction::ChooseNumber`. Composition didn't work: two secret
    /// `ChooseNumber`s cannot compare their numbers, since a nested choice
    /// keeps its own placeholder, and branching on the Corp's bid would put
    /// the bid in the continuation the Runner's decision carries, which a
    /// view passes through whole.
    PsiGame {
        on_match: Box<Effect>,
        on_differ: Box<Effect>,
    },
    /// Places `0` advancement counters on `acting_card` — e.g. Seamless
    /// Launch's "place 2 advancement counters on 1 installed card", Flood
    /// the Market's "1 advancement counter … for each remote server that
    /// has a card in its root and is protected by ice" (an `Amount` since
    /// Flood the Market, which would otherwise have been a
    /// `PlaceAdvancementCountersAmount` beside it, the shape
    /// `DealDamageAmount` took). Distinct
    /// from `AddCounters`, which targets the generic `counters` field;
    /// advancement tokens are their own thing (`InstalledCard::
    /// advancement_tokens`, what `ScoreAgenda` reads). Authored as the
    /// `then` of a `PromptChooseCards`, so `acting_card` is the card the
    /// Corp just selected. `RulesError::CardNotInstalled` if that card isn't
    /// a Corp install (only Corp cards can hold advancement counters).
    ///
    /// **Placing is not advancing** (CR 1.18.2), which is why this is named
    /// for what it does and emits `GameEvent::AdvancementCountersPlaced`
    /// rather than `CardAdvanced`: `Trigger::OnAdvance` must not fire, so
    /// Weyland Consortium: Built to Last is not paid when Key Performance
    /// Indicators merely places a counter. It was `AddAdvancementTokens`
    /// emitting `CardAdvanced` until September 2026, and the two rules had
    /// been one event since the effect was written.
    ///
    /// **Whether the target must be advanceable is the card's business, not
    /// this effect's.** Only 1.18.3's *advance* is restricted to agendas and
    /// cards that say "can be advanced", so a card printing that clause
    /// filters its own prompt with `CardFilter::Advanceable` and one that
    /// does not — Seamless Launch — legally targets ice. A blanket check
    /// here would break the latter.
    PlaceAdvancementCounters(Amount),
    /// Removes up to `Amount` advancement counters from the acting install
    /// — the first half of Hearts and Minds' "move 1 advancement counter
    /// from an installed card to an installed card you can advance", as the
    /// `then` of a selection of the card it leaves, whose placing half is a
    /// second selection. Moving is not advancing (CR 1.18.2). Composition
    /// didn't work: the only removal was a cost
    /// (`Cost::RemoveAdvancementCounters`), paid by the card that prints
    /// it, and this is some other card's counter. A no-op on a card that
    /// has left the table.
    RemoveAdvancementCounters(Amount),
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
    BoostStrengthAmount { amount: Amount, duration: EffectDuration },
    /// Moves the acting install — a root-slot Corp card — into the root of
    /// `ServerId`, Mercia B4LL4RD's "move this upgrade to the root of the
    /// server that piece of ice is protecting". The authored server is a
    /// placeholder: this only ever resolves as `PromptInstallCorpCard::
    /// then`, where `pending_choice::substitute_chosen_server` rewrites it
    /// to the server the Corp just installed into, the way a run offer's
    /// `AddAdditionalAccess` is rewritten. Not an install: no cost, no
    /// `CardInstalled`, the card keeps its rez state, counters and
    /// install id, and emits `GameEvent::CardMoved`. A no-op when the
    /// acting install is not a root-slot Corp card any more (it was
    /// trashed while the decision was parked), or is already there.
    MoveThisCardToRoot(ServerId),
    /// The Corp chooses another server, and the acting install — a root
    /// card — moves to its root: Lotus Haze's "move 1 rezzed upgrade to
    /// the root of another server", as the `then` of the choice of upgrade.
    /// Composition didn't work because every existing server choice either
    /// starts a run (`PromptChooseServer`) or installs a card
    /// (`PromptInstallCorpCard`); this one parks the same decision in a
    /// third mode (`PendingDecision::ChooseServer::move_to_root`) and
    /// resolves as `MoveThisCardToRoot`. The servers offered are the ones
    /// that exist, its own excepted: a remote is created by an install,
    /// not by a move.
    PromptMoveThisCardToAnotherRoot,
    /// Moves the acting install — a piece of ice — to the outermost
    /// position protecting the attacked server, from wherever it is:
    /// Tributary's "the first time each turn a run begins, you may move
    /// this ice to the outermost position protecting the attacked server.
    /// (The Runner will approach this ice.)" Not an install, as
    /// `MoveThisCardToRoot` is not: it keeps its rez state, counters and
    /// install id, and emits `GameEvent::CardMoved` when it changes server.
    /// The run follows through `run::reconcile_ice`, which at initiation
    /// re-anchors on the new outermost ice, so the Runner approaches it.
    /// A no-op with no run, or with the ice already outermost there.
    /// Composition didn't work: the only moves of ice are a swap of two
    /// (`SwapInstalledIce`), which cannot take a piece of ice to another
    /// server or ahead of a server with no ice.
    MoveThisIceToOutermost,
    /// The Runner encounters the ice this resolves as again, without
    /// moving (CR 6.5.9a, a forced encounter) — Sisyphus Protocol's "If you
    /// do, the Runner encounters that ice again", heard as the ice is
    /// passed (`acts_on_subject`). `run::force_encounter`: a new encounter
    /// of that ice, whose end goes back to the movement phase it was forced
    /// from without passing it again (`RunState::forced_encounter`). A
    /// no-op outside the movement phase or for ice no longer protecting the
    /// attacked server. Composition didn't work: nothing moved a run back
    /// to an encounter, and every encounter's end was a pass.
    ForceEncounter,
    /// Plays the acting card as an operation out of `from`, spending no
    /// click — Humanoid Resources' "you may play 1 operation from HQ"
    /// (`OwnHq`) and Plutus's "you may play 1 transaction operation from
    /// Archives" (`OwnArchives`). Shares `engine::play_operation_card`
    /// with the click action, so `OnPlay`, the Transaction reaction, the
    /// `play_requirement` and the filing all resolve identically.
    ///
    /// The zone is declared rather than detected: an operation played from
    /// Archives is removed from the game afterwards, and *which* cards may
    /// be played from there differs by who is asking — Petty Cash prints
    /// its own permission (`CardDefinition::playable_from_archives`, the
    /// click action's rule), where Plutus grants it to any transaction.
    PlayOperation {
        from: CardZoneRef,
    },
    /// Presents the acting card's own subroutines as a choice and resolves
    /// the one picked, as that card — Mycoweb's "resolve 1 subroutine on a
    /// rezzed sentry", where the enclosing `PromptChooseCards` has already
    /// made the chosen ice the acting card. Composition gets close but not
    /// there: `PresentChoice` needs its options written out in the JSON,
    /// and the options here are whatever the *other* card prints.
    ///
    /// Resolving as the chosen ice (not as Mycoweb) is what makes "trash 1
    /// installed program" trash through the sentry's own targeting; a
    /// subroutine whose effect is pinned to the ice *being encountered*
    /// (Ansel 1.0's install-inward) finds no host and no-ops, since the
    /// encountered ice is Mycoweb.
    ResolveSubroutineOfSelectedIce,
    /// The install the resolution acts as loses all its abilities for a
    /// duration (CR 9.1.9a) — Klevetnik's "choose 1 installed resource.
    /// That resource loses all abilities until your next turn ends", the
    /// chosen resource being what the selection's `then` acts as. A
    /// `rules::lingering` entry about that install (`Lingering::
    /// LosesAbilities`), read with Hush's standing loss by `rules::active::
    /// lost_abilities`. Composition didn't work: nothing took a card's
    /// abilities away; Hush's is a standing effect of the card hosted on
    /// the loser, and this one is made once and outlives its maker.
    LoseAbilities {
        until: EffectDuration,
        /// The cards that lose them are every card in the root of the
        /// attacked server, read whenever it is asked — Light the Fire!'s
        /// "During that run, cards in the root of the attacked server lose
        /// all abilities" (`lingering::On::RootOfAttackedServer`), so a card
        /// installed there mid-run loses them too and a redirect is
        /// followed. A field, for the reason `Prohibit`'s `this_install` and
        /// `encountered_ice` are: what the loss is about is the effect's.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        attacked_root: bool,
        /// The cards that lose them are both players' identities — Direct
        /// Access's "While you are resolving this event, each player's
        /// identity loses all abilities" (`lingering::On::Install` of the
        /// two identities' handles, which `rules::active` asks before
        /// counting an identity active).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        identities: bool,
    },
    /// During each encounter with the ice the resolution acts as, for the
    /// duration, the Runner cannot break more than `at_most` of its printed
    /// subroutines — Anvil's "the Runner cannot break this ice's printed
    /// subroutines for the remainder of this encounter" (0, `Encounter`),
    /// Unsmiling Tsarevna's "during each encounter with this ice for the
    /// remainder of that run, the Runner cannot break more than 1 of its
    /// printed subroutines" (1, `Run`). Hammer's `ContinuousKind::
    /// BreakLimit` with a duration and no exception: a `rules::lingering`
    /// entry about the ice (`Lingering::BreakLimit`), which `continuous::
    /// breaks_left` reads beside the table's. Composition didn't work:
    /// `BreakLimit` is declared and holds for as long as its ice is active,
    /// and these are made by an ability that resolved.
    LimitBreaks { at_most: u32, until: EffectDuration },
    /// The acting card's controller chooses a server, and the choice is
    /// remembered for the rest of the turn as that card's (`Lingering::
    /// ChosenServer`) — Tsakhia "Bankhar" Gantulga's "When your turn begins,
    /// you may choose a server" (the "may" is a `PresentChoice` around it).
    /// The card reads it back as "the chosen server" (`CardFilter::
    /// InChosenServer`). Parked as a `PendingDecision::ChooseServer` that
    /// remembers rather than runs (`remember`), over the servers that
    /// exist. Composition didn't work: every server choice started a run,
    /// installed or moved a card, and the mark is chosen at random.
    ///
    /// `only_protected_by_ice` narrows the offer to servers a piece of ice
    /// protects, rezzed or not — Climactic Showdown's "Choose a server
    /// protected by ice" — as `PromptChooseServer`'s field of that name
    /// narrows a run's.
    ChooseServer {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        only_protected_by_ice: bool,
    },
    /// For the rest of the encounter, whenever the Corp would resolve a
    /// subroutine on the encountered ice, it resolves instead the acting
    /// card's own printed subroutine — Tsakhia "Bankhar" Gantulga's
    /// "whenever the Corp would resolve a subroutine, instead they resolve
    /// "[subroutine] Do 1 net damage."", printed on the resource as a
    /// subroutine and read off it (`run::transition_subroutine`). Spends
    /// the card's chosen server (`Lingering::ChosenServer`), which is what
    /// makes it "the first encounter each turn with a piece of ice
    /// protecting the chosen server": the trigger that resolves this hears
    /// only an encounter with such ice, and a second one finds no choice.
    /// Composition didn't work: nothing replaced a subroutine's effect.
    ReplaceSubroutines,
    /// Spends the acting copy's chosen server (`Lingering::ChosenServer`)
    /// — what makes Security Testing's "the first time each turn you make
    /// a successful run on the chosen server" the first: the trigger hears
    /// only a run on the chosen server (`EventFilter::ChosenServer`), and a
    /// second one finds no choice. `ReplaceSubroutines` spends Tsakhia's
    /// the same way. Composition didn't work: `first_each_turn` counts the
    /// turn's moments by class, and the remotes are one class, so the
    /// first successful run on a chosen remote could not be told from the
    /// first on any remote.
    SpendChosenServer,
    /// Moves the run to the outermost position of `ServerId` — Proprionegation's
    /// "the Runner moves to the outermost position of Archives. (They
    /// approach any ice in that position.)". The run's ice list is rebuilt
    /// for the new server with **nothing passed**, which is what makes this
    /// different from `RedirectRunOnApproach`: that one arrives at the
    /// server having walked past everything, this one puts the Runner back
    /// at the front door. With no ice there the Runner is at the server
    /// approach instead, and `ServerApproached` is emitted and dispatched
    /// so that server's own root reactions still fire.
    ///
    /// Composition could not reach it: nothing else writes `RunState`'s
    /// server, ice list and position together, and every existing mover
    /// (`RedirectRunOnApproach`, `run::reconcile_ice`) keeps the Runner's
    /// progress rather than resetting it.
    ///
    /// `None` is the attacked server — Letheia Nisei's "the Runner moves to
    /// the outermost position of **this server**", heard as the Runner
    /// approaches it, so it is the server being run; written `null`, since
    /// the server it means is the run's and no card file can name it.
    MoveRunToOutermost(Option<ServerId>),
    /// Takes the acting card — an agenda the Runner has scored — out of
    /// the Runner's score area, removes tags equal to its printed points,
    /// and installs it unrezzed in a fresh remote. IP Enforcement's "as an
    /// additional cost to play this operation, remove X tags. Install 1
    /// agenda from the Runner's score area with a printed agenda point
    /// value equal to X."
    ///
    /// One effect rather than a cost plus an install because X *is* the
    /// chosen agenda's point value: the cost cannot be known until the
    /// card is picked, and `ability::pay_cost` never sees the selection.
    /// The destination is not offered — a stolen agenda coming back has
    /// only one sensible home, and installing it over an occupied remote
    /// would trash the occupant.
    InstallAgendaFromRunnerScoreArea,
    /// Swaps the ice the Runner is approaching with the acting card, which
    /// is a piece of ice in `origin` (HQ or Archives) — Mitra Aman's "you
    /// may swap the ice being approached with a piece of ice from Archives
    /// or HQ". The approached install keeps its position and handle and
    /// takes the new card, unrezzed as it arrives from a hidden zone; the
    /// ice that was there goes back to `origin`.
    ///
    /// `SwapInstalledIce` exchanges two cards already on the table and
    /// cannot reach a zone; `InstallFromZoneIgnoringCost` adds a card
    /// beside the approached ice rather than in its place, and has no way
    /// to send the displaced one back. A swap is one move, not two.
    ///
    /// `this_ice` swaps the install resolving it instead — Tatu-Bola's
    /// "When the Runner passes this ice, you may swap it with a piece of ice
    /// from HQ", said by a pass, when nothing is being approached. The
    /// acting install is the ice's own through the selection's `then`
    /// (`pending_choice::resolve_confirm_card_selection`), and it may be
    /// swapped at any step of the run. A flag rather than a second variant:
    /// what happens to the two cards is the same move.
    SwapApproachedIceWithCard {
        origin: CardZoneRef,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        this_ice: bool,
    },
    /// Reveals every card in `side`'s hand — Engram Flush's "[subroutine]
    /// Reveal the grip." Each is public (CR 1.21.3) and stays revealed
    /// until it moves or the ability has finished (`GameState::revealed`),
    /// so what follows can choose among them (`CardFilter::Revealed`).
    /// Composition didn't work: `RevealAtRandom` reveals a printed number
    /// drawn at random, and a selection over the hand (Vera Ivanovna
    /// Shuyskaya's, Touch-ups') shows its chooser only the cards it may
    /// choose, so a grip with none of the chosen type was never revealed
    /// at all.
    RevealHand(crate::rules::Side),
    /// The card whose text this is remembers a choice for `until` (CR
    /// 9.10.3) — Boomerang's "choose 1 installed piece of ice. Use this
    /// hardware only during encounters with that ice" (`Remembered::
    /// SelectedCard`, inside its selection's `then`, for as long as
    /// Boomerang is installed: 9.10.3c), and Engram Flush's "choose a card
    /// type. For the remainder of the encounter…" (`Remembered::CardType`,
    /// one under each option of the `PresentChoice` that is the choice).
    /// A `rules::lingering::LingeringEffect` about the chooser's install
    /// (`Lingering::ChosenCard`, `Lingering::ChosenCardType`), read back by
    /// `EffectRequirement::EncounteringChosenIce` and `CardFilter::
    /// OfChosenCardType`. The chooser is `ResolutionContext::
    /// prompting_install`, since inside a selection's `then` the acting
    /// install is the card chosen. Composition didn't work: Trieste Model
    /// Bioroids' choice is kept by the prohibition it makes, and Tsakhia's
    /// (`ChooseServer`) is a server parked as a decision of its own; these
    /// two make nothing but the choice, which another of the card's
    /// abilities reads.
    Remember { what: Remembered, until: EffectDuration },
}

/// What `Effect::Remember` keeps. Only what a card in the pool chooses
/// and refers back to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Remembered {
    /// The card a selection just chose — Boomerang's ice.
    SelectedCard,
    /// A card type — Engram Flush's.
    CardType(crate::dsl::CardType),
    /// An ice subtype — Chameleon's barrier, code gate or sentry, read back
    /// by `EffectRequirement::EncounteringChosenIceType`. Not a `CardType`:
    /// the three are the ice's types (`IceType`), which a break is
    /// restricted by.
    IceType(crate::dsl::IceType),
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

/// A dynamically-resolved quantity, computed at effect-evaluation time via
/// `rules::ability::resolve_amount` rather than authored as a flat literal —
/// the counterpart to plain `u32` fields on effects like `DealDamage`/
/// `AddAdditionalAccess`/`BoostStrength` for the handful of cards whose text
/// scales with some other piece of state. Deliberately a small, closed set
/// (not a general expression language) — extend only when a real card needs
/// a new formula. `TakeAllCountersAsCredits`/`GainCreditsPerCounter`
/// predate this enum and aren't folded into it (no behavior change, no card
/// needs the refactor yet) — see ROADMAP.md's tracking note for the planned
/// future consolidation. `GainCreditsPerCardAccessedThisRun` was the third,
/// and went when Amelia Earhart needed its count as a number
/// (`CardsAccessedLastRun`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Amount {
    /// A plain literal — lets an amount-typed effect field be authored with
    /// an ordinary fixed number when no dynamic formula is needed.
    Fixed(u32),
    /// Sum of printed agenda points on agendas the Corp has scored this
    /// turn (`TurnLog::agenda_points_scored`) — e.g. Neurospike. A sum, so
    /// not a `TimesThisTurn`, which counts.
    AgendaPointsScoredThisTurn,
    /// Sum of printed agenda points on agendas the Runner stole during the
    /// turn that ended most recently (`TurnLog::agenda_points_stolen` on
    /// `last_turn`) — Punitive Counterstrike, played on the Corp's turn, so
    /// that turn is the Runner's. `AgendaPointsScoredThisTurn`'s twin.
    AgendaPointsStolenLastTurn,
    /// How many times this trigger's moment has happened this turn, to
    /// anyone and about anything (`rules::turn_log`) — "if you made a
    /// successful run this turn" is `AmountAtLeast(TimesThisTurn(
    /// OnSuccessfulRun), 1)` (Carmen, Marjanah, Mutual Favor), "if you
    /// played an operation this turn" the same over `OnCardPlayed`
    /// (Nebula Talent Management). Composition didn't work because no
    /// existing `Amount` reads the turn; this one took over three
    /// requirements that each read a flag of their own
    /// (`MadeSuccessfulRunThisTurn`, `PlayedOperationThisTurn`, and
    /// `RunnerMadeSuccessfulRunLastTurn` below). **Not how a card says "the
    /// first time each turn"** — that is a word in the trigger condition.
    TimesThisTurn(crate::dsl::Trigger),
    /// The same count narrowed the way a trigger's `when` narrows it — "if
    /// you made a successful run on HQ, R&D, and Archives this turn" (Chain
    /// Reaction, one per server) and "if a piece of ice was rezzed this
    /// turn" (Underdome Irregulars). Read through `turn_log::Occurrences`,
    /// so it is no finer than the log counts (a `Class`), and a filter the
    /// log cannot answer counts 0. `TimesThisTurn` named no filter because
    /// `Amount` was `Copy`; dropping `Copy` cost nothing anywhere in the
    /// workspace (Vantage Point Stage 3b).
    TimesThisTurnWhen { trigger: crate::dsl::Trigger, when: crate::dsl::EventFilter },
    /// The same count for the turn that ended most recently, either
    /// side's (`GameState::last_turn`) — "play only if the Runner made a
    /// successful run during their last turn" (Public Trail, Measured
    /// Response), asked on the Corp's turn, when the last turn was the
    /// Runner's.
    TimesLastTurn(crate::dsl::Trigger),
    /// `TimesLastTurn` narrowed as `TimesThisTurnWhen` narrows this turn's
    /// — Active Policing's and Bring Them Home's "if the Runner … trashed a
    /// Corp card during their last turn" (`EventFilter::OwnedBy`), where
    /// the row alone counts the Runner's trash of their own program too.
    /// Composition didn't work: a last turn kept its row totals only
    /// (`turn_log::LastTurn`, removed) until a card asked it for a column.
    TimesLastTurnWhen { trigger: crate::dsl::Trigger, when: crate::dsl::EventFilter },
    /// `acting_card`'s own hosted generic counter count — e.g. Conduit's
    /// R&D-access bonus.
    HostedCounters,
    /// `acting_card`'s own hosted advancement token count (Corp installed
    /// cards only) — e.g. Clearinghouse, Urtica Cipher.
    HostedAdvancementTokens,
    /// How many cards are hosted on `acting_card` without being installed
    /// (`InstalledRunnerCard::hosted_cards`) — Read-Write Share's "Limit 4
    /// hosted cards", read through `EffectRequirement::AmountAtLeast`
    /// under a `Not`. `HostedCounters` counts counters, not cards.
    HostedCards,
    /// Count of Runner-installed icebreakers (`dsl::zone::CardFilter::
    /// Icebreaker`'s heuristic), including `acting_card` itself if it
    /// qualifies — e.g. Unity's pump ability.
    InstalledIcebreakerCount,
    /// Facedown cards currently in Archives — *Jinteki: Restoring Humanity*.
    FacedownCardsInArchives,
    /// How many card types there are among the faceup cards in Archives —
    /// Logjam's "1 advancement counter for each card type among faceup
    /// cards in Archives". Types, not subtypes, so every piece of ice is one
    /// type. No amount counted distinct kinds of anything.
    CardTypesAmongFaceupInArchives,
    /// The Corp's installs of a card out of HQ this turn (`rules::
    /// turn_log`'s sum beside the cells) — The Holo Man's "If you have not
    /// installed any cards from HQ this turn", `Not(AmountAtLeast(.., 1))`.
    /// Composition didn't work: `TimesThisTurn(OnInstall)` counts an
    /// install from Archives or R&D as well.
    CardsInstalledFromHqThisTurn,
    /// The Corp's installs this turn in the root of or protecting a remote
    /// server (`TurnLog::installed_in_remotes`) — A Teia: IP Recovery's
    /// "the first time each turn", which `first_each_turn` cannot narrow
    /// to a remote (the log's classes hold no server), as `AmountAtLeast`
    /// … `Not` 2 beside `when: InstalledIn(Remote)`.
    CardsInstalledInRemotesThisTurn,
    /// The times the Runner has gained [click] during a run this turn —
    /// Pichação's "If this is not the first time you gained [click] during
    /// a run this turn" (`TurnLog::click_gains_in_runs`, a sum beside
    /// `installed_from_hq`). Composition didn't work: gaining a click is a
    /// moment no card hears, so no cell of the log counts it, and "during a
    /// run" is no `Class`.
    ClickGainsInRunsThisTurn,
    /// The Corp cards added to Archives this turn, by any route —
    /// Regenesis's "if no Corp cards have been added to Archives this
    /// turn" (`TurnLog::added_to_archives`, a sum beside
    /// `installed_from_hq`). Composition didn't work: no `Trigger` hears
    /// every way into Archives — the Corp's discard at the end of their
    /// turn is dispatched to nobody — so no cell of the log counts it.
    CorpCardsAddedToArchivesThisTurn,
    /// The times this turn the action just finished was taken, counting it
    /// — Wage Workers' "if you have taken that action exactly 3 times this
    /// turn" (`TurnLog::times_taken`), with "that action" read off the
    /// triggering `GameEvent::ActionFinished`; 0 outside one. Composition
    /// didn't work: "the same action" (CR 5.2.5a) is no trigger and no
    /// `Class`, so `TimesThisTurn` cannot count one.
    TimesThisActionThisTurn,
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
    /// The cost printed on `acting_card` — its install cost for a Runner
    /// card ("Knickknack" O'Brian's "gain credits equal to its printed
    /// install cost"), its rez cost for ice, an asset or an upgrade
    /// (realloc()'s "gain credits equal to its printed rez cost"), where
    /// the acting card is the one the prompt selected. Read from the
    /// registry, so a discount the card was paid for with does not count.
    /// Was `PrintedInstallCost`, which realloc() would have had to read
    /// as a rez cost: `CardDefinition::cost` is the one printed number
    /// either way (CR 1.16.6a, 1.16.8).
    PrintedCost,
    /// The printed rez or play cost of the card the Runner is accessing —
    /// Lampades's price. `PrintedCost` reads the acting card, which in a
    /// paid ability is the card whose ability it is; the accessed card is
    /// the run's (`AccessPhase::card`). 0 when nothing is being accessed.
    AccessedCardPrintedCost,
    /// The printed cost of the card a nested cost just trashed
    /// (`ResolutionContext::paid_with`) — Kimberlite Field's "the printed
    /// rez cost of the Corp card you trashed". 0 outside what such a cost
    /// paid for. Composition didn't work: `PrintedCost` reads the acting
    /// card, which in an `if_paid` is the card whose text it is.
    PaidCardPrintedCost,
    /// The printed cost of the card the triggering trash is about — Ob
    /// Superheavy Logistics' "the trashed card's printed rez cost", read
    /// off `ResolutionContext::triggering_event` (`GameEvent::CardTrashed`)
    /// as `TimesThisActionThisTurn` is. 0 for any other event or none: a
    /// trash is the only moment a card in the pool reads it from. Composition didn't
    /// work: `PrintedCost` reads the acting card, and making the trashed
    /// card the acting one (`acts_on_subject`) would key Ob's "once per
    /// turn" and its prompt on the card in Archives.
    TriggeringCardPrintedCost,
    /// `u32` minus the cards the resolving `PromptChooseCards` selected
    /// (`ResolutionContext::selected_count`) — the R&D half of a sabotage
    /// of `u32`, resolved in the HQ selection's `then`. Saturating.
    RemainingAfterSelection(u32),
    /// How many cards the resolving `PromptChooseCards` selected
    /// (`ResolutionContext::selected_count`) — Meeting of Minds's "Gain
    /// 1[credit] for each card revealed this way", read in the reveal's
    /// `then`. 0 outside a selection's `then`.
    CardsSelected,
    /// The threat level: the greater of the two players' scores, in agenda
    /// points (Null Signal Games' *Elevation* rule — "the threat level is
    /// equal to the greatest score of any player"). Read through
    /// `EffectRequirement::AmountAtLeast` for Measured Response's "play
    /// only if the threat level is 4 or greater"; *N-Pot* and *Public
    /// Access Plaza* print the same clause at Stage 10.
    ThreatLevel,
    /// The Runner's current tag count — Doomscroll's "do 2 net damage if
    /// the Runner has at least 2 tags", read through
    /// `EffectRequirement::AmountAtLeast`. `EffectRequirement::IsTagged`
    /// answers only "at least one".
    RunnerTags,
    /// The Corp's bad publicity — Luana Campos's "you may host 1 of your
    /// bad publicity counters", which is not offered with none to host.
    /// `RunnerTags`'s sibling: no requirement asked it before.
    BadPublicity,
    /// The core damage the Runner has taken this game — Ontological
    /// Dependence's "−1 advancement requirement for each core damage the
    /// Runner has taken this game". The count that lowers their maximum
    /// hand size (`RunnerState::brain_damage`), which no card removes, so
    /// "this game" is all of it. No amount read it.
    CoreDamageTaken,
    /// The number `Effect::ChooseNumber` was answered with, inside its
    /// `then`. **A placeholder, written over when the number is chosen**
    /// (`Effect::with_chosen_number`, the convention
    /// `PromptChooseServer::on_success` already follows for its server):
    /// the `then` that resolves holds `Fixed(n)` wherever the card file
    /// wrote this. Rejected: a field on `ResolutionContext`, which is where
    /// the rest of the per-resolution scratch lives and which does not
    /// survive a park — and "the Runner loses 5[c] for each tag removed
    /// this way" comes after a tag removal another card may react to. A
    /// continuation is an `Effect`, so a number written into it rides
    /// along with no field anywhere. Resolves to 0 if it is ever read
    /// unsubstituted; `validate` refuses one written outside a `then`.
    ChosenNumber,
    /// Cards in the Runner's heap that print the subtype — Rising Tide's
    /// "+1 strength for each fracter in your heap", which was
    /// `StrengthModifier::PerFracterInHeap` until a continuous effect's
    /// number became an `Amount`. No existing variant counts a zone, so
    /// composition had nothing to compose; and not a general
    /// `CountInZone { zone, filter }`, because `Amount` is `Copy` and a
    /// `CardFilter` is not — the day a second zone is counted is the day
    /// to pay for that.
    InHeapWithSubtype(crate::dsl::CardSubtype),
    /// Agendas of this subtype in the acting card's controller's score
    /// area — Jeitinho's "if you have 3 **assassination** agendas in your
    /// score area". A card added as an agenda has the subtype it was added
    /// as and none it prints (`AsAgenda::subtype`, CR 10.1.3); a stolen or
    /// scored agenda has what it prints. `InHeapWithSubtype`'s shape, for
    /// the reason given there: `Amount` is `Copy`, a `CardFilter` is not.
    InScoreAreaWithSubtype(crate::dsl::CardSubtype),
    /// Pieces of ice protecting the server `acting_card` is installed on or
    /// protecting, itself included — Scatter Field's "while this is the
    /// only piece of ice protecting this server", as
    /// `Not(AmountAtLeast(.., 2))`, which was
    /// `StrengthModifier::WhileOnlyIceProtectingServer` until ice strength
    /// became a continuous effect. No existing variant counts installs by
    /// server, so composition had nothing to compose; a count rather than
    /// an "only ice" requirement, because "for each piece of ice protecting
    /// this server" is the sentence the next card prints. 0 for a card
    /// that is not a Corp install.
    IceProtectingThisServer,
    /// Remote servers that have a card in the root and are protected by
    /// ice — Flood the Market's "place 1 advancement counter on that card
    /// for each remote server that has a card in its root and is protected
    /// by ice". Any rez state, both halves, as the text does not say.
    /// Composition had nothing to compose: `IceProtectingThisServer` counts
    /// one server's ice, and no amount counts servers.
    ProtectedRemotesWithRootCards,
    /// Pieces of ice protecting a server the card names — Tailgate's "for
    /// each piece of ice protecting HQ", priced from the grip, where the
    /// card is on no server for `IceProtectingThisServer` to read.
    IceProtecting(ServerId),
    /// Cards the Runner accessed during the run that ended most recently
    /// (`CompletedRun::cards_accessed`) — Zahya Sadeghi's "gain 1[credit]
    /// for each card accessed", and Amelia Earhart's "if you accessed 3 or
    /// more cards during that run" through `AmountAtLeast`. It replaced
    /// `Effect::GainCreditsPerCardAccessedThisRun`, a gain that could not
    /// be a threshold; 0 before any run has ended.
    CardsAccessedLastRun,
    /// The credits the run's event still hosted when the last run ended
    /// (`CompletedRun::run_credits_left`) — what Trick Shot's "When that run
    /// ends, you may run a remote server" carries into that run, the event
    /// being still in play with its credits (CR 8.6.5). 0 with no run.
    RunCreditsLeftLastRun,
    /// How many cards the active run's breach of HQ or R&D may access:
    /// 1, plus each additional access granted so far (CR 7.3.5a–b,
    /// `RunState::additional_hq_access`/`additional_rd_access`) — "Pretty"
    /// Mary da Silva's "if you are allowed to access 2 or more cards in
    /// R&D during this breach". 0 for any other server, and with no run.
    AccessLimit(ServerId),
    /// How many times the Runner has encountered ice during the run in
    /// progress, counting the encounter under way (`RunState::encounters`)
    /// — S-Dobrado's "the first time you encounter a piece of ice during
    /// that run" (at most 1 as its encounter begins) and "the second time"
    /// (exactly 2). 0 with no run. Composition didn't work: the turn log
    /// counts the turn's encounters, and a run is not a turn.
    EncountersThisRun,
    /// How many times the Runner has passed ice during the run in progress
    /// (`RunState::ice_passed`) — Into the Depths' "for each time you
    /// passed ice this run". 0 with no run. Composition didn't work:
    /// `EncountersThisRun` counts encounters, and an unrezzed piece of ice
    /// is passed without one, a forced encounter met without a pass.
    IcePassedThisRun,
    /// How many times the Runner passed ice during the run that ended
    /// last (`CompletedRun::ice_passed`) — Bravado's "When that run ends,
    /// gain 6[credit] plus 1[credit] for each piece of ice you passed during
    /// that run", read by a run-end effect after the run has left
    /// `active_run`. `IcePassedThisRun`'s other half, as
    /// `CardsAccessedLastRun` is the access count's; 0 before any run has
    /// ended.
    IcePassedLastRun,
    /// The strength of the piece of ice being encountered, never below 0
    /// (`continuous::ice_strength`, which may be) — Arruaceiras Crew's
    /// "trash the ice you are encountering if its strength is 0 or less",
    /// `Not(AmountAtLeast(.., 1))`, since an `Amount` is unsigned. 0
    /// outside an encounter. No amount read a strength.
    EncounteredIceStrength,
    /// The strength of the acting Trojan's host ice, never below 0
    /// (`continuous::installed_ice_strength`) — Parasite's "When the
    /// strength of host ice is 0 or less, trash it", `Not(AmountAtLeast(..,
    /// 1))`. 0 for a card hosted on no ice. Composition didn't work:
    /// `EncounteredIceStrength` is the run's ice, and a Parasite's host
    /// reaches 0 on the Runner's turn start, with no run at all.
    HostIceStrength,
    /// The strength of the acting rig card, never below 0
    /// (`continuous::breaker_strength`) — the rule every interface ability
    /// is held to (CR 3.9.5g: "only … if the icebreaker has strength greater
    /// than or equal to the strength of the encountered ice"), for one that
    /// breaks nothing: Banner's "Interface → 2[credit]: Subroutines on the
    /// barrier you are encountering cannot end the run", as `Not(MoreThan(
    /// EncounteredIceStrength, ThisCardStrength))`. A break checks the
    /// strengths itself (`Effect::BreakSubroutines`); nothing else an
    /// interface ability did had needed to. 0 for anything not in the rig.
    ThisCardStrength,
    /// Subroutines on the piece of ice being encountered, printed and gained
    /// (`RunIce::subroutines`) — Physarum Entangler's "1[credit] for each
    /// subroutine it has", `Cost::CreditsAmount`'s number. 0 outside an
    /// encounter. No amount counted subroutines.
    EncounteredIceSubroutines,
    /// The Corp's installed cards the filter admits — Pulse's "The Runner
    /// loses 1[credit] for each **rezzed piece of harmonic ice**", `All([
    /// Ice, Rezzed, HasSubtype(Harmonic)])`. Counted as a selection over
    /// the Corp's installed cards reads the filter
    /// (`pending_choice::eligible_positions`), so an instance word
    /// (`Rezzed`) is asked of each copy and a type of the definition.
    /// Composition didn't work: every count of installs was one sentence's
    /// (`OtherUnrezzedIce`, `IceProtectingThisServer`), and a filter is
    /// what an `Amount` could not hold while it was `Copy`. "This server"
    /// (`InThisServer`, `InRootOfThisServer`) is the counting card's, as in
    /// a selection — Cayambe Grid's "2[credit] for each **advanced piece of
    /// ice protecting this server**".
    CorpInstalls(crate::dsl::CardFilter),
    /// The Runner's installed cards the filter admits — Tremolo's "for each
    /// installed piece of **cybernetic** hardware", `All([CardType(Hardware),
    /// HasSubtype(Cybernetic)])`. `CorpInstalls`' other side, counted the
    /// same way (a selection over the Runner's installed cards). Composition
    /// didn't work: `InstalledIcebreakerCount` is one sentence's count.
    RunnerInstalls(crate::dsl::CardFilter),
    /// The Runner's link (`continuous::link`, the identity's printed link
    /// plus what the rig declares) — DreamNet's "or you have at least
    /// 2[link]", `AmountAtLeast(Link, 2)`. Composition didn't work: link
    /// was asked only by a trace and the view.
    Link,
    /// `amount` less `by`, never below 0 — Tremolo's "3[credit]: … This
    /// ability costs 1[credit] less to use for each installed piece of
    /// cybernetic hardware", `Cost::CreditsAmount(Reduced { amount: Fixed(3),
    /// by: RunnerInstalls(…) })`: a cost lowered past 0 is 0 (CR 1.16.2a).
    /// Composition didn't work: an `Amount` is a count and every count was
    /// one number, and a `ContinuousKind` is about a card or a player, not
    /// one of a card's abilities. The cost's own words, so the ability
    /// prices itself wherever the payment asks.
    Reduced { amount: Box<Amount>, by: Box<Amount> },
    /// `amount` plus `by` — Chekist Scion's "give them 1 tag plus 1 tag for
    /// each hosted advancement counter", `GiveTags(Increased { amount:
    /// Fixed(1), by: HostedAdvancementTokens })`. `Reduced`'s other half.
    /// Composition didn't work: two `GiveTags` in a row are two
    /// instructions, so two tag events, two prevention windows and two
    /// "whenever the Runner takes tags", where taking a number of tags in
    /// one instruction is one aggregated effect (CR 9.12.2c).
    Increased { amount: Box<Amount>, by: Box<Amount> },
    /// `amount` taken `times` times — NAPD Cordon's "4[credit] plus
    /// 2[credit] **for each** advancement counter on that agenda",
    /// `Increased { Fixed(4), Times { AccessedCardAdvancementCounters, 2 } }`.
    /// Composition didn't work: `Increased` of a count with itself is the
    /// same number, and reads to a person as two counts where the card
    /// prints one at a rate.
    Times { amount: Box<Amount>, times: u32 },
    /// 1 for every `every` of `amount`, rounded down — Project Beale's
    /// "place 1 agenda counter on it **for every 2** hosted advancement
    /// counters past 3", `Every { Reduced { HostedAdvancementTokens,
    /// Fixed(3) }, 2 }`. `Times`'s other half. Composition didn't work: a
    /// ladder of `EffectIf`s, one per threshold, stops at the last rung
    /// written, and an agenda has no most counters it may be scored with.
    Every { amount: Box<Amount>, every: u32 },
    /// The advancement counters on the installed card being accessed — the
    /// agenda NAPD Cordon's additional cost to steal is about ("that
    /// agenda"), read off the access (`AccessState::pending_install`). 0
    /// for a card accessed out of HQ, R&D or Archives, which holds none.
    /// Composition didn't work: `HostedAdvancementTokens` is the acting
    /// install's, and the card paying is the lockdown.
    AccessedCardAdvancementCounters,
    /// Unrezzed pieces of ice other than `acting_card`'s install, wherever
    /// they are — Reverb's "lowered by 1[credit] for each other unrezzed
    /// piece of ice". No amount counted ice by rez state.
    OtherUnrezzedIce,
    /// The cards in a player's hand — HQ for the Corp, the grip for the
    /// Runner, whichever side is acting — Piranhas's "if there are more
    /// cards in HQ than in the grip", read through
    /// `EffectRequirement::MoreThan`. A side rather than a `CardZoneRef`,
    /// which is relative to the actor: the sentence names both hands. No
    /// amount counted a hand.
    CardsInHand(Side),
    /// The cards in `zone`, as the acting card's controller names it, that
    /// `filter` admits — Focus Group's "the number of revealed cards of the
    /// chosen type" in the Runner's grip (`OpponentHand`, `CardType`).
    /// `EffectRequirement::ZoneHasAtLeast`'s count, as a number. Composition
    /// didn't work: `CardsInHand` counts every card, and no amount read a
    /// zone through a filter.
    InZone { zone: crate::dsl::CardZoneRef, filter: crate::dsl::CardFilter },
    /// Copies of the acting card in `side`'s score area — Sting!'s "the
    /// number of copies of Sting! in the other player's score area", one
    /// trigger for a score (the Runner's) and one for a steal (the Corp's).
    /// By card, so a copy added there "as an agenda" under another name is
    /// not one. Composition didn't work: `InScoreAreaWithSubtype` counts a
    /// subtype, and Sting!'s, Ambush, is printed on other agendas.
    CopiesInScoreArea(Side),
    /// The counters of this kind on the acting card's controller's
    /// installed cards, all of them — The Nihilist's "remove any 2 virus
    /// counters from your installed cards", which is not offered unless
    /// there are 2 to remove. Composition didn't work: `HostedCounters` is
    /// one card's, and `RunnerInstalls(HostsCounters(..))` counts cards.
    CountersOnOwnInstalls(crate::dsl::CounterKind),
    /// The credits in a player's credit pool — Valentão's "End the run if
    /// you have more credits than the Runner", through
    /// `EffectRequirement::MoreThan`, as `CardsInHand` names both hands.
    /// The pool, not what the player could spend: a card's hosted credits
    /// are the card's, not the player's (CR 1.10.4). Composition didn't
    /// work: `RunnerCreditsAtMost` and `CorpCreditsAtLeast` compare with a
    /// number the card file writes.
    Credits(Side),
    /// How many times this trigger's moment has happened this turn to the
    /// copy resolving — its install, read off `InstalledCard::this_turn`
    /// (`turn_log::CopyTurn`, which counts only advancing and rezzing) —
    /// Cloud Eater's "if it was rezzed this turn", `AmountAtLeast(
    /// TimesThisTurnOnThisCopy(OnRez), 1)`. 0 for a trigger the copy does
    /// not count, or with no Corp install resolving. `TimesThisTurn` counts
    /// the turn's moments by class, and a class has no copy in it.
    TimesThisTurnOnThisCopy(crate::dsl::Trigger),
    /// How much of what is about to happen there is — the cards "you would
    /// draw" in The Class Act's "X is equal to the number of cards you
    /// would draw plus 1" (`Increased` by 1), read off the
    /// `GameEvent::AboutToResolve` the trigger heard. 0 for any other
    /// moment. Composition didn't work: no amount read the event a trigger
    /// heard beyond the card it was about.
    AboutToResolve,
    /// The agenda points in `Side`'s score area — Complete Image's "Play
    /// only if the Runner has 3 or more agenda points" (`AmountAtLeast`).
    /// Composition didn't work: no amount read a score.
    AgendaPoints(Side),
    /// The actions taken this turn (`TurnLog::actions_finished`), and how
    /// many different actions among them (`TurnLog::same_actions`, CR
    /// 5.2.5) — MirrorMorph's "If the first, second, and third actions you
    /// take on your turn are each different from one another, when the
    /// third action completes", which is 3 of each. Not
    /// `TimesThisTurn(OnActionFinished)`: the count of the trigger's own
    /// moment is what `first_each_turn` says, and `validate` refuses it.
    ActionsThisTurn,
    DifferentActionsThisTurn,
}

impl Amount {
    /// See `Effect::with_chosen_number`, for an amount that reads the
    /// number inside a filter or a sum: Khusyuk's "the number of your
    /// installed cards with that printed install cost, up to 6", which is
    /// `Reduced` over `InZone { filter: PrintedCostExactly(ChosenNumber) }`.
    pub fn with_chosen_number(self, number: u32) -> Amount {
        let boxed = |amount: Box<Amount>| Box::new(amount.with_chosen_number(number));
        match self {
            Amount::ChosenNumber => Amount::Fixed(number),
            Amount::InZone { zone, filter } => Amount::InZone { zone, filter: filter.with_chosen_number(number) },
            Amount::Reduced { amount, by } => Amount::Reduced { amount: boxed(amount), by: boxed(by) },
            Amount::Increased { amount, by } => Amount::Increased { amount: boxed(amount), by: boxed(by) },
            Amount::Times { amount, times } => Amount::Times { amount: boxed(amount), times },
            Amount::Every { amount, every } => Amount::Every { amount: boxed(amount), every },
            other => other,
        }
    }
}

/// What `Effect::EndTheRun` does the first time it would end a run with
/// an `Effect::ArmRunEndPrevention` standing — Shred's "The first time the
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

/// How long an effect that outlives its resolution lasts — a
/// `BoostStrength` pump, a `Prohibit`. Was `BoostDuration` while a boost
/// was the only thing with one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectDuration {
    /// Cleared when the current ICE encounter ends.
    Encounter,
    /// Cleared when the run ends — Gordian Blade's "+1 strength for the
    /// remainder of this run": one pump carries across every encounter of
    /// the run it was bought in, and no further.
    Run,
    /// Until the end of the turn it was made in, whoever's that is.
    Turn,
    /// Until the next turn of the resolving card's controller ends —
    /// Klevetnik's "until your next turn ends", made on the Runner's turn
    /// and lasting through the Corp's turn after it. Resolved to the number
    /// of that turn when the effect is made (`lingering::until`): turns
    /// alternate, so a player's next turn is the turn after this one, or
    /// the one after that when this turn is already theirs.
    ThroughYourNextTurn,
    /// For as long as the card that made it stays rezzed (CR 9.10.3c: a
    /// lingering effect that keeps a choice lasts until its source becomes
    /// inactive) — Trieste Model Bioroids' chosen ice, chosen as it is
    /// rezzed. Resolved to `Until::WhileRezzed` of the card whose text this
    /// is (`ResolutionContext::prompting_install`, since inside a
    /// selection's `then` the acting install is the card chosen), so a
    /// derez, a trash or a second rez ends it. Composition didn't work:
    /// Lycian Multi-Munition's `WhileRezzed` is its own, hard-wired into
    /// `GainIceSubtype`, and every duration here was a span of the game.
    WhileRezzed,
    /// For as long as the card that made it stays installed — the Runner's
    /// twin of `WhileRezzed`, since a rig card is active while installed
    /// (CR 9.10.3c): Boomerang's chosen ice. Resolved to `Until::
    /// WhileInstalled` of the card whose text this is, as `WhileRezzed`
    /// is. Not `WhileRezzed`, which a Runner card is never.
    WhileInstalled,
    /// Until the controller's next action this turn has been taken (CR
    /// 5.2): MirrorMorph's "take another different action", whose
    /// prohibition on repeating an action binds that action and no other
    /// (`Until::ActionsFinished`, counted off `TurnLog::actions_finished`).
    /// It ends with the turn if no action follows.
    NextAction,
}

/// What an `Effect::Prevent` prevents, as the card prints it after the word
/// "prevent". Only what a card in the pool prints: bad publicity, an expose
/// and a jack-out are each a variant here and an arm in `rules::prevention`
/// the day a card needs one (Zaibatsu Loyalty is deferred on expose, which
/// the engine does not have).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preventable {
    /// "Prevent up to 3 meat damage" (`kind: Some(Meat)`), "prevent 1 net
    /// damage". `None` is damage of any kind. "Up to" prevents as much of
    /// it as is left: nobody prevents less than they paid for, and a
    /// number to choose would need a decision the engine does not have
    /// (Rules Audit backlog item 6).
    Damage { kind: Option<DamageType>, up_to: u32 },
    /// "Prevent 1 tag."
    Tags(u32),
    /// "Prevent a player from trashing 1 installed program or piece of
    /// hardware" — one installed card the filter admits.
    Trash(CardFilter),
    /// "Prevent a “when encountered” ability on a piece of ice" (AirbladeX
    /// (JSRF Ed.)): one of the encountered ice's own `OnEncounter`
    /// triggers, about to resolve. Composition didn't work: prevention knew
    /// only damage, tags and a trash, each something a card's text does,
    /// and this is an ability that would resolve.
    EncounterAbility,
    /// "Prevent a Corp card ability from ending the run" (Lucky Charm): an
    /// "end the run" a Corp card's text resolves, about to end the run
    /// (`WouldHappen::RunEnds`). A Runner card's own "end the run" and a
    /// jack-out are not it. Composition didn't work: the one prevention of
    /// a run's end (Shred's) stands for a duration and asks nobody.
    RunEnding,
    /// "Reduce the base trace strength of a trace to 0" (Flip Switch): a
    /// trace about to be initiated (`WouldHappen::Trace`), which starts all
    /// the same, at base strength 0 when this was used. Composition didn't
    /// work: nothing could act between a trace's initiation and its bids.
    TraceBaseStrength,
}

/// Which ice an `Effect::ModifyStrength` changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrengthOf {
    /// The ice being encountered.
    #[default]
    Encountered,
    /// Each piece of ice, wherever it is installed, those installed later
    /// too.
    EachIce,
    /// The Corp install resolving the effect (`acting_install`).
    This,
}

impl StrengthOf {
    fn is_encountered(&self) -> bool {
        *self == StrengthOf::Encountered
    }

    fn this() -> StrengthOf {
        StrengthOf::This
    }

    fn is_this(&self) -> bool {
        *self == StrengthOf::This
    }
}

/// What a player cannot do while an `Effect::Prohibit` holds. Only what a
/// card in the pool prints; each is asked through `continuous::cannot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Prohibition {
    /// The Corp cannot score agendas.
    ScoreAgendas,
    /// The Runner cannot steal or trash the cards they access.
    StealOrTrash,
    /// The Runner cannot steal or trash an agenda they access — Pinhole
    /// Threading's "If that card is an agenda, you cannot steal or trash it
    /// during this access", made for the rest of the run before the card is
    /// chosen: the access is the run's last act, so "for the rest of this
    /// run" is "during this access", and a prohibition made after the card
    /// is seen, on whether it is an agenda, would show a facedown card's
    /// type in the view's in-effect list. Asked with `StealOrTrash` by
    /// `continuous::cannot_about`, of an agenda only. Composition didn't
    /// work: `StealOrTrash` would forbid trashing the asset Pinhole
    /// Threading is for.
    StealOrTrashAgendas,
    /// The Runner cannot lose or spend credits from their credit pool —
    /// Aircheck's "while this event is active, … you cannot lose or spend
    /// credits from your credit pool", for the run it makes. Asked by
    /// `payment::sources`, where the credit pool then holds nothing, and
    /// by `Effect::LoseCredits`.
    SpendOrLoseCreditPool,
    /// The Runner cannot spend credits, from any pool — Attini's "Threat 3
    /// → The Runner cannot spend credits while subroutines on this ice are
    /// resolving", a standing effect (`ContinuousKind::Cannot`). Asked by
    /// `payment::sources`, which then has nothing to offer: every credit a
    /// payment takes is a credit spent. Not `SpendOrLoseCreditPool`, which
    /// leaves the hosted credits spendable and forbids a loss.
    SpendCredits,
    /// A piece of ice's subroutines cannot end the run — Banner's
    /// "Subroutines on the barrier you are encountering cannot end the run
    /// for the remainder of this encounter", always about one ice
    /// (`Effect::Prohibit::encountered_ice`). Asked by `Effect::EndTheRun`
    /// while that ice's subroutines are resolving, the same stretch
    /// Attini's `ResolvingThisIcesSubroutines` names: the "end the run"
    /// resolves and does nothing, and the rest of the ice's subroutines
    /// resolve (CR 1.2.2, the "cannot" takes precedence; 1.2.4, the rest of
    /// the instruction is carried out). Nobody uses it, so it opens no
    /// window. The subroutines are the Corp's card's, so it binds
    /// the Corp. Not `EndRunPrevention`, Shred's, which is a prevention
    /// with a condition and a first time.
    EndTheRun,
    /// The Runner cannot run on a remote server — Front Company's "The
    /// first run each turn cannot be made against a remote server", a
    /// standing effect on the turn's first run (`ContinuousEffect::
    /// first_each_turn`, counting the turn's runs: `counted_as`). Asked
    /// where a run's server is announced (CR 6.3.2a): `run::start_run`,
    /// which every run goes through, and the servers a card's text offers
    /// to run (`Effect::PromptChooseServer`), which then leave the remotes
    /// out.
    RunOnRemote,
    /// The Runner cannot run at all — Excalibur's "The Runner cannot make
    /// another run this turn", made for the turn (`until: Turn`). Asked as
    /// a run is announced (`run::check_run_may_begin`), so the basic action
    /// and a card's text are refused alike, and the action list offers
    /// neither. "Another" needs no word of its own: the run under way when
    /// it is made has begun already, and is not refused.
    Run,
    /// The Runner cannot access cards other than one install — Adrian
    /// Seis's "If the bids differ, the Runner cannot access cards other
    /// than this upgrade for the remainder of that run", always bound to
    /// that install (`Effect::Prohibit::this_install`). Asked where the
    /// breach drops what can no longer be accessed (CR 7.4.2,
    /// `run::access::prune_candidates`).
    AccessOthers,
    /// The Runner cannot access one install — Adrian Seis's "If the bids
    /// match, the Runner cannot access this upgrade for the remainder of
    /// that run", bound as `AccessOthers` is and asked where it is.
    Access,
    /// One install's abilities cannot break subroutines — Hafrún's "choose
    /// 1 installed Runner card. That card's abilities cannot break
    /// subroutines for the remainder of that run", bound to the card the
    /// selection chose (`Effect::Prohibit::this_install`). Asked by the two
    /// break effects of the breaking install (`ability::breakable_now`),
    /// which then find nothing to break, so the ability is not offered.
    BreakSubroutines,
    /// The Corp skips their discard step — Midnight-3 Arcology's "Skip
    /// your discard step this turn", for the turn it is scored in. The
    /// game goes from the start of the discard phase straight to the step
    /// after the discard step: no hand size is checked and nothing is
    /// discarded (CR 5.5.4d). Asked by `turn::begin_discard_step`. Not a
    /// hand size, which the discard would still read, and not a lingering
    /// kind of its own: a step the player may not take for a duration is
    /// what `Lingering::Cannot` already is, and it rides in the view.
    DiscardStep,
    /// The Runner cannot use paid abilities printed on **bioroid** ice —
    /// Hákarl 1.0's "If you do, the Runner cannot use paid abilities
    /// printed on bioroid ice for the remainder of this turn": the "Lose
    /// [click]: Break 1 subroutine on this ice" that a bioroid prints for
    /// the Runner (Ansel 2.0's, Hákarl's own). Asked by
    /// `engine::activate_ability` of an ability on a bioroid the Runner
    /// would use, so the action list, which probes that, never offers one.
    /// Not `BreakSubroutines`, which is about one Runner install's
    /// abilities, and not a `CardFilter` payload: a prohibition is `Copy`
    /// and every one is asked by name (`Prohibition::ALL`).
    BioroidIceAbilities,
    /// The Corp cannot rez one install — Mitosis's "You cannot score or rez
    /// either of those cards this turn", bound to each card it installed
    /// (`Effect::Prohibit::this_install`, made by
    /// `PromptInstallCorpCard::if_installed`). Asked by `engine::rez_install`,
    /// the one place a Corp card is turned faceup, so the rez action, the
    /// action list's probe of it and a card's text that rezzes are all
    /// refused (`RulesError::RezRestricted`, which a text rez treats as an
    /// unaffordable one). Not a rez requirement, which a card prints about
    /// itself; this is another card's word about it, for a duration.
    Rez,
    /// Runner card abilities cannot break subroutines on one piece of ice
    /// — Trieste Model Bioroids' "choose 1 rezzed piece of bioroid ice.
    /// Runner card abilities cannot break subroutines on the chosen ice",
    /// bound to the chosen ice (`Effect::Prohibit::this_install`) for as
    /// long as Trieste is rezzed (`EffectDuration::WhileRezzed`). Asked by
    /// the two break effects (`ability::breakable_now`) when the breaker
    /// is a Runner card, so the ice's own "[click]: break" and the
    /// Runner-usable abilities a bioroid prints are untouched — they are
    /// Corp card abilities. Not `BreakSubroutines`, which is about the
    /// breaking install, never the broken one.
    BreakSubroutinesOnIce,
    /// The run cannot be declared successful — Transport Monopoly's
    /// "Hosted agenda counter: This run cannot be declared successful",
    /// for the run it is used in (`EffectDuration::Run`). Asked where the
    /// declaration is made (CR 6.9.5a, `continuous::
    /// may_be_declared_successful`), beside the standing word Flagship
    /// prints about its server (`ContinuousKind::
    /// CannotBeDeclaredSuccessful`), so the run is withheld its success and
    /// nothing else: it is not unsuccessful (CR 6.8.4a), and the breach
    /// follows. Not that kind with a duration: a standing effect is
    /// scanned off an active card and cannot outlive the use that made it.
    DeclaredSuccessful,
    /// The Runner cannot use **non-icebreaker** cards to break subroutines
    /// — NEXT Activation Command's, standing while the lockdown is in play
    /// (`ContinuousKind::Cannot`). Asked by the two break effects of a
    /// breaker that is not an icebreaker (`ability::breakable_now`), which
    /// then find nothing to break, so the ability is not offered: Boomerang,
    /// a bioroid's "Lose [click]: Break 1 subroutine", any card but a
    /// program with the icebreaker subtype. Not `BreakSubroutines`, which
    /// binds one install for a duration.
    BreakWithNonIcebreakers,
    /// The Corp cannot take an action of a kind already taken this turn
    /// (`turn_log::SameAction`, CR 5.2.5) — MirrorMorph's "take another
    /// **different** action", for the one action that follows
    /// (`EffectDuration::NextAction`). Asked by `engine::apply_action`'s
    /// guard and so by the action list.
    RepeatAnAction,
}

impl Prohibition {
    /// Every prohibition, for a question put about each of them
    /// (`view::build_client_view`'s `standing_cannot`).
    pub const ALL: [Prohibition; 17] = [
        Prohibition::ScoreAgendas,
        Prohibition::StealOrTrash,
        Prohibition::StealOrTrashAgendas,
        Prohibition::SpendOrLoseCreditPool,
        Prohibition::SpendCredits,
        Prohibition::EndTheRun,
        Prohibition::RunOnRemote,
        Prohibition::Run,
        Prohibition::AccessOthers,
        Prohibition::Access,
        Prohibition::BreakSubroutines,
        Prohibition::DiscardStep,
        Prohibition::BioroidIceAbilities,
        Prohibition::Rez,
        Prohibition::BreakSubroutinesOnIce,
        Prohibition::DeclaredSuccessful,
        Prohibition::BreakWithNonIcebreakers,
    ];

    /// The player it binds.
    pub fn binds(self) -> Side {
        match self {
            Prohibition::ScoreAgendas | Prohibition::EndTheRun | Prohibition::DiscardStep | Prohibition::Rez | Prohibition::RepeatAnAction => Side::Corp,
            Prohibition::StealOrTrash | Prohibition::StealOrTrashAgendas | Prohibition::SpendOrLoseCreditPool | Prohibition::SpendCredits | Prohibition::RunOnRemote | Prohibition::Run | Prohibition::AccessOthers | Prohibition::Access | Prohibition::BreakSubroutines | Prohibition::BioroidIceAbilities | Prohibition::BreakSubroutinesOnIce | Prohibition::DeclaredSuccessful | Prohibition::BreakWithNonIcebreakers => Side::Runner,
        }
    }

    /// The moment the thing prohibited is, where the turn log counts it —
    /// so "the **first** run each turn cannot…" can be said with
    /// `ContinuousEffect::first_each_turn` rather than a count written into
    /// the card. `None` for a prohibition no card says of a first.
    pub(crate) fn counted_as(self) -> Option<crate::dsl::Trigger> {
        match self {
            Prohibition::RunOnRemote => Some(crate::dsl::Trigger::OnRunStart),
            Prohibition::ScoreAgendas | Prohibition::StealOrTrash | Prohibition::StealOrTrashAgendas | Prohibition::SpendOrLoseCreditPool | Prohibition::SpendCredits | Prohibition::EndTheRun | Prohibition::Run | Prohibition::AccessOthers | Prohibition::Access | Prohibition::BreakSubroutines | Prohibition::DiscardStep | Prohibition::BioroidIceAbilities | Prohibition::Rez | Prohibition::BreakSubroutinesOnIce | Prohibition::DeclaredSuccessful | Prohibition::BreakWithNonIcebreakers | Prohibition::RepeatAnAction => None,
        }
    }
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
    /// Breaks up to the number chosen — the placeholder a `Cost::CreditsX`
    /// writes its X over (`Effect::with_chosen_number`, which turns it into
    /// `Fixed`): Lobisomem's "Break X barrier subroutines". Read anywhere
    /// else it breaks none, as `Amount::ChosenNumber` is 0.
    ChosenNumber,
}

impl Effect {
    /// `CardFilter::TrashedThisWay` written over as the cards a mill
    /// trashed (`Effect::Mill`'s `then`), in every selection the effect
    /// makes and every choice, sequence or condition around one. The
    /// shapes a mill's `then` is written in; any other effect is returned
    /// as it is.
    pub fn with_those_trashed(self, cards: &[crate::dsl::CardId]) -> Effect {
        let all = |effects: Vec<Effect>| effects.into_iter().map(|e| e.with_those_trashed(cards)).collect();
        match self {
            Effect::PromptChooseCards { side, source, filter, min, max, reveal, shuffle_after, destination, then, count, up_to } => {
                Effect::PromptChooseCards { side, source, filter: filter.with_those_trashed(cards), min, max, reveal, shuffle_after, destination, then, count, up_to }
            }
            Effect::Sequence(effects) => Effect::Sequence(all(effects)),
            Effect::PresentChoice { chooser, options, texts } => Effect::PresentChoice { chooser, options: all(options), texts },
            Effect::ResolveSomeOf { chooser, count, options, texts } => Effect::ResolveSomeOf { chooser, count, options: all(options), texts },
            Effect::Repeat { times, effect } => Effect::Repeat { times, effect: Box::new(effect.with_those_trashed(cards)) },
            Effect::EffectIf { condition, effect } => Effect::EffectIf { condition, effect: Box::new(effect.with_those_trashed(cards)) },
            other => other,
        }
    }

    /// Whether a selection in this effect names `CardFilter::ThatCard`, so
    /// a trigger asks what its moment is about only when the answer is
    /// written somewhere.
    pub fn names_that_card(&self) -> bool {
        let mut named = false;
        self.for_each_effect(&mut |effect| {
            if let Effect::PromptChooseCards { filter, .. } = effect {
                named |= filter.names_that_card();
            }
        });
        named
    }

    /// `CardTarget::Install(PLACEHOLDER)` written over as `install`, the card
    /// an `Effect::ForEach` is resolving for, through the shapes Game
    /// Over's is written in; any other effect is returned as it is.
    pub fn with_each_install(self, install: crate::rules::InstallId) -> Effect {
        let boxed = |effect: Box<Effect>| Box::new(effect.with_each_install(install));
        let all = |effects: Vec<Effect>| effects.into_iter().map(|e| e.with_each_install(install)).collect();
        let target = |target: CardTarget| match target {
            CardTarget::Install(crate::rules::InstallId::PLACEHOLDER) => CardTarget::Install(install),
            other => other,
        };
        match self {
            Effect::TrashCard(t) => Effect::TrashCard(target(t)),
            Effect::DerezCard(t) => Effect::DerezCard(target(t)),
            Effect::Sequence(effects) => Effect::Sequence(all(effects)),
            Effect::PresentChoice { chooser, options, texts } => Effect::PresentChoice { chooser, options: all(options), texts },
            Effect::EffectIf { condition, effect } => Effect::EffectIf { condition, effect: boxed(effect) },
            Effect::OfferPaidChoice { side, cost, if_paid, if_declined, text, if_able } => {
                Effect::OfferPaidChoice { side, cost, if_paid: boxed(if_paid), if_declined: boxed(if_declined), text, if_able }
            }
            other => other,
        }
    }

    /// `Amount::PaidCardPrintedCost` written over as `cost` in an install's
    /// discount — Rejig's "paying X[credit] less", read as the selection
    /// that installs is offered, since the install resolves on a later
    /// action with no payment to read it from. The shapes Rejig's is
    /// written in; any other effect is returned as it is.
    pub fn with_paid_card_cost(self, cost: u32) -> Effect {
        let boxed = |effect: Box<Effect>| Box::new(effect.with_paid_card_cost(cost));
        let all = |effects: Vec<Effect>| effects.into_iter().map(|e| e.with_paid_card_cost(cost)).collect();
        match self {
            Effect::InstallRunnerCardFromGripWithDiscount(Discount::Amount(of)) if *of == Amount::PaidCardPrintedCost => {
                Effect::InstallRunnerCardFromGripWithDiscount(Discount::Credits(cost))
            }
            Effect::Sequence(effects) => Effect::Sequence(all(effects)),
            Effect::PresentChoice { chooser, options, texts } => Effect::PresentChoice { chooser, options: all(options), texts },
            Effect::EffectIf { condition, effect } => Effect::EffectIf { condition, effect: boxed(effect) },
            other => other,
        }
    }

    /// `CardFilter::ThatCard` written over as `card`, the card the moment a
    /// trigger heard is about, in every selection the effect makes and
    /// every choice, sequence, condition or paid choice around one — the
    /// shapes Divested Trust's is written in; any other effect is returned
    /// as it is.
    /// `CardFilter::ChosenName` written over as `card` — what `Effect::
    /// ChooseCardName::then` becomes once the name is chosen: in a
    /// selection's filter, a condition (Complete Image's "if you trash a
    /// card with the chosen name"), and a delayed ability's condition
    /// (Whistleblower's "an agenda with the chosen name"). Stops at a nested
    /// `ChooseCardName`, whose placeholder is that choice's.
    pub fn with_chosen_name(self, card: &crate::dsl::CardId) -> Effect {
        let boxed = |effect: Box<Effect>| Box::new(effect.with_chosen_name(card));
        let all = |effects: Vec<Effect>| effects.into_iter().map(|e| e.with_chosen_name(card)).collect();
        match self {
            Effect::PromptChooseCards { side, source, filter, min, max, reveal, shuffle_after, destination, then, count, up_to } => Effect::PromptChooseCards {
                side,
                source,
                filter: filter.with_chosen_name(card),
                min,
                max,
                reveal,
                shuffle_after,
                destination,
                then: then.map(boxed),
                count,
                up_to,
            },
            Effect::Sequence(effects) => Effect::Sequence(all(effects)),
            Effect::PresentChoice { chooser, options, texts } => Effect::PresentChoice { chooser, options: all(options), texts },
            Effect::EffectIf { condition, effect } => Effect::EffectIf { condition: condition.with_chosen_name(card), effect: boxed(effect) },
            Effect::OfferPaidChoice { side, cost, if_paid, if_declined, text, if_able } => {
                Effect::OfferPaidChoice { side, cost, if_paid: boxed(if_paid), if_declined: boxed(if_declined), text, if_able }
            }
            Effect::LaterThisTurn { when, filter, every_time, effect, this_run } => Effect::LaterThisTurn {
                when,
                filter: filter.map(|filter| filter.with_chosen_name(card)),
                every_time,
                effect: boxed(effect),
                this_run,
            },
            other => other,
        }
    }

    pub fn with_that_card(self, card: &crate::dsl::CardId) -> Effect {
        let boxed = |effect: Box<Effect>| Box::new(effect.with_that_card(card));
        let all = |effects: Vec<Effect>| effects.into_iter().map(|e| e.with_that_card(card)).collect();
        match self {
            Effect::PromptChooseCards { side, source, filter, min, max, reveal, shuffle_after, destination, then, count, up_to } => Effect::PromptChooseCards {
                side,
                source,
                filter: filter.with_that_card(card),
                min,
                max,
                reveal,
                shuffle_after,
                destination,
                then: then.map(boxed),
                count,
                up_to,
            },
            Effect::Sequence(effects) => Effect::Sequence(all(effects)),
            Effect::PresentChoice { chooser, options, texts } => Effect::PresentChoice { chooser, options: all(options), texts },
            Effect::EffectIf { condition, effect } => Effect::EffectIf { condition, effect: boxed(effect) },
            Effect::OfferPaidChoice { side, cost, if_paid, if_declined, text, if_able } => {
                Effect::OfferPaidChoice { side, cost, if_paid: boxed(if_paid), if_declined: boxed(if_declined), text, if_able }
            }
            other => other,
        }
    }

    /// This effect with every `Amount::ChosenNumber` written over by
    /// `Fixed(number)` — what `Effect::ChooseNumber::then` becomes once the
    /// number is chosen. Stops at a nested `ChooseNumber`'s own `then`,
    /// whose placeholder is that choice's (its bounds are this one's).
    ///
    /// Ends in `other => other`, as `substitute_chosen_server` does, so a
    /// new variant holding an `Amount` could be missed here;
    /// `a_chosen_number_reaches_every_amount_a_card_writes` holds the pool
    /// to it instead — no card's `then` may come out still naming the
    /// placeholder.
    pub fn with_chosen_number(self, number: u32) -> Effect {
        let amount = |amount: Amount| amount.with_chosen_number(number);
        let boxed = |effect: Box<Effect>| Box::new(effect.with_chosen_number(number));
        let all = |effects: Vec<Effect>| effects.into_iter().map(|e| e.with_chosen_number(number)).collect();
        match self {
            Effect::GainCreditsAmount(side, a) => Effect::GainCreditsAmount(side, amount(a)),
            Effect::LoseCreditsAmount(side, a) => Effect::LoseCreditsAmount(side, amount(a)),
            Effect::DrawCardsAmount(side, a) => Effect::DrawCardsAmount(side, amount(a)),
            Effect::GiveTags(a) => Effect::GiveTags(amount(a)),
            Effect::RemoveTags(a) => Effect::RemoveTags(amount(a)),
            Effect::PlaceAdvancementCounters(a) => Effect::PlaceAdvancementCounters(amount(a)),
            Effect::Mill { deck, amount: a, then } => Effect::Mill { deck, amount: amount(a), then },
            Effect::DealDamageAmount(kind, a) => Effect::DealDamageAmount(kind, amount(a)),
            Effect::AddAdditionalAccessAmount { server, amount: a } => Effect::AddAdditionalAccessAmount { server, amount: amount(a) },
            Effect::BoostStrengthAmount { amount: a, duration } => Effect::BoostStrengthAmount { amount: amount(a), duration },
            Effect::ChooseNumber { chooser, min, max, of, then, text, secret } => {
                Effect::ChooseNumber { chooser, min, max: amount(max), of: of.map(amount), then, text, secret }
            }
            Effect::SetIdentityCopy(a) => Effect::SetIdentityCopy(amount(a)),
            Effect::RemoveCounters(a) => Effect::RemoveCounters(amount(a)),
            Effect::RemoveAdvancementCounters(a) => Effect::RemoveAdvancementCounters(amount(a)),
            // Focus Group's "place X advancement counters on 1 installed
            // card": the number rides into the card chosen.
            Effect::PromptChooseCards { side, source, filter, min, max, reveal, shuffle_after, destination, then, count, up_to } => Effect::PromptChooseCards {
                side,
                source,
                filter: filter.with_chosen_number(number),
                min,
                max,
                reveal,
                shuffle_after,
                destination,
                then: then.map(boxed),
                count,
                up_to,
            },
            // Its `then`'s number is the draw's, as a nested
            // `ChooseNumber`'s `then` is that choice's.
            Effect::IncreaseAboutToResolve { by, then } => Effect::IncreaseAboutToResolve { by: amount(by), then },
            Effect::BreakSubroutines { count: SubroutineBreakCount::ChosenNumber, restrict_to } => {
                Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(number), restrict_to }
            }
            Effect::Sequence(effects) => Effect::Sequence(all(effects)),
            Effect::PresentChoice { chooser, options, texts } => Effect::PresentChoice { chooser, options: all(options), texts },
            Effect::ResolveSomeOf { chooser, count, options, texts } => Effect::ResolveSomeOf { chooser, count, options: all(options), texts },
            Effect::Repeat { times, effect } => Effect::Repeat { times: amount(times), effect: boxed(effect) },
            Effect::EffectIf { condition, effect } => Effect::EffectIf { condition: condition.with_chosen_number(number), effect: boxed(effect) },
            Effect::OfferPaidChoice { side, cost, if_paid, if_declined, text, if_able } => {
                Effect::OfferPaidChoice { side, cost, if_paid: boxed(if_paid), if_declined: boxed(if_declined), text, if_able }
            }
            Effect::PsiGame { on_match, on_differ } => Effect::PsiGame { on_match: boxed(on_match), on_differ: boxed(on_differ) },
            other => other,
        }
    }

    /// This effect with every `CardFilter::InAttackedServer` a card
    /// selection names written over by `InServer(server)` — what an ability
    /// that outlives the run means by "that server"
    /// (`Effect::WhenThisTurnEnds`). Walks the nestings a card uses:
    /// `Sequence`, `EffectIf`, `OfferPaidChoice`, `PresentChoice` and a
    /// selection's `then`.
    pub fn with_attacked_server(self, server: ServerId) -> Effect {
        let boxed = |effect: Box<Effect>| Box::new(effect.with_attacked_server(server));
        let all = |effects: Vec<Effect>| effects.into_iter().map(|e| e.with_attacked_server(server)).collect();
        match self {
            Effect::PromptChooseCards { side, source, filter, min, max, reveal, shuffle_after, destination, then, count, up_to } => Effect::PromptChooseCards {
                side,
                source,
                filter: filter.with_attacked_server(server),
                min,
                max,
                reveal,
                shuffle_after,
                destination,
                then: then.map(boxed),
                count,
                up_to,
            },
            Effect::Sequence(effects) => Effect::Sequence(all(effects)),
            Effect::PresentChoice { chooser, options, texts } => Effect::PresentChoice { chooser, options: all(options), texts },
            Effect::EffectIf { condition, effect } => Effect::EffectIf { condition, effect: boxed(effect) },
            Effect::OfferPaidChoice { side, cost, if_paid, if_declined, text, if_able } => {
                Effect::OfferPaidChoice { side, cost, if_paid: boxed(if_paid), if_declined: boxed(if_declined), text, if_able }
            }
            Effect::PsiGame { on_match, on_differ } => Effect::PsiGame { on_match: boxed(on_match), on_differ: boxed(on_differ) },
            other => other,
        }
    }

    /// This effect with "this server" in an install's `not_in_root_of`
    /// written as `server`, through what a selection's continuation can
    /// hold — the server the card that prints it is in, or was, written in
    /// while the resolution still knows it (`ability`'s `PromptChooseCards`).
    pub fn with_this_server(self, server: ServerId) -> Effect {
        let boxed = |effect: Box<Effect>| Box::new(effect.with_this_server(server));
        let all = |effects: Vec<Effect>| effects.into_iter().map(|e| e.with_this_server(server)).collect();
        match self {
            Effect::PromptInstallCorpCard { not_in_root_of: Some(ThisServer::This), origin_zone, ignore_costs, discount, then, remote_only, central_only, another_server, new_remote, rez, if_rezzed, if_installed, ignore_credit_costs, any_position } => {
                Effect::PromptInstallCorpCard {
                    not_in_root_of: Some(ThisServer::Server(server)),
                    origin_zone,
                    ignore_costs,
                    discount,
                    then,
                    remote_only,
                    central_only,
                    another_server,
                    new_remote,
                    rez,
                    if_rezzed,
                    if_installed,
                    ignore_credit_costs,
                    any_position,
                }
            }
            Effect::PromptChooseCards { side, source, filter, min, max, reveal, shuffle_after, destination, then, count, up_to } => {
                Effect::PromptChooseCards { side, source, filter, min, max, reveal, shuffle_after, destination, then: then.map(boxed), count, up_to }
            }
            Effect::Sequence(effects) => Effect::Sequence(all(effects)),
            Effect::PresentChoice { chooser, options, texts } => Effect::PresentChoice { chooser, options: all(options), texts },
            Effect::EffectIf { condition, effect } => Effect::EffectIf { condition, effect: boxed(effect) },
            other => other,
        }
    }

    /// Calls `f` on this effect and then on every effect nested inside it,
    /// depth-first in authoring order.
    ///
    /// The nesting positions are the whole list of places one `Effect` can
    /// contain another — `Sequence`, `EffectIf`, `OfferPaidChoice` (both
    /// branches), `PresentChoice`, `PromptChooseCards::then`,
    /// `PromptChooseServer::on_success`, `PromptInstallCorpCard::then` and
    /// `if_rezzed`, `Trace::on_success` and `SetAccessReplacement`.
    /// `PromptInstallCorpCard` was walked as a leaf until Reanimation
    /// Protocol gave it a second nested effect. Kept as an exhaustive `match` with an
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
            | Effect::LaterThisTurn { effect, .. }
            | Effect::ChooseNumber { then: effect, .. }
            | Effect::ChooseCardName { then: effect, .. }
            | Effect::Repeat { effect, .. }
            | Effect::ForEach { effect, .. }
            | Effect::SetAccessReplacement { effect, .. } => effect.for_each_effect(f),
            Effect::OfferPaidChoice { if_paid, if_declined, .. } => {
                if_paid.for_each_effect(f);
                if_declined.for_each_effect(f);
            }
            Effect::PsiGame { on_match, on_differ } => {
                on_match.for_each_effect(f);
                on_differ.for_each_effect(f);
            }
            Effect::PromptChooseCards { then: Some(effect), .. }
            | Effect::Access { then: Some(effect), .. }
            | Effect::IncreaseAboutToResolve { then: Some(effect), .. } => effect.for_each_effect(f),
            Effect::Access { then: None, .. } => {}
            Effect::PromptChooseServer { on_success, on_start, .. } => {
                for effect in [on_success, on_start].into_iter().flatten() {
                    effect.for_each_effect(f);
                }
            }
            Effect::PromptChooseCards { then: None, .. } | Effect::IncreaseAboutToResolve { then: None, .. } => {}
            Effect::RevealAtRandom { each: Some(effect), .. } => effect.for_each_effect(f),
            Effect::RevealAtRandom { each: None, .. } => {}
            Effect::PromptInstallCorpCard { then, if_rezzed, if_installed, .. } => {
                for effect in [then, if_rezzed, if_installed].into_iter().flatten() {
                    effect.for_each_effect(f);
                }
            }
            // Leaves: everything that holds no `Effect`.
            Effect::GainCredits(..)
            | Effect::DealDamage(..)
            | Effect::ModifyStrength { .. }
            | Effect::DrawCards(..)
            | Effect::EndTheRun
            | Effect::GiveTags(..)
            | Effect::RemoveTags(..)
            | Effect::GiveBadPublicity(..)
            | Effect::RemoveBadPublicity(..)
            | Effect::TrashCard(..)
            | Effect::RemoveFromGame(..)
            | Effect::BoostStrength { .. }
            | Effect::BreakSubroutines { .. }
            | Effect::BreakSubroutinesUnconditionally { .. }
            | Effect::AddAdditionalAccess { .. }
            | Effect::LoseCredits(..)
            | Effect::LoseClicks(..)
            | Effect::GainClicks(..)
            | Effect::InitiateRun(..)
            | Effect::Prevent(..)
            | Effect::AddCounters(..)
            | Effect::RemoveCounters(..)
            | Effect::RezInstalled { .. }
            | Effect::TakeAllCountersAsCredits(..)
            | Effect::TrashCurrentlyAccessedCard
            | Effect::DerezCard(..)
            | Effect::GainCreditsPerCounter { .. }
            | Effect::SwapInstalledIce(..)
            | Effect::InstallFromZoneIgnoringCost { .. }
            | Effect::InstallRunnerCardFromGrip
            | Effect::InstallRunnerCardFromZone { .. }
            | Effect::SetAsideFromTopUntil { .. }
            | Effect::InstallRunnerCardFromGripWithDiscount(..)
            | Effect::RedirectRunOnApproach(..)
            | Effect::ArmRunEndPrevention(..)
            | Effect::Sabotage(..)
            | Effect::Mill { .. }
            | Effect::HostCardOnThisCard(_)
            | Effect::BypassEncounteredIce
            | Effect::PurgeVirusCounters
            | Effect::TurnFaceupInArchives
            | Effect::TurnArchivesFacedown
            | Effect::ResolveSomeOf { .. }
            | Effect::LoseCreditsAmount(..)
            | Effect::FlipIdentity
            | Effect::SetIdentityCopy(_)
            | Effect::IdentifyMark
            | Effect::AddToDeck(_)
            | Effect::AddToHand
            | Effect::ShuffleIntoDeck(..)
            | Effect::RevealHand(_)
            | Effect::Remember { .. }
            | Effect::PlaceRunCredits { .. }
            | Effect::InstallProgramOnHost { .. }
            | Effect::AddToScoreAreaAsAgenda(_)
            | Effect::WinTheGame
            | Effect::TurnHostedFaceup
            | Effect::GainSubroutine { .. }
            | Effect::GainIceSubtype { .. }
            | Effect::LookAtTopOfDeck { .. }
            | Effect::HostRigCardOnInstall { .. }
            | Effect::DrawCardsAmount(..)
            | Effect::Prohibit { .. }
            | Effect::PlaceAdvancementCounters(..)
            | Effect::RemoveAdvancementCounters(..)
            | Effect::DealDamageAmount(..)
            | Effect::AddAdditionalAccessAmount { .. }
            | Effect::BoostStrengthAmount { .. }
            | Effect::MoveThisCardToRoot(..)
            | Effect::PromptMoveThisCardToAnotherRoot
            | Effect::MoveThisIceToOutermost
            | Effect::ForceEncounter
            | Effect::PlayOperation { .. }
            | Effect::ResolveSubroutineOfSelectedIce
            | Effect::LoseAbilities { .. }
            | Effect::LimitBreaks { .. }
            | Effect::ChooseServer { .. }
            | Effect::ReplaceSubroutines
            | Effect::SpendChosenServer
            | Effect::MoveRunToOutermost(..)
            | Effect::InstallAgendaFromRunnerScoreArea
            | Effect::SwapApproachedIceWithCard { .. }
            | Effect::AllottedClicksNextTurn(..)
            | Effect::EndActionPhase
            | Effect::Score
            | Effect::StealAccessedCard
            | Effect::Breach(_)
            | Effect::GainCreditsAmount(..) => {}
        }
    }

    /// Whether resolving this effect can end the run, at any nesting
    /// depth.
    ///
    /// Over `for_each_effect` rather than a `matches!` on the top level,
    /// because ICE rarely says it plainly: a bioroid's subroutine is an
    /// `OfferPaidChoice` whose `if_declined` ends the run, and a
    /// conditional one is an `EffectIf`. The walk's exhaustive match is
    /// what keeps that true as the DSL grows.
    ///
    /// Asked by `netrunner_bots::eval`, of an unrezzed ICE ahead of the
    /// Runner: "no rig card can break it" is not the same claim as "it
    /// stops you", and over 96-game legs the two came apart badly — an
    /// unbreakable ICE with a subroutine that ends the run stopped 0.904
    /// of the runs that reached it, one without stopped 0.077, and the
    /// evaluator was counting both alike (ROADMAP Phase 2 §5 items 38,
    /// 39). Prevention effects are deliberately not consulted: the
    /// question is what the subroutine *does*, not whether this
    /// particular Runner could answer it.
    pub fn can_end_the_run(&self) -> bool {
        let mut ends = false;
        self.for_each_effect(&mut |effect| ends |= matches!(effect, Effect::EndTheRun));
        ends
    }

    /// Whether this effect refers to or acts on the cards hosted on its
    /// source — what makes a cost that uninstalls the source set them aside
    /// rather than trash them (CR 9.5.5).
    pub fn acts_on_hosted_cards(&self) -> bool {
        let mut acts = false;
        self.for_each_effect(&mut |effect| {
            acts |= matches!(effect, Effect::TrashCard(CardTarget::HostedOnThisCard))
                || matches!(effect, Effect::ShuffleIntoDeck(zones) if zones.contains(&crate::dsl::CardZoneRef::HostedOnSource));
        });
        acts
    }

    /// The prevention this effect holds, if it holds one — what makes the
    /// ability printing it an interrupt. The first found: no card prints
    /// two.
    pub fn prevents(&self) -> Option<Preventable> {
        let mut found = None;
        self.for_each_effect(&mut |effect| {
            if let (None, Effect::Prevent(what)) = (&found, effect) {
                found = Some(what.clone());
            }
        });
        found
    }

    /// Whether this effect, anywhere inside it, rezzes a card *and pays
    /// for it* (`RezInstalled { pay_cost: true, .. }` — Mycoweb's "paying
    /// 2[credit] less"), which is when the card's own ways to pay for its
    /// rez (`CardDefinition::rez_alternatives`) can ask the Corp something
    /// by the payment's replay. A rez "ignoring all costs" pays nothing and
    /// asks nothing (CR 1.16.5c). Read by `payment::could_ask`.
    pub fn rezzes_paying(&self) -> bool {
        let mut found = false;
        self.for_each_effect(&mut |effect| {
            found |= matches!(effect, Effect::RezInstalled { pay_cost: true, .. });
        });
        found
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


/// `SetAsideFromTopUntil::deck`'s default: every card but Deep Dive reads
/// the Runner's own stack.
pub(crate) fn the_runner() -> crate::rules::Side {
    crate::rules::Side::Runner
}

pub(crate) fn is_the_runner(side: &crate::rules::Side) -> bool {
    *side == crate::rules::Side::Runner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_strength_and_break_subroutines_round_trip_through_json() {
        let boost = Effect::BoostStrength { amount: 1, duration: EffectDuration::Encounter };
        let boost_json = serde_json::to_string(&boost).unwrap();
        assert_eq!(boost_json, r#"{"BoostStrength":{"amount":1,"duration":"Encounter"}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&boost_json).unwrap(), boost);

        let turn_boost = Effect::BoostStrength { amount: 2, duration: EffectDuration::Turn };
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
        let trace = Effect::Trace { base: 3, on_success: Box::new(Effect::GiveTags(Amount::Fixed(1))) };
        let trace_json = serde_json::to_string(&trace).unwrap();
        assert_eq!(trace_json, r#"{"Trace":{"base":3,"on_success":{"GiveTags":{"Fixed":1}}}}"#);
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
            server: Some(ServerId::Hq),
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

    /// The bioroid shape is the one that matters: the ICE does not say
    /// "end the run", it offers a payment and ends the run if the Runner
    /// declines. A predicate that read only the top level would call that
    /// subroutine harmless.
    #[test]
    fn ending_the_run_is_found_wherever_a_subroutine_buries_it() {
        assert!(Effect::EndTheRun.can_end_the_run());
        assert!(
            Effect::OfferPaidChoice {
                side: Side::Runner,
                cost: crate::dsl::Cost::Credits(1),
                if_paid: Box::new(Effect::GainCredits(Side::Runner, 1)),
                if_declined: Box::new(Effect::EndTheRun),
                text: None,
                if_able: false,
            }
            .can_end_the_run()
        );
        assert!(Effect::Sequence(vec![Effect::GiveTags(Amount::Fixed(1)), Effect::EndTheRun]).can_end_the_run());
        assert!(!Effect::GiveTags(Amount::Fixed(1)).can_end_the_run());
        assert!(
            !Effect::Sequence(vec![Effect::GiveTags(Amount::Fixed(1)), Effect::DealDamage(DamageType::Net, 1)]).can_end_the_run(),
            "a tagging, damaging subroutine no breaker covers still lets the Runner through"
        );
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
                    text: None,
                    if_able: false,
                }),
            },
            Effect::PresentChoice {
                chooser: Side::Corp,
                options: vec![
                    Effect::Trace { base: 2, on_success: Box::new(Effect::GiveTags(Amount::Fixed(1))) },
                    Effect::SetAccessReplacement { server: Some(ServerId::Hq), effect: Box::new(Effect::DrawCards(Side::Runner, 1)), optional: false },
                ],
                texts: Vec::new(),
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

/// The server a card names as "this server" — the one the card that prints
/// it is in — as a card file writes it (`This`), and as it is written in
/// once a resolution knows which (`Server`). `PromptInstallCorpCard::
/// not_in_root_of`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThisServer {
    This,
    Server(ServerId),
}
