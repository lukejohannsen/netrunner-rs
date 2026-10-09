use serde::{Deserialize, Serialize};

use crate::dsl::cost::Cost;
use crate::dsl::effect::Effect;
use crate::dsl::trigger::Trigger;
use crate::dsl::zone::CardZoneRef;
use crate::rules::Side;

/// A single costed/manually-activated ability: when/how it fires
/// (`trigger`), what must be paid to make it fire (`cost` — `None` for
/// automatic triggers), and what it does (`effect`, singular). Distinct
/// from `dsl::card::TriggeredEffect`, which models the common no-cost,
/// possibly-multi-effect case ("when played, do these N things for
/// free") — `AbilityDef` is scoped to what `TriggeredEffect` structurally
/// can't express: an optional `Cost`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityDef {
    pub trigger: Trigger,
    /// The printed line this ability implements, quoted from the card —
    /// "3[credit]: +1 strength." — for a client to label it with the
    /// card's own words; gated against the printed text by
    /// `printed_clauses_are_quoted_from_the_card`. Never read by the
    /// engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    /// A precondition gating whether this ability may even be activated,
    /// checked (via `rules::ability::check_requirement`) before `cost` is
    /// paid. `None` for the common case of no precondition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<EffectRequirement>,
    pub effect: Effect,
    /// A conditional discount off this specific ability's `cost` —
    /// `(condition, amount)` — applied every time `condition` holds, e.g.
    /// Marjanah's "if you made a successful run this turn, this ability
    /// costs 1 credit less to use." `None` for the common case. The
    /// per-install-cost sibling is a `ContinuousKind::InstallCost` on the
    /// card (Carmen); `engine::activate_ability`'s cost computation reads
    /// this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_discount_if: Option<(EffectRequirement, u32)>,
    /// Who may activate this ability, when that is not the card's own
    /// controller — N-Pot's "3[c]: break 1 subroutine on this ice. **Only
    /// the Runner can use this ability.**" The Runner pays the cost and
    /// the effect resolves as the ice. `None`, the common case, means the
    /// side whose card it is.
    ///
    /// Read by `engine::activate_ability` (which side pays and is checked
    /// for priority) and by `legal_actions::action_owner` (which seat is
    /// offered it), so the offer and the resolution agree about whose
    /// action it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_by: Option<crate::rules::Side>,
    /// The printed "Access →" flag (CR 9.3.6b): a *mid-access ability*
    /// (CR 9.5.2c), used only in the mid-access window at step 7.2.2 of
    /// accessing a card, and there only by the Runner, once per access
    /// (CR 9.2.10). Never in a paid ability window, which admits no
    /// mid-access ability (CR 9.2.7b).
    ///
    /// A flag on the ability rather than a requirement: Gourmand and
    /// Carnivore used to say it as `CurrentlyAccessingACard` and were
    /// used in the paid ability window the engine opened at each accessed
    /// card — a window the rules do not have, in which the Corp could
    /// also rez. And a flag rather than a `Trigger`: it is still a paid
    /// ability, with a trigger cost paid the same way (CR 9.5.1).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub access: bool,
    /// Used while the card is in its owner's hand, and only there — Tocsin's
    /// "[click], 1[credit], reveal and trash this ice from HQ:". An ability
    /// that can only affect the game from a zone is active in it (CR
    /// 9.1.8b), so this card has an ability in HQ and none on the table.
    /// Its own action, `PlayerAction::ActivateHandAbility`, because
    /// `ActivateAbility` names an install and a card in HQ has none. The
    /// only kind a pool card prints is an action (it begins with [click]),
    /// and `validate` holds it to that.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub from_hand: bool,
    /// This entry is the same printed ability as the earlier entry at this
    /// index, written as two because the card prices its first use apart —
    /// Pauleʼs Café's "1[credit]: Install 1 hosted card. The first card you
    /// install this way during each of your turns costs … less", whose
    /// plain install reads the discounted one's `OncePerTurn`. A use limit
    /// is the printed ability's (CR 9.3.6g, `OncePerTurnKey::ability`), so
    /// the two entries share one. `None`, the common case, is an ability of
    /// its own; `validate` holds the index to an earlier entry that names a
    /// `OncePerTurn`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_of: Option<usize>,
}

impl AbilityDef {
    /// Whether using this ability is an *action*: a paid ability whose
    /// trigger cost begins with [click] (CR 9.5.2a, 5.2.1). It is then
    /// taken in the action window of its user's turn and nowhere else —
    /// not in a paid ability window (CR 9.2.7b: "players cannot trigger
    /// actions ... in a paid ability window") and not during a run, which
    /// is an action still resolving (CR 5.2.2a) — and it is followed by
    /// what follows any action. A click further in ("Lose [click]") does
    /// not make one (CR 5.2.1a); no pool card prints that on an ability.
    pub fn is_action(&self) -> bool {
        self.trigger == Trigger::Paid && self.cost.as_ref().is_some_and(Cost::begins_with_click)
    }
}

