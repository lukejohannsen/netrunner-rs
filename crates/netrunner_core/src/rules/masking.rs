use serde::{Deserialize, Serialize};

use crate::dsl::{CardId, CardTarget, Cost, IceType};
use crate::rules::action::{PlayerAction, TargetZone};
use crate::rules::event::GameEvent;
use crate::rules::run::{AccessPhase, AccessState, EncounteredSubroutine, RunIce, RunPhase, RunState, ServerId};
use crate::rules::state::{ArchivedCard, CorpState, GamePhase, GameState, InstallId, InstallSlot, InstalledCard, InstalledRunnerCard, MemoryUnits, PaidAbilityWindow,
    PendingPrevention, PlayerResources, RunnerState, Side, TraceState,
};

/// A card zone whose contents are secret to everyone but its owner. The
/// count is always public (both players can see how many cards are in HQ or
/// R&D); only card identity is masked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskedZone {
    Visible(Vec<CardId>),
    Hidden { count: u32 },
}

/// An installed card as seen by a particular viewer: presence, server, and
/// rez status are always public, but an unrezzed card's identity is `None`
/// unless the viewer is its owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicInstalledCard {
    /// Never masked, and the field that makes this struct *addressable*: a
    /// viewer who cannot see `card` can still name this install to the
    /// engine. Real Netrunner needs that — the Runner may host a Trojan on
    /// unrezzed ICE and may swap ICE they cannot identify — and before this
    /// existed those actions had to carry the real `CardId`, handing the
    /// Runner the identity the mask had just removed. See
    /// `state::InstallId`.
    pub install_id: InstallId,
    /// Where this install sits in its controller's install list — never
    /// masked, for the same reason as `install_id`: the physical order of
    /// cards on the table is something both players can see.
    ///
    /// Carried because it is the ordering `PlayerAction::ToggleCardSelection`
    /// and `ActionSpace`'s installed-card segments both index by, and a
    /// consumer that rebuilds a `GameState` from a view — `netrunner_bots::
    /// determinize` — must reproduce it exactly or the caller's actions and
    /// the reconstruction's mean different cards. It is not derivable from
    /// `ServerView`, which groups by server rather than by install order,
    /// and install order is not the `install_id` order either: `Effect::
    /// InstallFromZoneIgnoringCost` (Brân 1.0) *inserts* rather than appends.
    pub position: usize,
    pub server: ServerId,
    /// Never masked — whether a card occupies a server's ICE-protection
    /// slot or its root (content) slot is visible to both sides regardless
    /// of the card's identity.
    pub slot: InstallSlot,
    pub rezzed: bool,
    pub card: Option<CardId>,
    /// Never masked — advancement tokens are public info on the physical
    /// card, same as `server`/`rezzed`.
    pub advancement_tokens: u32,
    /// Generic counters (see `dsl::card::CounterKind`), masked on exactly
    /// the same condition as `card`: `Some` to the owner always, and to the
    /// opponent once the card is rezzed; `None` on an unrezzed Corp card,
    /// whose counters would otherwise leak what it is (a *Nico Campaign*
    /// draining credits is recognisable long before it is rezzed).
    ///
    /// `Option<u32>` rather than a bare `u32` defaulting to `0` so a client
    /// can tell "hidden" from "rezzed, and genuinely holds no counters" —
    /// the two want different renderings, and collapsing them would make an
    /// empty card look identical to a concealed one.
    ///
    /// The *kind* of counter is deliberately not carried here: it is a
    /// static property on `CardDefinition`, and every client already holds
    /// a `CardRegistry` to resolve titles, so duplicating it into the view
    /// would be two sources of truth for one fact.
    pub counters: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicCorpState {
    pub resources: PlayerResources,
    pub hq: MaskedZone,
    pub r_and_d: MaskedZone,
    /// Partially masked: the Runner always sees how many cards are in
    /// Archives and which way up each one is, but a facedown card's
    /// identity is hidden from them. The Corp always sees its own zone in
    /// full. See `PublicArchivedCard`.
    pub archives: Vec<PublicArchivedCard>,
    pub installed: Vec<PublicInstalledCard>,
    /// Never masked — scored Agendas sit in a fully public score area.
    pub scored_agendas: Vec<CardId>,
    /// Never masked — Bad Publicity is public information in the real game,
    /// same treatment as `scored_agendas`.
    pub bad_publicity: u32,
    /// Recurring credits still unspent this turn, and the pool they refill
    /// to. Never masked: recurring credits sit as visible tokens on the
    /// card that grants them, so both players can always count them.
    pub recurring_credits: u32,
    pub recurring_credits_max: u32,
}

/// A Runner rig card as seen by any viewer: never hidden (see
/// `PublicRunnerState::rig`'s doc comment), including its current
/// (possibly pumped) strength — real Netrunner/Null Signal Games treats an
/// installed icebreaker's current strength as visible public information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicInstalledRunnerCard {
    pub card: CardId,
    /// Never masked — nothing in the rig is. Present so an action can name
    /// one specific copy of a card the Runner installed twice, which
    /// `card` alone cannot do. See `state::InstallId`.
    pub install_id: InstallId,
    pub current_strength: i32,
    /// Which piece of ICE this card is hosted on, for a Trojan Program
    /// (Botulus, Tranquilizer). Never masked, for the same reason as the
    /// rest of this struct: a rig card is face-up, and *where* it sits is
    /// as visible as the card itself — a physical Trojan is literally
    /// placed on the ICE it hosts on. An install handle, like every
    /// other reference to a Corp install in the view; the client resolves
    /// it against the server's `ice` list.
    pub hosted_on_ice: Option<InstallId>,
    /// Generic counters (see `dsl::card::CounterKind`) — a bare `u32`, not
    /// the `Option` its Corp counterpart carries. The asymmetry is not an
    /// oversight: a rig card is always face-up (see `PublicRunnerState::
    /// rig`), so there is no unrezzed state for counters to leak from and
    /// therefore no visibility rule to express. Virus counters on *Botulus*
    /// or credits on *Pennyshaver* are public the moment they are placed.
    pub counters: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRunnerState {
    pub resources: PlayerResources,
    pub memory_units: MemoryUnits,
    /// Never masked — Brain damage count, like `memory_units`, is plain
    /// public information (it visibly shrinks the Runner's max hand size).
    pub brain_damage: usize,
    /// Never masked — tags are plain public information in the real game.
    pub tags: u32,
    pub grip: MaskedZone,
    pub stack: MaskedZone,
    /// Never masked — Rig cards are always face-up once installed.
    pub rig: Vec<PublicInstalledRunnerCard>,
    /// Never masked — like Corp's `archives`, a Runner's discard pile is a
    /// fully public zone in the real game.
    pub heap: Vec<CardId>,
    /// Never masked — stolen Agendas sit in a fully public score area.
    pub scored_agendas: Vec<CardId>,
    /// Never masked — static link strength, like `tags`, is plain public
    /// information (relevant to both sides during a trace).
    pub link_strength: u32,
    /// Never masked — see `RunnerState::servers_run_this_turn`; a bot
    /// searching from the view needs it to offer Red Team's click only
    /// where the engine will.
    #[serde(default)]
    pub servers_run_this_turn: Vec<ServerId>,
}

/// A run's ICE as seen by a particular viewer: `rezzed` is always public,
/// but a face-down (unrezzed) ICE reveals *nothing* else — not just its
/// identity, but every identity-derived field (`current_strength`/
/// `ice_type`/`subroutines` are all printed on the hidden card face, same
/// as a real physical card) — unless the viewer is the Corp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRunIce {
    /// Public: the same per-instance handle `PublicInstalledCard` carries
    /// outside its identity gate. A client rezzing the approached ICE, or a
    /// search rebuilding the run from this view, addresses it by this.
    pub install_id: InstallId,
    pub rezzed: bool,
    pub identity: Option<PublicRunIceIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRunIceIdentity {
    pub card: CardId,
    pub current_strength: i32,
    pub ice_type: IceType,
    pub subroutines: Vec<EncounteredSubroutine>,
}

/// A pending per-card access decision as seen by a particular viewer —
/// masking mirrors `PublicAccessState::unaccessed_cards`/`resolved_cards`:
/// the card being decided on is identity-visible to the Runner always, and
/// to the Corp only when accessing (fully public) Archives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicAccessPhase {
    SelectNextCard { selectable_cards: MaskedZone },
    /// `decider` is public: which side is being asked to pay is not hidden
    /// information — both players watch the game wait on someone — and a
    /// client cannot render whose decision it is without it. The card's
    /// *identity* is still masked by the rule above, so the Corp being
    /// asked to pay for an unrevealed R&D trap tells the Runner only that
    /// some interactive trigger fired, which they can already see.
    PendingInteractiveTrigger { card: Option<CardId>, cost: Cost, decider: Side, can_pay: bool },
    PendingChoice { card: Option<CardId>, trash_cost: Option<u32>, mandatory_steal: bool, steal_cost: Option<Cost> },
}

