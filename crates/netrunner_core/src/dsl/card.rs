use serde::{Deserialize, Serialize};

use crate::dsl::ability::{AbilityDef, EffectRequirement, InteractiveOnAccess, SubroutineDef};
use crate::dsl::continuous::{ContinuousEffect, ContinuousKind, Scope};
use crate::dsl::cost::Cost;
use crate::dsl::effect::{Effect, EffectDuration};
use crate::dsl::trigger::{EventFilter, Subject, Trigger, TriggerAbout};
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

/// A printed subtype some card's text reads — first the two a reactive
/// identity filters its trigger by (`EventFilter::Card(HasSubtype(..))`:
/// Building a Better World's transactions, Noise's viruses) — distinct from
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
    /// A Trojan program — one that installs hosted on a piece of ice
    /// (`CardDefinition::installs_on_ice` is the mechanic; this is the
    /// printed tag). Bumi 1.0's "trash 1 installed trojan program" reads it
    /// through `CardFilter::HasSubtype`. Authored on every trojan (Botulus,
    /// Chromatophores, Tranquilizer).
    Trojan,
    /// A Bioroid — the printed subtype on Haas-Bioroid's click-breakable
    /// ice and its Academic upgrades. Read by LEO Construction: Labor
    /// Solutions' "trash 1 rezzed bioroid card in the root of or
    /// protecting the attacked server" through `CardFilter::HasSubtype`.
    /// Orthogonal to `CardDefinition::click_breakable`, which is the
    /// *mechanic* the bioroid ice share; Mercia B4LL4RD is a bioroid with
    /// no subroutines to click through. Authored on every bioroid (Ansel
    /// 1.0, Brân 1.0, Bumi 1.0, Mercia B4LL4RD).
    Bioroid,
    /// A Region upgrade — "Limit 1 region per server". A singleton
    /// restriction like `Console`, enforced by `engine::place_corp_card`
    /// (`RulesError::RegionLimitExceeded`) and pruned out of
    /// `engine::corp_install_destinations`' offer: a root already holding
    /// a rezzed or unrezzed region cannot take another. Mahkota Langit
    /// Grid is the pool's only region.
    Region,
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
    /// Which occurrences of `trigger` this hears — see `Subject`. Required
    /// exactly when `Trigger::names_a_subject`, absent otherwise; there is
    /// no default, because both wrong defaults are quiet (an agenda that
    /// reacts to every score; an identity that reacts to none), and
    /// `CardDefinition::validate` is what holds every card file to it —
    /// the embedded pool in `every_embedded_card_parses_and_validates`, an
    /// external directory at load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<Subject>,
    /// Which occurrences this means, by what they are about — "on HQ", "a
    /// virus program". Part of the trigger condition, evaluated in the
    /// listener scan; see `EventFilter` for why that is not `requirement`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<EventFilter>,
    /// The effects act on the card the moment is about, not on this one:
    /// Cookbook's "whenever you install a virus program, you may place 1
    /// virus counter on **it**". The requirement is still checked, and any
    /// once-per-turn spent, as this card. It was a rule in the dispatcher —
    /// "an *installed* card hearing `OnVirusInstalled` acts on the virus,
    /// the identity does not" — which is Cookbook's text and Noise's, kept
    /// in Rust under a trigger's name.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub acts_on_subject: bool,
    /// "The **first time each turn** …": this hears only the turn's first
    /// occurrence of what it listens for — its `trigger`, narrowed by its
    /// `when`, its controller's where the trigger is phrased about its
    /// controller (`rules::turn_log::Occurrences`). Part of the trigger
    /// *condition*, like `when`, and judged in the listener scan against a
    /// count that already includes the occurrence, so a second one was
    /// never a listener.
    ///
    /// **A fact about the turn, not a use of the card.** It was spelled
    /// `requirement: OncePerTurn`, which is a use limit: a card installed
    /// after the turn's first occurrence found its use unspent and fired
    /// on the second, and so did one whose first trigger stood down. It is
    /// not an `Amount::TimesThisTurn` compared with 1 either — that is
    /// asked at resolution, after a nested event may have counted a
    /// second, and a card file that writes a 1 can write a 0.
    ///
    /// A card's `first_each_turn` entries are one printed ability in as
    /// many entries as it has triggers ("an agenda is scored **or**
    /// stolen"), and share one count.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_each_turn: bool,
    /// The printed sentence this trigger implements, quoted from the
    /// card, when a card author has linked it; optional and ungated —
    /// see `AbilityDef::text` for the linked-clause idea.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub effects: Vec<Effect>,
    /// The intervening *if*: asked when the trigger resolves, against the
    /// state at that moment ("if the Runner is tagged"). Unmet, the entry
    /// is skipped silently — no error, and nothing spent — so a bonus that
    /// does not apply never blocks the install or run that offered it.
    /// A `OncePerTurn` here is spent once the effects have resolved
    /// (`ability::consume_requirement`). **Not where "the first time each
    /// turn" goes** — that is `first_each_turn`, part of the condition.
    /// Distinct from `CardDefinition::play_requirement`, which is a hard
    /// gate checked before a card can be played at all. `None` for the
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
    /// strength before any pumps. `Some` for `CardType::Ice(_)` (the number
    /// `continuous::ice_strength` starts from) and for breaker-style
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

    /// ◆ — unique. At most one copy may be *active* per side: when a second
    /// becomes active the older is trashed (`rules::checkpoint`). A facedown
    /// Corp card is not active, so two may sit side by side until one is
    /// rezzed. This is a rule of play, not a deckbuilding one — three copies
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

    /// "N[recurring-credit]" as the card prints it — Azimat's and Mahkota
    /// Langit Grid's 2, NBN: Making News's 2. Comprehensive Rules 1.10.5a:
    /// "When this card becomes active, place N credits on it. Before
    /// abilities meet their trigger conditions for your turn beginning, if
    /// there are fewer than N credits on this card, place credits on it
    /// until there are N credits on it." The credits are the card's hosted
    /// `counters` (`counter_kind: Credit`; an identity's are
    /// `CorpState::identity_counters`), and what they may be spent on is
    /// `pays_for`.
    ///
    /// **A declaration, never a trigger.** It was an `OnInstall`/`OnRez` and
    /// an `OnTurnStart` trigger on each card, resolving an
    /// `Effect::RefillCountersTo` — so the refill happened *as* the turn
    /// began, among the abilities the rule puts it before, and a player with
    /// another turn-start trigger was asked to order it (`dispatcher::
    /// offer_trigger_order` counts any trigger with no failing requirement).
    /// `rules::payment::refill` is a step of the turn now, and
    /// `payment::place_recurring` the step of becoming active (1.10.5b). On
    /// an identity it used to be a second mechanism altogether: a pair of
    /// fields on `CorpState`, spendable only because `pay_cost` checked
    /// whether a trace was active. `None` for the common case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurring_credits: Option<u32>,


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
    /// Dividends N — "when you score this agenda, place N agenda counters
    /// on it for each excess advancement counter" (Off the Books). Read
    /// once, by `engine::score_agenda`, which puts the counters on the
    /// `ScoredAgenda` it creates. A card field rather than an
    /// `OnAgendaScored` effect because the excess is known only at the
    /// moment of scoring, before the installed copy (and its tokens) is
    /// gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dividends: Option<u32>,
    /// This operation may be played from Archives, and is removed from the
    /// game after it resolves when it was — Petty Cash's "[click]: Play
    /// this operation from Archives. After it resolves, remove it from the
    /// game." Playing from Archives is the ordinary `PlayOperation` action
    /// over `CorpState::playable_hand`, so it costs the click any play
    /// costs; the card's own text refunds it. Not a paid ability: an
    /// ability needs an install to target, and a card in Archives has
    /// none.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub playable_from_archives: bool,



    /// What this card's hosted credits (`counters`, with `counter_kind:
    /// Credit`) may be spent on, beyond the card's own abilities — the words
    /// a card prints after "You can spend hosted credits to": Azimat's "pay
    /// trash costs", Open Market's "install connection and job resources".
    /// A list because a card may name two (Cyberfeeder). `rules::payment`
    /// is the one scan that reads it: a payment states its `Purpose`, and
    /// every active card whose words cover that purpose is a source for it
    /// — counted by the affordability question and drained by the payment,
    /// which therefore cannot disagree. It was `hosted_credits_usable_for`,
    /// one `Option` drained by whichever handler knew the purpose, each
    /// with its own affordability sum beside it; three of those sums forgot
    /// a pool the payment then took. Empty for the common case (hosted
    /// credits spendable only by the card's own text).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pays_for: Vec<PaysFor>,

    /// Trash this card the moment its hosted credits reach zero *through a
    /// payment* (`pays_for`, drained by `rules::payment`) — Open Market's "When it
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

    /// The card's printed rules text, for a client to show a person:
    /// NetrunnerDB's `text` with its HTML removed and its line breaks and
    /// `[credit]`-style symbols kept (`cards::netrunnerdb::strip_markup`),
    /// not the API's `stripped_text`, which spells the symbols out and
    /// joins the lines. **Never read by the engine**: the rules a card runs
    /// on are `triggers`, `abilities` and `subroutines`, and this is the
    /// sentence they were written from. Joined from the catalog like
    /// `artist`, so card files do not restate it and cannot drift from
    /// what was printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printed_text: Option<String>,

    /// Flavour text, for the same reader. NetrunnerDB's `flavor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flavor: Option<String>,

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


    /// This identity lets its Corp install agendas faceup — BANGUN: When
    /// Disaster Strikes. Modelled as permission to *rez* an installed
    /// agenda (`engine::rez_ice` rejects an agenda otherwise), because rez
    /// is already the engine's one "flip a Corp install faceup" action and
    /// it is already optional, where an install-time flag would have to be
    /// carried on `PlayerAction::InstallCard` and widen the `ActionSpace`
    /// install segment for one identity. The difference is timing only:
    /// this Corp may wait and flip the agenda later. An agenda's printed
    /// `cost` is 0, so nothing is paid either way, and the reminder text's
    /// "this does not make their abilities active" holds because no agenda
    /// in the pool declares an installed-and-rezzed ability.
    #[serde(default)]
    pub may_install_agendas_faceup: bool,

    /// Ways this card may be rezzed beyond simply paying for it — Biawak's
    /// "you can forfeit 1 agenda as you rez this ice to pay for 10[c] of
    /// its rez cost" and Plutus's "as an additional cost to rez this asset,
    /// forfeit 1 agenda or reveal and trash 3 cards from HQ".
    ///
    /// `engine::rez_ice` keeps the alternatives whose `requirement` holds
    /// **and whose resulting price the Corp can meet** (see
    /// `engine::rez_price` for why the second half is not optional),
    /// resolves the only one directly, and offers a `PresentChoice` when
    /// more than one survives. An empty list is the ordinary rez; a
    /// non-empty list with nothing available refuses the rez
    /// (`RulesError::NoAvailableRezAlternative`), which is how Plutus's
    /// *additional* cost differs from Biawak's *optional* discount: Biawak
    /// lists a plain no-op alternative alongside its forfeit, and Plutus
    /// does not.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rez_alternatives: Vec<RezAlternative>,
    /// What this card does for as long as it is active — "+1[mu]", "costs
    /// 2[c] less to install if…", "host ice gains barrier". **A standing
    /// effect goes here, never in a field of its own**: see
    /// `dsl::continuous` for the nine fields and six `StrengthModifier`
    /// variants this list replaced, and `rules::continuous` for the one
    /// scan that reads it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub continuous: Vec<ContinuousEffect>,
}

