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

/// The breaker-facing axis of a piece of ice: which of the three types a
/// typed breaker's `restrict_to` (and a `GainSubtype`) is matched against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IceType {
    Barrier,
    CodeGate,
    Sentry,
    /// Ice that prints none of the three — a Trap (Vicsek, Data Mine, Loot
    /// Box) or a Mythic (Rime, Konjin, Excalibur, Lycian Multi-Munition).
    /// It is still a piece of ice with subtypes of its own
    /// (`CardDefinition::subtypes`), but no fracter, decoder or killer
    /// interacts with it: only a breaker with no `restrict_to` (an AI) does
    /// (CR 3.9.5h: an interface ability that names no subtype "can be used
    /// on any piece of ice"), or one whose target has *gained* a type
    /// (`ContinuousKind::GainSubtype`), because the match is "prints it or
    /// has gained it".
    ///
    /// **A fourth variant rather than `CardType::Ice(Option<IceType>)`**
    /// (NSG card pool, Vantage Point Stage 2b, 26 September 2026): the
    /// question every reader asks is "is this ice a barrier?", and equality
    /// with a variant that is none of the three already answers no, so no
    /// matcher changed. An `Option` would have put `Some(..)` at every one
    /// of about two hundred sites and in every ice card file for a case one
    /// card in the pool prints. `validate` refuses it where it could only
    /// mean nothing: a breaker restricted to it, or a card gaining it.
    Other,
}

/// A printed subtype: every word Comprehensive Rules 2.16.7 lists, spelled
/// as the card prints it (`#[serde(rename)]` where Rust cannot), so a card's
/// subtypes are its catalog keywords read into this type. Distinct from
/// `CardType`, which is a card's primary type, not a tag on top of it.
///
/// **The whole list, once** (NSG card pool, Vantage Point Stage 1, 26
/// September 2026). It was ten words, each added when a card first read
/// it, and authored by hand on every card that carried it ("authored on
/// every fracter, the way `Virus` is authored on every virus program"),
/// which is a restatement of the catalog a card file could get wrong. Every
/// NSG tranche prints subtypes a card reads — AP, Destroyer, Observer,
/// Harmonic, Liability, Stealth, Virtual — so the list is the rules', and
/// `cards::embedded`'s join fills `CardDefinition::subtypes` from the
/// catalog the way it fills faction and influence. A homebrew card with no
/// catalog entry still authors its own.
///
/// Barrier, Code Gate and Sentry are here because they are printed
/// subtypes; `CardType::Ice(IceType)` is still what a breaker is matched
/// against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CardSubtype {
    Academic,
    Advertisement,
    #[serde(rename = "AI")]
    Ai,
    Alliance,
    Ambush,
    #[serde(rename = "AP")]
    Ap,
    Assassination,
    Barrier,
    Beanstalk,
    /// A Bioroid — the printed subtype on Haas-Bioroid's click-breakable
    /// ice and its Academic upgrades. Read by LEO Construction: Labor
    /// Solutions' "trash 1 rezzed bioroid card in the root of or
    /// protecting the attacked server" through `CardFilter::HasSubtype`.
    /// Orthogonal to `CardDefinition::click_breakable`, which is the
    /// *mechanic* the bioroid ice share; Mercia B4LL4RD is a bioroid with
    /// no subroutines to click through.
    Bioroid,
    #[serde(rename = "Black Ops")]
    BlackOps,
    Cast,
    #[serde(rename = "Caïssa")]
    Caissa,
    Character,
    Chip,
    Clan,
    Clone,
    Cloud,
    #[serde(rename = "Code Gate")]
    CodeGate,
    Companion,
    Condition,
    /// A Connection resource — the other type Open Market's credits pay for.
    Connection,
    /// "Limit 1 console per player" — e.g. Carnivore, Pennyshaver,
    /// Pantograph. Installing a second trashes the first at the next
    /// checkpoint (CR 3.8.5b, 10.3.1d; `checkpoint::enforce_consoles`). It
    /// was a refusal until Rules Conformance B.
    Console,
    #[serde(rename = "Consumer-grade")]
    ConsumerGrade,
    Corp,
    Corporation,
    Current,
    Cybernetic,
    Cyborg,
    Daemon,
    Decoder,
    #[serde(rename = "Deep Net")]
    DeepNet,
    Deflector,
    Department,
    Destroyer,
    Deva,
    Digital,
    Directive,
    Division,
    Double,
    Enforcer,
    Executive,
    Expansion,
    Expendable,
    Facility,
    /// An icebreaker that breaks barriers — Rising Tide's "+1 strength for
    /// each fracter in your heap" counts them.
    Fracter,
    #[serde(rename = "G-mod")]
    GMod,
    Gear,
    Genetics,
    Government,
    Grail,
    #[serde(rename = "Gray Ops")]
    GrayOps,
    Harmonic,
    Hostile,
    Icebreaker,
    Industrial,
    Initiative,
    /// A Job resource — Open Market's hosted credits pay to install one.
    Job,
    Killer,
    Liability,
    Link,
    Location,
    Lockdown,
    Mandate,
    Megacorp,
    Mod,
    Morph,
    Mythic,
    Natural,
    #[serde(rename = "NEXT")]
    Next,
    Observer,
    #[serde(rename = "Off-site")]
    OffSite,
    Orgcrime,
    #[serde(rename = "Police Department")]
    PoliceDepartment,
    Political,
    Priority,
    Psi,
    Public,
    /// A Region upgrade — "Limit 1 region per server". Installing one into
    /// a root that already holds a region, rezzed or not, trashes the old
    /// one as part of the install (CR 3.6.5d, 8.5.6a;
    /// `rules::install_trash`). It was a refusal until Rules Conformance B.
    /// Mahkota Langit Grid is the pool's only region.
    Region,
    Remote,
    Reprisal,
    Research,
    Ritzy,
    /// A Run event — the Runner's "run event" keyword. Read two ways:
    /// `EffectRequirement::RunEventActive` (Sang Kancil's cheaper boost
    /// while one is resolving) and `CardFilter::HasSubtype` (MuslihaT's
    /// "an icebreaker or a run event").
    Run,
    Sabotage,
    Security,
    #[serde(rename = "Security Protocol")]
    SecurityProtocol,
    Seedy,
    Sensie,
    Sentry,
    Source,
    Stealth,
    Subsidiary,
    Sysop,
    Terminal,
    Tracer,
    Transaction,
    Trap,
    Triple,
    /// A Trojan program — one that installs hosted on a piece of ice
    /// (`CardDefinition::installs_on_ice` is the mechanic; this is the
    /// printed tag). Bumi 1.0's "trash 1 installed trojan program" reads it
    /// through `CardFilter::HasSubtype`.
    Trojan,
    Unorthodox,
    Unsubstantiated,
    Vehicle,
    Virtual,
    Virus,
    Weapon,
}

