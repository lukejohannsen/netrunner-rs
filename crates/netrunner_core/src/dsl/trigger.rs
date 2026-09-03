use serde::{Deserialize, Serialize};

/// When (or how) an ability fires. Automatic variants name a rules-flow
/// moment; `Paid` marks an ability that never fires on its own and must be
/// explicitly activated by a player paying its `AbilityDef::cost`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    OnPlay,
    OnRunStart,
    /// Renamed from `OnIceEncountered` — folded into a single "OnX"
    /// vocabulary with the other automatic variants.
    OnEncounter,
    /// Renamed from `StartOfTurn`, to disambiguate from the unrelated
    /// `rules::state::GamePhase::StartOfTurn(Side)` variant, which names a
    /// turn sub-phase, not a card-ability trigger condition.
    OnTurnStart,
    /// Fires once, the moment this card is presented as the Runner's
    /// current access choice (when `GameEvent::CardAccessed` fires for it)
    /// — Ambush-style "when accessed" abilities (Snare!). Always fires
    /// unconditionally and cannot itself be paid off. A card that needs a
    /// payable "avoid this" reaction (Fetal AI) models that separately via
    /// `dsl::ability::InteractiveOnAccess`/`CardDefinition::interactive_on_access`,
    /// resolved *before* `OnAccessed` via `rules::run::state::AccessPhase::
    /// PendingInteractiveTrigger`.
    OnAccessed,
    /// Fires when this card is trashed via `PlayerAction::
    /// TrashAccessedCard` specifically — not other trash paths (a
    /// subroutine's `Effect::TrashCard`, a normal Corp trash action, etc.).
    /// Shock!-style "when trashed by the Runner accessing it" abilities.
    OnTrashedFromAccess,
    /// Fires when a run completes successfully — distinct from
    /// `OnRunStart`, which fires at initiation, not resolution.
    OnSuccessfulRun,
    /// Not a moment in time — marks an ability that only resolves when a
    /// player explicitly activates it and pays `AbilityDef::cost`. Should
    /// only ever appear as an `AbilityDef::trigger`, never inside a
    /// `TriggeredEffect` (which has no `Cost` field to pay).
    Paid,
    /// Two distinct dispatch sources, per install kind:
    /// - `PlayerAction::InstallCard` (Corp only): fires against the
    ///   *installing side's identity card* only, not the installed card
    ///   itself — e.g. Haas-Bioroid: Engineering the Future's "first
    ///   install each turn" bonus. Combine with
    ///   `EffectRequirement::FirstInstallThisTurn` to limit it to once per
    ///   turn.
    /// - `PlayerAction::InstallResource` (Runner only): fires against the
    ///   just-installed Resource card itself *and* the Runner's identity —
    ///   e.g. Red Team/Telework Contract's own "when you install this
    ///   resource, load N credits onto it."
    ///
    /// `ProgramInstalled`/`HardwareInstalled` reach the just-installed card
    /// the same way (Botulus's counter; GAMEDRAGON™ Pro's "when you install
    /// this hardware ... you may host it"). Hardware was the last type
    /// widened — no System Gateway hardware reacted to its own install.
    OnInstall,
    /// Fires against the Runner's identity card specifically when a run on
    /// HQ succeeds (`GameEvent::RunSucceeded { server: ServerId::Hq }`) —
    /// e.g. Gabriel Santiago. Combine with
    /// `EffectRequirement::FirstSuccessfulHqRunThisTurn` to limit it to once
    /// per turn.
    OnSuccessfulRunOnHq,
    /// Fires against every candidate (identity + rig) when a run succeeds
    /// against any central server (`ServerId::Hq`/`RnD`/`Archives`) —
    /// mirrors `OnSuccessfulRunOnHq`'s dispatch shape but for all three
    /// centrals rather than just HQ — e.g. Leech's "whenever you make a
    /// successful run on a central server, place 1 virus counter on this
    /// program."
    OnSuccessfulRunOnCentralServer,
    /// Fires against every candidate (identity + rig) when a run succeeds
    /// specifically against R&D (`GameEvent::RunSucceeded { server:
    /// ServerId::RnD }`) — mirrors `OnSuccessfulRunOnHq`'s exact dispatch
    /// shape but for R&D — e.g. Conduit's "whenever a successful run on R&D
    /// ends, you may place 1 virus counter on this program."
    OnSuccessfulRunOnRnD,
    /// Fires when the Corp scores an agenda (`PlayerAction::ScoreAgenda`) —
    /// dispatched twice: once against the scored agenda's own `CardId` (its
    /// own "on score" text, e.g. Hostile Takeover), then against the Corp's
    /// identity if one is set (a reactive identity ability, e.g. Jinteki:
    /// Personal Evolution).
    OnAgendaScored,
    /// Fires against the Corp's identity card when the Runner steals an
    /// agenda (`run::resolve_steal`) — e.g. Jinteki: Personal Evolution
    /// reacting to either scoring or a steal.
    OnAgendaStolen,
    /// Fires against the Corp's identity card whenever a
    /// `CardSubtype::Transaction` Operation is played — e.g. Weyland
    /// Consortium: Building a Better World.
    OnTransactionPlayed,
    /// Fires against the Runner's identity card whenever a
    /// `CardSubtype::Virus` Program is installed — e.g. Noise: Hacker
    /// Extraordinaire. Also fires against every OTHER Runner rig card
    /// declaring this trigger, but — for rig cards only, not the identity
    /// — the resulting effect acts on the just-installed virus program
    /// itself rather than the reacting card (see `ability::
    /// process_card_triggers_targeting`), e.g. Cookbook's "you may place 1
    /// virus counter on it."
    OnVirusInstalled,
    /// Fires the instant an `Effect::DealDamage` is parked in
    /// `GameState::pending_prevention` (`GameEvent::DamageAboutToResolve`),
    /// before its `WindowCheckpoint::Prevention` window opens — for a
    /// card's own *automatic* (non-`Paid`) reaction. A `Paid` prevention
    /// ability is instead activated player-side via
    /// `PlayerAction::ActivateAbility` during the window itself.
    OnDamageAboutToResolve,
    /// Mirrors `OnDamageAboutToResolve` for a parked `Effect::TrashCard`.
    OnTrashAboutToResolve,
    /// Fires against the card itself the instant it's rezzed
    /// (`GameEvent::IceRezzed` — despite the name, already fired for any
    /// Corp rez, not just ICE, per `engine::rez_ice`'s own doc comment) —
    /// e.g. Ping's "when you rez this ice during a run against this
    /// server, give the Runner 1 tag." Combine with
    /// `EffectRequirement::RezzedDuringRunAgainstThisServer` to scope it to
    /// a rez that happens mid-run against the card's own server.
    OnRez,
    /// Fires against every rezzed Corp Root-slot install in a server the
    /// Runner has just approached (`GameEvent::ServerApproached`) — the
    /// approach-server step, before the run is successful, so an ability
    /// here that ends the run (Anoetic Void, Manegarm Skunkworks) denies
    /// every "when your run is successful" trigger. Used to share
    /// `RunSucceeded`, which fired at the same moment and made every run
    /// successful before these could stop it (ROADMAP Rules Audit T9).
    OnApproachServer,
    /// Fires against the Runner's identity and every Runner rig card when
    /// a run concludes — `GameEvent::RunCompleted`'s "normal" ending only
    /// (see `dispatcher::dispatch_event`'s doc comment on this arm for why
    /// jack-out/effect-ended runs are out of scope for now) — e.g. Mayfly's
    /// "when this run ends, trash this program," Zahya Sadeghi's "when a
    /// run on HQ or R&D ends, you may gain 1 credit for each card
    /// accessed."
    OnRunEnded,
    /// Fires against the Runner's rig/resources when the Runner takes the
    /// *basic* click-to-draw action specifically (`GameEvent::
    /// BasicDrawActionTaken`) — not `Effect::DrawCards`, which never
    /// dispatches this. e.g. Verbal Plasticity's "the first time each turn
    /// you take the basic action to draw 1 card, instead draw 2 cards."
    OnBasicDrawAction,
    /// Fires against the Corp's identity whenever the Runner gains a tag
    /// (`GameEvent::TagsGiven { side: Side::Runner, .. }`) — e.g. NBN:
    /// Reality Plus's "the first time each turn the Runner takes a tag,
    /// gain 2 credits or draw 2 cards."
    OnTagsGiven,
    /// Fires against the Corp's identity whenever any installed card is
    /// advanced (`GameEvent::CardAdvanced`) — e.g. Weyland Consortium:
    /// Built to Last's "whenever you advance a card, gain 2 credits if it
    /// had no advancement counters" (combine with `EffectRequirement::
    /// WasFirstAdvancementThisCard` for the "had no counters" half).
    OnAdvance,
    /// The named side's discard phase has just ended — including when it was
    /// skipped entirely because they were already within hand size, since
    /// the phase still "ends" in rules terms. Fires against that side's
    /// identity only. e.g. Jinteki: Restoring Humanity's "when your discard
    /// phase ends, if there is a facedown card in Archives, gain 1
    /// credit." Deliberately not `OnTurnStart`: the discard phase ends
    /// before control passes, and the two are observably different moments.
    OnDiscardPhaseEnd,
    /// "When your action phase ends" — fired from `turn::end_turn`, before
    /// the end-of-turn paid-ability window, for the ending side's identity
    /// and its rig (Runner) or rezzed installs (Corp): Cacophony's
    /// sabotage; Mercia B4LL4RD and Nebula Talent Management later. A
    /// separate trigger from `OnDiscardPhaseEnd`, which is a later step.
    OnActionPhaseEnd,
}