/// One way of paying to rez a card — see
/// [`CardDefinition::rez_alternatives`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RezAlternative {
    /// Gate on the board: Biawak's and Plutus's forfeits need an agenda in
    /// the score area, Plutus's other half needs three cards in HQ.
    /// Unavailable alternatives are never offered, so a player cannot pick
    /// one that would resolve to nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<crate::dsl::EffectRequirement>,
    /// Paid before the rez, as the card being rezzed. May park a decision
    /// of its own (Plutus's "trash 3 cards from HQ" is a selection): the
    /// rez is the tail of a `Sequence`, which resumes after each parked
    /// step.
    pub pay: crate::dsl::Effect,
    /// Credits knocked off the rez cost by taking this alternative —
    /// Biawak's 10. `0` for an additional cost that buys no discount.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub discount: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
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

/// See `CardDefinition::pays_for` — what a card's hosted credits may be
/// spent on, as the card prints it. **A word about the payment, never a
/// site:** `rules::payment::covers` is the one place a word is matched
/// against what is being paid for (`payment::Purpose`), so a new word is a
/// variant here and an arm there, and no handler learns about it. Only what
/// a card in the pool prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaysFor {
    /// The trash cost of a card the Runner is accessing
    /// (`PlayerAction::TrashAccessedCard`) — Azimat.
    TrashCosts,
    /// The install cost of a card the filter admits — Open Market's "You
    /// can spend hosted credits to install connection and job resources".
    /// Definition-level filters only: the card being installed is not on
    /// the table yet. It was `ResourceInstalls { subtypes }`, drained by
    /// the click install alone, so a connection installed by a card's text
    /// could not be paid for from the market.
    Installing(crate::dsl::CardFilter),
    /// The rez cost of an asset in the root of, or a piece of ice
    /// protecting, the server this card is installed in — Mahkota Langit
    /// Grid's "You can spend hosted credits to rez assets in the root of
    /// this server and ice protecting this server". Upgrades in the root
    /// are deliberately not covered — the printed text names assets and
    /// ice.
    RezzingInThisServer,
    /// Either player's bid in a trace attempt — NBN: Making News's "Use
    /// these credits during trace attempts". It was not a word at all: the
    /// identity's credits were a source whenever the Corp paid anything
    /// while a trace was active, which was exact only because nothing else
    /// can be paid for then.
    TraceAttempts,
    /// The cost of a paid ability on an icebreaker — Cyberfeeder's and The
    /// Toolbox's "Use these credits to pay for using icebreakers". Pumps as
    /// well as breaks: "using" an icebreaker is using any of its abilities.
    /// An icebreaker is what `CardFilter::Icebreaker` says one is.
    UsingIcebreakers,
    /// The credit cost of the basic action that removes a tag — Crash
    /// Space's "You can spend hosted credits to take the basic action to
    /// remove 1 tag". Not a card's text removing tags, which costs nothing
    /// a pool could pay.
    RemovingTags,
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
    #[error("card {0:?} says what its hosted credits pay for but hosts no credits (`counter_kind: Credit`), or is an event or operation, which hosts nothing")]
    PaysForWithoutHostedCredits(CardId),
    #[error("card {0:?} prints recurring credits but hosts no credits (`counter_kind: Credit`) or says nothing they may be spent on (`pays_for`)")]
    RecurringCreditsWithNowhereToGo(CardId),
    #[error("Runner identity {0:?} prints recurring credits, and only the Corp's identity can host credits in this engine (`CorpState::identity_counters`)")]
    RunnerIdentityHostsNothing(CardId),
    #[error("card {0:?} trashes itself when a payment empties it (`trash_when_empty`) but no payment can take its credits (`pays_for` is empty)")]
    TrashWhenEmptyWithNothingToEmptyIt(CardId),
    #[error("card {0:?} reads `Amount::ChosenNumber` outside the `then` of an `Effect::ChooseNumber`, where no number has been chosen and it is 0")]
    ChosenNumberNobodyChose(CardId),
    #[error("Ice {0:?} must have a strength")]
    IceMissingStrength(CardId),
    #[error("card {0:?} of type {1:?} must not have a strength — only Ice and breaker-style Programs do")]
    UnexpectedStrength(CardId, CardType),
    #[error("Agenda {0:?} must have both agenda_points and advancement_requirement set")]
    AgendaMissingScoringFields(CardId),
    #[error("card {0:?} of type {1:?} must not have agenda_points — only Agenda does")]
    UnexpectedAgendaPoints(CardId, CardType),
    #[error("card {0:?}: a {1:?} trigger must say whether it hears `This` occurrence or `Any` (`subject`)")]
    TriggerMissingSubject(CardId, Trigger),
    #[error("card {0:?}: a {1:?} trigger is not about a card or a server, so it cannot name a `subject`")]
    TriggerSubjectWithNothingToName(CardId, Trigger),
    #[error("card {0:?}: a {1:?} trigger's `when` filters on something its moment is not about (a card filter needs a trigger about a card, a server filter one about a server)")]
    TriggerFilterOfTheWrongKind(CardId, Trigger),
    #[error("card {0:?}: a {1:?} trigger is not about a card, so its effects cannot act on one (`acts_on_subject`)")]
    TriggerActsOnNoCard(CardId, Trigger),
    #[error("card {0:?}: \"the first time each turn\" (`first_each_turn`) does not fit — {1}")]
    FirstTimeDoesNotFit(CardId, String),
    #[error("card {0:?}: `OncePerTurn` does not fit — {1}")]
    OncePerTurnDoesNotFit(CardId, &'static str),
    #[error("card {0:?}: a continuous {1} effect does not fit — {2}")]
    ContinuousEffectDoesNotFit(CardId, &'static str, &'static str),
    #[error("card {0:?}: a prohibition lasts a run or a turn — nothing prints one for an encounter, and the guards that ask are not asked during one")]
    ProhibitionForAnEncounter(CardId),
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
            memory_cost: None,
            installs_on_ice: false,
            hosted_cards_playable_from_grip: false,
            dividends: None,
            playable_from_archives: false,
            pays_for: Vec::new(),
            trash_when_empty: false,
            may_install_agendas_faceup: false,
            rez_alternatives: Vec::new(),
            influence_limit: None,
            additional_play_cost: None,
            click_breakable: false,
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
            printed_text: None,
            flavor: None,
            image_url: None,
            is_playable: false,
            persistent_after_trash: false,
            continuous: Vec::new(),
        }
    }
}