/// `run::AccessState` as seen by a particular viewer. In the real game the
/// Runner sees exactly which card they're accessing the instant access
/// begins, but the Corp doesn't learn which HQ/R&D card was hit unless it's
/// since landed in a public zone (Archives, a score area) — so identity
/// here is visible to the Runner unconditionally, and to the Corp only when
/// `server == ServerId::Archives` (Archives is always a public zone).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicAccessState {
    pub server: ServerId,
    pub unaccessed_cards: MaskedZone,
    pub resolved_cards: MaskedZone,
    /// Never masked — an install id is public, same as
    /// `PublicInstalledCard::install_id`; it says *which* root card is
    /// being resolved, not what it is.
    #[serde(default)]
    pub pending_install: Option<InstallId>,
    pub phase: PublicAccessPhase,
}

/// `run::RunState` as seen by a particular viewer. Drops
/// `bad_publicity_credits`/`additional_rd_access`/`additional_hq_access`/
/// `access_replacement` from the projection — none carry card identity,
/// none are consumed by any current renderer, and omitting a field is
/// always leak-safe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRunState {
    pub server: ServerId,
    pub phase: RunPhase,
    pub ice: Vec<PublicRunIce>,
    pub position: usize,
    pub access_state: Option<PublicAccessState>,
    pub jack_out_permitted: bool,
    /// Credits this run may still draw from Bad Publicity. Never masked:
    /// `PublicCorpState::bad_publicity` is already public, and how much of
    /// it this run has spent is something both players track openly —
    /// hiding it would make the Runner's affordable actions unreadable to
    /// the Corp while changing nothing about what is knowable.
    pub bad_publicity_credits: u32,
    /// Run-scoped credits granted by a card for this run only. Public for
    /// the same reason as `bad_publicity_credits`.
    pub bonus_run_credits: u32,
    /// Whether a card has barred stealing/trashing accessed cards for the
    /// rest of this run. A run-scoped restriction announced when it
    /// applies, so both players know it is in force.
    pub runner_cannot_steal_or_trash: bool,
}

/// `GameState` as visible to one player: hidden zones are collapsed to a
/// count, and unrezzed installed cards have their identity stripped unless
/// the viewer owns them. `phase` is never masked — turn structure is public.
/// `paid_ability_window` is likewise never masked — both players always see
/// whose priority it is and the current pass count. `active_trace` is
/// likewise never masked — both sides always see the trace strength and
/// whose bid is pending, matching the real game. `pending_prevention` gets
/// the same treatment, same rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicGameState {
    pub corp: PublicCorpState,
    pub runner: PublicRunnerState,
    pub phase: GamePhase,
    pub active_run: Option<PublicRunState>,
    pub paid_ability_window: Option<PaidAbilityWindow>,
    pub active_trace: Option<TraceState>,
    pub pending_prevention: Option<PendingPrevention>,
    /// Fully public — a pending paid choice/decision (who's offered it, its
    /// cost/options) carries no hidden information, same treatment as
    /// `active_trace`/`pending_prevention`.
    pub pending_paid_choice: Option<crate::rules::state::PendingPaidChoice>,
    pub pending_decision: Option<crate::rules::state::PendingDecision>,
}

/// Who a masked projection is for.
///
/// **A spectator sees the intersection of what the two players see.**
/// Every gate in this module has the shape "visible iff the viewer owns
/// it, or it is public" — `mask_zone`, `mask_installed_card`,
/// `mask_archived_card`, `mask_run_state`, the event and action maskers —
/// and is phrased as `viewer.is(owner)`, so a spectator, who owns neither
/// side, takes the hidden branch at all of them and there is no third code
/// path to keep honest. Rejected: an omniscient "caster" perspective.
/// Anyone may spectate a match, including a player of that very match, so
/// a union view would be a fog-of-war bypass; a *delayed* omniscient
/// stream is a different feature with its own gate.
///
/// Every masking entry point takes `impl Into<Viewer>`, so the many
/// callers that mask for a `Side` (every bot, the RL encoder, both TUIs)
/// pass it unchanged; wrapping each in `Viewer::Player` would have
/// documented nothing. A parallel `build_spectator_view` was rejected
/// too: two entry points into one masking policy is how they drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Viewer {
    Player(Side),
    Spectator,
}

impl From<Side> for Viewer {
    fn from(side: Side) -> Self {
        Viewer::Player(side)
    }
}

impl Viewer {
    /// Whether this viewer *is* `side` — the owner test every gate asks.
    pub fn is(self, side: Side) -> bool {
        self == Viewer::Player(side)
    }

    /// The seat this viewer sits in, if any.
    pub fn side(self) -> Option<Side> {
        match self {
            Viewer::Player(side) => Some(side),
            Viewer::Spectator => None,
        }
    }
}

pub fn mask_state_for_player(state: &GameState, viewer: impl Into<Viewer>) -> PublicGameState {
    let viewer = viewer.into();
    PublicGameState {
        corp: mask_corp_state(&state.corp, viewer.is(Side::Corp)),
        runner: mask_runner_state(&state.runner, viewer.is(Side::Runner)),
        phase: state.phase,
        active_run: state.active_run.as_ref().map(|run| mask_run_state(run, viewer)),
        paid_ability_window: state.paid_ability_window.clone(),
        active_trace: state.active_trace.clone(),
        pending_prevention: state.pending_prevention.clone(),
        pending_paid_choice: state.pending_paid_choice.clone(),
        pending_decision: state.pending_decision.clone(),
    }
}

/// A resolved `PlayerAction` as one particular viewer is entitled to see
/// it. The per-action log a seat receives (`netrunner_server`'s
/// `ActionLog`, the TUI's match log) carries the *opponent's* actions too,
/// and several of those name a card the viewer's `ClientView` conceals:
/// the Corp's `InstallCard { card_id }` is the facedown card it just put
/// on the table, its `DiscardCard` is a card going facedown to Archives,
/// and the Runner's `SelectCardToAccess`/`PassAccessedCard`/`Pay…`/
/// `Decline…` tell the Corp which HQ or R&D card was looked at. A viewer
/// always sees their own actions whole — they chose them.
///
/// `Concealed` keeps the shape and drops the identity, mirroring
/// `PublicInstalledCard { card: None }`: "the Corp installed a card into
/// Remote 0" is information the Runner *must* have, so the action is not
/// simply omitted. The alternative — `Option<CardId>` on `PlayerAction`
/// itself — was rejected: the engine would carry a view concern on a type
/// with an `ActionSpace` layout and forty-one-arm exhaustive matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicAction {
    Visible(PlayerAction),
    Concealed(ConcealedAction),
}

/// The public residue of an action whose card the viewer may not learn.
/// Exactly the variants of `PlayerAction` that can name a concealed card;
/// `mask_action_for_player`'s exhaustive match is what keeps this list
/// honest when an action is added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConcealedAction {
    /// A Corp install. Where it went and into which slot is public — both
    /// players watch the card land — only *what* landed is not.
    InstallCard { zone: TargetZone, slot: InstallSlot },
    /// A Corp discard, which goes facedown to Archives.
    DiscardCard,
    /// The Runner's access of a card in HQ or R&D, from the Corp's chair:
    /// the Corp does not learn which of its cards was looked at unless the
    /// access reveals it (a steal or trash lands the card in a public zone,
    /// and those actions stay `Visible`). Archives is under-disclosed by
    /// the same rule — everything there is faceup to both sides after an
    /// access — which is the accepted direction of error.
    SelectCardToAccess,
    PassAccessedCard,
    PayAccessTrigger,
    DeclineAccessTrigger,
}

