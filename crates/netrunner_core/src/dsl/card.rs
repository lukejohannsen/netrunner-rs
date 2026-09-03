use serde::{Deserialize, Serialize};

use crate::dsl::ability::{AbilityDef, EffectRequirement, InteractiveOnAccess, SubroutineDef};
use crate::dsl::cost::Cost;
use crate::dsl::effect::{Amount, Effect};
use crate::dsl::trigger::Trigger;
use crate::rules::Side;

/// `Ord` is derived so a set of ids has one canonical order regardless of
/// which `HashMap` it was pulled out of. `netrunner_bots::determinize`
/// depends on that: it seeds a shuffle over "every card that could be in
/// a hidden zone", and a seeded shuffle of an unordered input is not
/// reproducible.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CardId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IceType {
    Barrier,
    CodeGate,
    Sentry,
}

/// A conditional strength bonus layered on top of a card's stored
/// base/buff strength at query time — computed live by `rules::ability::
/// computed_strength`, never baked into `InstalledCard`/`InstalledRunnerCard`
/// itself, since the condition (icebreaker count, server type, hosted
/// advancement tokens) can change without any explicit strength-modifying
/// effect resolving. `CardDefinition`-level (shared across every instance
/// of the card), not per-installed-copy state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrengthModifier {
    /// Adds `0` (the payload) for each Runner-installed icebreaker
    /// (`dsl::zone::CardFilter::Icebreaker`'s heuristic — `CardType::Program`
    /// with `strength.is_some()`), including the card itself — e.g. Echelon.
    PerInstalledIcebreaker(i32),
    /// Adds `0` (the payload) while this ICE protects a remote server
    /// (`ServerId::Remote(_)`) — e.g. Palisade. Never applies to a central
    /// server (`Hq`/`RnD`/`Archives`).
    WhileProtectingRemote(i32),
    /// Adds `bonus` while this card carries at least `threshold` hosted
    /// advancement tokens — e.g. Pharos.
    WhileHostedAdvancementsAtLeast { threshold: u32, bonus: i32 },
    /// Adds the payload **per** hosted advancement token — Ice Wall's "+1
    /// strength for each hosted advancement counter." Not expressible as
    /// `WhileHostedAdvancementsAtLeast`, which is a threshold, not a rate.
    /// Corp-ICE-only like the two threshold variants: baked by
    /// `run::engine::build_run_ice` at encounter time (advancement cannot
    /// change mid-run — `AdvanceCard` is a Corp action-phase click).
    PerHostedAdvancement(i32),
    /// Adds the payload per `CardSubtype::Fracter` card in the Runner's
    /// heap — Rising Tide. Runner-side and live, like
    /// `PerInstalledIcebreaker`: the heap changes at any moment.
    PerFracterInHeap(i32),
}