impl CardDefinition {
    /// Checks the semantic rules `CardValidationError` documents. Structural
    /// well-formedness (right field types, valid enum tags) is already
    /// guaranteed by having deserialized successfully — this only catches
    /// combinations that parse fine but don't make sense as a real card.
    fn first_time_misfit(&self, why: String) -> CardValidationError {
        CardValidationError::FirstTimeDoesNotFit(self.id.clone(), why)
    }

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
        // `rules::payment` reads `pays_for` off active *installed* cards and
        // spends their `counters` as credits: a word on a card that hosts
        // none, or is never installed, is a pool nothing can reach.
        let hosts_credits = self.counter_kind == Some(CounterKind::Credit) && !matches!(self.card_type, CardType::Event | CardType::Operation);
        if !self.pays_for.is_empty() && !hosts_credits {
            return Err(CardValidationError::PaysForWithoutHostedCredits(self.id.clone()));
        }
        if self.recurring_credits.is_some() && (self.counter_kind != Some(CounterKind::Credit) || self.pays_for.is_empty()) {
            return Err(CardValidationError::RecurringCreditsWithNowhereToGo(self.id.clone()));
        }
        // No Runner identity in the pool prints them, and there is nowhere
        // to put them: refused, rather than a pool that silently never fills.
        if self.recurring_credits.is_some() && self.card_type == CardType::Identity && self.side == Side::Runner {
            return Err(CardValidationError::RunnerIdentityHostsNothing(self.id.clone()));
        }
        if self.trash_when_empty && self.pays_for.is_empty() {
            return Err(CardValidationError::TrashWhenEmptyWithNothingToEmptyIt(self.id.clone()));
        }
        // See `TriggeredEffect::subject`: no default, because either one is
        // wrong quietly. A Rust fixture that skips `validate` gets the
        // lenient reading `rules::listeners` documents; a card file never does.
        for triggered in &self.triggers {
            match (triggered.trigger.names_a_subject(), triggered.subject) {
                (true, None) => return Err(CardValidationError::TriggerMissingSubject(self.id.clone(), triggered.trigger)),
                (false, Some(_)) => return Err(CardValidationError::TriggerSubjectWithNothingToName(self.id.clone(), triggered.trigger)),
                _ => {}
            }
            // A filter of the wrong kind parses and then never passes, and
            // "act on it" with no card to be "it" resolves as the card
            // itself: both are a card silently doing something else.
            let about = triggered.trigger.about();
            let filter_fits = match &triggered.when {
                None => true,
                Some(EventFilter::Card(_) | EventFilter::InstalledCard(_)) => about == TriggerAbout::Card,
                Some(EventFilter::Server(_)) => about == TriggerAbout::Server,
                Some(EventFilter::Damage(_)) => about == TriggerAbout::Damage,
            };
            if !filter_fits {
                return Err(CardValidationError::TriggerFilterOfTheWrongKind(self.id.clone(), triggered.trigger));
            }
            if triggered.acts_on_subject && about != TriggerAbout::Card {
                return Err(CardValidationError::TriggerActsOnNoCard(self.id.clone(), triggered.trigger));
            }
            // `requirement: AmountAtLeast(TimesThisTurn(own trigger), …)` is
            // "the first time" spelled as an intervening if: asked when the
            // trigger resolves, after a nested event may have counted a
            // second, and with a number a card file can get wrong.
            if triggered.requirement.as_ref().is_some_and(|requirement| requirement.counts_this_turn(triggered.trigger)) {
                return Err(self.first_time_misfit(format!("a {:?} trigger's requirement counts {:?} this turn; say `first_each_turn`", triggered.trigger, triggered.trigger)));
            }
        }
        // "The first time each turn" is read off the turn's count of what
        // the entry listens for, so each refusal here is a card that would
        // load and then count the wrong thing, or nothing.
        let first_time: Vec<&TriggeredEffect> = self.triggers.iter().filter(|triggered| triggered.first_each_turn).collect();
        for triggered in &first_time {
            if let Err(why) = crate::rules::turn_log::Occurrences::meant_by(triggered.trigger, triggered.when.as_ref(), self.side) {
                return Err(self.first_time_misfit(why));
            }
            if triggered.subject == Some(Subject::This) {
                return Err(self.first_time_misfit("\"this\" happens to a card once; the first time each turn is about `Any`".to_string()));
            }
            if triggered.requirement.as_ref().is_some_and(EffectRequirement::mentions_once_per_turn) {
                return Err(self.first_time_misfit("`OncePerTurn` is a use limit on the card, and the first time each turn is a fact about the turn; a card prints one or the other".to_string()));
            }
        }
        // A card's first-time entries share one count, so two triggers one
        // event is an occurrence of would count that event twice.
        for (one, other) in [(Trigger::OnPlay, Trigger::OnOperationPlayed), (Trigger::OnInstall, Trigger::OnCardInstalled)] {
            if first_time.iter().any(|triggered| triggered.trigger == one) && first_time.iter().any(|triggered| triggered.trigger == other) {
                return Err(self.first_time_misfit(format!("one event is both a {one:?} and a {other:?}, and the card's first-time entries share a count")));
            }
        }
        // A use limit needs something that uses the card. A trigger that
        // fires and a paid ability that resolves do; nothing that reads a
        // standing effect does, so a `OncePerTurn` there would never be
        // spent. And the key is the card and which copy, so two once-per-
        // turn abilities on one card would share a use — no card prints two.
        if self.continuous.iter().any(|effect| effect.condition.as_ref().is_some_and(EffectRequirement::mentions_once_per_turn)) {
            return Err(CardValidationError::OncePerTurnDoesNotFit(self.id.clone(), "a continuous effect is read, never used, so its `while` cannot be a `OncePerTurn`"));
        }
        let once_per_turn = self.triggers.iter().filter_map(|triggered| triggered.requirement.as_ref()).chain(self.abilities.iter().filter_map(|ability| ability.requirement.as_ref()));
        if once_per_turn.filter(|requirement| requirement.mentions_once_per_turn()).count() > 1 {
            return Err(CardValidationError::OncePerTurnDoesNotFit(self.id.clone(), "two once-per-turn abilities on one card would share one use (`OncePerTurnKey` is the card and which copy)"));
        }
        for effect in self.continuous.iter().filter(|effect| effect.first_each_turn) {
            let Scope::Installing(filter) = &effect.applies_to else {
                return Err(self.first_time_misfit("a continuous effect is about the first of something only where it is about an install (`Installing`)".to_string()));
            };
            if let Err(why) = crate::rules::turn_log::Occurrences::installs(filter, self.side) {
                return Err(self.first_time_misfit(why));
            }
        }
        // `Prohibit { until: Encounter }` parses and would hold for a window
        // in which nobody scores, steals or trashes: a card that reads as
        // working and forbids nothing.
        let mut prohibits_for_an_encounter = false;
        // `Amount::ChosenNumber` outside an `Effect::ChooseNumber::then`
        // parses and reads as 0: a card that removes no tags and says
        // nothing. The substitution stops at a `then`, so a root it
        // changes names the placeholder where no number was chosen.
        let mut chosen_number_nobody_chose = false;
        let roots = self
            .abilities
            .iter()
            .map(|ability| &ability.effect)
            .chain(self.triggers.iter().flat_map(|triggered| &triggered.effects))
            .chain(self.subroutines.iter().map(|subroutine| &subroutine.effect))
            .chain(self.interactive_on_access.iter().flat_map(|interactive| &interactive.effects));
        for root in roots {
            root.for_each_effect(&mut |effect| {
                prohibits_for_an_encounter |= matches!(effect, Effect::Prohibit { until: EffectDuration::Encounter, .. });
            });
            chosen_number_nobody_chose |= root.clone().with_chosen_number(1) != *root;
        }
        if chosen_number_nobody_chose {
            return Err(CardValidationError::ChosenNumberNobodyChose(self.id.clone()));
        }
        if prohibits_for_an_encounter {
            return Err(CardValidationError::ProhibitionForAnEncounter(self.id.clone()));
        }
        // A continuous effect that does not fit parses and then applies to
        // nothing, which reads as a card that works: the scan finds no
        // target and the number is never added.
        for effect in &self.continuous {
            let misfit = |kind, why| Err(CardValidationError::ContinuousEffectDoesNotFit(self.id.clone(), kind, why));
            let hosted = matches!(self.card_type, CardType::Program | CardType::Hardware | CardType::Resource);
            if !matches!(self.card_type, CardType::Ice(_)) && effect.condition.as_ref().is_some_and(says_protecting_remote) {
                return misfit("ProtectingRemote", "only a piece of ice protects a server");
            }
            match (&effect.kind, &effect.applies_to) {
                (_, Scope::Host) if !hosted => return misfit("Host", "only a Runner's installed card is hosted on another"),
                (_, Scope::RootOfThisServer(_)) if self.card_type != CardType::Upgrade && self.card_type != CardType::Asset => {
                    return misfit("RootOfThisServer", "only an asset or an upgrade is in a server's root");
                }
                (ContinuousKind::Strength(_), Scope::This) if self.strength.is_none() => {
                    return misfit("Strength", "this card prints no strength to change");
                }
                (ContinuousKind::Strength(_), Scope::This | Scope::Host | Scope::Ice) => {}
                (ContinuousKind::Strength(_), _) => return misfit("Strength", "strength belongs to this card, its host, or ice"),
                (ContinuousKind::Memory(_), Scope::Controller) if self.side == Side::Runner => {}
                (ContinuousKind::Memory(_), _) => return misfit("Memory", "memory is the Runner's, so it applies to a Runner card's `Controller`"),
                (ContinuousKind::Link(_), Scope::Controller) if self.side == Side::Runner => {}
                (ContinuousKind::Link(_), _) => return misfit("Link", "link is the Runner's, so it applies to a Runner card's `Controller`"),
                (ContinuousKind::HandSize(_), Scope::Controller) => {}
                (ContinuousKind::HandSize(_), _) => return misfit("HandSize", "a maximum hand size is a player's, so it applies to the card's `Controller`"),
                (ContinuousKind::InstallCost(_), Scope::This | Scope::Installing(_)) => {}
                (ContinuousKind::InstallCost(_), _) => return misfit("InstallCost", "an install cost is this card's own or that of a card being `Installing`"),
                (ContinuousKind::RezCost(_), Scope::This | Scope::Ice | Scope::RootOfThisServer(_)) => {}
                (ContinuousKind::RezCost(_), _) => return misfit("RezCost", "only an installed Corp card is rezzed"),
                (ContinuousKind::TrashCost(_), Scope::This | Scope::RootOfThisServer(_)) => {}
                (ContinuousKind::TrashCost(_), _) => return misfit("TrashCost", "a trash cost is this card's own or that of a card in its server's root"),
                (ContinuousKind::GainSubtype(_), Scope::This | Scope::Host | Scope::Ice) => {}
                (ContinuousKind::GainSubtype(_), _) => return misfit("GainSubtype", "an ice subtype is gained by ice: this card, its host, or each piece"),
                (ContinuousKind::BoostsLastTheRun, Scope::This | Scope::Host) => {}
                (ContinuousKind::BoostsLastTheRun, _) => return misfit("BoostsLastTheRun", "a boost is an icebreaker's: this card or its host"),
            }
        }
        Ok(())
    }
}