impl CardSubtype {
    /// The subtype as a card prints it, the word a keyword line carries.
    pub fn printed(self) -> String {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::String(word)) => word,
            _ => format!("{self:?}"),
        }
    }

    /// The subtype a printed keyword names, `None` for a word the rules do
    /// not list.
    pub fn from_printed(word: &str) -> Option<CardSubtype> {
        serde_json::from_value(serde_json::Value::String(word.to_string())).ok()
    }
}

/// A kind of server, as an install restriction names one: a central or a
/// remote, or one central server by name ("HQ or R&D only").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerKind {
    Central,
    Remote,
    Hq,
    RnD,
}

impl ServerKind {
    /// Whether `server` is one of this kind.
    pub fn admits(self, server: crate::rules::ServerId) -> bool {
        use crate::rules::ServerId;
        match self {
            ServerKind::Central => !matches!(server, ServerId::Remote(_)),
            ServerKind::Remote => matches!(server, ServerId::Remote(_)),
            ServerKind::Hq => server == ServerId::Hq,
            ServerKind::RnD => server == ServerId::RnD,
        }
    }
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
    /// Active while the card is in its owner's heap, and only there (CR
    /// 9.1.8b: "abilities that can only affect the game state from a
    /// particular zone are active in that zone") — Jeitinho's "whenever
    /// you bypass a piece of ice, you may spend [click] to install this
    /// hardware from your heap". A card in the Runner's heap listens for
    /// these triggers and no others (`listeners`, `Heard::FromHeap`), and
    /// the same card on the table does not hear them. The Runner's heap
    /// only: no pool card prints one for Archives.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub from_heap: bool,
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

    /// "Central server only." / "Remote server only." / "HQ or R&D only."
    /// — where an upgrade may be installed, beyond what its type allows (an
    /// agenda or an asset is remote-only by type): any of the kinds listed,
    /// and anywhere when the list is empty. The Red Room is the first;
    /// Flagship names two servers (VP Stage 7g), which is why it is a list.
    /// A declaration the install reads, like `removed_after_play`: it is
    /// about where the card may go, not a standing effect of it. It holds
    /// at all times, inactive or not (CR 8.5.12), so a move honours it too
    /// (`Effect::MoveThisCardToRoot` and Lotus Haze's offer). Asked through
    /// `may_be_installed_in`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub install_only_in: Vec<ServerKind>,

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
    /// Cards hosted on this card are hosted facedown (Read-Write Share's
    /// "host 1 card from your grip facedown on this program"): their owner
    /// sees them and nobody else does. Read off the card by the mask
    /// (`masking::PublicInstalledRunnerCard::hosted_unseen`), which holds
    /// a registry, so nothing is seeded onto the install: unlike
    /// `hosted_cards_playable_from_grip`, no registry-less reader asks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hosts_facedown: bool,
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