/// Masks `actor`'s `action` for `viewer`. State-free on purpose: every
/// concealment here follows from the variant and who is looking, never
/// from what the card turned out to be.
///
/// Exhaustive over `PlayerAction` with no catch-all arm, so a new
/// card-bearing variant must be classified here before it compiles — the
/// `explain_action` discipline, and the opposite of the `_ =>` decode arms
/// the Rules Audit removed.
pub fn mask_action_for_player(action: &PlayerAction, actor: Side, viewer: impl Into<Viewer>) -> PublicAction {
    if viewer.into().is(actor) {
        return PublicAction::Visible(action.clone());
    }
    match action {
        PlayerAction::InstallCard { zone, slot, .. } => {
            PublicAction::Concealed(ConcealedAction::InstallCard { zone: *zone, slot: *slot })
        }
        PlayerAction::DiscardCard { .. } if actor == Side::Corp => PublicAction::Concealed(ConcealedAction::DiscardCard),
        // A Runner discard goes to the heap, which is never masked.
        PlayerAction::DiscardCard { .. } => PublicAction::Visible(action.clone()),
        PlayerAction::SelectCardToAccess { .. } => PublicAction::Concealed(ConcealedAction::SelectCardToAccess),
        PlayerAction::PassAccessedCard { .. } => PublicAction::Concealed(ConcealedAction::PassAccessedCard),
        PlayerAction::PayAccessTrigger { .. } => PublicAction::Concealed(ConcealedAction::PayAccessTrigger),
        PlayerAction::DeclineAccessTrigger { .. } => PublicAction::Concealed(ConcealedAction::DeclineAccessTrigger),
        // Faceup plays and Runner installs; steals and access-trashes,
        // which land the card in a public zone; a click-break, which only
        // a rezzed piece of ICE can be the target of; and everything that
        // names an `InstallId` or a position rather than a card.
        PlayerAction::GainCreditClick { .. }
        | PlayerAction::DrawCardClick { .. }
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
        | PlayerAction::KeepHand
        | PlayerAction::TakeMulligan
        | PlayerAction::ActivateAbility { .. }
        | PlayerAction::AdvanceCard { .. }
        | PlayerAction::ScoreAgenda { .. }
        | PlayerAction::RemoveTag
        | PlayerAction::PurgeVirusCounters
        | PlayerAction::ChooseTriggerToResolve { .. }
        | PlayerAction::TrashResource { .. }
        | PlayerAction::StealAgenda { .. }
        | PlayerAction::TrashAccessedCard { .. }
        | PlayerAction::PassPriority { .. }
        | PlayerAction::SubmitCorpTraceBid { .. }
        | PlayerAction::SubmitRunnerTraceBid { .. }
        | PlayerAction::AcceptPendingPaidChoice { .. }
        | PlayerAction::DeclinePendingPaidChoice
        | PlayerAction::ResolvePendingChoice { .. }
        | PlayerAction::ToggleCardSelection { .. }
        | PlayerAction::ConfirmCardSelection
        | PlayerAction::ChooseServerForPendingDecision { .. } => PublicAction::Visible(action.clone()),
    }
}

/// Masks one `GameEvent` for `viewer`: `Some` if the viewer may see it as
/// emitted (or, for `TraceInitiated`/`VirusCountersPurged`, with the
/// concealed card struck out), `None` if the event's information *is* the
/// identity of a card the viewer may not know. A dropped event costs
/// nothing a client renders today — the `ClientView` snapshot sent just
/// before it already carries the structural change — while a `GameEvent`
/// with an `Option<CardId>` would put a view concern on the type every
/// trigger in `dispatcher` matches on.
///
/// **Concealment is decided from `state`, the position the event's action
/// produced, and conservatively.** The one predicate,
/// `corp_card_concealed_from_runner`: a Corp card is off-limits to the
/// Runner while any installed copy is unrezzed or any Archives copy is
/// facedown. That reads the same facts `mask_installed_card` and
/// `mask_archived_card` read, so the log cannot disagree with the view;
/// and it errs toward hiding — a rezzed Nico Campaign's counters are
/// concealed while a second Nico sits unrezzed elsewhere — because the
/// engine does not record whether *this* copy was seen, and reconstructing
/// it from Archives order would be exactly the first-match-by-`CardId`
/// shape the Rules Audit spent six branches removing. Callers therefore
/// mask each entry against its own post-action state
/// (`netrunner_session::Session::last_entry_for`), never a batch against
/// a later one.
///
/// Exhaustive over `GameEvent` with no catch-all arm, for the same reason
/// as `mask_action_for_player`.
pub fn mask_event_for_player(event: &GameEvent, state: &GameState, viewer: impl Into<Viewer>) -> Option<GameEvent> {
    let viewer = viewer.into();
    let concealed = |card: &CardId| !viewer.is(Side::Corp) && corp_card_concealed_from_runner(state, card);
    let visible = || Some(event.clone());

    match event {
        // Always facedown by construction: a Corp install is unrezzed until
        // `rez_ice`, and `turn::discard_to_pile` sends an HQ discard
        // facedown because the Runner never saw it.
        GameEvent::CardInstalled { side: Side::Corp, .. } | GameEvent::CardDiscarded { side: Side::Corp, .. } => {
            viewer.is(Side::Corp).then(visible).flatten()
        }
        GameEvent::CardInstalled { .. } | GameEvent::CardDiscarded { .. } => visible(),
        // A Corp trash lands faceup only if the Runner had seen the card
        // (`ability::orient`); a facedown copy now in Archives means this
        // may have been it. Spin Doctor's self-removal is the only
        // `CardRemovedFromGame` and comes off a rezzed card, but
        // `removed_from_game` is projected into no view, so the same rule.
        GameEvent::CardTrashed { side: Side::Corp, card } | GameEvent::CardRemovedFromGame { side: Side::Corp, card } => {
            (!concealed(card)).then(visible).flatten()
        }
        GameEvent::CardTrashed { .. } | GameEvent::CardRemovedFromGame { .. } => visible(),
        // Advancement tokens are public on an unrezzed card; its identity
        // and its counters are not (`PublicInstalledCard`). Rig cards and
        // identities never match the predicate and pass through.
        // `CardDerezzed` is here too, although the Runner *saw* the card
        // while it was rezzed: the view has no notion of "seen before" and
        // renders the derezzed install as `card: None`, and the log's rule
        // is never to name what the view conceals. The StateUpdate still
        // shows which install flipped.
        GameEvent::CardAdvanced { card, .. }
        | GameEvent::CountersAdded { card, .. }
        | GameEvent::CountersRemoved { card, .. }
        | GameEvent::CardDerezzed { card } => (!concealed(card)).then(visible).flatten(),
        // Tāo Salonga swaps ICE the Runner may not be able to identify —
        // the emit site's comment deferred masking to here.
        GameEvent::IceSwapped { a, b } => (!concealed(a) && !concealed(b)).then(visible).flatten(),
        // The chooser saw what they picked — unless the selection was the
        // Runner's over the Corp's *installed* cards (Tāo's
        // `OpponentInstalled`), where the pending decision offered them
        // positions, not identities.
        GameEvent::CardsSelected { side, cards, revealed } => {
            let chooser_may_see = viewer.is(*side) && !cards.iter().any(concealed);
            (*revealed || chooser_may_see).then(visible).flatten()
        }
        // The same line `mask_run_state` draws: the Runner sees what they
        // access; the Corp learns which of its cards that was only in
        // Archives, where everything is faceup after an access.
        GameEvent::CardAccessed { server, .. } => {
            (viewer.is(Side::Runner) || *server == ServerId::Archives).then(visible).flatten()
        }
        GameEvent::AccessPassed { .. } => viewer.is(Side::Runner).then(visible).flatten(),
        // A prevention window naming an unrezzed Corp install. No card in
        // the pool opens one today (`PreventTrash` is unused); the rule is
        // here so the day one does, it is not a leak.
        GameEvent::TrashAboutToResolve { target: CardTarget::CorpInstalled { card, .. } }
        | GameEvent::TrashPrevented { target: CardTarget::CorpInstalled { card, .. } } => {
            (!concealed(card)).then(visible).flatten()
        }
        GameEvent::TrashAboutToResolve { .. } | GameEvent::TrashPrevented { .. } => visible(),
        // The one card-bearing field that can be struck out in place.
        GameEvent::TraceInitiated { base, initiating_card: Some(card) } if concealed(card) => {
            Some(GameEvent::TraceInitiated { base: *base, initiating_card: None })
        }
        GameEvent::TraceInitiated { .. } => visible(),
        // Every virus host today is a public rig card; this is the strip
        // the variant's doc comment promised for the day a Corp card holds
        // virus counters.
        GameEvent::VirusCountersPurged { cards } if cards.iter().any(concealed) => Some(GameEvent::VirusCountersPurged {
            cards: cards.iter().filter(|card| !concealed(card)).cloned().collect(),
        }),
        GameEvent::VirusCountersPurged { .. } => visible(),
        // Public by construction. Encounter events only ever name rezzed
        // ICE (`run::engine` passes unrezzed ICE without emitting them);
        // plays and Runner installs are faceup; `IceRezzed`, `AgendaScored`,
        // `AgendaStolen` and `CardTrashedFromAccess` land the card in a
        // public zone; `dispatcher` builds trigger plans from rezzed installs
        // only, and an ambush firing from HQ or a remote fires while the
        // Runner is accessing it; `SubroutineFired`'s `Effect` is the
        // printed text of rezzed ICE; and the rest carry no card at all.
        GameEvent::ClickSpent { .. }
        | GameEvent::CreditsGained { .. }
        | GameEvent::CardDrawn { .. }
        | GameEvent::IceApproached { .. }
        | GameEvent::IceEncountered { .. }
        | GameEvent::SubroutineBroken { .. }
        | GameEvent::SubroutineFired { .. }
        | GameEvent::IceStrengthModified { .. }
        | GameEvent::IcePassed { .. }
        | GameEvent::ServerApproached { .. }
        | GameEvent::RunSucceeded { .. }
        | GameEvent::RunJackedOut { .. }
        | GameEvent::RunCompleted { .. }
        | GameEvent::IceRezzed { .. }
        | GameEvent::RunInitiated { .. }
        | GameEvent::EventPlayed { .. }
        | GameEvent::OperationPlayed { .. }
        | GameEvent::HardwareInstalled { .. }
        | GameEvent::ProgramInstalled { .. }
        | GameEvent::ResourceInstalled { .. }
        | GameEvent::TurnEnded { .. }
        | GameEvent::TurnStarted { .. }
        | GameEvent::DiscardPending { .. }
        | GameEvent::DiscardPhaseEnded { .. }
        | GameEvent::AgendaStolen { .. }
        | GameEvent::DamageTaken { .. }
        | GameEvent::RunnerFlatlined
        | GameEvent::CreditsSpent { .. }
        | GameEvent::TagsGiven { .. }
        | GameEvent::TagsCleared { .. }
        | GameEvent::RunEndedByEffect { .. }
        | GameEvent::GameOver { .. }
        | GameEvent::AbilityActivated { .. }
        | GameEvent::CardTrashedFromAccess { .. }
        | GameEvent::PaidAbilityWindowOpened { .. }
        | GameEvent::PriorityPassed { .. }
        | GameEvent::PaidAbilityWindowClosed
        | GameEvent::StrengthBoosted { .. }
        | GameEvent::TraceCorpBidSubmitted { .. }
        | GameEvent::TraceRunnerBidSubmitted { .. }
        | GameEvent::TraceAvoided { .. }
        | GameEvent::TraceSuccessful { .. }
        | GameEvent::TagRemoved { .. }
        | GameEvent::TagsRemoved { .. }
        | GameEvent::TriggerOrderPending { .. }
        | GameEvent::TriggerOrderChosen { .. }
        | GameEvent::TriggerFired { .. }
        | GameEvent::BadPublicityCreditsSpent { .. }
        | GameEvent::BonusRunCreditsSpent { .. }
        | GameEvent::PendingCardSelectionOffered { .. }
        | GameEvent::MemoryLimitExceeded { .. }
        | GameEvent::PendingServerChoiceOffered { .. }
        | GameEvent::BadPublicityGiven { .. }
        | GameEvent::BadPublicityRemoved { .. }
        | GameEvent::HandKept { .. }
        | GameEvent::MulliganTaken { .. }
        | GameEvent::AdditionalAccessGranted { .. }
        | GameEvent::AccessReplacementSet { .. }
        | GameEvent::AccessReplaced { .. }
        | GameEvent::CreditsLost { .. }
        | GameEvent::ClicksLost { .. }
        | GameEvent::ClicksGained { .. }
        | GameEvent::RecurringCreditsSpent { .. }
        | GameEvent::AgendaScored { .. }
        | GameEvent::DamageAboutToResolve { .. }
        | GameEvent::DamagePrevented { .. }
        | GameEvent::MaxHandSizeGained { .. }
        | GameEvent::BasicDrawActionTaken { .. }
        | GameEvent::PendingChoicePresented { .. }
        | GameEvent::PendingChoiceResolved { .. }
        | GameEvent::PendingPaidChoiceOffered { .. }
        | GameEvent::PendingPaidChoiceAccepted { .. }
        | GameEvent::PendingPaidChoiceDeclined { .. } => visible(),
    }
}