/// A precondition gating an `AbilityDef`'s activation, a `CardDefinition::
/// play_requirement`'s play legality, or (as a soft/silent gate — see
/// `dsl::card::TriggeredEffect::requirement`) a `TriggeredEffect`'s firing.
/// Kept minimal — extend as new tag/state-conditional card text is needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectRequirement {
    /// The Runner must have at least one tag (`RunnerState::is_tagged()`).
    IsTagged,
    /// A printed "Once per turn →": true until this card has used the
    /// ability this turn, then false until the next turn begins, either
    /// side's. **A use limit on the card, not a fact about the turn** —
    /// what was used is `once_per_turn_used` on the card's side, keyed by
    /// the card and which copy of it (`OncePerTurnKey`), so three Telework
    /// Contracts have three uses. It carried a free-form tag until the
    /// view began carrying the set; every card file spelled the tag as its
    /// own id, and a shared string was a shared use. Checked read-only by
    /// `check_requirement` and spent by `consume_requirement` where the
    /// ability is actually used: a trigger that fired, a paid ability that
    /// resolved, an install that took a discount.
    OncePerTurn,
    /// A printed "Use this ability only once per run": `OncePerTurn`'s use
    /// limit, spent the same way, but held by the run
    /// (`RunState::once_per_run_used`) and so false outside one. Pressure
    /// Spike's threat pump. Composition didn't work: `OncePerTurn` would
    /// deny the pump to the turn's second run, and a lingering prohibition
    /// has nothing to name one ability of one card by.
    OncePerRun,
    /// "Use this ability only once per encounter" (Slap Vandal) — the
    /// encounter's twin of `OncePerRun`, kept on the encounter's tally
    /// (`EncounterTally::once_per_encounter_used`). Composition didn't
    /// work: `OncePerRun` would refuse the ability at the run's second
    /// piece of ice.
    OncePerEncounter,
    /// "Use this ability only during your turn" — the side asking is the
    /// active player (`listeners::active_side`), which holds through the
    /// paid ability windows of that player's own turn, runs included.
    /// Coalescence, and Valentina Ferreira Carvalho's "when you install
    /// this resource during your turn". Composition didn't work: no
    /// requirement or `Amount` reads whose turn it is, and `GamePhase::
    /// Action` is only a step of it.
    DuringYourTurn,
    /// The Runner's credit total is at most `0` — e.g. Whitespace's second
    /// subroutine ("if the Runner has 6 credits or less, end the run").
    RunnerCreditsAtMost(u32),
    /// The Runner has at least this many clicks left — e.g. Creative
    /// Commission/VRcation's "if you have any [click] remaining, lose
    /// [click]" (`RunnerClicksAtLeast(1)`). Checked *after* the event's own
    /// play cost has been paid, so playing one as the turn's last click
    /// correctly finds zero remaining and skips the loss rather than
    /// underflowing.
    RunnerClicksAtLeast(u32),
    /// `zone` (relative to the acting side, as `CardZoneRef` always is)
    /// holds at least `count` cards — e.g. Carnivore's "trash 2 cards from
    /// your grip", which is only *offerable* with 2 cards to trash. A gate
    /// on the ability, not a branch in its effect: `EffectIf` could skip the
    /// trash, but the ability would still be activated, its click paid and
    /// its `OncePerTurn` consumed, while `PromptChooseCards` silently
    /// declined to park with fewer than `min` eligible. The requirement is
    /// what keeps `legal_actions` from offering it at all.
    ZoneHasAtLeast {
        zone: CardZoneRef,
        count: u32,
        /// Count only the cards eligible under this filter — Maglectric
        /// Rapid's "derez 1 installed Corp card" is only worth offering
        /// while a rezzed one exists, and MuslihaT's "if that card is an
        /// icebreaker or a run event" gates a prompt on the top card's
        /// type. `None` (the default, and what every earlier card
        /// authored) counts the whole zone.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<crate::dsl::CardFilter>,
    },
    /// Generic negation of another requirement.
    Not(Box<EffectRequirement>),
    /// Generic conjunction of two requirements — e.g. Zahya Sadeghi's
    /// "once per turn" gate combined with "only when the run targeted HQ
    /// or R&D."
    And(Box<EffectRequirement>, Box<EffectRequirement>),
    /// A run is currently active against the server the checking card
    /// (`acting_card`/the triggering card) is itself installed in, and
    /// that run hasn't passed the ICE-encounter stage yet (`RunPhase::
    /// ApproachIce`/`EncounterIce`) — e.g. Ping's "when you rez this ice
    /// during a run against this server."
    RezzedDuringRunAgainstThisServer,
    /// A run is active against the server the checking card is installed
    /// in, at any step — The Red Room's "use this ability only during a run
    /// against another server" is `And(DuringRun, Not(this))`.
    /// `RezzedDuringRunAgainstThisServer` also asks about the step (before
    /// the server is reached), which "during a run against" does not. A
    /// Trojan's server is its host's: Living Mural's "a **sentry protecting
    /// this server**" is this with `Encountering(Sentry)`, since every piece
    /// of ice a run encounters protects the attacked server.
    RunAgainstThisServer,
    /// The most recent `Effect::DealDamage` **in this same resolution**
    /// discarded at least one card whose registry `cost` is odd — e.g.
    /// Diviner's subroutine ("if you trash a card this way with a printed
    /// play or install cost that is an odd number, end the run").
    ///
    /// Read from `ability::ResolutionContext::damage_discarded`, which
    /// `damage::apply_damage` fills for the `Sequence` that called it. A
    /// resolution that dealt no damage answers "not met" rather than
    /// inheriting an earlier action's discards.
    LastDamageTrashedOddCostCard,
    /// The most recent `Effect::DealDamage` in this same resolution
    /// discarded a card the filter admits — Saisentan's "whenever you
    /// trash a card **of the chosen type** with net damage from a
    /// subroutine on this ice, do 1 net damage" (`OfChosenCardType`, the
    /// type the ice remembered as it was encountered). Read from
    /// `ResolutionContext::damage_discarded`, as `LastDamageTrashedOdd
    /// CostCard` is. Composition didn't work: that one asks one fixed
    /// question of the discards, and this card's is a filter; Diviner's
    /// stays, because no filter says "an odd printed cost".
    LastDamageTrashed(crate::dsl::CardFilter),
    /// The most recently concluded run (`GameState::last_completed_run`)
    /// targeted HQ or R&D — e.g. Zahya Sadeghi's "when a run on HQ or R&D
    /// ends."
    LastRunWasOnHqOrRnD,
    /// The Runner stole at least one agenda during the most recently
    /// concluded run (`state::CompletedRun::agendas_stolen`) — e.g. AMAZE
    /// Amusements' "whenever a run on this server ends, if the Runner stole
    /// any agendas during that run". Pairs with `Trigger::OnRunEnded`,
    /// which is the only point at which that snapshot is populated.
    StoleAgendaDuringLastRun,
    /// The most recently concluded run was unsuccessful
    /// (`state::CompletedRun::unsuccessful`, CR 6.8.4) — Hannah "Wheels"
    /// Pilintra's "When that run ends, if it was unsuccessful, take 1 tag",
    /// asked by the run's end effect (`Effect::SetRunEndedEffect`).
    /// Composition didn't work: nothing said whether a run had failed, and
    /// "not successful" is not it (a run Crisium Grid stops is neither).
    LastRunUnsuccessful,
    /// The most recently concluded run was successful (`state::
    /// CompletedRun::successful`, declared so: CR 6.9.5a) — Boomerang's
    /// "When this run ends, if it was successful, you may shuffle 1 copy of
    /// Boomerang from your heap into your stack", asked by the run's end
    /// effect. Composition didn't work: `Not(LastRunUnsuccessful)` admits
    /// a run that was neither (6.8.4a and b: one Flagship kept from being
    /// declared successful, one whose remote ceased to exist).
    LastRunSuccessful,
    /// The Runner breached the server of the most recently concluded run
    /// during it (`state::CompletedRun::breached`) — Info Bounty's "gain
    /// 2[credit] **if you breached that server during that run**", asked
    /// as the run ends. Composition didn't work: a successful run is not a
    /// breach of its server (a breach can be replaced by another server's,
    /// and Flagship's run breaches without succeeding), and nothing kept
    /// the breach past the run.
    BreachedLastRunsServer,
    /// At least one card in the Corp's Archives is facedown
    /// (`state::ArchivedCard::facedown`) — e.g. Jinteki: Restoring
    /// Humanity's "if there is a facedown card in Archives".
    ArchivesHasFacedownCard,
    /// The access currently being resolved is in this server — read from
    /// `active_run.access_state.server`, so it answers "where is the Runner
    /// accessing this card *right now*", not where the card lives.
    ///
    /// Not composable from the existing vocabulary: nothing else in it can
    /// see the accessed server at all. Exists for Snare!'s "when the Runner
    /// accesses this asset anywhere except in Archives", which is spelled
    /// `Not(AccessingIn(Archives))` — hence the positive form here, leaving
    /// the negation to the `Not` combinator that already exists — and for
    /// "while the Runner is accessing this asset in R&D" (Esca, Snare!,
    /// Byte!: `ContinuousKind::RevealedWhileAccessed`). It was
    /// `AccessingArchives` until the second server was printed.
    AccessingIn(crate::rules::ServerId),
    /// `acting_card`'s current generic counter total (wherever it's
    /// currently installed/rigged) is at most `amount` — e.g. a
    /// hosted-credit-pool resource/asset detecting "this pool is now empty"
    /// right after spending it down, to gate an auto-trash `EffectIf`
    /// (Red Team, Telework Contract, Regolith Mining License, Nico
    /// Campaign). `RulesError::RequirementNotMet` (not a "card not found"
    /// error) if `acting_card` isn't currently installed/rigged at all —
    /// treated the same as "0 counters," since a card that's already gone
    /// trivially has no counters left to be above the threshold.
    ThisCardCountersAtMost(u32),
    /// `acting_card`'s current generic counter total is at least `amount`
    /// — the inverse comparison to `ThisCardCountersAtMost` — e.g.
    /// Tranquilizer's "if there are 3 or more hosted virus counters, derez
    /// host ice." Same "not installed/rigged at all" treatment as
    /// `ThisCardCountersAtMost` (0 counters, so never satisfied for
    /// `amount >= 1`).
    ThisCardCountersAtLeast(u32),
    /// The Runner's identity is on its flip side (`RunnerState::
    /// identity_flipped`) — the gate on a flip identity's back-side
    /// trigger (Dewi Subrotoputri). Its front-side twin is `Not(IdentityFlipped)`.
    IdentityFlipped,
    /// The controller's identity matches the filter, read off its
    /// definition — DreamNet's "If your identity is **digital**",
    /// `IdentityMatches(HasSubtype(Digital))`. Composition didn't work:
    /// `ActingCardMatches` reads the card resolving, which is DreamNet, and
    /// no other requirement reads a card that is neither acting nor in a
    /// zone. A flip identity's back side has the front's subtypes: the
    /// catalog gives only the front's (`docs/roadmap/nsg-card-pool.md`,
    /// Known limits).
    IdentityMatches(crate::dsl::CardFilter),
    /// The card the triggering event is about is of the Runner identity's
    /// faction — Storgotic Resonator's "a card that matches the faction of
    /// the Runner's identity". Composition didn't work: a `when` filter is
    /// read off the card's definition alone, with no state to find the
    /// Runner's identity in, and `IdentityMatches` reads the controller's
    /// identity, not the card that was trashed.
    TriggeringCardOfRunnersFaction,
    /// It is the controller's action phase — Daily Quest's "Rez only during
    /// your action phase" (`CardDefinition::rez_requirement`). Not
    /// `DuringYourTurn`, which holds through the windows as the turn begins
    /// and in the discard phase.
    DuringYourActionPhase,
    /// The Runner made a successful run on the server the acting card is
    /// installed in during their last turn — Daily Quest's "if the Runner
    /// did not make a successful run on this server during their last
    /// turn", asked as the Corp's turn begins. Read off
    /// `RunnerState::servers_run_successfully`, which holds the Runner's
    /// most recent turn's until their next begins. Composition didn't work:
    /// the turn log counts the remotes as one class, and "this server" is
    /// one of them.
    RunnerSucceededOnThisServerLastTurn,
    /// The controller's identity is copy `n` of itself (`CorpState::
    /// identity_copy`, set by `Effect::SetIdentityCopy`) — the gate on
    /// each of Méliès U's three reverse sides ("Side 1: When you flip this
    /// identity to this side…"): one card file holds all three, as one
    /// holds both sides of a flip identity, and only the copy in play
    /// speaks (CR 1.5.2b, 3.1.1a).
    IdentityCopy(u8),
    /// A run on `server` is in progress, at any step of it — Méliès U's
    /// "when you flip this identity to this side **during a run on HQ**".
    /// The server the run is on (`RunState::server`), which is where it
    /// was declared unless a card has moved it. Unlike `DuringRun`, the
    /// breach counts: nothing here moves the run.
    DuringRunOn(crate::rules::ServerId),
    /// The run in progress is breaching `server` (`RunState::breached`,
    /// from the breach's beginning to its end) — Mercury: Chrome
    /// Libertador's "When you breach HQ or R&D **during a run**", whose one
    /// printed ability needs to know which of the two it is adding an
    /// access to. Composition didn't work: `DuringRunOn` is the server the
    /// run is on, and a run on HQ can breach R&D instead (Beatriz Friere
    /// Gonzalez); `AccessingIn` is false until the first card is accessed,
    /// which is after "access 1 additional card" must be applied (CR
    /// 7.3.5b). A breach with no run (Cataloguer's) is not one.
    Breaching(crate::rules::ServerId),
    /// A run is in progress, at any step of it, the breach included (CR
    /// 6.9) — AirbladeX (JSRF Ed.)'s interrupt, "Prevent 1 net damage. Use
    /// this ability only during a run", whose damage can come from a card
    /// accessed (Snare!). Not `DuringRun`, which stops as the breach begins,
    /// where no paid ability window opens: the cards that read it move the
    /// run or act before the server, and an interrupt is used in the
    /// prevention window the breach's damage opens. A breach with no run is
    /// not one.
    RunInProgress,
    /// The acting agenda is in the Runner's score area — Oracle Thinktank's
    /// "The Corp can use this ability only if this agenda is in the
    /// Runner's score area". Its ability is found there as well as in the
    /// Corp's own (`engine::activate_ability`), so this is what keeps it
    /// out of the Corp's. Composition didn't work: no requirement asked
    /// where a scored card is.
    InRunnersScoreArea,
    /// The Runner is at the decision about a specific accessed card
    /// (`run::AccessPhase::PendingChoice`) and it is not an agenda —
    /// Gourmand's "trash the non-agenda card you are accessing". Read from
    /// the registry; an unregistered card counts as non-agenda. *That* a
    /// card is being accessed is `AbilityDef::access` on an ability, which
    /// is why `CurrentlyAccessingACard`, the requirement Carnivore said it
    /// with, is gone; this one stays because BANGUN's trigger reads it.
    CurrentlyAccessingNonAgenda,
    /// A subroutine has resolved during the active run
    /// (`RunState::subroutine_resolved`) — Ryō "Phoenix" Ōno's "a run becomes
    /// successful after a subroutine resolved during that run".
    SubroutineResolvedThisRun,
    /// A piece of ice has been derezzed during the active run
    /// (`RunState::ice_derezzed`) — Stegodon MK IV's "Each run, as long as a
    /// piece of ice has been derezzed during that run". Composition didn't
    /// work: the turn log counts derezzes by the turn, and "during that run"
    /// is the run's own.
    IceDerezzedThisRun,
    /// A subroutine has been broken during the active run
    /// (`RunState::subroutine_broken`) — Mercury: Chrome Libertador's "if
    /// you did not break any subroutines during that run", under a `Not`.
    /// Composition didn't work: `SubroutineResolvedThisRun` is the other
    /// fate of a subroutine, and the turn log counts breaks by the turn.
    SubroutineBrokenThisRun,
    /// The Runner is encountering a piece of ice and has already broken a
    /// subroutine during this encounter — Poison Vial's "Use this ability
    /// only if you have already broken a subroutine during this encounter".
    /// Read off the encounter's tally (`EncounterTally::broken_by`), which
    /// a break by any means marks — a breaker's, a click's — and which is
    /// reset as the encounter ends. Composition didn't work:
    /// `SubroutineBrokenThisRun` reaches back to the run's earlier ice.
    SubroutineBrokenThisEncounter,
    /// The acting install broke a subroutine during the encounter whose end
    /// is being heard (`GameEvent::EncounterEnded::broken_with`) — Crypsis's
    /// "Whenever an encounter ends, if you used this program to break a
    /// subroutine during that encounter". False off any other event.
    /// Composition didn't work: `SubroutineBrokenThisEncounter` asks the
    /// live tally, which is reset by the time the end is heard, and asks
    /// about any breaker rather than this one.
    BrokeASubroutineThatEncounter,
    /// The Runner has no unused memory (`memory::available_memory == 0`)
    /// — Dewi Subrotoputri's "if your [mu] is full"; "at least 1 unused
    /// [mu]" is `Not(MemoryFull)`.
    MemoryFull,
    /// `acting_card` is a Trojan Program (`dsl::CardDefinition::
    /// installs_on_ice`, `state::InstalledRunnerCard::hosted_on_ice`)
    /// currently hosted on the ICE the active run is encountering right
    /// now (`RunPhase::EncounterIce`, `RunState::ice[position]` matches
    /// the host) — e.g. Botulus's hosted-counter break ability, which only
    /// makes sense to activate while its host is actually being
    /// encountered. Hard-gates (errors, does not silently skip) via
    /// `AbilityDef::requirement`'s usual treatment.
    EncounteringHostIce,
    /// The active run is encountering the acting card itself — N-Pot's
    /// and Ansel 2.0's "break … subroutines **on this ice**. Only the Runner
    /// can use this ability." The break effect breaks whatever is being
    /// encountered, so without this N-Pot's ability broke a subroutine on
    /// any piece of ice the Runner met while an N-Pot was rezzed elsewhere.
    /// Composition didn't work: `EncounteringHostIce` asks about a Trojan's
    /// host, and the acting card here is the ice.
    EncounteringThisIce,
    /// The active run is encountering the piece of ice the acting card
    /// chose (`Effect::Remember`, `rules::lingering::chosen_card`) —
    /// Boomerang's "Use this hardware only during encounters with that
    /// ice". Composition didn't work: `EncounteringHostIce` asks about a
    /// Trojan's host and `EncounteringThisIce` about the acting ice, and
    /// the chosen ice is neither.
    EncounteringChosenIce,
    /// The active run is encountering ice of the subtype the acting card
    /// chose (`Effect::Remember`, `rules::lingering::chosen_ice_type`),
    /// printed or gained — Chameleon's "Break 1 subroutine on a piece of
    /// ice that has the chosen subtype". Asked as `Encountering` is.
    /// Composition didn't work: `Encountering` names its type in the card
    /// file, and this one is chosen as the program is installed.
    EncounteringChosenIceType,
    /// Subroutines on this ice are resolving — Attini's "while subroutines
    /// on this ice are resolving". The run is encountering the acting ice
    /// and a subroutine on it has resolved this encounter: from the first
    /// resolution at step 6.9.3c, through every interrupt window and
    /// decision a subroutine parks (CR 9.1.2b, whose example is Attini),
    /// until the encounter is complete, which the engine takes straight
    /// after the last one (`paid_ability::resolve_encounter_ice`). Not in
    /// the break window before it (6.9.3b), where nothing has resolved.
    /// Composition didn't work: `EncounteringThisIce` is true in that
    /// window too, and `SubroutineResolvedThisRun` is about any ice this
    /// run.
    ResolvingThisIcesSubroutines,
    /// The active run is encountering a piece of ICE right now
    /// (`RunPhase::EncounterIce`) — any ICE, unlike
    /// `EncounteringHostIce`'s "the one my host is."
    ///
    /// Carried by every icebreaker's `Paid` abilities. Real Netrunner only
    /// lets you use an icebreaker's abilities while encountering ICE;
    /// without this, Cleaver's "2[c]: +1 strength" was a legal action on
    /// the Corp's turn — affordable, permitted, and doing nothing. That
    /// mattered beyond tidiness: it made "does the opponent have a usable
    /// paid ability" answer yes on essentially every action, which is the
    /// gate `WindowCheckpoint::PostAction` depends on.
    DuringEncounter,
    /// The Runner is encountering ice of this type, printed or gained —
    /// Corsair's "the **barrier** you are encountering gets −3 strength".
    /// `DuringEncounter` does not say which ice, and an unrestricted
    /// strength cut would be offered, and paid for, against a sentry.
    /// Asked the way a restricted break is (`Effect::BreakSubroutines`).
    Encountering(crate::dsl::IceType),
    /// The Runner is encountering a piece of ice protecting their mark (CR
    /// 10.11, `lingering::mark`) — Tunnel Vision's "Break up to 2
    /// subroutines on a piece of ice **protecting your mark**". Read off
    /// where the ice is installed, not the run's server, so ice that moved
    /// or a forced encounter is judged by the server it protects; false
    /// while there is no mark. Composition didn't work: no requirement
    /// reads the mark, which is chosen each turn.
    EncounteringIceProtectingMark,
    /// The Runner's mark is `server` (CR 10.11, `lingering::mark`) — Carpe
    /// Diem's "You may run **your mark**", written as one branch per central
    /// server, and Backstitching's "during a run **on your mark**", which
    /// pairs it with `DuringRunOn`. False while there is no mark.
    /// Composition didn't work: a run's target is a `ServerId`, which has
    /// no word for the mark, and `EncounteringIceProtectingMark` asks where
    /// an encountered ice is installed, not which server the mark is.
    MarkIs(crate::rules::ServerId),
    /// The active run was begun by this card (`RunState::initiated_by`) —
    /// Baker's "[click]: Run Archives. When you would approach Archives…",
    /// whose second sentence is about the run its first began.
    ThisCardStartedTheRun,
    /// The run has passed all its ice and will approach this server next
    /// (the movement phase with no position left inward, CR 6.9.4g), with
    /// no redirect already set — the last paid ability window before
    /// Baker's "when you would approach Archives (after passing all ice),
    /// you may pay 1[credit] to instead change the attacked server".
    AboutToApproach(crate::rules::ServerId),
    /// The most recently concluded run (`GameState::last_completed_run`)
    /// accessed at least one card — e.g. Zahya Sadeghi, whose "once per
    /// turn" is consumed the moment her trigger fires, and whose
    /// `Trigger::OnRunEnded` also fires for a bounced or jacked-out run on
    /// HQ/R&D where the gain would be 0. Without this gate a 0-access run
    /// silently burned the once-per-turn a later run that turn would have
    /// paid out on.
    AccessedAnyCardDuringLastRun,
    /// The reacting card is, right now, an installed copy — the dispatch
    /// named an install that is still on the table. e.g. Urtica Cipher's
    /// "when the Runner accesses this asset **while it is installed**":
    /// `Trigger::OnAccessed` also fires for an R&D/HQ/Archives access,
    /// where an ambush's own text says it does not apply. Checked off
    /// `ResolutionContext::acting_install` (present exactly when the
    /// trigger fired against a live install), never by first-match
    /// `CardId` — a copy installed elsewhere must not answer for the copy
    /// being accessed out of R&D.
    ThisCardIsInstalled,
    /// This card hosts an installed card the filter admits (`state::
    /// InstalledRunnerCard::hosted_on_rig_card`) — Hackerspace's "while
    /// this resource has a hosted **companion** and a hosted
    /// **connection**", two of these under `And`. Not a count through
    /// `AmountAtLeast`: `Amount::HostedCards` counts the cards hosted
    /// without being installed, and this asks about kinds of installed
    /// ones.
    HostsInstalled(crate::dsl::CardFilter),
    /// The ice being encountered hosts an installed card the filter admits
    /// — Umbrella's "This program can only interface with ice hosting a
    /// **trojan** program" (`state::InstalledRunnerCard::hosted_on_ice`).
    /// Composition didn't work: `HostsInstalled` asks about the acting
    /// card, and `EncounteringHostIce` about the acting card's host.
    EncounteredIceHosts(crate::dsl::CardFilter),
    /// The advancement just placed by the `Trigger::OnAdvance` event
    /// currently being dispatched was the first one this card has ever
    /// received — e.g. Weyland Consortium: Built to Last's "whenever you
    /// advance a card, gain 2 credits if it had no advancement counters."
    ///
    /// Answered straight from the triggering event
    /// (`ability::ResolutionContext::triggering_event`): `GameEvent::
    /// CardAdvanced`'s `advancement_tokens == 1` *is* "this was the first".
    /// No `GameState` field backs it — the event already carried the fact.
    WasFirstAdvancementThisCard,
    /// The tags just taken (the triggering `GameEvent::TagsGiven`) were
    /// taken with none — Sebastião Souza Pessoa's "whenever you take 1 or
    /// more tags, if you had no tags". Read off the event, which says how
    /// many the Runner had (`TagsGiven::had`), the way
    /// `WasFirstAdvancementThisCard` reads `CardAdvanced`. Composition
    /// didn't work: by the time the trigger resolves the state holds the
    /// tags just taken, and `Not(AmountAtLeast(RunnerTags, n))` cannot
    /// name the n the event gave.
    HadNoTags,
    /// In the encounter the triggering pass ended, an icebreaker of this
    /// subtype broke one of the ice's printed subroutines — Virtual Service
    /// Agent's "if they did not break its printed subroutine with a
    /// decoder during that encounter", as `Not`. Read off the pass
    /// (`GameEvent::IcePassed::printed_broken_with`), the way `HadNoTags`
    /// reads `TagsGiven`. Composition didn't work: the encounter's tally is
    /// reset as the movement phase begins, before the pass's triggers
    /// resolve, and no other word says which kind of breaker broke a
    /// subroutine. `validate` holds it to `OnIcePassed` and to the four
    /// icebreaker subtypes (`run::BrokenWith::KINDS`).
    BrokePrintedSubroutineWith(crate::dsl::CardSubtype),
    /// The Corp has at least this many credits — Fransofia Ward's "if the
    /// Corp has 15[c] or more". `RunnerCreditsAtMost`'s Corp-side sibling.
    CorpCreditsAtLeast(u32),
    /// The active run was started by a `CardSubtype::Run` event
    /// (`run::RunState::initiated_by`) — Sang Kancil's "if a run event is
    /// active, this ability costs 2[c] less to use".
    RunEventActive,
    /// The install this trigger is reacting to cost no credits — Bling's
    /// "whenever you install a card without spending credits". Answered
    /// from the triggering `ProgramInstalled`/`HardwareInstalled`/
    /// `ResourceInstalled` event's `credits_paid`, the same way
    /// `WasFirstAdvancementThisCard` reads `CardAdvanced`.
    InstalledWithoutSpendingCredits,
    /// `Amount`, resolved against the acting card, is at least `u32` —
    /// Syailendra's "if it has 3 or more hosted advancement counters"
    /// (`Amount::HostedAdvancementTokens`). The comparison primitive over
    /// the `Amount` vocabulary, so the next threshold (a counter count, a
    /// card count, a threat level) is a new `Amount` at most, never a new
    /// requirement. `ThisCardCountersAtLeast` predates it and stays.
    AmountAtLeast(crate::dsl::Amount, u32),
    /// The first `Amount` is greater than the second — Piranhas's "End the
    /// run if there are more cards in HQ than in the grip". `AmountAtLeast`
    /// compares against a number the card file writes, and this compares
    /// two the state decides; no pool card compared two before.
    MoreThan(crate::dsl::Amount, crate::dsl::Amount),
    /// The active side has not finished an action yet this turn
    /// (`TurnLog::actions_finished`) — Petty Cash's play condition. Not an
    /// `Amount::TimesThisTurn`: finishing an action is not a moment any
    /// card hears, so it has no `Trigger` to be counted under. Not
    /// derivable from clicks either: see the field's doc.
    NoActionTakenThisTurn,
    /// The triggering `OperationPlayed` was played from Archives rather
    /// than HQ — Petty Cash's "if you played this operation from anywhere
    /// except HQ, gain [click]". Fails with no triggering event.
    PlayedFromArchives,
    /// The card the Runner is currently accessing is an *installed* card
    /// (`run::AccessState::pending_install`), rather than one found in HQ,
    /// R&D or Archives — with `rezzed_only`, an installed card that is
    /// also faceup.
    ///
    /// Two cards read it from opposite ends of one access: BANGUN's
    /// "whenever the Runner accesses a faceup installed agenda"
    /// (`rezzed_only: true`, and `Not(CurrentlyAccessingNonAgenda)` for the
    /// agenda half — the two compose, so neither needs a fused variant),
    /// and Aggressive Trendsetting's "the first time the Runner trashes an
    /// installed Corp card", checked from `Trigger::OnTrashedFromAccess`
    /// while the access that trashed it is still the pending one.
    /// A run is in progress and has not yet reached access — Proprionegation's
    /// "use this ability only during a run". `DuringEncounter` is the
    /// narrower sibling (the Runner committed to a specific piece of ice);
    /// this one is any point from initiation to the server approach, which
    /// is where a card that *moves* the run can still do something.
    DuringRun,
    /// The agenda whose scoring or theft is being reacted to came from the
    /// server this card sits on or protects — Lamplighter's "when an
    /// agenda is scored or stolen from this server or its root, trash this
    /// ice". Every rezzed install hears the event (Phật Gioan Baotixita
    /// reacts from anywhere), so the narrowing is the card's to declare.
    AgendaCameFromThisCardsServer,
    /// This card is installed protecting a remote server — Palisade's
    /// "while this ice is protecting a remote server", which was
    /// `StrengthModifier::WhileProtectingRemote` until ice strength became
    /// a continuous effect. Nothing composes to it: no requirement reads
    /// where the acting card is installed, and "a remote" is every server
    /// that is not one of three, which a list of ids cannot say
    /// (`EventFilter::Server`'s doc has the same sentence).
    ProtectingRemote,
    /// This card is installed protecting this server — Bathynomus's "while
    /// this ice is protecting **Archives**", `ProtectingRemote`'s sibling
    /// for a server with a name. Composition didn't work: `ProtectingRemote`
    /// is every server but three, and `ActingCardMatches(InServer(..))`
    /// reads the definition, which has no place; a scope that reads the
    /// copy (`IceProtectingThisServer`) is about every ice in a server,
    /// not this one.
    Protecting(crate::rules::ServerId),
    CurrentlyAccessingInstalledCard {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        rezzed_only: bool,
    },
    /// The card this resolves as matches the filter, read off its
    /// definition — Reanimation Protocol's "if you rezzed a piece of
    /// **non-liability** ice this way", asked as the ice just rezzed
    /// (`PromptInstallCorpCard::if_rezzed`). Composition didn't work: every
    /// other requirement that reads a card reads the triggering event's
    /// (`EventFilter::Card` is a trigger condition, not an "if") or a zone
    /// (`ZoneHasAtLeast`), and a card that has just been installed is in
    /// neither. The definition's half, and the copy's where the copy can
    /// say (`pending_choice::copy_matches`): Euler's "Use this ability only
    /// if this program was installed this turn" is
    /// `ActingCardMatches(InstalledThisTurn)` on the program's own copy.
    /// Any other instance filter (`Rezzed`) passes, as in `EventFilter::Card`.
    ActingCardMatches(crate::dsl::CardFilter),
    /// The scored agenda this resolves as was scored this turn —
    /// Witch Hunt's "When your action phase ends, if you scored this agenda
    /// this turn". Read off the copy in the Corp's score area
    /// (`ScoredAgenda::scored_on_turn`), by install, so a Witch Hunt scored
    /// last turn does not answer for one scored now; a stolen agenda was
    /// not scored and never answers. Not `AmountAtLeast(TimesThisTurn(
    /// OnAgendaScored), 1)`: that is *an* agenda, and a second Witch Hunt,
    /// or an earlier one, would have given the Runner 3 tags again.
    ThisAgendaScoredThisTurn,
    /// The card this resolves as could be scored now (`engine::scorable`)
    /// — Big Deal's "if able", asked ahead of its "you may score that
    /// card" so the choice is offered only when there is a score to take.
    /// Composition didn't work: `ActingCardMatches` reads the definition,
    /// and whether an agenda can be scored is the table's — its counters,
    /// its requirement as it stands, a lock, a cost.
    Scorable,
}