/// A card subtype the engine dispatches a reactive identity trigger off of
/// (`Trigger::OnTransactionPlayed`/`OnVirusInstalled`) — distinct from
/// `CardType`, which is a card's primary type, not a tag on top of it. Kept
/// minimal, extend as new subtype-gated triggers are needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardSubtype {
    Transaction,
    Virus,
    /// An icebreaker that breaks barriers — Rising Tide's "+1 strength for
    /// each fracter in your heap" counts them. Authored on every fracter
    /// (Cleaver, Corroder, Marjanah, Principia, Rising Tide), the way
    /// `Virus` is authored on every virus program.
    Fracter,
    /// A Job resource — Open Market's hosted credits pay to install one.
    Job,
    /// A Connection resource — the other type Open Market's credits pay for.
    Connection,
    /// A Run event — the Runner's "run event" keyword. Read two ways:
    /// `EffectRequirement::RunEventActive` (Sang Kancil's cheaper boost
    /// while one is resolving) and `CardFilter::HasSubtype` (MuslihaT's
    /// "an icebreaker or a run event"). Authored on every run event, the
    /// way `Fracter` is on every fracter.
    Run,
    /// A singleton restriction, not a trigger-dispatch tag like the other
    /// two variants: `engine::install_hardware` rejects installing a second
    /// `Console`-subtyped Hardware while one is already in the Runner's rig
    /// (`RulesError::ConsoleLimitExceeded`) — e.g. Carnivore, Pennyshaver,
    /// Pantograph ("Limit 1 console per player").
    Console,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardType {
    Agenda,
    Asset,
    Operation,
    Ice(IceType),
    Hardware,
    Resource,
    Program,
    Event,
    Identity,
    /// NetrunnerDB's Upgrade type. Installs into a server's root slot
    /// (`InstallSlot::Root`) exactly like an Asset, but — unlike Asset/Agenda
    /// — may also root on a central server (Hq/RnD/Archives), not just a
    /// remote (see `legal_actions::install_card_candidates`).
    Upgrade,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggeredEffect {
    pub trigger: Trigger,
    pub effects: Vec<Effect>,
    /// A *soft* precondition: if unmet, `ability::process_card_triggers`
    /// silently skips this entry (no error, no `RulesError` surfaced) and
    /// leaves any per-turn tracking flag untouched. Used exclusively by
    /// passive identity-reactive triggers (`Trigger::OnInstall`/
    /// `OnSuccessfulRunOnHq` gated by `EffectRequirement::
    /// FirstInstallThisTurn`/`FirstSuccessfulHqRunThisTurn`) so a
    /// bonus-already-used-this-turn case never blocks the install/run that
    /// triggered it. Distinct from `CardDefinition::play_requirement`, which is a hard
    /// gate checked before a card can even be played at all. `None` for the
    /// common case of an unconditional trigger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<EffectRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardDefinition {
    pub id: CardId,
    pub title: String,
    pub side: Side,
    pub card_type: CardType,
    pub cost: u32,
    pub triggers: Vec<TriggeredEffect>,

    /// Costed / manually-activated abilities. Additive to the JSON schema —
    /// an absent `"abilities"` key parses to an empty `Vec`.
    #[serde(default)]
    pub abilities: Vec<AbilityDef>,

    /// Runner-paid cost to trash this card off the table. `None` for the
    /// common case of cards that aren't trashable this way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trash_cost: Option<u32>,

    /// Runner-paid cost to steal this Agenda, if any (e.g. NAPD Contract's
    /// "pay 4 credits to steal"). `None` is the common case — a free steal
    /// — and is exactly when `run::AccessPhase::PendingChoice::
    /// mandatory_steal` is set. `Some` only for `CardType::Agenda`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steal_cost: Option<Cost>,

    /// Advancement tokens required before an agenda can be scored/stolen.
    /// `Some` only for `CardType::Agenda`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advancement_requirement: Option<u32>,

    /// Agenda point value when scored/stolen. `Some` only for
    /// `CardType::Agenda`; `win::agenda_value` reads it from the registry
    /// (an earlier comment here promised a "data-driven replacement" for a
    /// hardcoded lookup that had already been replaced).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agenda_points: Option<u32>,

    /// Minimum deck size this card's identity/format imposes. Pure
    /// deckbuilding metadata — nothing in the runtime state machine reads
    /// this yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_deck_size: Option<u32>,

    /// Base strength printed on an ICE, or an Icebreaker's printed
    /// strength before any pumps. `Some` for `CardType::Ice(_)` (the data
    /// source for `RunIce::current_strength`) and for breaker-style
    /// `CardType::Program`s (the data source for
    /// `InstalledRunnerCard::base_strength`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<i32>,

    /// This ICE's subroutines, printed top-to-bottom. `Vec::new()` for the
    /// common non-ICE case — an absent `"subroutines"` key parses to an
    /// empty `Vec`, same as `"abilities"`. Meaningful content only for
    /// `CardType::Ice(_)`.
    #[serde(default)]
    pub subroutines: Vec<SubroutineDef>,

    /// An optional "may pay a cost to prevent an access-time effect"
    /// trigger — e.g. Fetal AI's "pay 2c to avoid 2 net damage." `None` for
    /// the common case (no such trigger). See `InteractiveOnAccess`'s doc
    /// comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive_on_access: Option<InteractiveOnAccess>,

    /// Subtypes this card carries beyond its primary `card_type` — currently
    /// only meaningful for `CardType::Operation` (`CardSubtype::Transaction`)
    /// and `CardType::Program` (`CardSubtype::Virus`), each read at a
    /// specific engine dispatch site rather than generically. `Vec::new()`
    /// for the common case of no subtype.
    #[serde(default)]
    pub subtypes: Vec<CardSubtype>,

    /// ◆ — unique. At most one copy may be in play per side: installing a
    /// second trashes the first (`engine::trash_earlier_unique_copy`).
    /// This is an install-time rule, not a deckbuilding one — three copies
    /// in a deck are legal. Joined from the NetrunnerDB catalog's
    /// `uniqueness` on `numeric_id`, never authored in card JSON, so it
    /// cannot drift from the printed card (ROADMAP Rules Audit T7).
    #[serde(default)]
    pub unique: bool,

    /// Printed link (the ⚡ value on a Runner identity), added to the
    /// Runner's total in a trace. Joined from the catalog's `base_link` like
    /// `unique`, never authored. `None` for every non-identity; `Some(0)`
    /// for the many identities that print no link. Seeded into
    /// `RunnerState::link_strength` at setup — only identities carry it in
    /// the implemented pool (*Kate "Mac" McCaffrey* is the one non-zero
    /// case), so a static seed suffices; hardware that adds link would need
    /// a derived value the way `memory` derives the budget.
    #[serde(default)]
    pub base_link: Option<u32>,

    /// A hard precondition gating `PlayerAction::PlayEvent`/`PlayOperation`
    /// for this specific card — checked *before* its click/credit cost is
    /// paid, same placement as `AbilityDef::requirement` in
    /// `engine::activate_ability`. `None` for the overwhelmingly common case
    /// of no play restriction. Distinct from `TriggeredEffect::requirement`,
    /// which gates a *reactive* trigger firing (silently, no error) rather
    /// than blocking the play itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_requirement: Option<EffectRequirement>,

    /// A cost paid *in addition to* the click and `cost` credits every
    /// Event/Operation play spends — a **Double**'s "As an additional cost
    /// to play this event, spend [click]" (Scrounge), authored as
    /// `Cost::Clicks(1)`. Paid by `engine::play_event` right after the
    /// play's own click, before `OnPlay` resolves; `legal_actions`' probe
    /// therefore drops the play when the extra cost cannot be met. A
    /// `Cost` rather than a click count so the same field carries the next
    /// printed additional cost without a second field. `None` for the
    /// common case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_play_cost: Option<Cost>,

    /// Recurring-credit pool size for an identity, refilled to this amount
    /// at the start of every Corp turn (`turn::enter_start_of_turn`) and
    /// spendable on Corp trace bids before the Corp's own wallet — e.g. NBN:
    /// Making News's 2 recurring credits. `None` for the common case (no
    /// pool). `Some` only meaningful on an identity (`CardType::Identity`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurring_credits: Option<u32>,

    /// Credit discount applied to the first Program/Hardware the Runner
    /// installs each turn — e.g. Kate "Mac" McCaffrey: Digital Tinker's -1.
    /// `None` for the common case (no discount). `Some` only meaningful on
    /// an identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_install_discount: Option<u32>,

    /// Memory units this Program reserves while installed — mirrors
    /// `strength`'s shape exactly. `Some` only meaningful on
    /// `CardType::Program`; every playable Program declares one, and `None`
    /// reserves nothing.
    ///
    /// **The sole authority on the cost.** `PlayerAction::InstallProgram`
    /// used to carry a caller-supplied `memory_cost` that had to match this
    /// exactly — and `legal_actions` always named `0`, so every Program
    /// with a cost was rejected by its own legality probe and could never
    /// be installed. The action no longer carries one. Read here by
    /// `engine::install_program` and summed by `rules::memory` over the
    /// whole rig.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_cost: Option<u32>,

    /// Additional memory units this card grants the Runner (e.g. a
    /// console's "+1[mu]"). `None` for the common case (no MU bonus). The
    /// opposite direction of `memory_cost` — what Hardware *grants* rather
    /// than what a Program *spends*, and `rules::memory` sums both over the
    /// rig to derive the Runner's free memory.
    ///
    /// It therefore applies for exactly as long as the granting card is
    /// installed. It used to be added once at `install_hardware` time and
    /// never removed, so a trashed console kept granting memory forever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bonus: Option<u32>,

    /// Additional max hand size this card grants once, permanently, when it
    /// takes effect — Hardware (`install_hardware` time), an Agenda
    /// (`Effect::GainMaxHandSize` fired from its own `Trigger::
    /// OnAgendaScored`), or an identity (`GameState::setup`, read once like
    /// `recurring_credits_max`). `None` for the common case (no bonus).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_hand_size_bonus: Option<u32>,

    /// A conditional discount off this Program's install cost — `(condition,
    /// amount)` — applied every time `condition` holds (no once-per-turn
    /// consumption, unlike `first_install_discount`), e.g. Carmen's
    /// "if you made a successful run this turn, this program costs 2
    /// credits less to install." `None` for the common case (no such
    /// discount). Distinct from and stacks independently with
    /// `first_install_discount` — see `engine::install_program`'s cost
    /// computation. `Some` only meaningful on `CardType::Program`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_cost_discount_if: Option<(EffectRequirement, u32)>,

    /// A *scaling* discount off this Program's install cost, resolved by
    /// `rules::ability::resolve_amount` at pricing time — Principia's "costs
    /// 1[c] less to install for each other installed icebreaker". A sibling
    /// of `install_cost_discount_if` rather than a generalisation of it:
    /// that field is a fixed amount behind a condition, this one is an
    /// amount with no condition, and the two stack (neither card has both).
    /// Priced before the card enters the rig, so an `Amount` that counts
    /// the rig — `InstalledIcebreakerCount` — already excludes the card
    /// itself, which is what "each *other* installed icebreaker" means.
    /// `None` for the common case. `Some` only meaningful on
    /// `CardType::Program`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_cost_discount_amount: Option<Amount>,

    /// Marks a Trojan Program that must be installed onto a piece of ICE
    /// (`PlayerAction::InstallProgramOnIce`) rather than into the normal
    /// Rig-install flow — e.g. Botulus, Tranquilizer. `false` for the
    /// common case (every other Program). `legal_actions` excludes a
    /// `true` card from the ordinary `InstallProgram` candidate list and
    /// offers `InstallProgramOnIce` instead, paired with every Corp
    /// installed ICE.
    #[serde(default)]
    pub installs_on_ice: bool,
    /// Cards hosted on this card may be played or installed through the
    /// ordinary grip actions — Bling's "you can play or install hosted
    /// cards as if they were in your grip". Seeded onto
    /// `rules::state::InstalledRunnerCard::hosted_cards_playable` at install
    /// so the registry-less action space can enumerate them. Madani's
    /// hosted programs are *not* this: they install through its own
    /// ability only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hosted_cards_playable_from_grip: bool,
    /// Added to the Corp's cost to rez every piece of ice while this card
    /// is in the Runner's rig — Fransofia Ward's "the rez cost of each
    /// piece of ice is increased by 1[c]". Summed over the rig by
    /// `engine::rez_ice` next to the run-scoped
    /// `run::RunState::ice_rez_cost_modifier` (Tread Lightly), which is the
    /// same modifier with a run's lifetime instead of an install's.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub ice_rez_cost_modifier: i32,

    /// ICE subtypes this Trojan grants its host while hosted — Chromatophores'
    /// "Host ice gains barrier, code gate, and sentry." Read where a
    /// breaker's `restrict_to` is matched against the encountered ICE
    /// (`Effect::BreakSubroutines`): the ICE's effective subtypes are its
    /// printed `IceType` plus every subtype granted by a rig card hosted on
    /// it. A per-card list rather than a flag because the rules concept is
    /// "gains these subtypes", and a later card may grant one. Empty for
    /// every non-Trojan card, and for every Trojan that grants nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_ice_gains_subtypes: Vec<IceType>,

    /// What this Hardware does for the icebreaker it is hosted on
    /// (`state::InstalledRunnerCard::hosted_on_program`) — GAMEDRAGON™ Pro.
    /// `None` for every card that is not hosted on a program. Hosting
    /// itself is a relation the card's own triggers establish with
    /// `Effect::HostRigCardOnInstall`; this field is only what the relation
    /// *means* while it holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosted_breaker_bonus: Option<HostedBreakerBonus>,

    /// What this card's hosted credits (`counters`, with `counter_kind:
    /// Credit`) may be spent on, beyond the card's own abilities — Azimat's
    /// "You can spend hosted credits to pay trash costs." The restriction
    /// vocabulary this engine deferred until a card needed it (ROADMAP
    /// Phase 1 §4): real recurring credits are purpose-restricted, so the
    /// pool cannot live in `ability::pay_cost`'s purpose-blind waterfall.
    /// Instead the site that knows the purpose drains matching pools first
    /// — `run::access::resolve_trash` for `TrashCosts`. One value today,
    /// built against one card; extend when the next card names a purpose.
    /// `None` for the common case (hosted credits spendable only by the
    /// card's own text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosted_credits_usable_for: Option<HostedCreditUse>,

    /// Trash this card the moment its hosted credits reach zero *through a
    /// pool drain* (`hosted_credits_usable_for`) — Open Market's "When it
    /// is empty, trash it." The card's own turn-start take handles the
    /// other way it empties with an `EffectIf`, as Telework Contract does;
    /// only the engine-side drain needs the flag, since no card text runs
    /// there. `false` for the common case.
    #[serde(default)]
    pub trash_when_empty: bool,

    /// Marks Bioroid-style ICE the Runner may break a subroutine on by
    /// losing a click instead of matching it with an icebreaker
    /// (`PlayerAction::BreakSubroutineWithClick`) — e.g. Ansel 1.0, Brân
    /// 1.0. `false` for the common case (every other ICE). Deliberately
    /// not a new `IceType` variant: "Bioroid" is orthogonal to the
    /// Barrier/CodeGate/Sentry axis `IceType`/`restrict_to` matching
    /// already models, no breaker in this card pool claims to break
    /// Bioroid-typed subroutines, and the real card text's "Bioroid"
    /// subtype is otherwise flavor — it's carried in `keywords` (e.g.
    /// `"Sentry - Bioroid - Destroyer"`) for display/metadata purposes
    /// only, same as any other keyword string.
    #[serde(default)]
    pub click_breakable: bool,

    /// A conditional strength bonus layered on top of this card's live
    /// base/buff strength at query time — see `StrengthModifier`'s doc
    /// comment and `rules::ability::computed_strength`. `None` for the
    /// common case (no such conditional bonus). Meaningful on `CardType::
    /// Ice(_)` and breaker-style `CardType::Program`s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength_modifier: Option<StrengthModifier>,

    /// Which kind of generic counter (`state::InstalledCard::counters`/
    /// `InstalledRunnerCard::counters`) this card's own text places/spends —
    /// e.g. a virus-counter Program, a Corp asset with power counters, or a
    /// hosted-credit card. Purely descriptive metadata for card authors;
    /// `Effect::AddCounters`/`RemoveCounters` operate on the raw `counters`
    /// field directly and don't themselves read or enforce this. `None` for
    /// the common case (no counters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_kind: Option<CounterKind>,

    /// NetrunnerDB's numeric card code, if this definition was sourced from
    /// or cross-referenced against the NetrunnerDB catalog
    /// (`cards::netrunnerdb`). `None` for the hand-authored baseline set,
    /// which predates having one. Indexed by `CardRegistry::by_numeric_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numeric_id: Option<crate::card::CardId>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<crate::card::Faction>,

    /// Purely descriptive, e.g. "Program: Icebreaker - Killer" — never read
    /// by engine logic; `card_type` remains the sole authoritative type
    /// field for gameplay branching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_line: Option<String>,

    /// Full NetrunnerDB subtype/trait list (descriptive). Distinct from and
    /// not conflated with `subtypes`, the small closed set the engine
    /// actually dispatches triggers on.
    #[serde(default)]
    pub keywords: Vec<String>,

    /// NetrunnerDB pack/set code, e.g. `"sg"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_code: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub influence_cost: Option<u32>,

    /// Per-card max-copies override (e.g. 1 for a restricted-list card).
    /// `None` falls back to the flat `MAX_COPIES_PER_CARD`/deckbuilding
    /// validator constants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck_limit: Option<u32>,
    /// An identity's printed influence budget, joined from the catalog —
    /// Ryō "Phoenix" Ōno prints 17 where every System Gateway identity
    /// prints 15. `None` means `format::DEFAULT_INFLUENCE_LIMIT` (a
    /// hand-authored identity, or a catalog `null` — see
    /// `unlimited_influence`, which is the flag that means "no budget").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub influence_limit: Option<u32>,
    /// The catalog's `influence_limit: null` — an identity with no
    /// influence budget at all. True only for the *Learn to Play* starter
    /// identities, whose preset decks mix every faction; the deckbuilding
    /// validator skips its influence check for such an identity instead of
    /// applying the flat `DEFAULT_INFLUENCE_LIMIT`. A flag rather than an
    /// `Option<u32>` limit because `None` would be ambiguous between
    /// "unset" and "unlimited", and no identity in the pool carries a
    /// printed limit other than the default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unlimited_influence: bool,

    /// Illustrator credit, sourced from NetrunnerDB's `illustrator` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,

    /// True placeholder — always `None` today; no fetch/derivation logic
    /// exists yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,

    /// True only for cards with real gameplay data (the hand-authored
    /// baseline set and any future hand-authored card). False for
    /// NetrunnerDB-sourced, catalog-only entries with no DSL data.
    /// `rules::deck::validate_deck` rejects any deck referencing a card
    /// where this is false.
    #[serde(default)]
    pub is_playable: bool,

    /// "Persistent" cards, whose `Trigger::OnRunEnded` ability still
    /// resolves for the remainder of a run in which the Runner trashed them
    /// — e.g. AMAZE Amusements' "(If the Runner trashes this card while
    /// accessing it, this ability still applies for the remainder of this
    /// run.)". Only meaningful on a Root-slot Corp install; trashing one
    /// during a run against its own server records it in
    /// `RunState::persistent_trashed_upgrades`, which
    /// `dispatcher::dispatch_event` then includes in `OnRunEnded`'s
    /// audience. `false` for every other card.
    #[serde(default)]
    pub persistent_after_trash: bool,
}

/// See `CardDefinition::counter_kind`'s doc comment. Kept minimal, extend as new
/// counter-kind-gated behavior is needed — mirrors `CardSubtype`'s own
/// "extend as needed" precedent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterKind {
    Virus,
    Power,
    Credit,
}