/// Whether the Runner may not currently be told that a Corp card named
/// `card` was involved in something: true while any installed copy is
/// unrezzed or any Archives copy is facedown — the two places the Runner's
/// view hides a Corp card's identity while still showing that a card is
/// there. HQ and R&D are deliberately *not* consulted: three copies of a
/// card in a deck would then conceal every event about the rezzed one on
/// the table, and the Runner's log would go dark on the Corp entirely.
fn corp_card_concealed_from_runner(state: &GameState, card: &CardId) -> bool {
    state.corp.installed.iter().any(|installed| installed.card == *card && !installed.rezzed)
        || state.corp.archives.iter().any(|archived| archived.card == *card && archived.facedown)
}

fn mask_run_ice(ice: &RunIce, owner_view: bool) -> PublicRunIce {
    let identity_visible = owner_view || ice.rezzed;
    PublicRunIce {
        install_id: ice.install_id,
        rezzed: ice.rezzed,
        identity: identity_visible.then(|| PublicRunIceIdentity {
            card: ice.card_id.clone(),
            current_strength: ice.current_strength,
            ice_type: ice.ice_type,
            subroutines: ice.subroutines.clone(),
        }),
    }
}

fn mask_access_phase(phase: &AccessPhase, card_visible: bool) -> PublicAccessPhase {
    match phase {
        AccessPhase::SelectNextCard { selectable_cards } => {
            PublicAccessPhase::SelectNextCard { selectable_cards: mask_zone(selectable_cards, card_visible) }
        }
        AccessPhase::PendingInteractiveTrigger { card_id, cost, decider, can_pay } => PublicAccessPhase::PendingInteractiveTrigger {
            card: card_visible.then(|| card_id.clone()),
            cost: cost.clone(),
            decider: *decider,
            can_pay: *can_pay,
        },
        AccessPhase::PendingChoice { card_id, trash_cost, mandatory_steal, steal_cost } => PublicAccessPhase::PendingChoice {
            card: card_visible.then(|| card_id.clone()),
            trash_cost: *trash_cost,
            mandatory_steal: *mandatory_steal,
            steal_cost: steal_cost.clone(),
        },
    }
}

fn mask_access_state(access: &AccessState, card_visible: bool) -> PublicAccessState {
    PublicAccessState {
        server: access.server,
        unaccessed_cards: mask_zone(&access.unaccessed_cards, card_visible),
        resolved_cards: mask_zone(&access.resolved_cards, card_visible),
        pending_install: access.pending_install,
        phase: mask_access_phase(&access.phase, card_visible),
    }
}

fn mask_run_state(run: &RunState, viewer: Viewer) -> PublicRunState {
    // Only the Runner sees accessed-card identities before the accessed
    // server is Archives (an always-public zone) — the Corp, and a
    // spectator, learn what was hit when it lands in a public zone.
    let card_visible = viewer.is(Side::Runner) || run.server == ServerId::Archives;
    PublicRunState {
        server: run.server,
        phase: run.phase,
        ice: run.ice.iter().map(|ice| mask_run_ice(ice, viewer.is(Side::Corp))).collect(),
        position: run.position,
        access_state: run.access_state.as_ref().map(|access| mask_access_state(access, card_visible)),
        jack_out_permitted: run.jack_out_permitted,
        bad_publicity_credits: run.bad_publicity_credits,
        bonus_run_credits: run.bonus_run_credits,
        runner_cannot_steal_or_trash: run.runner_cannot_steal_or_trash,
    }
}

fn mask_zone(cards: &[CardId], owner_view: bool) -> MaskedZone {
    if owner_view {
        MaskedZone::Visible(cards.to_vec())
    } else {
        MaskedZone::Hidden {
            count: cards.len() as u32,
        }
    }
}

fn mask_installed_card(installed: &InstalledCard, position: usize, owner_view: bool) -> PublicInstalledCard {
    let identity_visible = owner_view || installed.rezzed;
    PublicInstalledCard {
        // Outside the `identity_visible` gate on purpose: these are the
        // handles that let a viewer act on a card they cannot identify.
        install_id: installed.install_id,
        position,
        server: installed.server,
        slot: installed.slot,
        rezzed: installed.rezzed,
        card: identity_visible.then(|| installed.card.clone()),
        advancement_tokens: installed.advancement_tokens,
        // Same predicate as `card`, deliberately reusing the local rather
        // than restating the condition: counters and identity are hidden
        // together or not at all, and two copies of the rule could drift.
        counters: identity_visible.then_some(installed.counters),
    }
}

/// One Archives card as seen by a given viewer. `facedown` is public to
/// both sides — everyone can see the shape of the pile — but `card` is
/// `None` for a facedown card viewed by the Runner, who has never seen it.
/// The Corp, looking at its own zone, always gets `Some`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicArchivedCard {
    /// `None` only when the card is facedown and the viewer is the Runner.
    pub card: Option<CardId>,
    pub facedown: bool,
}

fn mask_archived_card(archived: &ArchivedCard, owner_view: bool) -> PublicArchivedCard {
    let visible = owner_view || !archived.facedown;
    PublicArchivedCard {
        card: visible.then(|| archived.card.clone()),
        facedown: archived.facedown,
    }
}

fn mask_corp_state(corp: &CorpState, owner_view: bool) -> PublicCorpState {
    PublicCorpState {
        resources: corp.resources.clone(),
        hq: mask_zone(&corp.hq, owner_view),
        r_and_d: mask_zone(&corp.r_and_d, owner_view),
        archives: corp.archives.iter().map(|a| mask_archived_card(a, owner_view)).collect(),
        installed: corp
            .installed
            .iter()
            .enumerate()
            .map(|(position, card)| mask_installed_card(card, position, owner_view))
            .collect(),
        scored_agendas: corp.scored_agendas.clone(),
        bad_publicity: corp.bad_publicity,
        recurring_credits: corp.recurring_credits,
        recurring_credits_max: corp.recurring_credits_max,
    }
}