impl EffectRequirement {
    /// See `Effect::with_chosen_number`: a condition inside a chosen
    /// number's `then` may read the number ("if you removed 2 or more").
    /// See `Effect::with_chosen_name`: Complete Image's "if you trash a
    /// card with the chosen name this way".
    pub fn with_chosen_name(self, card: &crate::dsl::CardId) -> EffectRequirement {
        match self {
            EffectRequirement::LastDamageTrashed(filter) => EffectRequirement::LastDamageTrashed(filter.with_chosen_name(card)),
            EffectRequirement::ActingCardMatches(filter) => EffectRequirement::ActingCardMatches(filter.with_chosen_name(card)),
            EffectRequirement::Not(inner) => EffectRequirement::Not(Box::new(inner.with_chosen_name(card))),
            EffectRequirement::And(a, b) => EffectRequirement::And(Box::new(a.with_chosen_name(card)), Box::new(b.with_chosen_name(card))),
            other => other,
        }
    }

    pub fn with_chosen_number(self, number: u32) -> EffectRequirement {
        match self {
            EffectRequirement::AmountAtLeast(crate::dsl::Amount::ChosenNumber, at_least) => {
                EffectRequirement::AmountAtLeast(crate::dsl::Amount::Fixed(number), at_least)
            }
            EffectRequirement::Not(inner) => EffectRequirement::Not(Box::new(inner.with_chosen_number(number))),
            EffectRequirement::And(a, b) => {
                EffectRequirement::And(Box::new(a.with_chosen_number(number)), Box::new(b.with_chosen_number(number)))
            }
            other => other,
        }
    }