/// Whether `requirement` asks `ProtectingRemote` anywhere in it.
fn says_protecting_remote(requirement: &EffectRequirement) -> bool {
    match requirement {
        EffectRequirement::ProtectingRemote => true,
        EffectRequirement::Not(inner) => says_protecting_remote(inner),
        EffectRequirement::And(a, b) => says_protecting_remote(a) || says_protecting_remote(b),
        _ => false,
    }
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
                subject: Some(Subject::This), when: None, acts_on_subject: false, first_each_turn: false,
                text: None,
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
                subject: Some(Subject::This), when: None, acts_on_subject: false, first_each_turn: false,
                text: None,
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
            vec![SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun, only_breakable_by: None }]
        );
        assert!(card.triggers.is_empty());
    }

    #[test]
    fn parses_corroder_from_json() {
        use crate::dsl::cost::Cost;
        use crate::dsl::effect::{EffectDuration, SubroutineBreakCount};

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
                    text: Some("1[credit]: +1 strength.".to_string()),
                    trigger: Trigger::Paid,
                    cost: Some(Cost::Credits(1)),
                    // Every icebreaker ability carries this — real
                    // Netrunner only permits them while encountering ICE.
                    requirement: Some(EffectRequirement::DuringEncounter),
                    effect: Effect::BoostStrength { amount: 1, duration: EffectDuration::Encounter },
                    cost_discount_if: None, used_by: None },
                AbilityDef {
                    text: Some("Interface → 1[credit]: Break 1 barrier subroutine.".to_string()),
                    trigger: Trigger::Paid,
                    cost: Some(Cost::Credits(1)),
                    // Every icebreaker ability carries this — real
                    // Netrunner only permits them while encountering ICE.
                    requirement: Some(EffectRequirement::DuringEncounter),
                    effect: Effect::BreakSubroutines {
                        count: SubroutineBreakCount::Fixed(1),
                        restrict_to: Some(IceType::Barrier),
                    },
                    cost_discount_if: None, used_by: None },
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
        card.subroutines = vec![SubroutineDef { text: "oops".to_string(), effect: Effect::EndTheRun, only_breakable_by: None }];

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
    /// A filter of the wrong kind parses and then never passes; "act on it"
    /// with no card to be "it" resolves as the card itself. Both are a card
    /// quietly doing something other than its text.
    #[test]
    fn a_triggers_filter_and_its_it_must_fit_what_the_trigger_is_about() {
        let with = |trigger: Trigger, subject: Option<Subject>, when: Option<EventFilter>, acts_on_subject: bool| CardDefinition {
            id: CardId("homebrew".to_string()),
            side: Side::Runner,
            card_type: CardType::Resource,
            triggers: vec![TriggeredEffect { trigger, subject, when, acts_on_subject, first_each_turn: false, text: None, effects: vec![], requirement: None }],
            ..Default::default()
        };
        let on_hq = || Some(EventFilter::Server(vec![crate::rules::ServerId::Hq]));
        let a_virus = || Some(EventFilter::Card(crate::dsl::CardFilter::HasSubtype(CardSubtype::Virus)));

        assert_eq!(with(Trigger::OnSuccessfulRun, Some(Subject::Any), on_hq(), false).validate(), Ok(()));
        assert_eq!(with(Trigger::OnCardInstalled, Some(Subject::Any), a_virus(), true).validate(), Ok(()));
        assert!(matches!(
            with(Trigger::OnCardInstalled, Some(Subject::Any), on_hq(), false).validate(),
            Err(CardValidationError::TriggerFilterOfTheWrongKind(_, Trigger::OnCardInstalled))
        ));
        assert!(matches!(
            with(Trigger::OnTurnStart, None, a_virus(), false).validate(),
            Err(CardValidationError::TriggerFilterOfTheWrongKind(_, Trigger::OnTurnStart))
        ));
        assert!(matches!(
            with(Trigger::OnSuccessfulRun, Some(Subject::Any), None, true).validate(),
            Err(CardValidationError::TriggerActsOnNoCard(_, Trigger::OnSuccessfulRun))
        ));
        // A kind of damage is the third thing a moment can be about (Net
        // Shield), with nothing a card could be "this" of.
        let net = || Some(EventFilter::Damage(crate::dsl::DamageType::Net));
        assert_eq!(with(Trigger::OnDamageAboutToResolve, None, net(), false).validate(), Ok(()));
        assert!(matches!(
            with(Trigger::OnDamageAboutToResolve, Some(Subject::Any), net(), false).validate(),
            Err(CardValidationError::TriggerSubjectWithNothingToName(_, Trigger::OnDamageAboutToResolve))
        ));
        assert!(matches!(
            with(Trigger::OnSuccessfulRun, Some(Subject::Any), net(), false).validate(),
            Err(CardValidationError::TriggerFilterOfTheWrongKind(_, Trigger::OnSuccessfulRun))
        ));
        assert!(matches!(
            with(Trigger::OnDamageAboutToResolve, None, on_hq(), false).validate(),
            Err(CardValidationError::TriggerFilterOfTheWrongKind(_, Trigger::OnDamageAboutToResolve))
        ));
    }

    /// "The first time each turn" is read off the turn's count of what the
    /// entry listens for; each of these would load and then count the wrong
    /// thing, or nothing.
    #[test]
    fn validate_refuses_a_first_time_the_turn_cannot_count() {
        use crate::dsl::continuous::Number;
        use crate::dsl::{Amount, CardFilter};
        use crate::rules::ServerId;
        let first = |trigger: Trigger, subject: Option<Subject>, when: Option<EventFilter>, requirement: Option<EffectRequirement>| TriggeredEffect {
            trigger,
            subject,
            when,
            acts_on_subject: false,
            first_each_turn: true,
            text: None,
            effects: vec![],
            requirement,
        };
        let card = |side: Side, triggers: Vec<TriggeredEffect>| CardDefinition { id: CardId("first".to_string()), side, card_type: CardType::Identity, triggers, ..Default::default() };
        let refused = |card: CardDefinition| matches!(card.validate(), Err(CardValidationError::FirstTimeDoesNotFit(..)));
        let any = Some(Subject::Any);
        let programs = || Some(EventFilter::Card(CardFilter::CardType(CardType::Program)));

        // What the pool prints loads.
        assert_eq!(card(Side::Runner, vec![first(Trigger::OnSuccessfulRun, any, Some(EventFilter::Server(vec![ServerId::Hq])), None)]).validate(), Ok(()));
        assert_eq!(card(Side::Runner, vec![first(Trigger::OnCardInstalled, any, programs(), None)]).validate(), Ok(()));
        assert_eq!(card(Side::Corp, vec![first(Trigger::OnAgendaScored, any, None, None), first(Trigger::OnAgendaStolen, any, None, None)]).validate(), Ok(()));

        // Finer than a class, or a filter on a card a player did not see.
        assert!(refused(card(Side::Runner, vec![first(Trigger::OnCardInstalled, any, Some(EventFilter::Card(CardFilter::HasSubtype(crate::dsl::CardSubtype::Virus))), None)])));
        assert!(refused(card(Side::Corp, vec![first(Trigger::OnInstall, any, Some(EventFilter::Card(CardFilter::CardType(CardType::Agenda))), None)])));
        assert!(refused(card(Side::Corp, vec![first(Trigger::OnRez, any, Some(EventFilter::Card(CardFilter::CardType(CardType::Ice(IceType::Barrier)))), None)])));
        // A use limit and a fact about the turn are two things.
        assert!(refused(card(Side::Corp, vec![first(Trigger::OnTagsGiven, None, None, Some(EffectRequirement::OncePerTurn))])));
        assert!(refused(card(Side::Corp, vec![first(Trigger::OnAgendaScored, Some(Subject::This), None, None)])));
        // One install is an `OnInstall` and an `OnCardInstalled`.
        assert!(refused(card(Side::Runner, vec![first(Trigger::OnInstall, any, None, None), first(Trigger::OnCardInstalled, any, None, None)])));
        // "The first time" spelled as an intervening if.
        let mut counted = first(Trigger::OnTagsGiven, None, None, Some(EffectRequirement::Not(Box::new(EffectRequirement::AmountAtLeast(Amount::TimesThisTurn(Trigger::OnTagsGiven), 2)))));
        counted.first_each_turn = false;
        assert!(refused(card(Side::Corp, vec![counted])));

        let discount = |applies_to: Scope, condition: Option<EffectRequirement>| CardDefinition {
            id: CardId("first".to_string()),
            side: Side::Runner,
            card_type: CardType::Hardware,
            continuous: vec![ContinuousEffect { kind: ContinuousKind::InstallCost(Number { per: -1, of: Amount::Fixed(1) }), applies_to, condition, first_each_turn: true, text: None }],
            ..Default::default()
        };
        assert_eq!(discount(Scope::Installing(CardFilter::CardType(CardType::Program)), None).validate(), Ok(()));
        assert!(refused(discount(Scope::Installing(CardFilter::Icebreaker), None)));
        assert!(matches!(
            discount(Scope::Installing(CardFilter::CardType(CardType::Program)), Some(EffectRequirement::OncePerTurn)).validate(),
            Err(CardValidationError::OncePerTurnDoesNotFit(..))
        ));
        let once = |trigger: Trigger| TriggeredEffect { first_each_turn: false, ..first(trigger, None, None, Some(EffectRequirement::OncePerTurn)) };
        assert_eq!(card(Side::Corp, vec![once(Trigger::OnTagsGiven)]).validate(), Ok(()));
        assert!(matches!(card(Side::Corp, vec![once(Trigger::OnTagsGiven), once(Trigger::OnTagRemoved)]).validate(), Err(CardValidationError::OncePerTurnDoesNotFit(..))));
        assert!(refused(discount(Scope::Controller, None)));
    }

    /// A continuous effect that does not fit parses and then reaches
    /// nothing, which looks like a card that works — so a card file is
    /// refused rather than quietly printing a number nobody adds.
    #[test]
    fn a_continuous_effect_must_fit_the_card_that_prints_it() {
        use crate::dsl::continuous::Number;
        let flat = |per: i32| Number { per, of: crate::dsl::Amount::Fixed(1) };
        let with = |side: Side, card_type: CardType, strength: Option<i32>, kind: ContinuousKind, applies_to: Scope| CardDefinition {
            id: CardId("homebrew".to_string()),
            side,
            card_type,
            strength,
            continuous: vec![ContinuousEffect { kind, applies_to, condition: None, first_each_turn: false, text: None }],
            ..Default::default()
        };
        let refused = |card: CardDefinition| matches!(card.validate(), Err(CardValidationError::ContinuousEffectDoesNotFit(..)));
        let asset = || crate::dsl::CardFilter::CardType(CardType::Asset);

        // The pool's own shapes.
        assert_eq!(with(Side::Runner, CardType::Program, Some(0), ContinuousKind::Strength(flat(1)), Scope::This).validate(), Ok(()));
        assert_eq!(with(Side::Runner, CardType::Hardware, None, ContinuousKind::Strength(flat(1)), Scope::Host).validate(), Ok(()));
        assert_eq!(with(Side::Runner, CardType::Hardware, None, ContinuousKind::Memory(flat(1)), Scope::Controller).validate(), Ok(()));
        assert_eq!(with(Side::Corp, CardType::Identity, None, ContinuousKind::HandSize(flat(2)), Scope::Controller).validate(), Ok(()));
        assert_eq!(with(Side::Runner, CardType::Resource, None, ContinuousKind::RezCost(flat(1)), Scope::Ice).validate(), Ok(()));
        assert_eq!(with(Side::Corp, CardType::Upgrade, None, ContinuousKind::TrashCost(flat(2)), Scope::RootOfThisServer(asset())).validate(), Ok(()));

        // A strength with nothing to change; memory for a player who has none.
        assert!(refused(with(Side::Runner, CardType::Hardware, None, ContinuousKind::Strength(flat(1)), Scope::This)));
        assert!(refused(with(Side::Corp, CardType::Asset, None, ContinuousKind::Memory(flat(1)), Scope::Controller)));
        // A relation the card can never be in.
        assert!(refused(with(Side::Corp, CardType::Ice(IceType::Barrier), Some(1), ContinuousKind::GainSubtype(IceType::Sentry), Scope::Host)));
        assert!(refused(with(Side::Corp, CardType::Ice(IceType::Barrier), Some(1), ContinuousKind::TrashCost(flat(1)), Scope::RootOfThisServer(asset()))));
        // A kind aimed at something it cannot change.
        assert!(refused(with(Side::Runner, CardType::Hardware, None, ContinuousKind::Memory(flat(1)), Scope::Ice)));
        assert!(refused(with(Side::Runner, CardType::Hardware, None, ContinuousKind::HandSize(flat(1)), Scope::This)));
        assert!(refused(with(Side::Runner, CardType::Hardware, None, ContinuousKind::InstallCost(flat(-1)), Scope::Controller)));
        assert!(refused(with(Side::Runner, CardType::Hardware, None, ContinuousKind::BoostsLastTheRun, Scope::Controller)));

        // Palisade's "while this ice is protecting a remote server": ice
        // says it, and nothing else protects a server to say it about.
        let while_protecting = |card_type: CardType| {
            let strength = matches!(card_type, CardType::Ice(_)).then_some(2);
            let mut card = with(Side::Corp, card_type, strength, ContinuousKind::RezCost(flat(-1)), Scope::This);
            card.continuous[0].condition = Some(EffectRequirement::Not(Box::new(EffectRequirement::ProtectingRemote)));
            card
        };
        assert_eq!(while_protecting(CardType::Ice(IceType::Barrier)).validate(), Ok(()));
        assert!(refused(while_protecting(CardType::Upgrade)));
    }

    /// A prohibition is for a run or a turn. One for an encounter parses,
    /// and then holds only while nobody could do the thing it forbids —
    /// found wherever the effect is nested, since Ansel's is a subroutine
    /// and Luminal's the second step of a trigger.
    #[test]
    fn validate_refuses_a_prohibition_that_lasts_an_encounter() {
        use crate::dsl::effect::Prohibition;
        let ice = |until| CardDefinition {
            id: CardId("bar".to_string()),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::CodeGate),
            strength: Some(1),
            subroutines: vec![SubroutineDef {
                text: "The Runner cannot steal or trash Corp cards.".to_string(),
                effect: Effect::Sequence(vec![Effect::Prohibit { what: Prohibition::StealOrTrash, until }]),
                only_breakable_by: None,
            }],
            ..CardDefinition::default()
        };
        assert_eq!(ice(EffectDuration::Run).validate(), Ok(()));
        assert_eq!(ice(EffectDuration::Turn).validate(), Ok(()));
        assert_eq!(ice(EffectDuration::Encounter).validate(), Err(CardValidationError::ProhibitionForAnEncounter(CardId("bar".to_string()))));
    }

    /// `Amount::ChosenNumber` means something only inside the `then` of the
    /// `ChooseNumber` that asked for it. Outside one it parses and is 0 —
    /// a card that reads as working and removes no tags.
    #[test]
    fn validate_refuses_a_chosen_number_nobody_chose() {
        use crate::dsl::effect::Amount;
        let event = |effect| CardDefinition {
            id: CardId("baz".to_string()),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::CodeGate),
            strength: Some(1),
            subroutines: vec![SubroutineDef { text: String::new(), effect, only_breakable_by: None }],
            ..CardDefinition::default()
        };
        let asked = Effect::ChooseNumber {
            chooser: Side::Corp,
            min: 0,
            max: Amount::Fixed(2),
            of: None,
            then: Box::new(Effect::Sequence(vec![Effect::RemoveTags(Amount::ChosenNumber)])),
            text: "Remove up to 2 tags".to_string(),
        };
        assert_eq!(event(asked).validate(), Ok(()));
        assert_eq!(
            event(Effect::Sequence(vec![Effect::RemoveTags(Amount::ChosenNumber)])).validate(),
            Err(CardValidationError::ChosenNumberNobodyChose(CardId("baz".to_string())))
        );
        // A bound is outside the `then` too: there is no number yet to
        // bound the number by.
        let bounded_by_itself = Effect::ChooseNumber {
            chooser: Side::Runner,
            min: 0,
            max: Amount::ChosenNumber,
            of: None,
            then: Box::new(Effect::Sequence(Vec::new())),
            text: String::new(),
        };
        assert_eq!(event(bounded_by_itself).validate(), Err(CardValidationError::ChosenNumberNobodyChose(CardId("baz".to_string()))));
    }

    #[test]
    fn validate_refuses_hosted_credit_words_that_nothing_could_reach() {
        let id = || CardId("pool".to_string());
        let card = |edit: fn(&mut CardDefinition)| {
            let mut card = CardDefinition {
                id: id(),
                side: Side::Runner,
                card_type: CardType::Program,
                counter_kind: Some(CounterKind::Credit),
                recurring_credits: Some(2),
                pays_for: vec![PaysFor::TrashCosts],
                ..CardDefinition::default()
            };
            edit(&mut card);
            card.validate()
        };
        assert_eq!(card(|_| {}), Ok(()), "Azimat's shape");
        assert_eq!(card(|c| c.counter_kind = None), Err(CardValidationError::PaysForWithoutHostedCredits(id())));
        assert_eq!(card(|c| c.card_type = CardType::Event), Err(CardValidationError::PaysForWithoutHostedCredits(id())), "an event is never installed");
        assert_eq!(card(|c| c.pays_for.clear()), Err(CardValidationError::RecurringCreditsWithNowhereToGo(id())), "credits nothing may be spent on");
        assert_eq!(card(|c| c.card_type = CardType::Identity), Err(CardValidationError::RunnerIdentityHostsNothing(id())));
        assert_eq!(
            card(|c| {
                c.card_type = CardType::Identity;
                c.side = Side::Corp;
            }),
            Ok(()),
            "NBN: Making News's shape"
        );
        assert_eq!(
            card(|c| {
                c.recurring_credits = None;
                c.pays_for.clear();
                c.trash_when_empty = true;
            }),
            Err(CardValidationError::TrashWhenEmptyWithNothingToEmptyIt(id()))
        );
    }
}