/// See `CardDefinition::hosted_breaker_bonus`. What a Hardware hosted on an
/// icebreaker does for its host — GAMEDRAGON™ Pro's "Host icebreaker gets +1
/// strength. Abilities that increase its strength last for the remainder of
/// the run (instead of any shorter duration)."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostedBreakerBonus {
    /// Added to the host's live strength by `rules::ability::
    /// computed_runner_strength` for as long as the hosting holds — a live
    /// term like `StrengthModifier`, never baked into the host's
    /// `base_strength`, so unhosting (or the hardware leaving play) drops
    /// it with nothing to unwind.
    #[serde(default)]
    pub strength: i32,
    /// When true, an `Effect::BoostStrength` the host resolves with
    /// `BoostDuration::Encounter` is recorded as `Run` instead — the boost
    /// outlives the encounter it was bought in. Only a lengthening: a
    /// `Turn` boost is already longer and stays `Turn`.
    #[serde(default)]
    pub boosts_last_the_run: bool,
}

/// See `CardDefinition::hosted_credits_usable_for` — the purpose a card's
/// hosted credits may be spent on. Deliberately a closed enum with one
/// value: each purpose names the one engine site that drains the pool, and
/// a value nothing drains would be a dead restriction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostedCreditUse {
    /// The trash cost of a card the Runner is accessing
    /// (`PlayerAction::TrashAccessedCard`) — Azimat.
    TrashCosts,
    /// The install cost of a Resource carrying any of `subtypes` — Open
    /// Market's "You can spend hosted credits to install connection and job
    /// resources". Drained by `engine::install_resource`.
    ResourceInstalls { subtypes: Vec<CardSubtype> },
}