    /// Whether what this gates may be used from the Runner's score area:
    /// an `InRunnersScoreArea` the ability requires, alone or under `And`
    /// (never under `Not`, which keeps it out). An agenda there is
    /// inactive "unless the agenda's card text specifies otherwise" (CR
    /// 3.2.3, 4.5.4), and Oracle Thinktank's "only if this agenda is in
    /// the Runner's score area" is the pool's one card that does.
    pub fn works_from_runners_score_area(&self) -> bool {
        match self {
            EffectRequirement::InRunnersScoreArea => true,
            EffectRequirement::And(one, other) => one.works_from_runners_score_area() || other.works_from_runners_score_area(),
            _ => false,
        }
    }

    /// Whether this is, or contains under `And`/`Not`, a `OncePerRun` — or
    /// a `OncePerEncounter`, the same use limit over a shorter stretch,
    /// keyed the same way and held to the same two rules by `validate`.
    pub fn mentions_once_per_run(&self) -> bool {
        match self {
            EffectRequirement::OncePerRun | EffectRequirement::OncePerEncounter => true,
            EffectRequirement::And(one, other) => one.mentions_once_per_run() || other.mentions_once_per_run(),
            EffectRequirement::Not(inner) => inner.mentions_once_per_run(),
            _ => false,
        }
    }

