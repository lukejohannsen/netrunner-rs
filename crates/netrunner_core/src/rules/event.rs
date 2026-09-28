use serde::{Deserialize, Serialize};

use crate::dsl::{EffectDuration, CardId, DamageType, Effect};
use crate::rules::run::ServerId;
use crate::rules::state::{InstallId, Side, WouldHappen};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEvent {
    ClickSpent { side: Side },
    CreditsGained { side: Side, amount: u32 },
    CardDrawn { side: Side },
    IceApproached { server: ServerId, position: u32 },
    IceEncountered { card_id: CardId, strength: i32, subroutine_count: usize },
    /// `strength` is the ice's strength as the subroutine was broken
    /// (`continuous::ice_strength`), carried because what a card hears
    /// about the break is read off the event (`EventFilter::Ice`, The
    /// Tungsten Tailor's "a piece of ice with 0 or less strength") and a
    /// strength read later could have moved.
    SubroutineBroken { card_id: CardId, index: usize, strength: i32 },
    SubroutineFired { card_id: CardId, index: usize, effect: Effect },
    /// The ice being encountered gained a subroutine ahead of the others
    /// for the rest of the encounter (`Effect::GainSubroutine`); `text` is
    /// its printed clause, which is how a log names it. An occurrence of
    /// nothing a card hears.
    SubroutineGained { card_id: CardId, text: String },
    IceStrengthModified { card_id: CardId, new_strength: i32, delta: i32 },
    /// `after_fully_breaking`: the ice is passed after an encounter in
    /// which the Runner fully broke it (CR 6.1.3f) — false for a piece of
    /// ice passed unrezzed, and for one whose encounter ended with a
    /// subroutine unbroken. Sipa reads it.
    IcePassed { server: ServerId, position: u32, after_fully_breaking: bool },
    /// The Runner fully broke the ice being encountered: the first time
    /// this encounter that every subroutine on it was broken (CR 6.5.7a).
    /// Lethe gives a tag for it. `position` is where it stands in the
    /// run's ice, as `IceBypassed`'s is.
    IceFullyBroken { card_id: CardId, position: u32 },
    /// The Runner bypassed the ice being encountered
    /// (`Effect::BypassEncounteredIce`, Fransofia Ward): its remaining
    /// subroutines will not fire and its own "when encountered" reactions
    /// do not resolve. The `IcePassed` follows on the next `Continue`.
    IceBypassed { card_id: CardId, position: u32 },
    /// An encounter with this ice ended (CR 6.9.3e), however it ended: the
    /// Runner passed on from it, "end the run" ended it with the run (CR
    /// 6.1.4), or the ice left the table or was derezzed during it.
    /// Knowledge Seeker's "whenever an encounter with this ice ends" hears
    /// it. The install is carried because the run may be over by the time
    /// anybody listens, and with it the run's list of ice.
    EncounterEnded { card_id: CardId, install: crate::rules::state::InstallId },
    /// `card`, `side`'s, is shown to both players and goes back to where it
    /// was (CR 1.21.3) — the Runner revealing Esca, Snare! or Byte! while
    /// accessing it in R&D (`ContinuousKind::RevealedWhileAccessed`), which
    /// is how the Corp learns which of its cards was accessed there. Public
    /// by definition; no card hears a reveal yet.
    CardRevealed { side: Side, card: CardId },
    /// `cards`, `side`'s, went faceup into the set-aside zone, in that
    /// order (The Wizard's Chest, `Effect::SetAsideFromTopUntil`). Public:
    /// set aside faceup, so shown to both (CR 4.8.6). No card hears it.
    CardsSetAside { side: Side, cards: Vec<CardId> },
    /// The run against `server` reached its Success Phase and was not
    /// declared successful (CR 6.9.5a, 6.8.4a): a card said runs there
    /// cannot be (`ContinuousKind::CannotBeDeclaredSuccessful`, Flagship).
    /// It is not unsuccessful either, and the breach follows. An occurrence
    /// of nothing a card hears — "when a run is successful" is exactly
    /// what did not happen.
    RunNotDeclaredSuccessful { server: ServerId },
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
        /// Whether the card came out of HQ — The Holo Man's "if you have
        /// not installed any cards from HQ this turn" (`turn_log`'s sum)
        /// and Stoke the Embers' "When you install this agenda from anywhere
        /// except HQ" (`EventFilter::InstalledFromHq`). Public: both players
        /// see which zone a card leaves.
        #[serde(default)]
        from_hq: bool,
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
    /// `Effect::AddToDeck` put `card` on top of `side`'s deck (`top`) or
    /// under it. `revealed` when it came out of a zone both players see —
    /// the Runner's heap, the rig (Beta Build) — so the opponent may be
    /// told which card now sits in a deck they cannot see into; out of the
    /// grip, HQ, R&D or Archives it is its owner's to know
    /// (`masking::mask_event_for_player`), as a `CardsSelected` is.
    CardAddedToDeck { side: Side, card: CardId, top: bool, revealed: bool },
    /// An installed card went back to its owner's hand — Janaína “JK”
    /// Dumont Kindelán's "add this asset to HQ" (`Cost::AddSelfToHq`).
    /// `faceup` is whether it was rezzed as it left, which is what decides
    /// whether a viewer who may not identify a facedown Corp install
    /// learns what it was (`masking`); the handle is public, as it is for
    /// an install. An occurrence of nothing a card hears.
    CardAddedToHand { side: Side, card: Option<CardId>, install: crate::rules::state::InstallId, faceup: bool },
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
    /// The Runner suffered `amount` damage, and `responsible` is who did it
    /// (CR 10.4.1): the side of the card whose text dealt it — a Corp card
    /// "does" damage and a Runner card makes the Runner "suffer" it, so
    /// in this pool the card's side is the verb's — and the Runner for
    /// damage suffered to pay a cost (Semak-samun). `None` for damage no
    /// card dealt, which nobody did. "Whenever you do damage" (AU Co.)
    /// hears only its own side's; it heard every `DamageTaken`, and so
    /// counted Topan's and Semak-samun's damage as the Corp's.
    DamageTaken {
        damage_type: DamageType,
        amount: usize,
        #[serde(default)]
        responsible: Option<Side>,
    },
    RunnerFlatlined,
    CreditsSpent { side: Side, amount: u32 },
    /// `had` is how many tags `side` had before these — Sebastião Souza
    /// Pessoa's "whenever you take 1 or more tags, **if you had no tags**",
    /// a fact about the moment that the state has lost by the time the
    /// trigger resolves (`EffectRequirement::HadNoTags`).
    TagsGiven { side: Side, amount: u32, had: u32 },
    /// `Cost::ClearTags` zeroed the Runner's tag count. Named for clearing,
    /// not purging — see `Cost::ClearTags`'s doc comment. `side` is whose
    /// tags they were and `by` the player who removed them, as on
    /// `TagsRemoved`.
    TagsCleared { side: Side, by: Side },
    /// `card`, `side`'s, went to its owner's discard pile. `by` is the
    /// player who carried the trash out (CR 1.14.5a) — the controller of
    /// the text that trashed it, the chooser of a selection, the payer of
    /// a cost, the player installing over it — and `None` when the rules
    /// did it: a card leaving with its host, the unique rule, a card
    /// emptied of the credits it hosted. Hiram "0mission" Svensson hears
    /// the Runner trash a piece of hardware from any location
    /// (`Trigger::OnCardTrashed`, an occurrence only when `by` names a
    /// player). `installed`: it was on the table when it was trashed, not
    /// in a hand, a deck or hosted uninstalled — Boi-tatá's "if you
    /// trashed any of your installed cards this turn", which the turn log
    /// counts by `turn_log::Class::Card`'s `installed` and could not while
    /// every trash was counted as one out of a hand.
    CardTrashed { side: Side, card: CardId, installed: bool, by: Option<Side> },
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
    /// One or more cards left R&D for Archives at once — a mill, a cost, a
    /// selection — beside the per-card `CardTrashed`s, the R&D twin of
    /// `CardsTrashedFromHq`. `by` is who carried the trash out (CR 1.14.5),
    /// as on `CardTrashed`: Nuvem SA hears the Corp's own, never the
    /// Runner's trash of a card accessed there.
    CardsTrashedFromRnD { count: u32, by: Option<Side> },
    /// The resolution of `card` has finished — an operation played, or an
    /// action on an expendable card used from HQ — announced once whatever
    /// it parked has resolved (`DeferredTrigger::announce`). Nuvem SA:
    /// Law of the Land hears it.
    FinishedResolving { side: Side, card: CardId },
    /// An agenda left the Corp's score area as a forfeit (Biawak's rez,
    /// Plutus's). Paired with `CardRemovedFromGame`, which says where it
    /// went; this one says *why*, which is what `Trigger::OnForfeit` keys
    /// off.
    AgendaForfeited { card: CardId },
    /// A card was added to the Corp's score area "as an agenda" worth
    /// `points` (CR 10.1.3) — Myōshu out of Archives, Word on the Street out
    /// of the Runner's rig. It was not scored (CR 1.17.3f), so this is an
    /// occurrence of nothing a card hears.
    AddedToScoreAreaAsAgenda { card: CardId, points: i32 },
    /// An agenda was added to the Corp's score area by a card's text, worth
    /// what it prints (CR 1.17.3e) — Kingmaking's "add 1 agenda worth 1 or
    /// less agenda points from HQ to your score area", a selection whose
    /// `destination` is the score area. Neither scored nor stolen, so an
    /// occurrence of nothing a card hears; public, as the score area is.
    AgendaAddedToScoreArea { card: CardId, agenda_points: u32 },
    /// Credits gained by a resolving card's ability, naming the card —
    /// emitted with `CreditsGained` whenever the resolution has an acting
    /// card. The Zwicky Group: Invisible Hands draws off it. Carries no
    /// amount: no reader needs one, and `CreditsGained` has it.
    AbilityGainedCredits { side: Side, card: CardId },
    RunEndedByEffect { server: ServerId },
    GameOver { winner: Side },
    /// `install` is the copy used, `None` for an identity or a card used
    /// from the hand; `action` is whether the ability is an action (CR
    /// 9.5.2a: its cost begins with [click]), which is what Juli Moreira
    /// Lee's "the first time each turn you take an action on an installed
    /// resource" hears (`Trigger::OnActionTaken`). The listener scan reads
    /// no registry, so the event says it.
    AbilityActivated { side: Side, card_id: CardId, ability_index: usize, install: Option<InstallId>, action: bool },
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
    /// Advancement counters were removed from an install, as a cost
    /// (`Cost::RemoveAdvancementCounters`, Sacrifice Zone Expansion) or by a
    /// card's text (`Effect::RemoveAdvancementCounters`, Hearts and Minds);
    /// `advancement_tokens` is what is left. Masked as
    /// `AdvancementCountersPlaced` is.
    AdvancementCountersRemoved {
        install: crate::rules::state::InstallId,
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
    /// The Runner's basic action removed one of `side`'s tags; `by` is the
    /// Runner, and is carried so every tag removal says who removed it.
    TagRemoved { side: Side, by: Side },
    /// `amount` of `side`'s tags came off, removed by `by`: the controller
    /// of the effect that removed them, or the player who paid a
    /// remove-a-tag cost. **Whose tags and who removed them are two
    /// players** — Synapse Global's "[click], remove 1 tag" is the Corp
    /// removing the Runner's — and Valentina Ferreira Carvalho's "whenever
    /// **you** remove 1 or more tags" hears only the Runner's removals
    /// (`listeners::moments` makes `by` the moment's player).
    TagsRemoved { side: Side, amount: u32, by: Side },
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
    /// A payment took `amount` credits from somewhere other than `side`'s
    /// credit pool — hosted credits, bad publicity's, a run event's, an
    /// identity's — once per payment, whatever the pools, and beside the
    /// events that say which. Shackleton Grid's "when the Runner spends
    /// credits from outside their credit pool during a run against this
    /// server": a moment of its own because no spend event was one — the
    /// three that exist are per pool, so a payment from two pools would
    /// have been two moments. `run_against` is the server of the run in
    /// progress as the credits were spent, on the event because the
    /// payer dispatches it after the effect it paid for (`ability::
    /// dispatch_cost_events`), which may have ended the run.
    CreditsSpentFromOutsidePool { side: Side, amount: u32, run_against: Option<ServerId> },
    /// A breach of Archives turned `count` facedown cards faceup (CR
    /// 7.3.2), once per breach — Nurse Hạnh's "whenever 2 or more facedown
    /// cards in Archives are turned faceup". Never emitted for none.
    ArchivesTurnedFaceup { count: u32 },
    /// `side` looked at `cards`, the top of `deck`'s owner's deck, top
    /// first (`Effect::LookAtTopOfDeck`). Shown to `side` alone
    /// (`masking::mask_event_for_player`): a look is not a reveal, and the
    /// deck's owner does not know its order either.
    CardsLookedAt { side: Side, deck: Side, cards: Vec<CardId> },
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
    /// A Corp install is about to leave the table (CR 8.5.1b), announced
    /// by `rules::uninstall` before it goes — and only when the card is
    /// active and its own text interrupts it (Luana Campos), because only
    /// an active interrupt is marked pending (CR 9.9.4b) and nothing else
    /// hears the moment. So the card is always rezzed, and public.
    AboutToBeUninstalled { card: CardId, install: crate::rules::state::InstallId },
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
    ///
    /// `secret` when the decision was (`Effect::ChooseNumber::secret`):
    /// then the other player never receives this event
    /// (`masking::mask_event_for_player`) and the answering action is
    /// concealed in their log.
    NumberChosen { chooser: Side, amount: u32, secret: bool },
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
            | GameEvent::CardsLookedAt { .. } | GameEvent::CardRevealed { .. } | GameEvent::CardsSetAside { .. }
            | GameEvent::AccessPassed { .. } | GameEvent::AgendaStolen { .. } | GameEvent::IceRezzed { .. }
            | GameEvent::IceEncountered { .. } | GameEvent::DamageTaken { .. } | GameEvent::CardsTrashedFromHq { .. }
            | GameEvent::CardsTrashedFromRnD { .. }
            | GameEvent::MulliganTaken { .. } | GameEvent::HandKept { .. } | GameEvent::TraceInitiated { .. }
            | GameEvent::TraceCorpBidSubmitted { .. } | GameEvent::TraceRunnerBidSubmitted { .. }
            | GameEvent::TraceAvoided { .. } | GameEvent::TraceSuccessful { .. } | GameEvent::GameOver { .. }
            | GameEvent::RunnerFlatlined | GameEvent::TurnStarted { .. } | GameEvent::TurnEnded { .. } => true,
            GameEvent::FinishedResolving { .. } => false,
            GameEvent::ClickSpent { .. } | GameEvent::CreditsGained { .. } | GameEvent::IceApproached { .. }
            | GameEvent::SubroutineBroken { .. } | GameEvent::SubroutineFired { .. } | GameEvent::SubroutineGained { .. }
            | GameEvent::RunNotDeclaredSuccessful { .. }
            | GameEvent::IceStrengthModified { .. } | GameEvent::IcePassed { .. } | GameEvent::IceBypassed { .. }
            | GameEvent::EncounterEnded { .. }
            | GameEvent::IceFullyBroken { .. }
            | GameEvent::ServerApproached { .. } | GameEvent::RunSucceeded { .. } | GameEvent::RunJackedOut { .. }
            | GameEvent::RunCompleted { .. } | GameEvent::CardInstalled { .. } | GameEvent::CardDerezzed { .. }
            | GameEvent::IceSwapped { .. } | GameEvent::CardMoved { .. } | GameEvent::RunInitiated { .. }
            | GameEvent::EventPlayed { .. } | GameEvent::OperationPlayed { .. } | GameEvent::HardwareInstalled { .. }
            | GameEvent::ProgramInstalled { .. } | GameEvent::ResourceInstalled { .. }
            | GameEvent::DiscardPending { .. } | GameEvent::DiscardPhaseEnded { .. }
            | GameEvent::CardDiscarded { .. } | GameEvent::CardAddedToDeck { .. }
            | GameEvent::CardHosted { .. } | GameEvent::IdentityFlipped { .. } | GameEvent::ActionPhaseEnded { .. }
            | GameEvent::RunEndPrevented { .. } | GameEvent::RunRedirected { .. } | GameEvent::CreditsSpent { .. }
            | GameEvent::TagsGiven { .. } | GameEvent::TagsCleared { .. } | GameEvent::CardTrashed { .. }
            | GameEvent::CardRemovedFromGame { .. } | GameEvent::AgendaForfeited { .. } | GameEvent::AddedToScoreAreaAsAgenda { .. } | GameEvent::AgendaAddedToScoreArea { .. } | GameEvent::CardAddedToHand { .. }
            | GameEvent::AbilityGainedCredits { .. } | GameEvent::RunEndedByEffect { .. }
            | GameEvent::AbilityActivated { .. } | GameEvent::CardAdvanced { .. }
            | GameEvent::AdvancementCountersPlaced { .. } | GameEvent::AdvancementCountersRemoved { .. }
            | GameEvent::PaidAbilityWindowOpened { .. }
            | GameEvent::PriorityPassed { .. } | GameEvent::PaidAbilityWindowClosed
            | GameEvent::StrengthBoosted { .. } | GameEvent::TagRemoved { .. } | GameEvent::TagsRemoved { .. }
            | GameEvent::TriggerOrderPending { .. } | GameEvent::TriggerOrderChosen { .. }
            | GameEvent::TriggerFired { .. } | GameEvent::VirusCountersPurged { .. }
            | GameEvent::PaymentChoiceOffered { .. }
            | GameEvent::BadPublicityCreditsSpent { .. } | GameEvent::BonusRunCreditsSpent { .. }
            | GameEvent::CreditsSpentFromOutsidePool { .. } | GameEvent::ArchivesTurnedFaceup { .. }
            | GameEvent::CardsSelected { .. } | GameEvent::PendingCardSelectionOffered { .. }
            | GameEvent::MemoryLimitExceeded { .. } | GameEvent::PendingServerChoiceOffered { .. }
            | GameEvent::BadPublicityGiven { .. } | GameEvent::BadPublicityRemoved { .. }
            | GameEvent::AdditionalAccessGranted { .. } | GameEvent::AccessReplacementSet { .. }
            | GameEvent::AccessReplaced { .. } | GameEvent::CreditsLost { .. } | GameEvent::ClicksLost { .. }
            | GameEvent::ClicksGained { .. }
            | GameEvent::AgendaScored { .. } | GameEvent::AboutToResolve { .. } | GameEvent::AboutToBeUninstalled { .. }
            | GameEvent::Prevented { .. } | GameEvent::CountersAdded { .. } | GameEvent::CountersRemoved { .. }
            | GameEvent::BasicDrawActionTaken { .. }
            | GameEvent::PendingChoicePresented { .. } | GameEvent::PendingChoiceResolved { .. }
            | GameEvent::NumberChoiceOffered { .. } | GameEvent::NumberChosen { .. }
            | GameEvent::PendingPaidChoiceOffered { .. } | GameEvent::PendingPaidChoiceAccepted { .. }
            | GameEvent::PendingPaidChoiceDeclined { .. } => false,
        }
    }
}