/// Semantic checks `serde`'s structural `Deserialize` can't express on its
/// own (e.g. "an `Agenda` shouldn't have `subroutines`"). Not wired into
/// `CardRegistry::insert`/`from_cards`/`from_json` — several existing test
/// helpers across this workspace build intentionally sparse/synthetic
/// `CardDefinition`s (`blank_card` and similar) that would fail these checks. Only a
/// real card-authoring path (the filesystem loader, `cards::loader`) calls
/// this explicitly.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CardValidationError {
    #[error("Agenda {0:?} must not have subroutines")]
    AgendaHasSubroutines(CardId),
    #[error("Ice {0:?} must have a strength")]
    IceMissingStrength(CardId),
    #[error("card {0:?} of type {1:?} must not have a strength — only Ice and breaker-style Programs do")]
    UnexpectedStrength(CardId, CardType),
    #[error("Agenda {0:?} must have both agenda_points and advancement_requirement set")]
    AgendaMissingScoringFields(CardId),
    #[error("card {0:?} of type {1:?} must not have agenda_points — only Agenda does")]
    UnexpectedAgendaPoints(CardId, CardType),
}

/// Every field at its neutral value, matching what serde fills in for an
/// absent key — so `CardDefinition { .. }` literals in tests and fixtures can
/// spell out only the fields they care about via `..Default::default()`
/// instead of restating all ~36 and breaking every time one is added.
///
/// `side` and `card_type` have no meaningful neutral value; the placeholders
/// here exist only so `Default` can be implemented at all, and any caller
/// that cares must override them. (They are not `#[serde(default)]` fields —
/// deserializing a card still requires both.)
impl Default for CardDefinition {
    fn default() -> Self {
        Self {
            id: CardId(String::new()),
            title: String::new(),
            side: Side::Corp,
            card_type: CardType::Operation,
            cost: 0,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: None,
            agenda_points: None,
            min_deck_size: None,
            strength: None,
            subroutines: Vec::new(),
            interactive_on_access: None,
            subtypes: Vec::new(),
            unique: false,
            base_link: None,
            play_requirement: None,
            recurring_credits: None,
            first_install_discount: None,
            memory_cost: None,
            memory_bonus: None,
            max_hand_size_bonus: None,
            install_cost_discount_if: None,
            installs_on_ice: false,
            hosted_cards_playable_from_grip: false,
            ice_rez_cost_modifier: 0,
            host_ice_gains_subtypes: Vec::new(),
            hosted_breaker_bonus: None,
            hosted_credits_usable_for: None,
            trash_when_empty: false,
            influence_limit: None,
            additional_play_cost: None,
            install_cost_discount_amount: None,
            click_breakable: false,
            strength_modifier: None,
            counter_kind: None,
            numeric_id: None,
            faction: None,
            type_line: None,
            keywords: Vec::new(),
            set_code: None,
            influence_cost: None,
            deck_limit: None,
            unlimited_influence: false,
            artist: None,
            image_url: None,
            is_playable: false,
            persistent_after_trash: false,
        }
    }
}