    /// Whether using what this gates spends its card's `OncePerTurn` — one
    /// under `And`, never one under `Not`, which reads the use and spends
    /// nothing (`ability::consume_requirement`). Pauleʼs Café's plain
    /// install is `Not(And(DuringYourTurn, OncePerTurn))`: offered only once
    /// its discounted twin, the turn's first install that way, is spent.
    pub fn spends_once_per_turn(&self) -> bool {
        match self {
            EffectRequirement::OncePerTurn => true,
            EffectRequirement::And(one, other) => one.spends_once_per_turn() || other.spends_once_per_turn(),
            _ => false,
        }
    }

    /// Whether this is, or contains under `And`/`Not`, a `OncePerTurn`.
    pub fn mentions_once_per_turn(&self) -> bool {
        match self {
            EffectRequirement::OncePerTurn => true,
            EffectRequirement::And(one, other) => one.mentions_once_per_turn() || other.mentions_once_per_turn(),
            EffectRequirement::Not(inner) => inner.mentions_once_per_turn(),
            _ => false,
        }
    }

    /// Whether this reads `Amount::ThreatLevel` — a score — anywhere under
    /// `And`/`Not`. What an agenda is worth may not depend on it
    /// (`CardDefinition::validate`): a score is the sum of those worths.
    pub fn reads_the_score(&self) -> bool {
        use crate::dsl::Amount::ThreatLevel;
        match self {
            EffectRequirement::AmountAtLeast(amount, _) => *amount == ThreatLevel,
            EffectRequirement::MoreThan(one, other) => *one == ThreatLevel || *other == ThreatLevel,
            EffectRequirement::And(one, other) => one.reads_the_score() || other.reads_the_score(),
            EffectRequirement::Not(inner) => inner.reads_the_score(),
            _ => false,
        }
    }