    /// This event is removed from the game once it has resolved, rather
    /// than trashed — Take a Dive's and Kompromat's last line, "Remove
    /// this event from the game." It lands in
    /// `RunnerState::removed_from_game`, where nothing that reads the heap
    /// finds it. A declaration rather than an effect: the event is still
    /// resolving when its last instruction does, and `engine::play_event`
    /// is what files a played event, so it is the one place that can file
    /// this one elsewhere — an `OnPlay` that parks a decision (Take a
    /// Dive's run offer) would otherwise leave the card to be trashed
    /// after its own removal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed_after_play: bool,



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
    /// "Install only faceup." — Sacrifice Zone Expansion. The agenda is
    /// installed faceup, which this engine writes as `InstalledCard::
    /// rezzed` without a rez (no cost, no `OnRez`), the same flag BANGUN's
    /// flip sets; and because its own text directs it, its abilities are
    /// active while it is installed (CR 3.2.3a, `rules::active`), which
    /// BANGUN's are not. An agenda only (`validate`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub installs_faceup: bool,

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
    /// "Rez only during your turn" (Front Company): when this card may be
    /// turned faceup, asked as the card by `engine::rez_install`, the one
    /// place a Corp card is rezzed, so a card's text that rezzes it is held
    /// to it as the rez action is (CR 1.2.2, the "cannot" takes
    /// precedence). A field beside `play_requirement` and
    /// `install_only_in`, the card's other restrictions on its own way into
    /// play, and not a continuous effect: a standing effect is what an
    /// *active* card does, and a card being rezzed is not active yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rez_requirement: Option<EffectRequirement>,
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
    /// Paid before the rez, beside its credits: Biawak's and Plutus's
    /// `Cost::Forfeit(1)`, Plutus's `Cost::Trash` of three cards from HQ;
    /// none for the plain rez Biawak lists beside its forfeit. An
    /// alternative is offered only when this is affordable
    /// (`ability::cost_is_affordable`, the scan the payment reads), and the
    /// rez is priced with its discount (`engine::rez_price`), so a player
    /// cannot pick one that would resolve to nothing.
    ///
    /// It was an `Effect` behind a requirement — `ForfeitAgendas` behind
    /// `ScoreAreaHasAtLeast`, a card selection behind `ZoneHasAtLeast` —
    /// resolved as "pay, then rez" out of a `PresentChoice`, which is what
    /// kept the forfeit an effect that never asked which agenda.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<crate::dsl::Cost>,
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
    /// The Corp's bad publicity, hosted on a card where "it has no effect
    /// while hosted" (Luana Campos). Named so the view can say what the
    /// counters are; they are counted with the rest.
    BadPublicity,
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
    /// Anything, while a run is in progress — Methuselah's and
    /// Touchstone's "You can spend hosted credits during runs". A word
    /// about *when*, where the others are about *what*, so during a run a
    /// pool that prints it is as broad as the credit pool
    /// (`payment::class_of`), and a narrower pool (Cyberfeeder's) is spent
    /// before it unasked.
    DuringRuns,
    /// Anything, during a run this card's own ability began — Debbie
    /// "Downtown" Moreira's "[click]: Run any server. You can spend hosted
    /// credits during that run". `DuringRuns` is every run, and the card's
    /// run is the one whose `RunState::initiated_by` it is. As broad as
    /// the credit pool during that run, as `DuringRuns` is during any.
    DuringItsRun,
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
    #[error("card {0:?}: `IceType::Other` is ice with none of the three types, never a type — refused on {1}")]
    OtherIsNotAnIceType(CardId, &'static str),
    #[error("card {0:?}: a trigger active in the heap (`from_heap`) is the Runner's — the heap is the only zone listened to")]
    HeapTriggerOnCorpCard(CardId),
    #[error("card {0:?}: `GainIceSubtype` is \"this ice gains\" — said on a card that is not ice, it has nothing to act on")]
    SubtypeGainedByNonIce(CardId),
    #[error("Agenda {0:?} must not have subroutines")]
    AgendaHasSubroutines(CardId),
    #[error("card {0:?}: an ability used from the hand (`from_hand`) must be an action — a paid ability whose cost begins with [click]")]
    HandAbilityNotAnAction(CardId),
    #[error("card {0:?}: only a Runner card's paid ability can be a mid-access ability (`access`, CR 9.3.6b)")]
    AccessFlagOnWhatCannotBeOne(CardId),
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
    #[error("card {0:?} sets which copy of its identity is in play (`Effect::SetIdentityCopy`) other than as a Corp identity's secret number (`ChooseNumber` with `secret`), CR 1.5.2b")]
    IdentityCopySetInTheOpen(CardId),
    #[error("card {0:?} prints an X cost (`Cost::CreditsX`) somewhere but first in an ability's cost, or on something that is not an ability; X is chosen before anything is paid (CR 1.16.2c)")]
    XCostNotFirst(CardId),
    #[error("card {0:?} resolves something for each card of a random reveal (`Effect::RevealAtRandom::each`) that is not a move into a deck (`AddToDeck`, `ShuffleIntoDeck`, in a `Sequence`), which could park a decision and drop the cards revealed after it")]
    RevealedCardsCannotWait(CardId),
    #[error("Ice {0:?} must have a strength")]
    IceMissingStrength(CardId),
    #[error("card {0:?} of type {1:?} must not have a strength — only Ice and breaker-style Programs do")]
    UnexpectedStrength(CardId, CardType),
    #[error("Agenda {0:?} must have both agenda_points and advancement_requirement set")]
    AgendaMissingScoringFields(CardId),
    #[error("card {0:?} of type {1:?} must not have agenda_points — only Agenda does")]
    UnexpectedAgendaPoints(CardId, CardType),
    #[error("card {0:?} of type {1:?} cannot say \"install only faceup\" — only an agenda is installed facedown and says so")]
    FaceupInstallNotAnAgenda(CardId, CardType),
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
    #[error("card {0:?}: a prohibition lasts a run or a turn — the guards that ask are not asked during an encounter — except that the encountered ice's subroutines cannot end the run, which is about that ice (`encountered_ice`) and lasts that encounter")]
    ProhibitionForAnEncounter(CardId),
    #[error("card {0:?}: which kind of icebreaker broke the printed subroutines (`BrokePrintedSubroutineWith`) is read off a pass, so only an `OnIcePassed` trigger asks it, and only about an icebreaker subtype (AI, decoder, fracter, killer)")]
    BrokenWithOutsideAPass(CardId),
    #[error("card {0:?}: a subroutine is gained by the ice being encountered, so only an `OnEncounter` trigger that `acts_on_subject` can say it")]
    GainedSubroutineWithNoEncounter(CardId),
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
            hosts_facedown: false,
            dividends: None,
            playable_from_archives: false,
            removed_after_play: false,
            pays_for: Vec::new(),
            trash_when_empty: false,
            may_install_agendas_faceup: false,
            installs_faceup: false,
            rez_alternatives: Vec::new(),
            rez_requirement: None,
            influence_limit: None,
            additional_play_cost: None,
            install_only_in: Vec::new(),
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
    /// Whether this card may be installed in, or moved to, `server`'s root
    /// as far as its own restriction goes (`install_only_in`).
    pub fn may_be_installed_in(&self, server: crate::rules::ServerId) -> bool {
        self.install_only_in.is_empty() || self.install_only_in.iter().any(|kind| kind.admits(server))
    }

    /// Checks the semantic rules `CardValidationError` documents. Structural
    /// well-formedness (right field types, valid enum tags) is already
    /// guaranteed by having deserialized successfully — this only catches
    /// combinations that parse fine but don't make sense as a real card.
    fn first_time_misfit(&self, why: String) -> CardValidationError {
        CardValidationError::FirstTimeDoesNotFit(self.id.clone(), why)
    }

    /// Whether `filter` can narrow `triggered`: a filter of the wrong kind
    /// parses and then never passes. A conjunction's parts are each asked
    /// here, so each is held to what it could be on its own.
    fn filter_fits(&self, triggered: &TriggeredEffect, filter: &EventFilter) -> bool {
        let about = triggered.trigger.about();
        match filter {
            // A trash does not say whether the card was installed.
            EventFilter::InstalledCard(_) if triggered.trigger == Trigger::OnCardTrashed => false,
            EventFilter::Card(_) | EventFilter::InstalledCard(_) => about == TriggerAbout::Card,
            EventFilter::Server(_) => about == TriggerAbout::Server,
            EventFilter::Damage(_) => about == TriggerAbout::Damage,
            EventFilter::AtLeast(_) => about == TriggerAbout::Cards,
            // Only a moment that names a player can be made one's.
            EventFilter::Whose(_) => triggered.trigger.states_whose(),
            EventFilter::OwnedBy { .. } => about == TriggerAbout::Card && triggered.trigger.states_whose(),
            EventFilter::ByThis => triggered.trigger == Trigger::OnIceFullyBroken,
            // Only a card that is hosted has a host to be about.
            EventFilter::Host => about == TriggerAbout::Card && self.installs_on_ice,
            // Only a Corp install's moment says where the card came
            // from (`listeners::Moment::from_hq`).
            EventFilter::InstalledFromHq(_) | EventFilter::InstalledIn(_) => triggered.trigger == Trigger::OnInstall && self.side == crate::rules::Side::Corp,
            // Only an install is in a root or not.
            EventFilter::InRoot => triggered.trigger == Trigger::OnInstall,
            // Only a trash says which pile the card left.
            EventFilter::TrashedFrom(_) => triggered.trigger == Trigger::OnCardTrashed,
            // Only what the moment states: a pass says whether the ice
            // was outermost and fully broken, a break its strength.
            EventFilter::Ice(required) => {
                triggered.trigger.is_about_ice_in_a_run() && required.admits(crate::dsl::IceFacts::stated_by(triggered.trigger))
            }
            // `validate` reads a conjunction part by part, and refuses one
            // nested in another.
            EventFilter::All(_) => false,
        }
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
        if !is_agenda && self.installs_faceup {
            return Err(CardValidationError::FaceupInstallNotAnAgenda(self.id.clone(), self.card_type.clone()));
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
        // "Access →" is a flag on a Runner's paid ability (CR 9.3.6b), used
        // only by the Runner in the mid-access window: on anything else it
        // is an ability that could never be used.
        if self.abilities.iter().any(|ability| ability.access && (ability.trigger != Trigger::Paid || self.side != Side::Runner)) {
            return Err(CardValidationError::AccessFlagOnWhatCannotBeOne(self.id.clone()));
        }
        // An ability used from the hand is taken as an action
        // (`ActivateHandAbility` is classified as one), which is the only
        // kind a pool card prints, and it is no mid-access ability.
        if self.abilities.iter().any(|ability| ability.from_hand && (!ability.is_action() || ability.access)) {
            return Err(CardValidationError::HandAbilityNotAnAction(self.id.clone()));
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
            let fits = |filter: &EventFilter| self.filter_fits(triggered, filter);
            let filter_fits = match &triggered.when {
                None => true,
                // Two or more parts, each fitting on its own; one part is
                // that part, written plainly.
                Some(EventFilter::All(parts)) => parts.len() >= 2 && parts.iter().all(|part| !matches!(part, EventFilter::All(_)) && fits(part)),
                Some(filter) => fits(filter),
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
            // "The first time each turn you advance this agenda" is counted
            // on the copy (`InstalledCard::this_turn`), which only a Corp
            // install keeps; a moment about a Runner card is, so far, one
            // that happens to it once.
            if triggered.subject == Some(Subject::This) && (self.side == Side::Runner || !crate::rules::turn_log::CopyTurn::counts(triggered.trigger)) {
                return Err(self.first_time_misfit(format!(
                    "the copy of a Corp install counts only what a card asks of it, which is being advanced; a {:?} about this card is not counted",
                    triggered.trigger
                )));
            }
            if triggered.requirement.as_ref().is_some_and(EffectRequirement::mentions_once_per_turn) {
                return Err(self.first_time_misfit("`OncePerTurn` is a use limit on the card, and the first time each turn is a fact about the turn; a card prints one or the other".to_string()));
            }
        }
        // One printed ability counts on one thing: the copy, or the turn.
        let about_this = first_time.iter().filter(|triggered| triggered.subject == Some(Subject::This)).count();
        if about_this > 0 && about_this < first_time.len() {
            return Err(self.first_time_misfit("a card's first-time entries share one count, and it is either this copy's or the turn's".to_string()));
        }
        // A card's first-time entries share one count, so two triggers one
        // event is an occurrence of would count that event twice.
        for (one, other) in [(Trigger::OnPlay, Trigger::OnCardPlayed), (Trigger::OnInstall, Trigger::OnCardInstalled)] {
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
        if once_per_turn.clone().filter(|requirement| requirement.mentions_once_per_turn()).count() > 1 {
            return Err(CardValidationError::OncePerTurnDoesNotFit(self.id.clone(), "two once-per-turn abilities on one card would share one use (`OncePerTurnKey` is the card and which copy)"));
        }
        // The same two rules hold for the run's use limit, which is keyed
        // the same way.
        if self.continuous.iter().any(|effect| effect.condition.as_ref().is_some_and(EffectRequirement::mentions_once_per_run)) {
            return Err(CardValidationError::OncePerTurnDoesNotFit(self.id.clone(), "a continuous effect is read, never used, so its `while` cannot be a `OncePerRun`"));
        }
        if once_per_turn.filter(|requirement| requirement.mentions_once_per_run()).count() > 1 {
            return Err(CardValidationError::OncePerTurnDoesNotFit(self.id.clone(), "two once-per-run abilities on one card would share one use (`OncePerTurnKey` is the card and which copy)"));
        }
        // Which breaker broke the printed subroutines is read off a pass
        // (`GameEvent::IcePassed`), so only a pass's trigger can ask it; and
        // only an icebreaker's subtype can be one it names, since the pass
        // records no other.
        for triggered in &self.triggers {
            let mut named = Vec::new();
            if let Some(requirement) = &triggered.requirement {
                broke_printed_with(requirement, &mut named);
            }
            let misfit = !named.is_empty()
                && (triggered.trigger != Trigger::OnIcePassed || named.iter().any(|subtype| !crate::rules::BrokenWith::KINDS.contains(subtype)));
            if misfit {
                return Err(CardValidationError::BrokenWithOutsideAPass(self.id.clone()));
            }
        }
        let mut named = Vec::new();
        for requirement in self.abilities.iter().filter_map(|ability| ability.requirement.as_ref()).chain(self.continuous.iter().filter_map(|effect| effect.condition.as_ref())) {
            broke_printed_with(requirement, &mut named);
        }
        if !named.is_empty() {
            return Err(CardValidationError::BrokenWithOutsideAPass(self.id.clone()));
        }
        for effect in self.continuous.iter().filter(|effect| effect.first_each_turn) {
            let counted = match &effect.kind {
                ContinuousKind::Cannot(what) => what.counted_as(),
                _ => None,
            };
            let occurrences = match (&effect.applies_to, counted) {
                (Scope::Installing(filter), _) => crate::rules::turn_log::Occurrences::installs(filter, self.side),
                (Scope::Playing(filter), _) => crate::rules::turn_log::Occurrences::plays(filter, self.side),
                (Scope::Player(_), Some(trigger)) => crate::rules::turn_log::Occurrences::meant_by(trigger, None, self.side),
                _ => {
                    return Err(self.first_time_misfit(
                        "a continuous effect is about the first of something only where it is about an install, a play (`Installing`, `Playing`) or a prohibition the turn counts (`Prohibition::counted_as`)".to_string(),
                    ));
                }
            };
            if let Err(why) = occurrences {
                return Err(self.first_time_misfit(why));
            }
        }
        // `Prohibit { until: Encounter }` parses and would hold for a window
        // in which nobody scores, steals or trashes: a card that reads as
        // working and forbids nothing. The one prohibition that is about an
        // encounter — Banner's, that the encountered ice's subroutines
        // cannot end the run — is about that ice and lasts that encounter,
        // and nothing else is.
        let mut prohibits_for_an_encounter = false;
        // `Amount::ChosenNumber` outside an `Effect::ChooseNumber::then`
        // parses and reads as 0: a card that removes no tags and says
        // nothing. The substitution stops at a `then`, so a root it
        // changes names the placeholder where no number was chosen.
        let mut chosen_number_nobody_chose = false;
        // `restrict_to: Some(Other)` parses and breaks exactly the ice a
        // breaker with no restriction breaks, which is not what a card
        // restricted to a type means.
        let mut restricted_to_no_type = false;
        // `SetIdentityCopy` said in the open would tell the Runner the copy
        // the Corp chose, and on anything but a Corp identity there is no
        // copy to set (`CorpState::identity_copy`): every one must be the
        // `then` of a secret number on one.
        let (mut copies_set, mut copies_set_secretly) = (0usize, 0usize);
        let sets_a_copy = |effect: &Effect| {
            let mut count = 0usize;
            effect.for_each_effect(&mut |e| count += usize::from(matches!(e, Effect::SetIdentityCopy(_))));
            count
        };
        let roots = self
            .abilities
            .iter()
            .map(|ability| &ability.effect)
            .chain(self.triggers.iter().flat_map(|triggered| &triggered.effects))
            .chain(self.subroutines.iter().map(|subroutine| &subroutine.effect))
            .chain(self.interactive_on_access.iter().flat_map(|interactive| &interactive.effects));
        // A random reveal resolves its `then` once per card with nothing to
        // wait on, so a `then` must be one that never parks.
        let mut revealed_cards_wait = false;
        let never_parks = |effect: &Effect| {
            let mut plain = true;
            effect.for_each_effect(&mut |e| plain &= matches!(e, Effect::Sequence(_) | Effect::AddToDeck(_) | Effect::ShuffleIntoDeck(_)));
            plain
        };
        // An ability that prints an X names it as the chosen number, which
        // the payer writes in (`Cost::CreditsX`): its effect is exempt from
        // the check below, and its X must come first in the cost.
        let x_first = |cost: &crate::dsl::Cost| match cost {
            crate::dsl::Cost::CreditsX { .. } => true,
            crate::dsl::Cost::AllOf(parts) => matches!(parts.first(), Some(crate::dsl::Cost::CreditsX { .. })) && !parts[1..].iter().any(crate::dsl::Cost::names_x),
            _ => false,
        };
        if self.abilities.iter().filter_map(|ability| ability.cost.as_ref()).any(|cost| cost.names_x() && !x_first(cost)) {
            return Err(CardValidationError::XCostNotFirst(self.id.clone()));
        }
        let x_effects: Vec<&Effect> =
            self.abilities.iter().filter(|ability| ability.cost.as_ref().is_some_and(crate::dsl::Cost::names_x)).map(|ability| &ability.effect).collect();
        for root in roots {
            root.for_each_effect(&mut |effect| {
                if let Effect::RevealAtRandom { each: Some(each), .. } = effect {
                    revealed_cards_wait |= !never_parks(each);
                }
                if let Effect::Prohibit { what, until, encountered_ice, .. } = effect {
                    let about_the_ice = *what == crate::dsl::Prohibition::EndTheRun;
                    prohibits_for_an_encounter |= about_the_ice != *encountered_ice
                        || about_the_ice != (*until == EffectDuration::Encounter);
                }
                restricted_to_no_type |= matches!(effect, Effect::BreakSubroutines { restrict_to: Some(IceType::Other), .. });
                copies_set += usize::from(matches!(effect, Effect::SetIdentityCopy(_)));
                if let Effect::ChooseNumber { secret: true, then, .. } = effect {
                    copies_set_secretly += sets_a_copy(then);
                }
            });
            if !x_effects.iter().any(|x| std::ptr::eq(*x, root)) {
                chosen_number_nobody_chose |= root.clone().with_chosen_number(1) != *root;
            }
        }
        if chosen_number_nobody_chose {
            return Err(CardValidationError::ChosenNumberNobodyChose(self.id.clone()));
        }
        if revealed_cards_wait {
            return Err(CardValidationError::RevealedCardsCannotWait(self.id.clone()));
        }
        let a_corp_identity = self.side == Side::Corp && self.card_type == CardType::Identity;
        if copies_set > 0 && (copies_set > copies_set_secretly || !a_corp_identity) {
            return Err(CardValidationError::IdentityCopySetInTheOpen(self.id.clone()));
        }
        if prohibits_for_an_encounter {
            return Err(CardValidationError::ProhibitionForAnEncounter(self.id.clone()));
        }
        // "It gains a subroutine" acts on a piece of ice the trigger is
        // about (`Effect::GainSubroutine`), so it is said only by a trigger
        // that acts on its subject: for the encounter, as the ice is
        // encountered; for the rest of the run, as it is encountered or
        // rezzed. Anywhere else it would find no ice, or no encounter, and
        // do nothing, and nothing is encountered outside a run.
        let gains = |effect: &Effect| {
            let mut found = Vec::new();
            effect.for_each_effect(&mut |e| if let Effect::GainSubroutine { duration, .. } = e { found.push(*duration) });
            found
        };
        let fits = |triggered: &TriggeredEffect, duration: EffectDuration| {
            triggered.acts_on_subject
                && match duration {
                    EffectDuration::Encounter => triggered.trigger == Trigger::OnEncounter,
                    EffectDuration::Run => matches!(triggered.trigger, Trigger::OnEncounter | Trigger::OnRez),
                    EffectDuration::Turn => false,
                }
        };
        if self.triggers.iter().any(|triggered| triggered.effects.iter().flat_map(gains).any(|duration| !fits(triggered, duration)))
            || self.abilities.iter().any(|ability| !gains(&ability.effect).is_empty())
            || self.subroutines.iter().any(|subroutine| !gains(&subroutine.effect).is_empty())
            || self.interactive_on_access.iter().flat_map(|interactive| &interactive.effects).any(|effect| !gains(effect).is_empty())
        {
            return Err(CardValidationError::GainedSubroutineWithNoEncounter(self.id.clone()));
        }
        if restricted_to_no_type {
            return Err(CardValidationError::OtherIsNotAnIceType(self.id.clone(), "a breaker restricted to it"));
        }
        if self.side == Side::Corp && self.triggers.iter().any(|triggered| triggered.from_heap) {
            return Err(CardValidationError::HeapTriggerOnCorpCard(self.id.clone()));
        }
        // "This ice gains the chosen subtypes" acts on the acting install,
        // so only ice can say it, and gaining `Other` would mean nothing.
        let mut gained = Vec::new();
        let mut gains = |effect: &Effect| effect.for_each_effect(&mut |e| if let Effect::GainIceSubtype(subtype) = e { gained.push(*subtype) });
        self.triggers.iter().flat_map(|triggered| &triggered.effects).for_each(&mut gains);
        self.abilities.iter().map(|ability| &ability.effect).for_each(&mut gains);
        self.subroutines.iter().map(|subroutine| &subroutine.effect).for_each(&mut gains);
        if gained.contains(&IceType::Other) {
            return Err(CardValidationError::OtherIsNotAnIceType(self.id.clone(), "a card gaining it"));
        }
        if !gained.is_empty() && !matches!(self.card_type, CardType::Ice(_)) {
            return Err(CardValidationError::SubtypeGainedByNonIce(self.id.clone()));
        }
        // A continuous effect that does not fit parses and then applies to
        // nothing, which reads as a card that works: the scan finds no
        // target and the number is never added.
        for effect in &self.continuous {
            let misfit = |kind, why| Err(CardValidationError::ContinuousEffectDoesNotFit(self.id.clone(), kind, why));
            let hosted = matches!(self.card_type, CardType::Program | CardType::Hardware | CardType::Resource);
            let played = matches!(self.card_type, CardType::Event | CardType::Operation);
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
                // A Trojan's server is its host's (Monkeywrench's "each other
                // piece of ice protecting this server").
                (_, Scope::IceProtectingThisServer(_)) if self.card_type != CardType::Upgrade && self.card_type != CardType::Asset && !self.installs_on_ice => {
                    return misfit("IceProtectingThisServer", "only an asset or an upgrade is in a server's root, and only a Trojan is hosted on its ice");
                }
                (ContinuousKind::Strength(_), Scope::This | Scope::Host | Scope::Ice | Scope::IceProtectingThisServer(_) | Scope::Rig(_)) => {}
                (ContinuousKind::Strength(_), _) => return misfit("Strength", "strength belongs to this card, its host, ice, or the rig's cards"),
                (ContinuousKind::Memory(_), Scope::Controller) if self.side == Side::Runner => {}
                (ContinuousKind::Memory(_), _) => return misfit("Memory", "memory is the Runner's, so it applies to a Runner card's `Controller`"),
                (ContinuousKind::Link(_), Scope::Controller) if self.side == Side::Runner => {}
                (ContinuousKind::Link(_), _) => return misfit("Link", "link is the Runner's, so it applies to a Runner card's `Controller`"),
                (ContinuousKind::HandSize(_), Scope::Controller) => {}
                (ContinuousKind::HandSize(_), _) => return misfit("HandSize", "a maximum hand size is a player's, so it applies to the card's `Controller`"),
                (_, Scope::InstallingOntoThis(_)) if !hosted || self.side != Side::Runner => {
                    return misfit("InstallingOntoThis", "only a Runner's installed card has cards installed onto it");
                }
                (ContinuousKind::InstallCost(_), Scope::This | Scope::Installing(_) | Scope::InstallingOntoThis(_)) => {}
                (ContinuousKind::InstallCost(_), _) => return misfit("InstallCost", "an install cost is this card's own or that of a card being `Installing`"),
                (ContinuousKind::RezCost(_), Scope::This | Scope::Ice | Scope::RootOfThisServer(_) | Scope::IceProtectingThisServer(_)) => {}
                (ContinuousKind::RezCost(_), _) => return misfit("RezCost", "only an installed Corp card is rezzed"),
                (ContinuousKind::TrashCost(_), Scope::This | Scope::RootOfThisServer(_)) => {}
                (ContinuousKind::TrashCost(_), _) => return misfit("TrashCost", "a trash cost is this card's own or that of a card in its server's root"),
                (ContinuousKind::GainSubtype(IceType::Other), _) => {
                    return Err(CardValidationError::OtherIsNotAnIceType(self.id.clone(), "a card gaining it"));
                }
                (ContinuousKind::GainSubtype(_), Scope::This | Scope::Host | Scope::Ice) => {}
                (ContinuousKind::GainSubtype(_), _) => return misfit("GainSubtype", "an ice subtype is gained by ice: this card, its host, or each piece"),
                (ContinuousKind::PlayCost(_) | ContinuousKind::PlayClicks(_), Scope::This) if !played => {
                    return misfit("PlayCost", "only an event or an operation is played");
                }
                (ContinuousKind::PlayCost(_) | ContinuousKind::PlayClicks(_), Scope::This | Scope::Playing(_)) => {}
                (ContinuousKind::PlayCost(_) | ContinuousKind::PlayClicks(_), _) => {
                    return misfit("PlayCost", "a play cost is this card's own or that of a card being `Playing`");
                }
                (ContinuousKind::StealCost(_), Scope::Stealing(_) | Scope::StealingFromThisServer) => {}
                (ContinuousKind::RemoteServerLimit(_), Scope::Controller) => {}
                (ContinuousKind::RemoteServerLimit(_), _) => return misfit("RemoteServerLimit", "a limit on remote servers is its controller's"),
                (ContinuousKind::AdditionalTrashCost(_), Scope::This) => {}
                (ContinuousKind::AdditionalTrashCost(_), _) => {
                    return misfit("AdditionalTrashCost", "an additional cost to trash a card the Runner accesses is that card's own");
                }
                (ContinuousKind::StealCost(_), _) => {
                    return misfit("StealCost", "an additional cost to steal is about an agenda being `Stealing`; an agenda's own is its `steal_cost`");
                }
                (ContinuousKind::ScoreCost(_), Scope::Scoring(_)) => {}
                (ContinuousKind::ScoreCost(_), _) => return misfit("ScoreCost", "an additional cost to score is about an agenda being `Scoring`"),
                (ContinuousKind::BasicTrashCost(_), Scope::This) if self.card_type == CardType::Resource => {}
                (ContinuousKind::BasicTrashCost(_), Scope::Trashing(_)) => {}
                (ContinuousKind::BasicTrashCost(_), _) => {
                    return misfit("BasicTrashCost", "the basic action trashes a resource: this one's own, or one being `Trashing`");
                }
                (ContinuousKind::AgendaPoints(_), Scope::ScoreArea(_)) if self.card_type == CardType::Agenda => {}
                (ContinuousKind::AgendaPoints(_), _) => return misfit("AgendaPoints", "an agenda's points change in a score area, said by the agenda (`ScoreArea`)"),
                (ContinuousKind::BoostsLastTheRun, Scope::This | Scope::Host) => {}
                (ContinuousKind::BoostsLastTheRun, _) => return misfit("BoostsLastTheRun", "a boost is an icebreaker's: this card or its host"),
                (ContinuousKind::MayHost, Scope::InstallingOntoThis(_)) => {}
                (ContinuousKind::MayHost, _) => return misfit("MayHost", "what may be installed onto this card is said by `InstallingOntoThis`"),
                (ContinuousKind::CannotBeDeclaredSuccessful | ContinuousKind::AccessOthersAtMost(_), Scope::RunsOnThisServer)
                    if matches!(self.card_type, CardType::Upgrade | CardType::Asset) => {}
                (ContinuousKind::CannotBeDeclaredSuccessful | ContinuousKind::AccessOthersAtMost(_), _) => {
                    return misfit("RunsOnThisServer", "a run's success and its accesses are said of the runs on this card's server, by a card in its root");
                }
                (kind, Scope::Player(_)) if !matches!(kind, ContinuousKind::Cannot(_)) => {
                    return misfit("Player", "only a prohibition is about a player named by side");
                }
                (_, Scope::RunsOnThisServer) => return misfit("RunsOnThisServer", "only a run's success and its accesses are about the runs on a server"),
                (ContinuousKind::BreakLimit { .. } | ContinuousKind::TrashLimit(_), Scope::This) if matches!(self.card_type, CardType::Ice(_)) => {}
                (ContinuousKind::BreakLimit { .. } | ContinuousKind::TrashLimit(_), _) => {
                    return misfit("BreakLimit", "what may be broken on or trashed with a piece of ice during its encounter is said by the ice of itself (`This`)");
                }
                (ContinuousKind::RevealedWhileAccessed, Scope::This) if self.side == Side::Corp => {}
                (ContinuousKind::RevealedWhileAccessed, _) => {
                    return misfit("RevealedWhileAccessed", "only a Corp card is accessed, and it says so of itself (`This`)");
                }
                (ContinuousKind::Cannot(what), Scope::Player(side)) if what.binds() == *side => {}
                (ContinuousKind::Cannot(_), _) => return misfit("Cannot", "a prohibition is about the player it binds (`Player`)"),
            }
        }
        Ok(())
    }
}

/// The subtypes `requirement` asks `BrokePrintedSubroutineWith` about,
/// anywhere in it.
fn broke_printed_with(requirement: &EffectRequirement, named: &mut Vec<CardSubtype>) {
    match requirement {
        EffectRequirement::BrokePrintedSubroutineWith(subtype) => named.push(*subtype),
        EffectRequirement::Not(inner) => broke_printed_with(inner, named),
        EffectRequirement::And(a, b) => {
            broke_printed_with(a, named);
            broke_printed_with(b, named);
        }
        _ => {}
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
                subject: Some(Subject::This), when: None, acts_on_subject: false, first_each_turn: false, from_heap: false,
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
                subject: Some(Subject::This), when: None, acts_on_subject: false, first_each_turn: false, from_heap: false,
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
                    cost_discount_if: None, used_by: None, access: false, from_hand: false },
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
                    cost_discount_if: None, used_by: None, access: false, from_hand: false },
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
            triggers: vec![TriggeredEffect { trigger, subject, when, acts_on_subject, first_each_turn: false, from_heap: false, text: None, effects: vec![], requirement: None }],
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
        // A conjunction is two or more parts, each fitting on its own
        // (Hostile Architecture's "the Runner trashes any of your installed
        // cards"), never one part or a conjunction inside another.
        let the_runners = || EventFilter::OwnedBy { owner: Side::Corp, whose: Side::Runner };
        let installed = || EventFilter::TrashedFrom(vec![crate::dsl::TrashedFrom::Installed]);
        let all = |parts: Vec<EventFilter>| Some(EventFilter::All(parts));
        assert_eq!(with(Trigger::OnCardTrashed, Some(Subject::Any), all(vec![the_runners(), installed()]), false).validate(), Ok(()));
        for when in [all(vec![installed()]), all(vec![the_runners(), all(vec![installed(), installed()]).unwrap()]), all(vec![the_runners(), on_hq().unwrap()])] {
            assert!(matches!(
                with(Trigger::OnCardTrashed, Some(Subject::Any), when, false).validate(),
                Err(CardValidationError::TriggerFilterOfTheWrongKind(_, Trigger::OnCardTrashed))
            ));
        }
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
            from_heap: false,
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
        // "This" happens to a scored agenda once; advancing one happens
        // again and again (Sacrifice Zone Expansion).
        assert!(refused(card(Side::Corp, vec![first(Trigger::OnAgendaScored, Some(Subject::This), None, None)])));
        assert_eq!(card(Side::Corp, vec![first(Trigger::OnAdvance, Some(Subject::This), None, None)]).validate(), Ok(()));
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
        let once = |trigger: Trigger| TriggeredEffect { first_each_turn: false, from_heap: false, ..first(trigger, None, None, Some(EffectRequirement::OncePerTurn)) };
        assert_eq!(card(Side::Corp, vec![once(Trigger::OnTagsGiven)]).validate(), Ok(()));
        assert!(matches!(card(Side::Corp, vec![once(Trigger::OnTagsGiven), once(Trigger::OnTagRemoved)]).validate(), Err(CardValidationError::OncePerTurnDoesNotFit(..))));
        assert!(refused(discount(Scope::Controller, None)));
    }

    /// A continuous effect that does not fit parses and then reaches
    /// nothing, which looks like a card that works — so a card file is
    /// `IceType::Other` is ice with none of the three types. Restricting a
    /// breaker to it would break what an AI breaks, and gaining it would
    /// gain nothing, so neither parses into a card.
    #[test]
    fn other_is_never_a_type_to_break_or_to_gain() {
        let gains_other = CardDefinition {
            id: CardId("homebrew".to_string()),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::Barrier),
            strength: Some(1),
            continuous: vec![ContinuousEffect {
                kind: ContinuousKind::GainSubtype(IceType::Other),
                applies_to: Scope::This,
                condition: None,
                first_each_turn: false,
                text: None,
            }],
            ..Default::default()
        };
        assert!(matches!(gains_other.validate(), Err(CardValidationError::OtherIsNotAnIceType(..))));
        let breaks_other = CardDefinition {
            id: CardId("homebrew".to_string()),
            side: Side::Runner,
            card_type: CardType::Program,
            strength: Some(1),
            abilities: vec![AbilityDef {
                trigger: Trigger::Paid,
                text: Some("Break 1 subroutine.".to_string()),
                cost: None,
                requirement: None,
                effect: Effect::BreakSubroutines { count: crate::dsl::SubroutineBreakCount::Fixed(1), restrict_to: Some(IceType::Other) },
                cost_discount_if: None,
                used_by: None,
                access: false,
                from_hand: false,
            }],
            ..Default::default()
        };
        assert!(matches!(breaks_other.validate(), Err(CardValidationError::OtherIsNotAnIceType(..))));
    }

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
    /// An ability used from the hand is taken as an action, so it must be
    /// one: a paid ability whose cost begins with [click].
    #[test]
    fn validate_refuses_a_hand_ability_that_is_not_an_action() {
        let ice = |cost: Cost| CardDefinition {
            id: CardId("alarm".to_string()),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::CodeGate),
            strength: Some(1),
            abilities: vec![AbilityDef {
                trigger: Trigger::Paid,
                text: None,
                cost: Some(cost),
                requirement: None,
                effect: Effect::EndTheRun,
                cost_discount_if: None,
                used_by: None,
                access: false,
                from_hand: true,
            }],
            ..CardDefinition::default()
        };
        assert_eq!(ice(Cost::AllOf(vec![Cost::Clicks(1), Cost::RevealAndTrashSelf])).validate(), Ok(()));
        assert_eq!(ice(Cost::RevealAndTrashSelf).validate(), Err(CardValidationError::HandAbilityNotAnAction(CardId("alarm".to_string()))));
    }

    /// A gained subroutine is the encountered ice's for the encounter, so
    /// only a trigger on the encounter that acts on "it" may say one.
    /// "This ice gains the chosen subtypes" (`Effect::GainIceSubtype`) is
    /// said by ice, of one of the three types.
    #[test]
    fn validate_refuses_a_subtype_gained_by_a_card_that_is_not_ice_or_of_no_type() {
        let card = |card_type: CardType, subtype: IceType| CardDefinition {
            id: CardId("gainer".to_string()),
            side: Side::Corp,
            strength: matches!(card_type, CardType::Ice(_)).then_some(1),
            card_type,
            triggers: vec![TriggeredEffect {
                trigger: Trigger::OnRez,
                subject: Some(Subject::This),
                when: None,
                acts_on_subject: false,
                first_each_turn: false, from_heap: false,
                text: None,
                effects: vec![Effect::GainIceSubtype(subtype)],
                requirement: None,
            }],
            ..Default::default()
        };
        assert_eq!(card(CardType::Ice(IceType::Other), IceType::Sentry).validate(), Ok(()));
        assert!(matches!(card(CardType::Ice(IceType::Other), IceType::Other).validate(), Err(CardValidationError::OtherIsNotAnIceType(..))));
        let asset = card(CardType::Asset, IceType::Sentry).validate();
        assert!(matches!(asset, Err(CardValidationError::SubtypeGainedByNonIce(_))), "{asset:?}");
    }

    #[test]
    fn validate_refuses_a_gained_subroutine_outside_an_encounter() {
        let gains = Effect::GainSubroutine { subroutine: Box::new(SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun, only_breakable_by: None }), after: false, duration: EffectDuration::Encounter, count: None };
        let resource = |trigger: Trigger, acts_on_subject: bool| CardDefinition {
            id: CardId("gainer".to_string()),
            side: Side::Runner,
            card_type: CardType::Resource,
            triggers: vec![TriggeredEffect {
                trigger,
                subject: Some(Subject::Any),
                when: None,
                acts_on_subject,
                first_each_turn: false, from_heap: false,
                text: None,
                effects: vec![gains.clone()],
                requirement: None,
            }],
            ..CardDefinition::default()
        };
        assert_eq!(resource(Trigger::OnEncounter, true).validate(), Ok(()));
        let refused = Err(CardValidationError::GainedSubroutineWithNoEncounter(CardId("gainer".to_string())));
        assert_eq!(resource(Trigger::OnEncounter, false).validate(), refused);
        assert_eq!(resource(Trigger::OnCardInstalled, true).validate(), refused);
    }

    #[test]
    fn validate_refuses_a_prohibition_that_lasts_an_encounter() {
        use crate::dsl::effect::Prohibition;
        let ice = |what, until, encountered_ice| CardDefinition {
            id: CardId("bar".to_string()),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::CodeGate),
            strength: Some(1),
            subroutines: vec![SubroutineDef {
                text: "The Runner cannot steal or trash Corp cards.".to_string(),
                effect: Effect::Sequence(vec![Effect::Prohibit { what, until, copies_of_it: false, this_install: false, encountered_ice }]),
                only_breakable_by: None,
            }],
            ..CardDefinition::default()
        };
        let refused = Err(CardValidationError::ProhibitionForAnEncounter(CardId("bar".to_string())));
        assert_eq!(ice(Prohibition::StealOrTrash, EffectDuration::Run, false).validate(), Ok(()));
        assert_eq!(ice(Prohibition::StealOrTrash, EffectDuration::Turn, false).validate(), Ok(()));
        assert_eq!(ice(Prohibition::StealOrTrash, EffectDuration::Encounter, false).validate(), refused);
        // Banner's: about the encountered ice, for that encounter, and
        // nothing else is.
        assert_eq!(ice(Prohibition::EndTheRun, EffectDuration::Encounter, true).validate(), Ok(()));
        assert_eq!(ice(Prohibition::EndTheRun, EffectDuration::Run, true).validate(), refused);
        assert_eq!(ice(Prohibition::EndTheRun, EffectDuration::Encounter, false).validate(), refused);
        assert_eq!(ice(Prohibition::StealOrTrash, EffectDuration::Encounter, true).validate(), refused);
    }

    /// Which kind of breaker broke the printed subroutines is on the pass,
    /// so only a pass can ask it, and only about an icebreaker's subtype.
    #[test]
    fn validate_refuses_a_breaker_kind_asked_anywhere_but_a_pass() {
        let asks = |trigger, subtype| CardDefinition {
            id: CardId("vsa".to_string()),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::CodeGate),
            strength: Some(2),
            triggers: vec![TriggeredEffect {
                trigger,
                subject: Some(Subject::This),
                requirement: Some(EffectRequirement::Not(Box::new(EffectRequirement::BrokePrintedSubroutineWith(subtype)))),
                effects: vec![Effect::GiveTags(crate::dsl::Amount::Fixed(1))],
                when: None,
                acts_on_subject: false,
                first_each_turn: false,
                from_heap: false,
                text: None,
            }],
            ..CardDefinition::default()
        };
        let refused = Err(CardValidationError::BrokenWithOutsideAPass(CardId("vsa".to_string())));
        assert_eq!(asks(Trigger::OnIcePassed, CardSubtype::Decoder).validate(), Ok(()));
        assert_eq!(asks(Trigger::OnEncounterEnded, CardSubtype::Decoder).validate(), refused);
        assert_eq!(asks(Trigger::OnIcePassed, CardSubtype::Virus).validate(), refused, "not an icebreaker's subtype");
    }

    /// "Host ice" is a Trojan's to say, and only of a moment about a card.
    #[test]
    fn validate_refuses_host_ice_on_a_card_with_no_host() {
        let hears = |installs_on_ice, trigger| CardDefinition {
            id: CardId("saci".to_string()),
            side: Side::Runner,
            card_type: CardType::Program,
            installs_on_ice,
            triggers: vec![TriggeredEffect {
                trigger,
                subject: Some(Subject::Any),
                requirement: None,
                effects: vec![Effect::GainCredits(Side::Runner, 3)],
                when: Some(EventFilter::Host),
                acts_on_subject: false,
                first_each_turn: false,
                from_heap: false,
                text: None,
            }],
            ..CardDefinition::default()
        };
        assert_eq!(hears(true, Trigger::OnDerez).validate(), Ok(()));
        assert_eq!(hears(false, Trigger::OnDerez).validate(), Err(CardValidationError::TriggerFilterOfTheWrongKind(CardId("saci".to_string()), Trigger::OnDerez)));
        assert_eq!(hears(true, Trigger::OnRunStart).validate(), Err(CardValidationError::TriggerFilterOfTheWrongKind(CardId("saci".to_string()), Trigger::OnRunStart)), "a run begins on a server");
    }

    /// Only an install goes into a root or not, and "the first" of the
    /// Corp's root installs is counted: the log sees its ice as ice.
    #[test]
    fn validate_admits_in_root_only_on_an_install_and_as_a_first_time() {
        let hears = |trigger| CardDefinition {
            id: CardId("lago_paranoa_shelter".to_string()),
            side: Side::Runner,
            card_type: CardType::Resource,
            triggers: vec![TriggeredEffect {
                trigger,
                subject: Some(Subject::Any),
                requirement: None,
                effects: vec![Effect::DrawCards(Side::Runner, 1)],
                when: Some(EventFilter::InRoot),
                acts_on_subject: false,
                first_each_turn: true,
                from_heap: false,
                text: None,
            }],
            ..CardDefinition::default()
        };
        assert_eq!(hears(Trigger::OnInstall).validate(), Ok(()));
        assert_eq!(hears(Trigger::OnRez).validate(), Err(CardValidationError::TriggerFilterOfTheWrongKind(CardId("lago_paranoa_shelter".to_string()), Trigger::OnRez)));
    }

    /// `Amount::ChosenNumber` means something only inside the `then` of the
    /// `ChooseNumber` that asked for it. Outside one it parses and is 0 —
    /// a card that reads as working and removes no tags.
    #[test]
    fn validate_refuses_an_access_flag_that_nothing_could_use() {
        let card = |side, trigger| CardDefinition {
            id: CardId("baz".to_string()),
            side,
            card_type: CardType::Resource,
            abilities: vec![AbilityDef {
                text: None,
                trigger,
                cost: None,
                requirement: None,
                effect: Effect::TrashCurrentlyAccessedCard,
                cost_discount_if: None,
                used_by: None,
                access: true,
                from_hand: false,
            }],
            ..CardDefinition::default()
        };
        assert_eq!(card(Side::Runner, Trigger::Paid).validate(), Ok(()));
        let refused = Err(CardValidationError::AccessFlagOnWhatCannotBeOne(CardId("baz".to_string())));
        assert_eq!(card(Side::Corp, Trigger::Paid).validate(), refused);
        assert_eq!(card(Side::Runner, Trigger::OnRunStart).validate(), refused);
    }

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
            secret: false,
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
            secret: false,
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