impl CardDefinition {
    /// Checks the semantic rules `CardValidationError` documents. Structural
    /// well-formedness (right field types, valid enum tags) is already
    /// guaranteed by having deserialized successfully — this only catches
    /// combinations that parse fine but don't make sense as a real card.
    pub fn validate(&self) -> Result<(), CardValidationError> {
        let is_ice = matches!(self.card_type, CardType::Ice(_));
        let is_breaker_style_program = matches!(self.card_type, CardType::Program);
        let is_agenda = matches!(self.card_type, CardType::Agenda);

        if is_agenda && !self.subroutines.is_empty() {
            return Err(CardValidationError::AgendaHasSubroutines(self.id.clone()));
        }
        if is_ice && self.strength.is_none() {
            return Err(CardValidationError::IceMissingStrength(self.id.clone()));
        }
        if !is_ice && !is_breaker_style_program && self.strength.is_some() {
            return Err(CardValidationError::UnexpectedStrength(self.id.clone(), self.card_type.clone()));
        }
        if is_agenda && (self.agenda_points.is_none() || self.advancement_requirement.is_none()) {
            return Err(CardValidationError::AgendaMissingScoringFields(self.id.clone()));
        }
        // `agenda_points` (the win-condition value) is Agenda-exclusive, but
        // `advancement_requirement` alone is allowed on any card type — it's
        // also how a non-Agenda card (an Asset/Ice) declares "you can
        // advance this," which carries no scoring semantics of its own.
        if !is_agenda && self.agenda_points.is_some() {
            return Err(CardValidationError::UnexpectedAgendaPoints(self.id.clone(), self.card_type.clone()));
        }
        Ok(())
    }
}