    /// Whether this compares `Amount::TimesThisTurn(trigger)` with a
    /// number, anywhere under `And`/`Not` — see `CardDefinition::validate`.
    pub fn counts_this_turn(&self, trigger: Trigger) -> bool {
        match self {
            EffectRequirement::AmountAtLeast(crate::dsl::Amount::TimesThisTurn(counted), _) => *counted == trigger,
            EffectRequirement::And(one, other) => one.counts_this_turn(trigger) || other.counts_this_turn(trigger),
            EffectRequirement::Not(inner) => inner.counts_this_turn(trigger),
            _ => false,
        }
    }
}

/// Who decides an `InteractiveOnAccess`, and what paying its cost does.
///
/// Both real printings are "may pay to change what happens on access", but
/// they are mirror images of each other — opposite payer *and* opposite
/// polarity — so one flag captures both rather than two near-identical
/// mechanisms. Note that the effects always resolve on exactly one of the
/// two branches, which is what keeps `resolve_access_trigger` a single
/// function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AccessInteraction {
    /// The Runner may pay `cost` to *prevent* `effects` — Fetal AI's "you
    /// may pay 2 [credit] to avoid 2 net damage". Declining resolves them.
    /// The default, so a card that states no interaction reads as the
    /// original Runner-side avoidance this type was introduced for.
    #[default]
    RunnerPaysToPrevent,
    /// The Corp may pay `cost` to *apply* `effects` — Snare!'s "you may pay
    /// 4 [credit]. If you do, give the Runner 1 tag and do 3 net damage."
    /// Declining resolves nothing.
    CorpPaysToApply,
}