/// Deliberately still `effective_strength()`, not `ability::
/// computed_runner_strength` — `mask_state_for_player`'s whole call chain has
/// no `CardRegistry` parameter today (a much wider signature change than
/// this milestone's actual cards justify: it would ripple into every
/// consumer crate's `mask_state_for_player` call site). A card with a
/// `StrengthModifier` (e.g. Echelon) therefore displays its strength here
/// without that live bonus — the *mechanical* result (`Effect::
/// BreakSubroutines`'s strength contest, which does call
/// `computed_runner_strength`) is unaffected and always correct; only this
/// masked-view number can lag behind it. Revisit if a real UI consumer ever
/// needs the displayed number to match.
fn mask_installed_runner_card(card: &InstalledRunnerCard) -> PublicInstalledRunnerCard {
    PublicInstalledRunnerCard {
        card: card.card.clone(),
        install_id: card.install_id,
        current_strength: card.effective_strength(),
        hosted_on_ice: card.hosted_on_ice,
        counters: card.counters,
    }
}

fn mask_runner_state(runner: &RunnerState, owner_view: bool) -> PublicRunnerState {
    PublicRunnerState {
        resources: runner.resources.clone(),
        memory_units: runner.memory_units,
        brain_damage: runner.brain_damage,
        tags: runner.tags,
        grip: mask_zone(&runner.grip, owner_view),
        stack: mask_zone(&runner.stack, owner_view),
        rig: runner.rig.iter().map(mask_installed_runner_card).collect(),
        heap: runner.heap.clone(),
        scored_agendas: runner.scored_agendas.clone(),
        link_strength: runner.link_strength,
        servers_run_this_turn: runner.servers_run_this_turn.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::{AgendaPoints, Clicks, Credits, InstallSlot};

    fn game_state(corp: CorpState) -> GameState {
        GameState {
            corp,
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Corp),
            ..Default::default()
        }
    }

    fn corp_state_with_cards() -> CorpState {
        CorpState {
            resources: PlayerResources {
                credits: Credits(5),
                clicks: Clicks(3),
                agenda_points: AgendaPoints(0),
            },
            hq: vec![CardId("hedge_fund".to_string())],
            r_and_d: vec![CardId("ice_wall".to_string()), CardId("enigma".to_string())],
            archives: vec![ArchivedCard::facedown(CardId("cyberdex_trial".to_string()))],
            installed: vec![
                InstalledCard {
                    install_id: InstallId(1069),
                    card: CardId("ice_wall".to_string()),
                    slot: InstallSlot::Ice,
                    ..Default::default()
                },
                InstalledCard {
                    install_id: InstallId(1070),
                    card: CardId("enigma".to_string()),
                    server: ServerId::RnD,
                    slot: InstallSlot::Ice,
                    rezzed: true,
                    advancement_tokens: 2,
                    ..Default::default()
                },
            ],
            scored_agendas: vec![CardId("hostile_takeover".to_string())],
            ..Default::default()
        }
    }

    #[test]
    fn corp_view_shows_own_hq_and_rd_contents() {
        let state = game_state(corp_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Corp);

        assert_eq!(
            masked.corp.hq,
            MaskedZone::Visible(vec![CardId("hedge_fund".to_string())])
        );
        assert_eq!(
            masked.corp.r_and_d,
            MaskedZone::Visible(vec![
                CardId("ice_wall".to_string()),
                CardId("enigma".to_string())
            ])
        );
    }

    #[test]
    fn runner_view_hides_corp_hq_and_rd_contents_but_shows_counts() {
        let state = game_state(corp_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Runner);

        assert_eq!(masked.corp.hq, MaskedZone::Hidden { count: 1 });
        assert_eq!(masked.corp.r_and_d, MaskedZone::Hidden { count: 2 });
    }

    #[test]
    fn runner_view_hides_unrezzed_installed_card_identity() {
        let state = game_state(corp_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Runner);

        let unrezzed = &masked.corp.installed[0];
        assert_eq!(unrezzed.server, ServerId::Hq);
        assert!(!unrezzed.rezzed);
        assert_eq!(unrezzed.card, None);
    }

    #[test]
    fn runner_view_reveals_rezzed_installed_card_identity() {
        let state = game_state(corp_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Runner);

        let rezzed = &masked.corp.installed[1];
        assert!(rezzed.rezzed);
        assert_eq!(rezzed.card, Some(CardId("enigma".to_string())));
    }

    #[test]
    fn corp_view_shows_own_installed_cards_even_unrezzed() {
        let state = game_state(corp_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Corp);

        let unrezzed = &masked.corp.installed[0];
        assert!(!unrezzed.rezzed);
        assert_eq!(unrezzed.card, Some(CardId("ice_wall".to_string())));
    }

    fn runner_state_with_cards() -> RunnerState {
        RunnerState {
            resources: PlayerResources {
                credits: Credits(5),
                clicks: Clicks(3),
                agenda_points: AgendaPoints(0),
            },
            memory_units: MemoryUnits(4),
            grip: vec![CardId("sure_gamble".to_string())],
            stack: vec![CardId("modded".to_string()), CardId("clone_chip".to_string())],
            rig: vec![InstalledRunnerCard {
                card: CardId("gordian_blade".to_string()),
                base_strength: 2,
                encounter_strength_buff: 1,
                ..Default::default()
            }],
            heap: vec![CardId("easy_mark".to_string())],
            scored_agendas: vec![CardId("priority_requisition".to_string())],
            ..Default::default()
        }
    }

    fn game_state_with_runner(runner: RunnerState) -> GameState {
        GameState {
            corp: corp_state_with_cards(),
            runner,
            phase: GamePhase::Action(Side::Runner),
            ..Default::default()
        }
    }

    #[test]
    fn runner_view_shows_own_grip_and_stack_contents() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Runner);

        assert_eq!(
            masked.runner.grip,
            MaskedZone::Visible(vec![CardId("sure_gamble".to_string())])
        );
        assert_eq!(
            masked.runner.stack,
            MaskedZone::Visible(vec![
                CardId("modded".to_string()),
                CardId("clone_chip".to_string())
            ])
        );
    }

    #[test]
    fn corp_view_hides_runner_grip_and_stack_contents_but_shows_counts() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Corp);

        assert_eq!(masked.runner.grip, MaskedZone::Hidden { count: 1 });
        assert_eq!(masked.runner.stack, MaskedZone::Hidden { count: 2 });
    }

    #[test]
    fn a_facedown_archives_card_hides_its_identity_from_the_runner_but_not_the_corp() {
        let state = game_state(corp_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        // The Corp always sees its own zone in full.
        assert_eq!(
            masked_for_corp.corp.archives,
            vec![PublicArchivedCard { card: Some(CardId("cyberdex_trial".to_string())), facedown: true }]
        );
        // The Runner sees the pile's shape — one card, facedown — but never
        // learns which card it is.
        assert_eq!(masked_for_runner.corp.archives, vec![PublicArchivedCard { card: None, facedown: true }]);
    }

    #[test]
    fn a_faceup_archives_card_is_visible_to_both_sides() {
        let mut corp = corp_state_with_cards();
        corp.archives = vec![ArchivedCard::faceup(CardId("hedge_fund".to_string()))];
        let state = game_state(corp);

        let expected = vec![PublicArchivedCard { card: Some(CardId("hedge_fund".to_string())), facedown: false }];
        assert_eq!(mask_state_for_player(&state, Side::Corp).corp.archives, expected);
        assert_eq!(mask_state_for_player(&state, Side::Runner).corp.archives, expected);
    }

    #[test]
    fn corp_bad_publicity_is_never_masked() {
        let mut corp = corp_state_with_cards();
        corp.bad_publicity = 3;
        let state = game_state(corp);

        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        assert_eq!(masked_for_corp.corp.bad_publicity, 3);
        assert_eq!(masked_for_runner.corp.bad_publicity, 3);
    }

    /// Counters follow the card's identity exactly: the Corp sees its own
    /// unrezzed card's counters, the Runner sees `None` for the same card,
    /// and both see them once it is rezzed.
    ///
    /// The hidden case is the one that matters — a *Nico Campaign*'s credit
    /// count would otherwise identify an unrezzed asset outright.
    #[test]
    fn corp_installed_counters_are_masked_exactly_like_the_card_identity() {
        let mut corp = corp_state_with_cards();
        corp.installed[0].counters = 4; // ice_wall, unrezzed
        corp.installed[1].counters = 7; // enigma, rezzed
        let state = game_state(corp);

        let for_corp = mask_state_for_player(&state, Side::Corp);
        let for_runner = mask_state_for_player(&state, Side::Runner);

        // Unrezzed: owner sees the count, opponent sees nothing at all.
        assert_eq!(for_corp.corp.installed[0].counters, Some(4));
        assert_eq!(for_runner.corp.installed[0].counters, None);
        // ...and that is the same condition gating the identity itself.
        assert!(for_corp.corp.installed[0].card.is_some());
        assert!(for_runner.corp.installed[0].card.is_none());

        // Rezzed: public to both.
        assert_eq!(for_corp.corp.installed[1].counters, Some(7));
        assert_eq!(for_runner.corp.installed[1].counters, Some(7));
    }

    /// A rezzed card holding no counters must stay distinguishable from one
    /// whose counters are hidden — `Some(0)` versus `None`. Collapsing both
    /// to `0` is the specific mistake this asserts against.
    #[test]
    fn zero_counters_on_a_rezzed_card_is_some_zero_not_none() {
        let state = game_state(corp_state_with_cards());
        let for_runner = mask_state_for_player(&state, Side::Runner);

        assert_eq!(for_runner.corp.installed[1].counters, Some(0), "rezzed, genuinely empty");
        assert_eq!(for_runner.corp.installed[0].counters, None, "unrezzed, concealed");
    }

    /// The Runner half carries no visibility rule at all — a rig card is
    /// always face-up, so its counters are public the moment they land.
    #[test]
    fn runner_rig_counters_are_visible_to_both_sides() {
        let mut runner = runner_state_with_cards();
        runner.rig[0].counters = 3;
        let state = game_state_with_runner(runner);

        let for_corp = mask_state_for_player(&state, Side::Corp);
        let for_runner = mask_state_for_player(&state, Side::Runner);

        assert_eq!(for_corp.runner.rig[0].counters, 3);
        assert_eq!(for_runner.runner.rig[0].counters, 3);
    }

    #[test]
    fn a_trojans_host_ice_is_visible_to_both_sides() {
        let mut runner = runner_state_with_cards();
        runner.rig[0].hosted_on_ice = Some(InstallId(77));
        let state = game_state_with_runner(runner);

        for side in [Side::Corp, Side::Runner] {
            let masked = mask_state_for_player(&state, side);
            assert_eq!(
                masked.runner.rig[0].hosted_on_ice,
                Some(InstallId(77)),
                "{side:?} should see where a Trojan is hosted"
            );
        }
    }

    #[test]
    fn corp_recurring_credits_are_visible_to_both_sides() {
        let mut state = game_state_with_runner(runner_state_with_cards());
        state.corp.recurring_credits = 1;
        state.corp.recurring_credits_max = 2;

        for side in [Side::Corp, Side::Runner] {
            let masked = mask_state_for_player(&state, side);
            assert_eq!(masked.corp.recurring_credits, 1, "{side:?}");
            assert_eq!(masked.corp.recurring_credits_max, 2, "{side:?}");
        }
    }

    #[test]
    fn run_scoped_credit_pools_and_restrictions_are_visible_to_both_sides() {
        let mut state = game_state_with_runner(runner_state_with_cards());
        state.active_run = Some(RunState {
            bad_publicity_credits: 2,
            bonus_run_credits: 3,
            runner_cannot_steal_or_trash: true,
            ..Default::default()
        });

        for side in [Side::Corp, Side::Runner] {
            let run = mask_state_for_player(&state, side).active_run.expect("the run is public");
            assert_eq!(run.bad_publicity_credits, 2, "{side:?}");
            assert_eq!(run.bonus_run_credits, 3, "{side:?}");
            assert!(run.runner_cannot_steal_or_trash, "{side:?}");
        }
    }

    /// The one field on an unrezzed Corp install that stays visible.
    ///
    /// `install_id` sits deliberately *outside* `mask_installed_card`'s
    /// `identity_visible` gate. It has to: real Netrunner lets the Runner
    /// host a Trojan on unrezzed ICE and swap ICE they cannot identify, so
    /// they must be able to *name* a card whose identity is hidden. Masking
    /// it too would leave those actions with no handle but the `CardId`,
    /// which is exactly the leak it was introduced to close.
    #[test]
    fn install_id_is_never_masked_even_on_an_unrezzed_card() {
        let state = game_state(corp_state_with_cards());
        let for_runner = mask_state_for_player(&state, Side::Runner);
        let for_corp = mask_state_for_player(&state, Side::Corp);

        let unrezzed = &for_runner.corp.installed[0];
        assert_eq!(unrezzed.card, None, "the premise: this card's identity is hidden");
        assert_eq!(unrezzed.counters, None, "and so are its counters");
        assert_eq!(
            unrezzed.install_id,
            state.corp.installed[0].install_id,
            "but its handle is not — the Runner can still name it"
        );

        // Both viewers agree on every id, which is what lets an action one
        // side submits mean the same install to the engine.
        let runner_ids: Vec<_> = for_runner.corp.installed.iter().map(|c| c.install_id).collect();
        let corp_ids: Vec<_> = for_corp.corp.installed.iter().map(|c| c.install_id).collect();
        assert_eq!(runner_ids, corp_ids);
    }

    #[test]
    fn runner_rig_is_never_masked() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        let expected = vec![PublicInstalledRunnerCard {
            card: CardId("gordian_blade".to_string()),
            install_id: InstallId::PLACEHOLDER,
            current_strength: 3,
            hosted_on_ice: None,
            counters: 0,
        }];
        assert_eq!(masked_for_corp.runner.rig, expected);
        assert_eq!(masked_for_runner.runner.rig, expected);
    }

    #[test]
    fn runner_rig_current_strength_is_never_masked() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        // base_strength 2 + encounter_strength_buff 1 = 3, from
        // runner_state_with_cards().
        assert_eq!(masked_for_corp.runner.rig[0].current_strength, 3);
        assert_eq!(masked_for_runner.runner.rig[0].current_strength, 3);
    }

    #[test]
    fn runner_heap_is_never_masked() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        let expected = vec![CardId("easy_mark".to_string())];
        assert_eq!(masked_for_corp.runner.heap, expected);
        assert_eq!(masked_for_runner.runner.heap, expected);
    }

    #[test]
    fn runner_tags_and_brain_damage_are_never_masked() {
        let mut runner = runner_state_with_cards();
        runner.tags = 2;
        runner.brain_damage = 1;
        let state = game_state_with_runner(runner);

        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        assert_eq!(masked_for_corp.runner.tags, 2);
        assert_eq!(masked_for_runner.runner.tags, 2);
        assert_eq!(masked_for_corp.runner.brain_damage, 1);
        assert_eq!(masked_for_runner.runner.brain_damage, 1);
    }

    #[test]
    fn scored_agendas_are_never_masked() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        let expected_corp = vec![CardId("hostile_takeover".to_string())];
        let expected_runner = vec![CardId("priority_requisition".to_string())];
        assert_eq!(masked_for_corp.corp.scored_agendas, expected_corp);
        assert_eq!(masked_for_runner.corp.scored_agendas, expected_corp);
        assert_eq!(masked_for_corp.runner.scored_agendas, expected_runner);
        assert_eq!(masked_for_runner.runner.scored_agendas, expected_runner);
    }

    #[test]
    fn advancement_tokens_are_never_masked() {
        let state = game_state(corp_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        // installed[1] ("enigma") is rezzed with 2 advancement tokens.
        assert_eq!(masked_for_corp.corp.installed[1].advancement_tokens, 2);
        assert_eq!(masked_for_runner.corp.installed[1].advancement_tokens, 2);
    }

    use crate::dsl::{Effect, SubroutineDef};
    use crate::rules::run::SubroutineStatus;

    fn run_ice(id: &str, rezzed: bool) -> RunIce {
        RunIce {
            install_id: crate::rules::InstallId::PLACEHOLDER,
            card_id: CardId(id.to_string()),
            current_strength: 3,
            ice_type: IceType::Barrier,
            subroutines: if rezzed {
                vec![EncounteredSubroutine {
                    id: 0,
                    definition: SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun },
                    status: SubroutineStatus::Pending,
                }]
            } else {
                Vec::new()
            },
            rezzed,
        }
    }

    fn run_state(server: ServerId, ice: Vec<RunIce>, access_state: Option<AccessState>) -> RunState {
        RunState {
            server,
            phase: if access_state.is_some() { RunPhase::AccessingCard } else { RunPhase::ApproachIce },
            ice,
            access_state,
            jack_out_permitted: true,
            ..Default::default()
        }
    }

    fn state_with_run(run: RunState) -> GameState {
        let mut state = game_state_with_runner(runner_state_with_cards());
        state.active_run = Some(run);
        state
    }

    #[test]
    fn unrezzed_ice_identity_is_hidden_from_runner_but_visible_to_corp() {
        let run = run_state(ServerId::Hq, vec![run_ice("ice_wall", false)], None);
        let state = state_with_run(run);

        let for_runner = mask_state_for_player(&state, Side::Runner);
        let ice = &for_runner.active_run.as_ref().unwrap().ice[0];
        assert!(!ice.rezzed);
        assert_eq!(ice.identity, None);

        let for_corp = mask_state_for_player(&state, Side::Corp);
        let ice = &for_corp.active_run.as_ref().unwrap().ice[0];
        assert_eq!(ice.identity.as_ref().unwrap().card, CardId("ice_wall".to_string()));
    }

    #[test]
    fn rezzed_ice_identity_and_subroutines_are_visible_to_both_sides() {
        let run = run_state(ServerId::Hq, vec![run_ice("enigma", true)], None);
        let state = state_with_run(run);

        for side in [Side::Corp, Side::Runner] {
            let masked = mask_state_for_player(&state, side);
            let identity = masked.active_run.as_ref().unwrap().ice[0].identity.as_ref().unwrap();
            assert_eq!(identity.card, CardId("enigma".to_string()));
            assert_eq!(identity.subroutines.len(), 1);
        }
    }

    #[test]
    fn accessed_hq_card_identity_is_hidden_from_corp_but_visible_to_runner() {
        let access = AccessState { pending_install: None, resolved_installs: Vec::new(),
            unaccessed_cards: vec![CardId("agenda".to_string())],
            phase: AccessPhase::PendingChoice {
                card_id: CardId("hedge_fund".to_string()),
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            },
            ..Default::default()
        };
        let run = run_state(ServerId::Hq, Vec::new(), Some(access));
        let state = state_with_run(run);

        let for_corp = mask_state_for_player(&state, Side::Corp);
        let corp_access = for_corp.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(corp_access.unaccessed_cards, MaskedZone::Hidden { count: 1 });
        assert!(matches!(&corp_access.phase, PublicAccessPhase::PendingChoice { card: None, .. }));

        let for_runner = mask_state_for_player(&state, Side::Runner);
        let runner_access = for_runner.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(runner_access.unaccessed_cards, MaskedZone::Visible(vec![CardId("agenda".to_string())]));
        assert!(matches!(
            &runner_access.phase,
            PublicAccessPhase::PendingChoice { card: Some(id), .. } if *id == CardId("hedge_fund".to_string())
        ));
    }

    #[test]
    fn accessed_archives_card_identity_is_visible_to_both_sides() {
        let access = AccessState { pending_install: None, resolved_installs: Vec::new(),
            server: ServerId::Archives,
            phase: AccessPhase::PendingChoice {
                card_id: CardId("cyberdex_trial".to_string()),
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            },
            ..Default::default()
        };
        let run = run_state(ServerId::Archives, Vec::new(), Some(access));
        let state = state_with_run(run);

        for side in [Side::Corp, Side::Runner] {
            let masked = mask_state_for_player(&state, side);
            let access = masked.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
            assert!(matches!(
                &access.phase,
                PublicAccessPhase::PendingChoice { card: Some(id), .. } if *id == CardId("cyberdex_trial".to_string())
            ));
        }
    }
    // ---- per-viewer action and event masking ----

    fn id(s: &str) -> CardId {
        CardId(s.to_string())
    }

    #[test]
    fn a_corp_install_action_is_concealed_from_the_runner_but_not_the_corp() {
        let action = PlayerAction::InstallCard { card_id: id("ice_wall"), zone: ServerId::Remote(0), slot: InstallSlot::Ice };
        assert_eq!(
            mask_action_for_player(&action, Side::Corp, Side::Runner),
            PublicAction::Concealed(ConcealedAction::InstallCard { zone: ServerId::Remote(0), slot: InstallSlot::Ice })
        );
        assert_eq!(mask_action_for_player(&action, Side::Corp, Side::Corp), PublicAction::Visible(action.clone()));
    }

    #[test]
    fn a_corp_discard_is_concealed_but_a_runner_discard_is_not() {
        let action = PlayerAction::DiscardCard { card_id: id("hedge_fund") };
        assert_eq!(
            mask_action_for_player(&action, Side::Corp, Side::Runner),
            PublicAction::Concealed(ConcealedAction::DiscardCard)
        );
        assert_eq!(mask_action_for_player(&action, Side::Runner, Side::Corp), PublicAction::Visible(action.clone()));
    }

    #[test]
    fn the_runners_access_actions_are_concealed_from_the_corp_unless_the_card_leaves_the_zone() {
        let concealed = [
            (PlayerAction::SelectCardToAccess { card_id: id("hedge_fund") }, ConcealedAction::SelectCardToAccess),
            (PlayerAction::PassAccessedCard { card_id: id("hedge_fund") }, ConcealedAction::PassAccessedCard),
            (PlayerAction::PayAccessTrigger { card_id: id("hedge_fund") }, ConcealedAction::PayAccessTrigger),
            (PlayerAction::DeclineAccessTrigger { card_id: id("hedge_fund") }, ConcealedAction::DeclineAccessTrigger),
        ];
        for (action, expected) in concealed {
            assert_eq!(mask_action_for_player(&action, Side::Runner, Side::Corp), PublicAction::Concealed(expected));
            assert_eq!(mask_action_for_player(&action, Side::Runner, Side::Runner), PublicAction::Visible(action.clone()));
        }
        for action in [
            PlayerAction::StealAgenda { card_id: id("hostile_takeover") },
            PlayerAction::TrashAccessedCard { card_id: id("nico_campaign") },
        ] {
            assert_eq!(mask_action_for_player(&action, Side::Runner, Side::Corp), PublicAction::Visible(action.clone()));
        }
    }

    #[test]
    fn a_corp_install_or_discard_event_is_dropped_for_the_runner_only() {
        let state = game_state(corp_state_with_cards());
        for event in [
            GameEvent::CardInstalled { side: Side::Corp, card: id("ice_wall"), server: ServerId::Hq },
            GameEvent::CardDiscarded { side: Side::Corp, card: id("hedge_fund") },
        ] {
            assert_eq!(mask_event_for_player(&event, &state, Side::Runner), None);
            assert_eq!(mask_event_for_player(&event, &state, Side::Corp), Some(event.clone()));
        }
        let runner_discard = GameEvent::CardDiscarded { side: Side::Runner, card: id("sure_gamble") };
        assert_eq!(mask_event_for_player(&runner_discard, &state, Side::Corp), Some(runner_discard.clone()));
    }

    #[test]
    fn advancing_or_counting_an_unrezzed_card_drops_the_event_for_the_runner() {
        // `ice_wall` is installed unrezzed; `enigma` is rezzed.
        let state = game_state(corp_state_with_cards());
        for (hidden, shown) in [
            (
                GameEvent::CardAdvanced { card: id("ice_wall"), advancement_tokens: 1 },
                GameEvent::CardAdvanced { card: id("enigma"), advancement_tokens: 3 },
            ),
            (
                GameEvent::CountersAdded { card: id("ice_wall"), amount: 2 },
                GameEvent::CountersAdded { card: id("enigma"), amount: 2 },
            ),
            (
                GameEvent::CountersRemoved { card: id("ice_wall"), amount: 1 },
                GameEvent::CountersRemoved { card: id("enigma"), amount: 1 },
            ),
            // Tranquilizer derezzed it: the Runner saw it rezzed, but the
            // view now conceals it, and the log follows the view.
            (GameEvent::CardDerezzed { card: id("ice_wall") }, GameEvent::CardDerezzed { card: id("enigma") }),
        ] {
            assert_eq!(mask_event_for_player(&hidden, &state, Side::Runner), None);
            assert_eq!(mask_event_for_player(&hidden, &state, Side::Corp), Some(hidden.clone()));
            assert_eq!(mask_event_for_player(&shown, &state, Side::Runner), Some(shown.clone()));
        }
    }

    #[test]
    fn a_facedown_copy_in_archives_conceals_a_corp_trash() {
        // `cyberdex_trial` sits facedown in Archives; nothing hides `hostile_takeover`.
        let state = game_state(corp_state_with_cards());
        let hidden = GameEvent::CardTrashed { side: Side::Corp, card: id("cyberdex_trial") };
        let shown = GameEvent::CardTrashed { side: Side::Corp, card: id("hostile_takeover") };
        assert_eq!(mask_event_for_player(&hidden, &state, Side::Runner), None);
        assert_eq!(mask_event_for_player(&hidden, &state, Side::Corp), Some(hidden.clone()));
        assert_eq!(mask_event_for_player(&shown, &state, Side::Runner), Some(shown.clone()));

        let runner_trash = GameEvent::CardTrashed { side: Side::Runner, card: id("cyberdex_trial") };
        assert_eq!(mask_event_for_player(&runner_trash, &state, Side::Runner), Some(runner_trash.clone()));
    }

    #[test]
    fn a_swap_naming_an_unrezzed_ice_is_dropped_for_the_runner() {
        let state = game_state(corp_state_with_cards());
        let hidden = GameEvent::IceSwapped { a: id("ice_wall"), b: id("enigma") };
        let shown = GameEvent::IceSwapped { a: id("enigma"), b: id("enigma") };
        assert_eq!(mask_event_for_player(&hidden, &state, Side::Runner), None);
        assert_eq!(mask_event_for_player(&hidden, &state, Side::Corp), Some(hidden.clone()));
        assert_eq!(mask_event_for_player(&shown, &state, Side::Runner), Some(shown.clone()));
    }

    #[test]
    fn an_unrevealed_selection_is_seen_only_by_a_chooser_who_could_see_the_cards() {
        let state = game_state(corp_state_with_cards());
        let corp_picks_from_hq = GameEvent::CardsSelected { side: Side::Corp, cards: vec![id("hedge_fund")], revealed: false };
        assert_eq!(mask_event_for_player(&corp_picks_from_hq, &state, Side::Corp), Some(corp_picks_from_hq.clone()));
        assert_eq!(mask_event_for_player(&corp_picks_from_hq, &state, Side::Runner), None);

        // Tāo Salonga: the Runner selected over the Corp's installed cards
        // by position and must not learn what they picked.
        let runner_picks_unrezzed_ice = GameEvent::CardsSelected { side: Side::Runner, cards: vec![id("ice_wall")], revealed: false };
        assert_eq!(mask_event_for_player(&runner_picks_unrezzed_ice, &state, Side::Runner), None);

        let revealed = GameEvent::CardsSelected { side: Side::Corp, cards: vec![id("hedge_fund")], revealed: true };
        assert_eq!(mask_event_for_player(&revealed, &state, Side::Runner), Some(revealed.clone()));
    }

    #[test]
    fn an_hq_access_event_is_hidden_from_the_corp_but_an_archives_access_is_not() {
        let state = game_state(corp_state_with_cards());
        let hq = GameEvent::CardAccessed { card: id("hedge_fund"), server: ServerId::Hq, install: None };
        let archives = GameEvent::CardAccessed { card: id("cyberdex_trial"), server: ServerId::Archives, install: None };
        assert_eq!(mask_event_for_player(&hq, &state, Side::Corp), None);
        assert_eq!(mask_event_for_player(&hq, &state, Side::Runner), Some(hq.clone()));
        assert_eq!(mask_event_for_player(&archives, &state, Side::Corp), Some(archives.clone()));

        let passed = GameEvent::AccessPassed { card: id("hedge_fund") };
        assert_eq!(mask_event_for_player(&passed, &state, Side::Corp), None);
        assert_eq!(mask_event_for_player(&passed, &state, Side::Runner), Some(passed.clone()));
    }

    #[test]
    fn a_trace_from_a_concealed_card_is_struck_out_rather_than_dropped() {
        let state = game_state(corp_state_with_cards());
        let event = GameEvent::TraceInitiated { base: 3, initiating_card: Some(id("ice_wall")) };
        assert_eq!(
            mask_event_for_player(&event, &state, Side::Runner),
            Some(GameEvent::TraceInitiated { base: 3, initiating_card: None })
        );
        assert_eq!(mask_event_for_player(&event, &state, Side::Corp), Some(event.clone()));

        let purge = GameEvent::VirusCountersPurged { cards: vec![id("botulus"), id("ice_wall")] };
        assert_eq!(
            mask_event_for_player(&purge, &state, Side::Runner),
            Some(GameEvent::VirusCountersPurged { cards: vec![id("botulus")] })
        );
    }

    #[test]
    fn a_prevention_window_on_an_unrezzed_install_is_dropped_for_the_runner() {
        let state = game_state(corp_state_with_cards());
        let hidden = GameEvent::TrashAboutToResolve {
            target: CardTarget::CorpInstalled { card: id("ice_wall"), server: ServerId::Hq },
        };
        let rig = GameEvent::TrashPrevented { target: CardTarget::RunnerRig(id("botulus")) };
        assert_eq!(mask_event_for_player(&hidden, &state, Side::Runner), None);
        assert_eq!(mask_event_for_player(&rig, &state, Side::Runner), Some(rig.clone()));
    }

    #[test]
    fn public_events_pass_through_unchanged_for_both_sides() {
        let state = game_state(corp_state_with_cards());
        for event in [
            GameEvent::CreditsGained { side: Side::Corp, amount: 1 },
            GameEvent::IceRezzed { card: id("ice_wall"), server: ServerId::Hq, install: InstallId(1069) },
            GameEvent::AgendaScored { card: id("hostile_takeover"), agenda_points: 1, server: ServerId::Remote(0) },
            GameEvent::CardTrashedFromAccess { card: id("nico_campaign"), cost_paid: 3 },
        ] {
            for viewer in [Side::Corp, Side::Runner] {
                assert_eq!(mask_event_for_player(&event, &state, viewer), Some(event.clone()));
            }
        }
    }

    // ----- `Viewer::Spectator`: the intersection of the two seats -----

    #[test]
    fn a_spectator_sees_neither_hand_nor_deck_and_each_board_as_the_opponent_does() {
        let state = game_state_with_runner(runner_state_with_cards());
        let spectator = mask_state_for_player(&state, Viewer::Spectator);
        assert!(matches!(spectator.corp.hq, MaskedZone::Hidden { count: 1 }));
        assert!(matches!(spectator.corp.r_and_d, MaskedZone::Hidden { count: 2 }));
        assert!(matches!(spectator.runner.grip, MaskedZone::Hidden { count: 1 }));
        assert!(matches!(spectator.runner.stack, MaskedZone::Hidden { count: 2 }));
        assert_eq!(spectator.corp.archives[0].card, None, "a facedown Archives card stays facedown");
        assert_eq!(spectator.corp, mask_state_for_player(&state, Side::Runner).corp, "the Corp's board as the Runner sees it");
        assert_eq!(spectator.runner, mask_state_for_player(&state, Side::Corp).runner, "the Runner's board as the Corp sees it");
    }

    #[test]
    fn a_spectator_sees_unrezzed_run_ice_as_the_runner_does_and_accessed_cards_as_the_corp_does() {
        let access = AccessState {
            unaccessed_cards: vec![CardId("agenda".to_string())],
            phase: AccessPhase::PendingChoice {
                card_id: CardId("hedge_fund".to_string()),
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            },
            ..Default::default()
        };
        let run = run_state(ServerId::Hq, vec![run_ice("ice_wall", false)], Some(access));
        let state = state_with_run(run);

        let spectator = mask_state_for_player(&state, Viewer::Spectator).active_run.unwrap();
        let for_runner = mask_state_for_player(&state, Side::Runner).active_run.unwrap();
        let for_corp = mask_state_for_player(&state, Side::Corp).active_run.unwrap();
        assert_eq!(spectator.ice[0].identity, None);
        assert_eq!(spectator.ice, for_runner.ice, "unrezzed ICE is the Corp's secret");
        assert_eq!(spectator.access_state, for_corp.access_state, "the accessed HQ card is the Runner's secret");
    }

    #[test]
    fn a_spectators_log_conceals_both_sides_card_naming_actions() {
        let corp_install = PlayerAction::InstallCard { card_id: id("ice_wall"), zone: ServerId::Remote(0), slot: InstallSlot::Ice };
        assert_eq!(
            mask_action_for_player(&corp_install, Side::Corp, Viewer::Spectator),
            mask_action_for_player(&corp_install, Side::Corp, Side::Runner)
        );
        let runner_access = PlayerAction::SelectCardToAccess { card_id: id("hedge_fund") };
        assert_eq!(
            mask_action_for_player(&runner_access, Side::Runner, Viewer::Spectator),
            mask_action_for_player(&runner_access, Side::Runner, Side::Corp)
        );
        // Neither seat's own-action exemption applies to someone who sits in neither.
        assert!(matches!(mask_action_for_player(&corp_install, Side::Corp, Viewer::Spectator), PublicAction::Concealed(_)));
        assert!(matches!(mask_action_for_player(&runner_access, Side::Runner, Viewer::Spectator), PublicAction::Concealed(_)));
    }

    #[test]
    fn a_spectator_gets_every_event_the_more_restricted_seat_gets_and_no_other() {
        let state = game_state(corp_state_with_cards());
        let corp_secret = GameEvent::CardInstalled { side: Side::Corp, card: id("ice_wall"), server: ServerId::Hq };
        let runner_secret = GameEvent::AccessPassed { card: id("hedge_fund") };
        let public = GameEvent::CardAccessed { card: id("cyberdex_trial"), server: ServerId::Archives, install: None };
        assert_eq!(mask_event_for_player(&corp_secret, &state, Viewer::Spectator), None);
        assert_eq!(mask_event_for_player(&runner_secret, &state, Viewer::Spectator), None);
        assert_eq!(mask_event_for_player(&public, &state, Viewer::Spectator), Some(public.clone()));
        // Struck out in place, as for the Runner.
        let trace = GameEvent::TraceInitiated { base: 2, initiating_card: Some(id("ice_wall")) };
        assert_eq!(
            mask_event_for_player(&trace, &state, Viewer::Spectator),
            mask_event_for_player(&trace, &state, Side::Runner)
        );
    }
}