/// `skip_serializing_if` for the `i32` modifier fields, which serde cannot
/// express inline.
fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEDGE_FUND_JSON: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/corp/hedge_fund.json"));
    const SURE_GAMBLE_JSON: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/data/runner/sure_gamble.json"
    ));
    const ICE_WALL_JSON: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/corp/ice_wall.json"));
    const CORRODER_JSON: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/runner/corroder.json"));

    #[test]
    fn parses_hedge_fund_from_json() {
        let card: CardDefinition = serde_json::from_str(HEDGE_FUND_JSON).expect("valid card JSON");

        assert_eq!(card.id, CardId("hedge_fund".to_string()));
        assert_eq!(card.title, "Hedge Fund");
        assert_eq!(card.side, Side::Corp);
        assert_eq!(card.card_type, CardType::Operation);
        assert_eq!(card.cost, 5);
        assert_eq!(
            card.triggers,
            vec![TriggeredEffect {
                trigger: Trigger::OnPlay,
                effects: vec![Effect::GainCredits(Side::Corp, 9)],
                requirement: None,
            }]
        );
        assert!(card.abilities.is_empty());
    }

    #[test]
    fn parses_sure_gamble_from_json() {
        let card: CardDefinition = serde_json::from_str(SURE_GAMBLE_JSON).expect("valid card JSON");

        assert_eq!(card.id, CardId("sure_gamble".to_string()));
        assert_eq!(card.title, "Sure Gamble");
        assert_eq!(card.side, Side::Runner);
        assert_eq!(card.card_type, CardType::Event);
        assert_eq!(card.cost, 5);
        assert_eq!(
            card.triggers,
            vec![TriggeredEffect {
                trigger: Trigger::OnPlay,
                effects: vec![Effect::GainCredits(Side::Runner, 9)],
                requirement: None,
            }]
        );
        assert!(card.abilities.is_empty());
    }

    #[test]
    fn parses_ice_wall_from_json() {
        let card: CardDefinition = serde_json::from_str(ICE_WALL_JSON).expect("valid card JSON");

        assert_eq!(card.id, CardId("ice_wall".to_string()));
        assert_eq!(card.title, "Ice Wall");
        assert_eq!(card.side, Side::Corp);
        assert_eq!(card.card_type, CardType::Ice(IceType::Barrier));
        assert_eq!(card.cost, 1);
        assert_eq!(card.strength, Some(1));
        assert_eq!(
            card.subroutines,
            vec![SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun }]
        );
        assert!(card.triggers.is_empty());
    }

    #[test]
    fn parses_corroder_from_json() {
        use crate::dsl::cost::Cost;
        use crate::dsl::effect::{BoostDuration, SubroutineBreakCount};

        let card: CardDefinition = serde_json::from_str(CORRODER_JSON).expect("valid card JSON");

        assert_eq!(card.id, CardId("corroder".to_string()));
        assert_eq!(card.title, "Corroder");
        assert_eq!(card.side, Side::Runner);
        assert_eq!(card.card_type, CardType::Program);
        assert_eq!(card.cost, 2);
        assert_eq!(card.strength, Some(2));
        assert!(card.triggers.is_empty());
        assert_eq!(
            card.abilities,
            vec![
                AbilityDef {
                    trigger: Trigger::Paid,
                    cost: Some(Cost::Credits(1)),
                    // Every icebreaker ability carries this — real
                    // Netrunner only permits them while encountering ICE.
                    requirement: Some(EffectRequirement::DuringEncounter),
                    effect: Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter },
                    cost_discount_if: None,
                },
                AbilityDef {
                    trigger: Trigger::Paid,
                    cost: Some(Cost::Credits(1)),
                    // Every icebreaker ability carries this — real
                    // Netrunner only permits them while encountering ICE.
                    requirement: Some(EffectRequirement::DuringEncounter),
                    effect: Effect::BreakSubroutines {
                        count: SubroutineBreakCount::Fixed(1),
                        restrict_to: Some(IceType::Barrier),
                    },
                    cost_discount_if: None,
                },
            ]
        );
    }

    fn blank_card(id: &str, card_type: CardType) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type,
            is_playable: true,
            ..Default::default()
        }
    }

    #[test]
    fn well_formed_ice_wall_passes_validation() {
        let card: CardDefinition = serde_json::from_str(ICE_WALL_JSON).expect("valid card JSON");
        assert_eq!(card.validate(), Ok(()));
    }

    #[test]
    fn well_formed_hedge_fund_passes_validation() {
        let card: CardDefinition = serde_json::from_str(HEDGE_FUND_JSON).expect("valid card JSON");
        assert_eq!(card.validate(), Ok(()));
    }

    #[test]
    fn agenda_with_subroutines_fails_validation() {
        let mut card = blank_card("bad_agenda", CardType::Agenda);
        card.agenda_points = Some(3);
        card.advancement_requirement = Some(4);
        card.subroutines = vec![SubroutineDef { text: "oops".to_string(), effect: Effect::EndTheRun }];

        assert_eq!(card.validate(), Err(CardValidationError::AgendaHasSubroutines(CardId("bad_agenda".to_string()))));
    }

    #[test]
    fn ice_missing_strength_fails_validation() {
        let card = blank_card("bad_ice", CardType::Ice(IceType::Barrier));
        assert_eq!(card.validate(), Err(CardValidationError::IceMissingStrength(CardId("bad_ice".to_string()))));
    }

    #[test]
    fn non_ice_non_program_with_strength_fails_validation() {
        let mut card = blank_card("bad_asset", CardType::Asset);
        card.strength = Some(2);

        assert_eq!(
            card.validate(),
            Err(CardValidationError::UnexpectedStrength(CardId("bad_asset".to_string()), CardType::Asset))
        );
    }

    #[test]
    fn breaker_style_program_with_strength_passes_validation() {
        let mut card = blank_card("corroder", CardType::Program);
        card.strength = Some(2);
        assert_eq!(card.validate(), Ok(()));
    }

    #[test]
    fn agenda_missing_scoring_fields_fails_validation() {
        let card = blank_card("bad_agenda", CardType::Agenda);
        assert_eq!(
            card.validate(),
            Err(CardValidationError::AgendaMissingScoringFields(CardId("bad_agenda".to_string())))
        );
    }

    #[test]
    fn non_agenda_with_agenda_points_fails_validation() {
        let mut card = blank_card("bad_asset", CardType::Asset);
        card.agenda_points = Some(1);

        assert_eq!(
            card.validate(),
            Err(CardValidationError::UnexpectedAgendaPoints(CardId("bad_asset".to_string()), CardType::Asset))
        );
    }

    /// `advancement_requirement` alone (no `agenda_points`) is legal on any
    /// card type — it's how a non-Agenda card declares "you can advance
    /// this" (e.g. System Gateway's Urtica Cipher/Clearinghouse/Pharos),
    /// which carries no agenda-scoring semantics.
    #[test]
    fn non_agenda_with_advancement_requirement_but_no_agenda_points_passes_validation() {
        let mut card = blank_card("advanceable_asset", CardType::Asset);
        card.advancement_requirement = Some(3);

        assert_eq!(card.validate(), Ok(()));
    }
}