impl AccessInteraction {
    /// The side that chooses, and pays if it chooses to.
    pub fn payer(self) -> Side {
        match self {
            AccessInteraction::RunnerPaysToPrevent => Side::Runner,
            AccessInteraction::CorpPaysToApply => Side::Corp,
        }
    }

    /// Whether `effects` resolve when the payer *declines*. Paying always
    /// does the opposite.
    pub fn effects_resolve_on_decline(self) -> bool {
        match self {
            AccessInteraction::RunnerPaysToPrevent => true,
            AccessInteraction::CorpPaysToApply => false,
        }
    }
}

/// An optional "may pay `cost` to change what `effects` do" access-time
/// trigger — Fetal AI's "pay 2 [credit] to avoid 2 net damage", or Snare!'s
/// "you may pay 4 [credit]" to inflict them. Lives on
/// `dsl::card::CardDefinition::interactive_on_access`, `None` for the common case of
/// no such trigger. Resolved before the card's normal (unconditional)
/// `Trigger::OnAccessed` effects, via `rules::run::state::AccessPhase::
/// PendingInteractiveTrigger` and `PlayerAction::PayAccessTrigger`/
/// `DeclineAccessTrigger` (`rules::run::access::resolve_pay_access_trigger`/
/// `resolve_decline_access_trigger`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractiveOnAccess {
    pub cost: Cost,
    pub effects: Vec<Effect>,
    /// Absent means `RunnerPaysToPrevent` — see `AccessInteraction`.
    #[serde(default)]
    pub interaction: AccessInteraction,
    /// A precondition on the trigger firing at all, checked when the card
    /// is presented for access. Unmet means no decision is parked and
    /// access proceeds straight to its normal `PendingChoice` — the trigger
    /// simply does not apply here. Snare!'s "anywhere except in Archives"
    /// is `Not(AccessingIn(Archives))`. `None` for a trigger that always
    /// applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<EffectRequirement>,
}

/// A subroutine's printed text and the `Effect` it resolves into, when the
/// Corp lets it fire (or the Runner fails to break it).
///
/// Wired into `rules::run::state::RunIce` via `EncounteredSubroutine`,
/// individually addressable and status-tracked
/// (`SubroutineStatus::{Pending, Broken, Resolved}`);
/// `rules::run::engine::step_subroutine`/`transition_subroutine` consult
/// and fire the real `Effect` payload on resolution. `engine::initiate_run`
/// (via `build_run_ice`) populates real `SubroutineDef`s from
/// `CardDefinition::subroutines` at run start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubroutineDef {
    pub text: String,
    pub effect: Effect,
    /// The subroutine can only be broken by an icebreaker printed with
    /// this subtype — Semak-samun's "The Runner cannot break the printed
    /// subroutine on this ice except using a fracter." Consulted by every
    /// break path (`Effect::BreakSubroutines`, `BreakSubroutinesUnconditionally`
    /// and the bioroid click break), each of which leaves such a
    /// subroutine pending for a breaker without the subtype. Per
    /// subroutine rather than per card because the printed text says
    /// "the printed subroutine": a subroutine a later effect adds to the
    /// ice would not carry it. `None` for the common case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only_breakable_by: Option<crate::dsl::CardSubtype>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardId, CardTarget, DamageType};

    /// Test-local wrapper, not a production type — exists only to exercise
    /// `AbilityDef`/`SubroutineDef` (and, through them, `Cost`, `Effect`,
    /// `CardTarget`) together in one JSON document.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Bundle {
        abilities: Vec<AbilityDef>,
        subroutines: Vec<SubroutineDef>,
    }

    const BUNDLE_JSON: &str = r#"
    {
        "abilities": [
            {
                "trigger": "Paid",
                "cost": { "Credits": 3 },
                "effect": { "DealDamage": ["Net", 1] }
            },
            {
                "trigger": "Paid",
                "cost": "TrashSelf",
                "effect": { "GiveTags": { "Fixed": 1 } }
            }
        ],
        "subroutines": [
            {
                "text": "Trash a program.",
                "effect": { "TrashCard": { "RunnerRig": "gordian_blade" } }
            }
        ]
    }
    "#;

    #[test]
    fn ability_and_subroutine_ast_round_trips_through_json() {
        let bundle: Bundle = serde_json::from_str(BUNDLE_JSON).expect("valid ability/subroutine JSON");

        assert_eq!(
            bundle.abilities[0],
            AbilityDef {
                text: None,
                trigger: Trigger::Paid,
                cost: Some(Cost::Credits(3)),
                requirement: None,
                effect: Effect::DealDamage(DamageType::Net, 1),
                cost_discount_if: None, used_by: None, access: false, from_hand: false, part_of: None }
        );
        assert_eq!(
            bundle.abilities[1],
            AbilityDef {
                text: None,
                trigger: Trigger::Paid,
                cost: Some(Cost::TrashSelf),
                requirement: None,
                effect: Effect::GiveTags(crate::dsl::Amount::Fixed(1)),
                cost_discount_if: None, used_by: None, access: false, from_hand: false, part_of: None }
        );
        assert_eq!(
            bundle.subroutines[0],
            SubroutineDef {
                text: "Trash a program.".to_string(),
                effect: Effect::TrashCard(CardTarget::RunnerRig(CardId("gordian_blade".to_string()))),
                only_breakable_by: None,
            }
        );

        // Round-trip: re-serialize then re-parse — locks in the wire format
        // both directions, not just one-way parsing of a hand-written fixture.
        let re_serialized = serde_json::to_string(&bundle).expect("bundle should serialize");
        let round_tripped: Bundle =
            serde_json::from_str(&re_serialized).expect("re-serialized JSON should parse");
        assert_eq!(bundle, round_tripped);
    }
}
