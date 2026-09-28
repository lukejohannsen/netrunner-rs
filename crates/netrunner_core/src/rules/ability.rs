use crate::cards::CardRegistry;
use crate::dsl::{
    card_matches_filter, Amount, EffectDuration, CardFilter, CardId, CardSubtype, CardTarget, Cost, Effect,
    EffectRequirement, HostedCardOrigin, StackZone, SubroutineBreakCount, Trigger, TriggeredEffect,
};
use crate::rules::continuous;
use crate::rules::lingering::{self, Lingering, LingeringEffect, On, Until};
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::listeners;
use crate::rules::event::GameEvent;
use crate::rules::payment::{self, Purpose};
use crate::rules::prevention;
use crate::rules::uninstall;
use crate::rules::run::{self, AccessPhase, RunPhase, ServerId, SubroutineStatus};
use crate::rules::state::{
    ArchivedCard, Clicks, Credits, DeferredTrigger, GameState, InstallId, InstallSlot, InstalledCard, InstalledRunnerCard, OncePerTurnKey, PendingChoiceResume, PendingDecision, PendingPaidChoice,
    PendingPaidChoiceResume, Side, WouldHappen,
    TraceResume, TraceState,
};

/// Everything about the resolution *currently in flight* that an effect or
/// requirement may need and cannot read from `GameState`.
///
/// **Transient by construction:** built at the top of a resolution, dropped
/// when it ends, never serialized, never crossing a `PlayerAction`
/// boundary. That is the whole point — it replaces
/// `GameState::last_discarded_cards` and `last_advancement_was_first`,
/// which outlived their resolutions and could be read stale.
///
/// Anything that must *survive* a parked decision belongs on `GameState`
/// instead. `GameState::last_completed_run` is exactly that case and
/// deliberately stays there: `Trigger::OnRunEnded` can be deferred into
/// `GameState::deferred_triggers` and fire on a later `PlayerAction`, by
/// which time any context built here is long gone.
///
/// This is the home AGENTS.md's State Hygiene Rule asks for. A new card
/// needing resolution context adds a field here rather than a scratchpad
/// field on `GameState`.
#[derive(Debug, Default, Clone)]
pub struct ResolutionContext<'a> {
    /// Which card is resolving this — absorbed from `evaluate_effect`'s
    /// former `acting_card` parameter rather than added alongside it, so
    /// arity is unchanged. See `evaluate_effect`'s doc comment for what it
    /// means per-effect.
    pub acting_card: Option<&'a CardId>,
    /// Which *install* of `acting_card` is resolving this, when it is an
    /// installed card. Every "this card" lookup — counters, advancement,
    /// self-trash, host, strength, once-per-turn — resolves through this
    /// when it is `Some`, and only falls back to the first install matching
    /// `acting_card` when it is `None` (an identity, an event, an
    /// operation, a subroutine). Before it existed every such lookup was
    /// first-match by `CardId`: two Fermenters shared one counter pool and
    /// cashing the second trashed the first, Nico Campaign #2 loaded its
    /// counters onto #1, a decoy Urtica Cipher dealt the other Urtica's
    /// damage (ROADMAP Rules Audit §4). A `Some` install that has since
    /// left play resolves to *nothing*, never to a sibling copy.
    pub acting_install: Option<InstallId>,
    /// The event whose triggers are being dispatched, when this resolution
    /// is a trigger rather than a directly activated ability. `None` for an
    /// ability the player activated, a subroutine, or a cost payment.
    ///
    /// Read by `EffectRequirement::WasFirstAdvancementThisCard`, which
    /// needs `GameEvent::CardAdvanced`'s `advancement_tokens` — the fact
    /// the deleted `last_advancement_was_first` field existed to carry.
    /// Generalizes: the next trigger needing its own event's payload reads
    /// it here.
    pub triggering_event: Option<&'a GameEvent>,
    /// Cards a `DealDamage` discarded earlier in this same `Sequence`,
    /// as returned by `damage::apply_damage`. Backs
    /// `EffectRequirement::LastDamageTrashedOddCostCard` (*Diviner*).
    pub damage_discarded: Vec<CardId>,
    /// Credits actually removed by the most recent `Effect::LoseCredits`
    /// in this same resolution — overwritten per `LoseCredits`, never
    /// accumulated, mirroring `damage_discarded`'s contract. Backs
    /// `Amount::CreditsLostThisResolution` (*Account Siphon*).
    pub credits_lost: u32,
    /// How many cards the `PromptChooseCards` this `then` belongs to
    /// selected — `Amount::RemainingAfterSelection` (a sabotage's R&D
    /// half). 0 outside a selection's `then`.
    pub selected_count: u32,
    /// The card whose printed text this resolution is a *continuation* of,
    /// when that differs from `acting_card`. Read only by the effects that
    /// park a decision (`PromptChooseCards`, `PresentChoice`,
    /// `OfferPaidChoice`, `PromptChooseServer`, a sabotage) to fill the
    /// parked decision's `source_card`; `None` means "the acting card".
    ///
    /// A `then` after a card selection resolves *as the selected card* —
    /// `resolve_confirm_card_selection` sets `acting_card` to it so Plutus
    /// replays the transaction it chose and Seamless Launch advances the
    /// Offworld Office it picked — and that is right for what the effect
    /// *does*. It is wrong for what the effect *is*: AU Co.'s "look at the
    /// top 3, trash 1, then add the top 2 to HQ" parked its second prompt as
    /// whichever card it had just trashed, so a livelock inside that prompt
    /// was reported against *Measured Response*, the card on top of R&D
    /// (ROADMAP Phase 2 §5). Touch-ups shows why the two cannot be one
    /// field: its `then` is a `Sequence` that advances the chosen install
    /// and *then* presents a choice — the first half needs the selection,
    /// the second is Touch-ups still talking. A fresh card resolution
    /// (`play_operation_card`, a fired trigger) builds a context with this
    /// `None`, so a prompt parked by a card Plutus replays is that card's.
    pub prompting_card: Option<&'a CardId>,
    /// The acting install as it was just before its cost was paid, for an
    /// effect that reads a card its own cost has taken off the table —
    /// Fermenter's "[click], [trash]: Gain 2[credit] for each hosted virus
    /// counter", Clearinghouse's "trash this asset to do 1 meat damage for
    /// each hosted advancement counter". The cost is paid first (Comprehensive
    /// Rules 1.16), so without this the effect counted nothing; with the
    /// trash written as the effect's last step instead, the trash could be
    /// prevented, which a cost cannot be (1.16.1a).
    ///
    /// Taken by the payer (`last_known`) and read only by `counters_of` and
    /// `advancement_tokens_of`, and by them only when `acting_install` is
    /// `Some` and has left play — so it is the same object's memory and
    /// never a sibling copy's. On the context rather than on `GameState`
    /// because it is read within this one resolution: both cards read it
    /// before anything they do can park, and it does not survive a park.
    pub last_known: Option<LastKnown>,
    /// The cards that were hosted on the acting install when its own cost
    /// took it off the table, set aside rather than trashed with it because
    /// the effect acts on them (CR 9.5.5: "set aside any hosted cards ...
    /// as the trigger cost is paid") — Read-Write Share's "[trash]: Shuffle
    /// all hosted cards into your stack". Filled by the payer
    /// (`engine::activate_ability`), which trashes whatever the effect
    /// leaves. On the context because the effect that reads it resolves
    /// without parking.
    pub set_aside: Vec<CardId>,
}

/// What `ResolutionContext::last_known` remembers of an install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastKnown {
    pub counters: u32,
    pub advancement_tokens: u32,
    /// The server a Corp install was in — Hype Machine's "the root of this
    /// server", after its "[trash]:" has taken it off the table.
    pub server: Option<ServerId>,
}

/// The acting install's numbers now, for a payer to put on the effect's
/// context before a cost can remove the install. `None` without an install
/// to remember.
pub(crate) fn last_known(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<LastKnown> {
    ctx.acting_install?;
    let counters = counters_of(state, ctx)?;
    Some(LastKnown {
        counters,
        advancement_tokens: advancement_tokens_of(state, ctx).unwrap_or(0),
        server: acting_corp_install(state, ctx).map(|installed| installed.server),
    })
}

/// The server `ctx`'s Corp install is in, or was in when its own cost took
/// it off the table.
fn acting_server(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<ServerId> {
    acting_corp_install(state, ctx).map(|installed| installed.server).or_else(|| remembered(state, ctx).and_then(|known| known.server))
}

/// Whether `ctx`'s install was named and is no longer on the table — the
/// one case `last_known` is read in.
fn acting_install_has_left(state: &GameState, ctx: &ResolutionContext<'_>) -> bool {
    ctx.acting_install.is_some() && acting_corp_position(state, ctx).is_none() && acting_rig_position(state, ctx).is_none()
}

impl<'a> ResolutionContext<'a> {
    /// The common case: a resolution attributed to `acting_card`, with no
    /// triggering event and nothing accumulated yet.
    pub fn for_card(acting_card: Option<&'a CardId>) -> Self {
        ResolutionContext { acting_card, ..ResolutionContext::default() }
    }

    /// A trigger's resolution: attributed to `acting_card` and carrying the
    /// event that fired it.
    pub fn for_trigger(acting_card: Option<&'a CardId>, triggering_event: Option<&'a GameEvent>) -> Self {
        ResolutionContext { acting_card, triggering_event, ..ResolutionContext::default() }
    }

    /// A resolution attributed to one specific install of `acting_card` —
    /// an activated ability, or a trigger on an installed card.
    pub fn for_install(acting_install: InstallId, acting_card: &'a CardId) -> Self {
        ResolutionContext { acting_card: Some(acting_card), acting_install: Some(acting_install), ..ResolutionContext::default() }
    }

    /// A trigger's resolution on an installed card: `for_trigger` plus the
    /// install. `acting_install` is `None` for a source with no install.
    pub fn for_install_trigger(
        acting_install: Option<InstallId>,
        acting_card: Option<&'a CardId>,
        triggering_event: Option<&'a GameEvent>,
    ) -> Self {
        ResolutionContext { acting_card, acting_install, triggering_event, ..ResolutionContext::default() }
    }

    /// Rebuilds the context a parked resolution had when it parked —
    /// `PendingPaidChoice::source_install` and friends carry the install
    /// across the `PlayerAction` boundary for exactly this.
    pub fn for_parked(acting_install: Option<InstallId>, acting_card: Option<&'a CardId>) -> Self {
        ResolutionContext { acting_card, acting_install, ..ResolutionContext::default() }
    }

    /// The card a decision parked by this resolution is *attributed* to:
    /// the continuation's owner when there is one, else the acting card.
    /// Fills `prompting_card` on whatever gets parked; never `source_card`,
    /// which stays the acting card because resume reads it as such. See
    /// `prompting_card`.
    pub fn attributed_card(&self) -> Option<CardId> {
        self.prompting_card.or(self.acting_card).cloned()
    }
}

/// The Corp install `ctx` is acting as: by `acting_install` when it has one
/// (and `None` if that install has left play — never a sibling copy), else
/// the first install of `acting_card`. The four `acting_*` helpers below
/// are the only way effect resolution looks up "this card"; see
/// `ResolutionContext::acting_install` for why.
fn acting_corp_install<'s>(state: &'s GameState, ctx: &ResolutionContext<'_>) -> Option<&'s InstalledCard> {
    acting_corp_position(state, ctx).map(|position| &state.corp.installed[position])
}

fn acting_corp_install_mut<'s>(state: &'s mut GameState, ctx: &ResolutionContext<'_>) -> Option<&'s mut InstalledCard> {
    acting_corp_position(state, ctx).map(|position| &mut state.corp.installed[position])
}

/// The scored agenda `ctx` is acting as — by `acting_install` only, since
/// a score area can hold two copies of one agenda and only the install
/// handle tells them apart (Off the Books spending its own counters).
fn acting_scored_position(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<usize> {
    let install = ctx.acting_install?;
    state.corp.scored_agendas.iter().position(|scored| scored.install_id == install)
}

/// Whether `ctx` is resolving as the Corp's identity — which has no
/// install, so every "this card" lookup that walks the table misses it.
/// The counter helpers fall through to `CorpState::identity_counters`
/// on this (AU Co.).
fn acting_is_corp_identity(state: &GameState, ctx: &ResolutionContext<'_>) -> bool {
    ctx.acting_install.is_none() && ctx.acting_card.is_some() && ctx.acting_card == state.corp.identity.as_ref()
}

fn acting_corp_position(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<usize> {
    match ctx.acting_install {
        Some(install) => state.corp.installed.iter().position(|c| c.install_id == install),
        None => ctx.acting_card.and_then(|card| state.corp.installed.iter().position(|c| &c.card == card)),
    }
}

/// The rig card `ctx` is acting as — the Runner-side twin of
/// [`acting_corp_install`].
fn acting_rig_card<'s>(state: &'s GameState, ctx: &ResolutionContext<'_>) -> Option<&'s InstalledRunnerCard> {
    acting_rig_position(state, ctx).map(|position| &state.runner.rig[position])
}

fn acting_rig_position(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<usize> {
    match ctx.acting_install {
        Some(install) => state.runner.rig.iter().position(|c| c.install_id == install),
        None => ctx.acting_card.and_then(|card| state.runner.rig.iter().position(|c| &c.card == card)),
    }
}

/// Applies a single, already-resolved `Effect` to `state` in place.
///
/// A deliberate new hybrid mutation convention: mutate-in-place like
/// `damage::apply_damage`/`run::access_server` (the caller has already
/// cloned/validated phase, so this never needs to reclone), but fallible
/// unlike them — some `Effect` arms genuinely can fail against a
/// well-formed state (`TrashCard` naming a target that isn't where it's
/// claimed to be) while others structurally cannot.
///
/// `acting_card` identifies which card is resolving this effect: for
/// `BoostStrength`/`BreakSubroutines` it's specifically "whichever Runner
/// rig card activated the ability" (`RulesError::UnresolvedCardTarget` if
/// `None`); for `TrashCard(CardTarget::ThisCard)` it's simply "the card this
/// effect is printed on" (Corp or Runner alike), resolved via
/// `trash_this_card` — `RulesError::MissingActingCardContext` if `None`.
/// Callers that aren't resolving a specific card's own ability/trigger
/// (subroutine resolution) pass `None`.
pub fn evaluate_effect(
    state: &mut GameState,
    effect: &Effect,
    ctx: &mut ResolutionContext<'_>,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let acting_card = ctx.acting_card;
    match effect {
        Effect::GainCredits(side, amount) => gain_credits_from_ability(state, registry, *side, *amount, ctx),

        // Damage, a tag and the trash of an installed card are each about to
        // happen before they happen: `rules::prevention` is the one door.
        Effect::DealDamage(damage_type, amount) => {
            prevention::would(state, registry, WouldHappen::Damage { kind: *damage_type, amount: *amount as u32 }, ctx)
        }

        Effect::ModifyStrength { delta, ice: which, duration } => {
            let source = |fallback: &CardId| acting_card.cloned().unwrap_or_else(|| fallback.clone());
            // "The rezzed ice gets +3 strength for the remainder of that run"
            // (Brasília Government Grid): the Corp install resolving, which
            // need not be encountered — a rez during a run comes as the ice
            // is approached.
            if *which == crate::dsl::StrengthOf::This {
                let install = ctx.acting_install.filter(|install| state.corp.installed.iter().any(|c| c.install_id == *install && c.slot == crate::rules::state::InstallSlot::Ice));
                let (Some(install), Some(card_id)) = (install, acting_card.cloned()) else { return Err(RulesError::UnresolvedCardTarget) };
                let until = lingering::until(state, *duration)?;
                state.lingering.push(LingeringEffect { what: Lingering::Strength(*delta), on: On::Install(install), until, source: ctx.attributed_card().unwrap_or_else(|| card_id.clone()) });
                return Ok(Vec::new());
            }
            // "Each piece of ice … for the remainder of this run" (ezaM):
            // every piece of ice, the ones installed later too, which is
            // `On::EachIce` asked at every read — not one entry per ice.
            if *which == crate::dsl::StrengthOf::EachIce {
                let until = lingering::until(state, *duration)?;
                let encountered = state.active_run.as_ref().filter(|run| run.phase == RunPhase::EncounterIce).and_then(|run| run.ice.get(run.position)).cloned();
                let fallback = encountered.as_ref().map_or_else(|| CardId(String::new()), |ice| ice.card_id.clone());
                state.lingering.push(LingeringEffect { what: Lingering::Strength(*delta), on: On::EachIce, until, source: source(&fallback) });
                return Ok(encountered
                    .map(|ice| GameEvent::IceStrengthModified { new_strength: continuous::ice_strength(state, registry, &ice), card_id: ice.card_id, delta: *delta })
                    .into_iter()
                    .collect());
            }
            let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
            if run.phase != RunPhase::EncounterIce {
                return Err(RulesError::NotInEncounter);
            }
            let position = run.position;
            // `NotInEncounter` doubles as the defensive fallback here if
            // `position` were ever out of bounds — an invariant violation
            // that shouldn't happen while `phase == EncounterIce`, but
            // `.get` avoids a raw-index panic regardless.
            let ice = run.ice.get(position).ok_or(RulesError::NotInEncounter)?;
            let card_id = ice.card_id.clone();
            // "For the remainder of this encounter" (Leech). This wrote the
            // delta into `RunIce::current_strength`, where nothing took it
            // back out, so it lasted the run.
            let until = lingering::until(state, *duration)?;
            let (on, source) = (ice.install_id, source(&card_id));
            state.lingering.push(LingeringEffect { what: Lingering::Strength(*delta), on: On::Install(on), until, source });
            let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
            let new_strength = continuous::ice_strength(state, registry, &run.ice[position]);
            Ok(vec![GameEvent::IceStrengthModified { card_id, new_strength, delta: *delta }])
        }

        Effect::DrawCards(side, amount) => {
            // Mirrors engine::draw_card_click's existing per-card pattern,
            // generalized to `amount` and either side's deck. An empty
            // stack is a silent stop for the Runner, who never decks out.
            // The Corp loses: "The Runner wins if the Corp is required to
            // draw a card from R&D but cannot because R&D is empty" (CR
            // 1.7.2c). That is a failed attempt, not a standing condition,
            // so it is here rather than in `checkpoint`; it was a silent
            // stop for both, and Sprint or Spin Doctor on an empty R&D
            // drew nothing and played on.
            let mut events = Vec::new();
            for _ in 0..*amount {
                let drawn = match side {
                    Side::Corp => state.corp.r_and_d.pop(),
                    Side::Runner => state.runner.stack.pop(),
                };
                match drawn {
                    Some(card) => {
                        match side {
                            Side::Corp => state.corp.hq.push(card),
                            Side::Runner => state.runner.grip.push(card),
                        }
                        events.push(GameEvent::CardDrawn { side: *side });
                    }
                    None => {
                        if *side == Side::Corp {
                            events.extend(crate::rules::win::end_game(state, Side::Runner));
                        }
                        break;
                    }
                }
            }
            Ok(events)
        }

        Effect::EndTheRun => {
            // Ending a run that has already ended is a no-op, not an
            // error: a card's own text can reach here twice (Biawak's
            // second subroutine after its first ended the run), and an
            // `Err` there fails the whole action that got here — which,
            // when that action is the confirmation of a parked selection,
            // leaves a decision nothing can resolve. The view-path sweep
            // found exactly that at seed 19 as 10,000 fruitless toggles.
            if state.active_run.is_none() {
                return Ok(Vec::new());
            }
            // A standing prevention is asked first (Shred).
            if let Some(events) = prevention::run_ending(state, registry, ctx)? {
                return Ok(events);
            }
            // The encounter ends with the run (CR 6.1.4), and is heard
            // first: Knowledge Seeker's "whenever an encounter with this
            // ice ends" after its own "End the run".
            let encounter = state.active_run.as_ref().and_then(run::encounter_ends);
            let run = run::end_run(state).expect("checked Some above");
            let server = run.server;
            let mut events = Vec::new();
            for event in encounter.into_iter().chain([GameEvent::RunEndedByEffect { server }]) {
                dispatcher::emit(state, registry, &mut events, event)?;
            }
            Ok(events)
        }

        Effect::GiveTags(amount) => {
            let amount = resolve_amount(amount, ctx, state, registry);
            prevention::would(state, registry, WouldHappen::Tags { amount }, ctx)
        }

        Effect::RemoveTags(amount) => {
            // The event reports what actually came off, not what was asked
            // for: a trigger keyed on a tag being removed (Synapse Global)
            // must not fire on a request that removed nothing.
            let removed = state.runner.tags.min(resolve_amount(amount, ctx, state, registry));
            state.runner.tags -= removed;
            let by = acting_side(acting_card, registry);
            let event = GameEvent::TagsRemoved { side: Side::Runner, amount: removed, by };
            // Dispatched here rather than from the caller, for the same
            // reason `DamageTaken` is: the event is produced deep in an
            // effect and returned, and Synapse Global: Faster than Thought
            // reacts to it.
            let mut events = vec![event.clone()];
            events.extend(dispatcher::dispatch_event(state, registry, &event)?);
            Ok(events)
        }

        Effect::GiveBadPublicity(amount) => {
            let amount = resolve_amount(amount, ctx, state, registry);
            // Taking none is not taking bad publicity: Editorial Division
            // must not hear a Luana Campos that hosted nothing.
            if amount == 0 {
                return Ok(Vec::new());
            }
            state.corp.bad_publicity = state.corp.bad_publicity.saturating_add(amount);
            // Dispatched here, as `RemoveTags` dispatches its removal:
            // Editorial Division hears the Corp take it.
            let mut events = Vec::new();
            dispatcher::emit(state, registry, &mut events, GameEvent::BadPublicityGiven { amount })?;
            Ok(events)
        }

        Effect::RemoveBadPublicity(amount) => {
            state.corp.bad_publicity = state.corp.bad_publicity.saturating_sub(*amount);
            Ok(vec![GameEvent::BadPublicityRemoved { amount: *amount }])
        }

        Effect::RemoveFromGame(CardTarget::ThisCard) => remove_this_card_from_game(state, registry, ctx),
        // Ashen Epilogue's "remove the top 5 cards of your stack from the
        // game", one card at a time as `MillRnDAmount` trashes; an empty
        // deck has nothing to remove.
        Effect::RemoveFromGame(CardTarget::TopOfStack { side, zone }) => {
            let (deck, removed) = match (side, zone) {
                (Side::Runner, StackZone::Stack) => (&mut state.runner.stack, &mut state.runner.removed_from_game),
                (Side::Corp, StackZone::RAndD) => (&mut state.corp.r_and_d, &mut state.corp.removed_from_game),
                _ => return Err(RulesError::UnresolvedCardTarget),
            };
            let Some(card) = deck.pop() else { return Ok(Vec::new()) };
            removed.push(card.clone());
            Ok(vec![GameEvent::CardRemovedFromGame { side: *side, card }])
        }
        Effect::RemoveFromGame(_) => Err(RulesError::UnresolvedCardTarget),

        Effect::TrashCard(target) => {
            // Hosted, uninstalled cards (Bling's) have no prevention
            // window of their own and no single owner: each goes to its
            // own side's discard pile.
            if matches!(target, CardTarget::HostedOnThisCard) {
                let mut events = trash_hosted_cards(state, registry, ctx)?;
                events.extend(dispatch_trashes(state, registry, &events)?);
                return Ok(events);
            }
            // Only an installed card's trash can be prevented, and only
            // then is it resolved to a handle first: with nobody to ask,
            // the target is trashed the way it always was.
            match installed_target(state, registry, target, ctx) {
                Some(what) if prevention::could_prevent(state, registry, &what) => prevention::would(state, registry, what, ctx),
                _ => {
                    let mut events = trash_card(state, registry, target, ctx)?;
                    events.extend(dispatch_trashes(state, registry, &events)?);
                    Ok(events)
                }
            }
        }

        Effect::Prevent(word) => prevention::prevent(state, registry, word),

        Effect::AddCounters(amount) => modify_counters(state, ctx, i64::from(*amount)),

        Effect::RemoveCounters(amount) => {
            let amount = resolve_amount(amount, ctx, state, registry);
            modify_counters(state, ctx, -i64::from(amount))
        }

        Effect::TakeAllCountersAsCredits(side) => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let current = counters_of(state, ctx).ok_or_else(|| RulesError::CardNotEligibleForCounters(card_id.clone()))?;
            let mut events = modify_counters(state, ctx, -i64::from(current))?;
            events.extend(gain_credits_from_ability(state, registry, *side, current, ctx)?);
            Ok(events)
        }


        Effect::TrashCurrentlyAccessedCard => run::trash_currently_accessed_card_without_cost(state, registry),

        Effect::DerezCard(target) => {
            let (install, card_id, _server) = resolve_corp_installed_target(state, target, ctx)?;
            let installed = state
                .corp
                .installed
                .iter_mut()
                .find(|c| c.install_id == install)
                .ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            // A faceup agenda is neither rezzed nor unrezzed (CR 8.1.1).
            if installed.rezzed && !installed.is_rezzed(registry) {
                return Ok(Vec::new());
            }
            installed.rezzed = false;
            Ok(vec![GameEvent::CardDerezzed { install, card: Some(card_id) }])
        }

        Effect::GainCreditsPerCounter { side, credits_per_counter } => {
            acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let current = counters_of(state, ctx).unwrap_or(0);
            let amount = current.saturating_mul(*credits_per_counter);
            gain_credits_from_ability(state, registry, *side, amount, ctx)
        }

        Effect::SwapInstalledIce(a, b) => {
            // Legal mid-run: `run::reconcile_ice` rebuilds the run's ICE
            // list from `corp.installed` at the next step and keeps
            // `position` on the same install. This used to be refused
            // outright (`CannotSwapIceDuringActiveRun`) because the list
            // was a snapshot that could not follow a swap.
            for id in [a, b] {
                if state.find_corp_install(*id).is_none() {
                    return Err(RulesError::InstallNotFound(*id));
                }
            }
            let pos_a =
                state.corp.installed.iter().position(|c| c.install_id == *a).expect("checked above");
            let pos_b =
                state.corp.installed.iter().position(|c| c.install_id == *b).expect("checked above");
            let (card_a, card_b) =
                (state.corp.installed[pos_a].card.clone(), state.corp.installed[pos_b].card.clone());
            let (server_a, slot_a) = (state.corp.installed[pos_a].server, state.corp.installed[pos_a].slot);
            let (server_b, slot_b) = (state.corp.installed[pos_b].server, state.corp.installed[pos_b].slot);
            state.corp.installed[pos_a].server = server_b;
            state.corp.installed[pos_a].slot = slot_b;
            state.corp.installed[pos_b].server = server_a;
            state.corp.installed[pos_b].slot = slot_a;
            // Both handles and both identities: `Trigger` dispatch is
            // keyed by `CardId`, and the handles are what let
            // `masking::mask_event_for_player` strike an identity the
            // Runner may not learn while keeping the fact that these two
            // installs traded places — which is on the table for anyone to
            // see, and which no `PlayerAction` names.
            Ok(vec![GameEvent::IceSwapped { a: *a, b: *b, a_card: Some(card_a), b_card: Some(card_b) }])
        }

        Effect::InstallFromZoneIgnoringCost { card_id, origin_zone, into, slot, insert_after } => {
            // Archives is a `Vec<ArchivedCard>` while HQ is a plain
            // `Vec<CardId>`, so the removal is done per-zone rather than
            // through one shared `&mut Vec<CardId>` handle. Orientation is
            // irrelevant here — the card is leaving Archives entirely.
            let removed = match origin_zone {
                crate::dsl::CardZoneRef::OwnHq => {
                    state.corp.hq.iter().position(|c| c == card_id).map(|pos| {
                        state.corp.hq.remove(pos);
                    })
                }
                crate::dsl::CardZoneRef::OwnArchives => {
                    state.corp.archives.iter().position(|c| &c.card == card_id).map(|pos| {
                        state.corp.archives.remove(pos);
                    })
                }
                _ => return Err(RulesError::UnresolvedCardTarget),
            };
            removed.ok_or_else(|| RulesError::CardNotInHand { side: Side::Corp, card: card_id.clone() })?;

            let card_def =
                registry.get(card_id).ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
            let resolved_slot = slot.unwrap_or(match card_def.card_type {
                crate::dsl::CardType::Ice(_) => crate::rules::InstallSlot::Ice,
                _ => crate::rules::InstallSlot::Root,
            });
            // An authored `slot: Ice` with a filter that admits a non-ICE
            // would otherwise put an agenda in the ICE column; same guard
            // `engine::install_card` applies to the player's own installs.
            if resolved_slot == crate::rules::InstallSlot::Ice && !matches!(card_def.card_type, crate::dsl::CardType::Ice(_)) {
                return Err(RulesError::CardTypeMismatch { card: card_id.clone(), expected: "ice" });
            }
            let install_id = state.allocate_install_id();
            let new_card = crate::rules::InstalledCard {
                card: card_id.clone(),
                install_id,
                server: *into,
                slot: resolved_slot,
                // "Install only faceup" (Sacrifice Zone Expansion) holds
                // for an install by a card's text too.
                rezzed: card_def.installs_faceup,
                advancement_tokens: 0,
                counters: 0,
                installed_this_turn: true,
                seen_by_runner: false,
                this_turn: Default::default(),
            };
            match insert_after {
                Some(host) => {
                    let host_pos = state.corp.installed.iter().position(|c| c.install_id == *host);
                    match host_pos {
                        Some(i) => state.corp.installed.insert(i + 1, new_card),
                        None => state.corp.installed.push(new_card),
                    }
                }
                None => state.corp.installed.push(new_card),
            }
            // Emitted, not returned bare: an install by a card's text is an
            // install, and "the first time each turn you install a card"
            // (Haas-Bioroid: Engineering the Future) hears it. It did not —
            // Brân 1.0's install went unheard and the identity then paid for
            // the turn's *second* install as its first — until
            // `dispatcher::audit` refused the undispatched event.
            let mut events = Vec::new();
            dispatcher::emit(
                state,
                registry,
                &mut events,
                GameEvent::CardInstalled {
                    side: Side::Corp,
                    install: install_id,
                    card: Some(card_id.clone()),
                    server: *into,
                    from_hq: matches!(origin_zone, crate::dsl::CardZoneRef::OwnHq),
                },
            )?;
            Ok(events)
        }

        Effect::DrawCardsAmount(side, amount) => {
            let resolved = resolve_amount(amount, ctx, state, registry);
            evaluate_effect(state, &Effect::DrawCards(*side, resolved), ctx, registry)
        }

        Effect::InstallRunnerCardFromZone { from, discount } => {
            use crate::rules::engine::{can_install_runner_card_from_zone_with_discount, install_runner_card_from_zone_with_discount, RunnerCardSource};
            let source = match from {
                crate::dsl::CardZoneRef::OwnHeap => RunnerCardSource::Heap,
                crate::dsl::CardZoneRef::OwnSetAside => RunnerCardSource::SetAside,
                _ => return Err(RulesError::UnresolvedCardTarget),
            };
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            // Same leniency as the grip variant: an uninstallable pick stays
            // where it is.
            if !can_install_runner_card_from_zone_with_discount(state, registry, &card_id, source, discount.credits()) {
                return Ok(Vec::new());
            }
            install_runner_card_from_zone_with_discount(state, registry, card_id, source, discount.credits())
        }

        // Read down the stack from its top (the end of the `Vec`) until
        // enough match; each card goes faceup into the set-aside zone.
        Effect::SetAsideFromTopUntil { filter, count } => {
            let mut matched = 0;
            let mut cards = Vec::new();
            while matched < *count {
                let Some(card) = state.runner.stack.pop() else { break };
                if registry.get(&card).is_some_and(|definition| crate::dsl::card_matches_filter(definition, filter)) {
                    matched += 1;
                }
                state.runner.set_aside.push(card.clone());
                cards.push(card);
            }
            if cards.is_empty() {
                return Ok(Vec::new());
            }
            Ok(vec![GameEvent::CardsSetAside { side: Side::Runner, cards }])
        }

        Effect::InstallRunnerCardFromGripWithDiscount(discount) => {
            use crate::rules::engine::{can_install_runner_card_from_zone_with_discount, install_runner_card_from_zone_with_discount, RunnerCardSource};
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            if !can_install_runner_card_from_zone_with_discount(state, registry, &card_id, RunnerCardSource::Grip, discount.credits()) {
                return Ok(Vec::new());
            }
            let events = install_runner_card_from_zone_with_discount(state, registry, card_id.clone(), RunnerCardSource::Grip, discount.credits())?;
            // The card resolving is installed now, and the rest of this
            // resolution means that install: Beta Build's "when that run
            // ends, if that program has not been uninstalled" reads the
            // handle through the run it starts (`PendingDecision::
            // ChooseServer::source_install`, `Effect::AddToDeck`).
            ctx.acting_install = state.runner.rig.iter().rev().find(|c| c.card == card_id).map(|c| c.install_id).or(ctx.acting_install);
            Ok(events)
        }

        Effect::InstallRunnerCardFromHost => {
            use crate::rules::engine::{can_install_runner_card_from_zone, install_runner_card_from_zone_paying_cost, RunnerCardSource};
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            let host = ctx.acting_install.ok_or(RulesError::MissingActingCardContext)?;
            if !can_install_runner_card_from_zone(state, registry, &card_id, RunnerCardSource::Hosted(host)) {
                return Ok(Vec::new());
            }
            install_runner_card_from_zone_paying_cost(state, registry, card_id, RunnerCardSource::Hosted(host))
        }

        Effect::RedirectRunOnApproach(target) => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            run.redirect_on_approach = Some(*target);
            Ok(Vec::new())
        }

        Effect::SetRunEndedEffect(effect) => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            run.on_end_effect = Some(effect.clone());
            run.on_end_card = acting_card.cloned();
            run.on_end_install = ctx.acting_install;
            Ok(Vec::new())
        }

        Effect::WhenThisTurnEnds(effect) => {
            let card = acting_card.ok_or(RulesError::MissingActingCardContext)?.clone();
            let effect = match state.active_run.as_ref() {
                Some(run) => (**effect).clone().with_attacked_server(run.server),
                None => (**effect).clone(),
            };
            state.delayed.push(crate::rules::lingering::DelayedAbility {
                when: Trigger::OnDiscardPhaseEnd,
                turn: state.turn,
                effect,
                card,
                install: ctx.acting_install,
            });
            Ok(Vec::new())
        }

        Effect::Breach(server) => crate::rules::run::start_breach(state, registry, *server),

        Effect::EndActionPhase => {
            let side = carried_out_by(registry, ctx).ok_or(RulesError::MissingActingCardContext)?;
            crate::rules::turn::force_action_phase_end(state, side, registry)
        }

        // Drawn together and first, so the `each` that moves one cannot be
        // dealt it again; revealed, so each is public (CR 1.21.3) and stays
        // so until it moves (`GameState::revealed`).
        Effect::RevealAtRandom { side, count, each } => {
            let mut hand: Vec<CardId> = match side {
                Side::Corp => state.corp.hq.clone(),
                Side::Runner => state.runner.grip.clone(),
            };
            let mut drawn = Vec::new();
            while drawn.len() < *count as usize && !hand.is_empty() {
                let index = (state.next_u64() % hand.len() as u64) as usize;
                drawn.push(hand.remove(index));
            }
            let mut events = Vec::new();
            for card in &drawn {
                state.revealed.push(crate::rules::state::RevealedCard { side: *side, card: card.clone() });
                events.push(GameEvent::CardRevealed { side: *side, card: card.clone() });
            }
            if let Some(each) = each {
                for card in &drawn {
                    let mut revealed = ResolutionContext::for_card(Some(card));
                    revealed.prompting_card = ctx.prompting_card.or(acting_card);
                    events.extend(evaluate_effect(state, each, &mut revealed, registry)?);
                }
            }
            Ok(events)
        }

        Effect::ArmRunEndPrevention(prevention) => {
            state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
            let source = acting_card.ok_or(RulesError::MissingActingCardContext)?.clone();
            state.lingering.push(LingeringEffect {
                what: Lingering::PreventRunEnding(*prevention),
                on: On::Player(Side::Corp),
                until: Until::EndOfRun,
                source,
            });
            Ok(Vec::new())
        }

        Effect::Sabotage(count) => {
            let hq = state.corp.hq.len() as u32;
            let rd = state.corp.r_and_d.len() as u32;
            let from_hq_max = (*count).min(hq);
            let from_hq_min = count.saturating_sub(rd).min(from_hq_max);
            if from_hq_max == 0 {
                // Nothing to choose: it all comes off the top of R&D.
                return evaluate_effect(state, &Effect::MillRnDAmount(Amount::Fixed(*count)), ctx, registry);
            }
            state.pending_decision = Some(PendingDecision::ChooseCards {
                side: Side::Corp,
                source: crate::dsl::CardZoneRef::OwnHq,
                filter: CardFilter::Any,
                min: from_hq_min,
                max: from_hq_max,
                reveal: false,
                shuffle_after: false,
                destination: Some(crate::dsl::CardZoneRef::OwnArchives),
                then: Some(Box::new(Effect::MillRnDAmount(Amount::RemainingAfterSelection(*count)))),
                selected: Vec::new(),
                source_card: acting_card.cloned(),
                prompting_card: ctx.attributed_card(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingCardSelectionOffered { side: Side::Corp, min: from_hq_min, max: from_hq_max, source: ctx.attributed_card() }])
        }

        Effect::MillRnDAmount(amount) => {
            let count = resolve_amount(amount, ctx, state, registry);
            let mut events = Vec::new();
            let mut trashed = 0;
            for _ in 0..count {
                if state.corp.r_and_d.is_empty() {
                    break;
                }
                let milled = trash_card(state, registry, &CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::RAndD }, ctx)?;
                events.extend(dispatch_trashes(state, registry, &milled)?);
                events.extend(milled);
                trashed += 1;
            }
            // The batch, after its cards, as HQ's is (Nuvem SA's "the first
            // time you trash a card from R&D").
            if trashed > 0 {
                let batch = GameEvent::CardsTrashedFromRnD { count: trashed, by: carried_out_by(registry, ctx) };
                dispatcher::emit(state, registry, &mut events, batch)?;
            }
            Ok(events)
        }

        Effect::HostCardOnThisCard(origin) => {
            let acting = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            let card = match origin {
                HostedCardOrigin::RandomFromHq => {
                    if state.corp.hq.is_empty() {
                        return Ok(Vec::new());
                    }
                    let index = (state.next_u64() % state.corp.hq.len() as u64) as usize;
                    state.corp.hq.remove(index)
                }
                HostedCardOrigin::TopOfStack => match state.runner.stack.pop() {
                    Some(card) => card,
                    None => return Ok(Vec::new()),
                },
                HostedCardOrigin::AccessedCard => {
                    let position = acting_rig_position(state, ctx)
                        .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: acting.clone() })?;
                    let host = state.runner.rig[position].install_id;
                    return run::host_currently_accessed_card(state, registry, host);
                }
            };
            let position = acting_rig_position(state, ctx)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: acting.clone() })?;
            state.runner.rig[position].hosted_cards.push(card.clone());
            Ok(vec![GameEvent::CardHosted { card, host: acting }])
        }

        // A moment Lethe hears, so it goes through the one door.
        Effect::BypassEncounteredIce => {
            let mut events = Vec::new();
            for event in run::bypass_encountered_ice(state)? {
                dispatcher::emit(state, registry, &mut events, event)?;
            }
            Ok(events)
        }

        Effect::FlipIdentity => {
            // Whichever identity is resolving. Only an identity carries a
            // flip side, so the acting card names the side directly; the
            // Runner is the fallback because Dewi Subrotoputri was the
            // only flip identity before Nebula Talent Management.
            let side = if acting_card.is_some() && acting_card == state.corp.identity.as_ref() { Side::Corp } else { Side::Runner };
            match side {
                Side::Corp => state.corp.identity_flipped = !state.corp.identity_flipped,
                Side::Runner => state.runner.identity_flipped = !state.runner.identity_flipped,
            }
            // A moment since Méliès U: "when you flip this identity to this
            // side" (`Trigger::OnIdentityFlipped`).
            let mut events = Vec::new();
            dispatcher::emit(state, registry, &mut events, GameEvent::IdentityFlipped { side })?;
            Ok(events)
        }

        Effect::SetIdentityCopy(copy) => {
            // `validate` holds this to a Corp identity's secret number; the
            // number is at most the copies the card prints, so it fits.
            let copy = resolve_amount(copy, ctx, state, registry);
            state.corp.identity_copy = u8::try_from(copy).unwrap_or(u8::MAX);
            Ok(Vec::new())
        }

        // The operation was filed in Archives, faceup, before its text
        // resolved (`engine::play_operation_card`); the newest faceup copy
        // is this one.
        Effect::AddToScoreAreaAsAgenda(as_agenda) => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?.clone();
            // A Runner card, out of the rig into the Runner's score area.
            // Leaving the rig is leaving play, so what it hosts is trashed
            // with it, as the cost of the same name does.
            if let Some(position) = acting_rig_position(state, ctx) {
                let removed = state.runner.rig.remove(position);
                let mut events = cascade_trash_hosted_on_rig_card(state, registry, &removed);
                events.push(add_to_score_area_as_agenda(state, Side::Runner, removed.card, *as_agenda));
                return Ok(events);
            }
            let Some(position) = state.corp.archives.iter().rposition(|archived| archived.card == card_id && !archived.facedown) else {
                return Ok(Vec::new());
            };
            state.corp.archives.remove(position);
            Ok(vec![add_to_score_area_as_agenda(state, Side::Corp, card_id, *as_agenda)])
        }

        // "It" is the encountered ice, as the trigger's subject; a
        // resolution that finds the encounter over (a run ended by a
        // trigger ahead of it) has nothing left to add to.
        Effect::GainSubroutine { subroutine, after, duration } => {
            let Some(install) = ctx.acting_install else { return Err(RulesError::MissingActingCardContext) };
            let Some(run) = state.active_run.as_mut() else { return Ok(Vec::new()) };
            // For the rest of the run: kept on the run for every encounter
            // with this ice to come (`run::engine::add_gained_for_the_run`).
            if *duration == crate::dsl::EffectDuration::Run {
                run.gained_for_the_run.push(run::GainedForTheRun { ice: install, subroutine: (**subroutine).clone(), after: *after });
            }
            // And for the encounter in progress, if it is this ice's. "It" is
            // the encountered ice, as the trigger's subject; a resolution that
            // finds the encounter over (a run ended by a trigger ahead of it)
            // has nothing left to add to.
            if run.phase != RunPhase::EncounterIce {
                return Ok(Vec::new());
            }
            let position = run.position;
            let Some(ice) = run.ice.get_mut(position).filter(|ice| ice.install_id == install) else { return Ok(Vec::new()) };
            let gained = run::EncounteredSubroutine { id: 0, definition: (**subroutine).clone(), status: SubroutineStatus::Pending, gained: true };
            if *after {
                ice.subroutines.push(gained);
            } else {
                ice.subroutines.insert(0, gained);
            }
            run::renumber_subroutines(ice);
            Ok(vec![GameEvent::SubroutineGained { card_id: ice.card_id.clone(), text: subroutine.text.clone() }])
        }

        Effect::WinTheGame => {
            let card = acting_card.cloned().ok_or(RulesError::MissingActingCardContext)?;
            let winner = registry.get(&card).map(|definition| definition.side).ok_or_else(|| RulesError::CardNotFoundInRegistry(card.clone()))?;
            let mut events = vec![GameEvent::WonByCardText { winner, card }];
            events.extend(crate::rules::win::end_game(state, winner));
            Ok(events)
        }

        Effect::GainIceSubtype(subtype) => {
            let Some(install) = ctx.acting_install else { return Err(RulesError::MissingActingCardContext) };
            let rezzed_ice = state.corp.installed.iter().any(|card| card.install_id == install && card.rezzed && card.slot == InstallSlot::Ice);
            if !rezzed_ice {
                return Ok(Vec::new());
            }
            let source = acting_card.cloned().ok_or(RulesError::MissingActingCardContext)?;
            state.lingering.push(LingeringEffect { what: Lingering::GainSubtype(*subtype), on: On::Install(install), until: lingering::Until::WhileRezzed(install), source });
            Ok(Vec::new())
        }

        Effect::InstallProgramOnHost { card, from } => {
            use crate::rules::engine::{can_install_program_onto, install_program_onto, ProgramHost, RunnerCardSource};
            let source = match from {
                crate::dsl::CardZoneRef::OwnGrip => RunnerCardSource::Grip,
                crate::dsl::CardZoneRef::OwnHeap => RunnerCardSource::Heap,
                crate::dsl::CardZoneRef::OwnStack => RunnerCardSource::Stack,
                _ => return Err(RulesError::UnresolvedCardTarget),
            };
            let host = ctx.acting_install.ok_or(RulesError::UnresolvedCardTarget)?;
            match card {
                // The ice was chosen: this resolves as it.
                Some(program) => {
                    if !can_install_program_onto(state, registry, program, source, true) || state.find_corp_install(host).is_none() {
                        return Ok(Vec::new());
                    }
                    install_program_onto(state, registry, program.clone(), source, ProgramHost::Ice(host))
                }
                None => {
                    let program = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
                    let trojan = registry.get(&program).is_some_and(|def| def.installs_on_ice);
                    if !can_install_program_onto(state, registry, &program, source, trojan) {
                        return Ok(Vec::new());
                    }
                    if !trojan {
                        return install_program_onto(state, registry, program, source, ProgramHost::RigCard(host));
                    }
                    let choose_ice = Effect::PromptChooseCards {
                        side: Side::Runner,
                        source: crate::dsl::CardZoneRef::OpponentInstalled,
                        filter: CardFilter::Ice,
                        min: 1,
                        max: 1,
                        reveal: false,
                        shuffle_after: false,
                        destination: None,
                        then: Some(Box::new(Effect::InstallProgramOnHost { card: Some(program), from: from.clone() })),
                    };
                    evaluate_effect(state, &choose_ice, ctx, registry)
                }
            }
        }

        Effect::PlaceRunCredits(amount) => {
            let credits = resolve_amount(amount, ctx, state, registry);
            state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?.bonus_run_credits += credits;
            Ok(Vec::new())
        }

        Effect::ShuffleIntoDeck(zones) => {
            let mut cards = Vec::new();
            for zone in zones {
                match zone {
                    crate::dsl::CardZoneRef::HostedOnSource if ctx.set_aside.is_empty() => {
                        if let Some(position) = acting_rig_position(state, ctx) {
                            cards.extend(std::mem::take(&mut state.runner.rig[position].hosted_cards));
                        }
                    }
                    crate::dsl::CardZoneRef::HostedOnSource => cards.extend(std::mem::take(&mut ctx.set_aside)),
                    crate::dsl::CardZoneRef::OwnGrip => cards.append(&mut state.runner.grip),
                    crate::dsl::CardZoneRef::OwnHeap => cards.append(&mut state.runner.heap),
                    crate::dsl::CardZoneRef::OwnSetAside => cards.append(&mut state.runner.set_aside),
                    _ => return Err(RulesError::UnresolvedCardTarget),
                }
            }
            // Nothing to shuffle in is nothing to do — unless the card
            // names no zone at all, which is "shuffle your stack" (Bring
            // Them Home's "the Runner shuffles it into the stack", after
            // `AddToDeck` has put it there).
            if cards.is_empty() && !zones.is_empty() {
                return Ok(Vec::new());
            }
            // Only a rig card hosts cards, and only its owner's: hosted from
            // the grip. Nothing is recorded: which cards is the Runner's to
            // know, and the stack's count says how many — the grip's and
            // the heap's are public already.
            state.runner.stack.extend(cards);
            crate::rules::pending_choice::shuffle_decks(state, Side::Runner, &crate::dsl::CardZoneRef::OwnStack, None);
            Ok(Vec::new())
        }

        Effect::AddToDeck(end) => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            let top = *end == crate::dsl::DeckEnd::Top;
            // Both decks draw from the end of the `Vec`: the end is the top
            // and index 0 the bottom.
            let place = |deck: &mut Vec<CardId>, card: CardId| if top { deck.push(card) } else { deck.insert(0, card) };
            if registry.get(&card_id).is_some_and(|card| card.side == Side::Corp) {
                let in_hq = state.corp.hq.iter().position(|c| c == &card_id);
                if take_revealed(state, Side::Corp, &card_id) {
                    return Ok(in_hq.map_or_else(Vec::new, |position| {
                        let card = state.corp.hq.remove(position);
                        place(&mut state.corp.r_and_d, card.clone());
                        vec![GameEvent::CardAddedToDeck { side: Side::Corp, card, top, revealed: true }]
                    }));
                }
                let taken = if let Some(position) = in_hq {
                    Some(state.corp.hq.remove(position))
                } else if let Some(position) = state.corp.archives.iter().position(|a| a.card == card_id) {
                    Some(state.corp.archives.remove(position).card)
                } else if let Some(position) = state.corp.r_and_d.iter().position(|c| c == &card_id) {
                    Some(state.corp.r_and_d.remove(position))
                } else {
                    None
                };
                return Ok(taken.map_or_else(Vec::new, |card| {
                    place(&mut state.corp.r_and_d, card.clone());
                    vec![GameEvent::CardAddedToDeck { side: Side::Corp, card, top, revealed: false }]
                }));
            }
            // "That program", named by its install: the one this
            // resolution installed, if it is still in the rig. Gone, or
            // reinstalled under another handle, it is not moved.
            if let Some(install) = ctx.acting_install {
                if !state.runner.rig.iter().any(|c| c.install_id == install && c.card == card_id) {
                    return Ok(Vec::new());
                }
                let removed = crate::rules::pending_choice::remove_installed_card(state, registry, Side::Runner, &crate::dsl::CardZoneRef::OwnInstalled, install)?
                    .ok_or(RulesError::InstallNotFound(install))?;
                let (card, mut events) = (removed.card, removed.cascade);
                place(&mut state.runner.stack, card.clone());
                events.push(GameEvent::CardAddedToDeck { side: Side::Runner, card, top, revealed: true });
                return Ok(events);
            }
            if take_revealed(state, Side::Runner, &card_id) {
                let Some(position) = state.runner.grip.iter().position(|c| c == &card_id) else { return Ok(Vec::new()) };
                state.runner.grip.remove(position);
                place(&mut state.runner.stack, card_id.clone());
                return Ok(vec![GameEvent::CardAddedToDeck { side: Side::Runner, card: card_id, top, revealed: true }]);
            }
            for (zone, revealed) in [(&mut state.runner.heap, true), (&mut state.runner.grip, false)] {
                if let Some(position) = zone.iter().position(|c| c == &card_id) {
                    zone.remove(position);
                    place(&mut state.runner.stack, card_id.clone());
                    return Ok(vec![GameEvent::CardAddedToDeck { side: Side::Runner, card: card_id, top, revealed }]);
                }
            }
            Ok(Vec::new())
        }

        Effect::LookAtTopOfDeck { deck, count } => {
            let looker = carried_out_by(registry, ctx).ok_or(RulesError::MissingActingCardContext)?;
            let pile = match deck {
                Side::Corp => &state.corp.r_and_d,
                Side::Runner => &state.runner.stack,
            };
            // The top is the end of the `Vec`; named top first.
            let cards: Vec<CardId> = pile.iter().rev().take(*count as usize).cloned().collect();
            if cards.is_empty() {
                return Ok(Vec::new());
            }
            Ok(vec![GameEvent::CardsLookedAt { side: looker, deck: *deck, cards }])
        }

        // Onto a piece of ice: Spree's "host 1 installed trojan program on a
        // piece of ice protecting the attacked server", a trojan moved from
        // the ice it was on.
        Effect::HostRigCardOnInstall { card, host } if state.corp.installed.iter().any(|c| c.install_id == *host && c.slot == crate::rules::state::InstallSlot::Ice) => {
            let host_card = state.corp.installed.iter().find(|c| c.install_id == *host).map(|c| c.card.clone()).expect("checked above");
            let hosted = state.runner.rig.iter_mut().find(|c| c.install_id == *card).ok_or(RulesError::InstallNotFound(*card))?;
            hosted.hosted_on_ice = Some(*host);
            hosted.hosted_on_rig_card = None;
            Ok(vec![GameEvent::CardHosted { card: hosted.card.clone(), host: host_card }])
        }
        Effect::HostRigCardOnInstall { card, host } => {
            if card == host {
                return Err(RulesError::InstallNotFound(*host));
            }
            if !state.runner.rig.iter().any(|c| c.install_id == *host) {
                return Err(RulesError::InstallNotFound(*host));
            }
            let hosted = state
                .runner
                .rig
                .iter_mut()
                .find(|c| c.install_id == *card)
                .ok_or(RulesError::InstallNotFound(*card))?;
            hosted.hosted_on_rig_card = Some(*host);
            let hosted_card = hosted.card.clone();
            let host_card = state.runner.rig.iter().find(|c| c.install_id == *host).map(|c| c.card.clone()).unwrap();
            Ok(vec![GameEvent::CardHosted { card: hosted_card, host: host_card }])
        }

        Effect::InstallRunnerCardFromGrip => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            // Eligibility may have shifted since the selection was offered
            // (`CardFilter::InstallableRunnerCard` checked it then — but
            // Mutual Favor's fetched icebreaker was never filtered on
            // affordability at all). If the pick is not installable, the
            // card simply stays in the grip: the same "nothing to do"
            // leniency `PromptChooseCards`'s fewer-than-`min` case
            // establishes, never an error that would fail the decision
            // resolving it.
            if !crate::rules::engine::can_install_runner_card_from_grip(state, registry, &card_id) {
                return Ok(Vec::new());
            }
            crate::rules::engine::install_runner_card_from_grip_paying_cost(state, registry, card_id)
        }

        Effect::Prohibit { what, until, copies_of_it, this_install } => {
            let until = lingering::until(state, *until)?;
            // The card whose text it is, for whoever shows it; a prohibition
            // with no card behind it has nothing to be shown as. About
            // copies, the acting card is the one revealed, and the text is
            // the card that asked for it.
            let acting = acting_card.cloned().ok_or(RulesError::UnresolvedCardTarget)?;
            let (on, source) = if *copies_of_it {
                (On::CopiesOf(acting.clone()), ctx.attributed_card().unwrap_or(acting))
            } else if *this_install {
                let install = ctx.acting_install.ok_or(RulesError::UnresolvedCardTarget)?;
                (On::Install(install), ctx.attributed_card().unwrap_or(acting))
            } else {
                (On::Player(what.binds()), acting)
            };
            state.lingering.push(LingeringEffect { what: Lingering::Cannot(*what), on, until, source });
            Ok(Vec::new())
        }

        Effect::PlaceAdvancementCounters(amount) => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let placed = resolve_amount(amount, ctx, state, registry);
            let installed =
                acting_corp_install_mut(state, ctx).ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            installed.advancement_tokens = installed.advancement_tokens.saturating_add(placed);
            let advancement_tokens = installed.advancement_tokens;
            let install = installed.install_id;
            // Not `CardAdvanced`: placing a counter is not advancing (CR
            // 1.18.2), so this must not reach `Trigger::OnAdvance`.
            Ok(vec![GameEvent::AdvancementCountersPlaced { install, card: Some(card_id.clone()), advancement_tokens }])
        }

        Effect::RemoveAdvancementCounters(amount) => {
            let removed = resolve_amount(amount, ctx, state, registry);
            let Some(installed) = acting_corp_install_mut(state, ctx) else { return Ok(Vec::new()) };
            installed.advancement_tokens = installed.advancement_tokens.saturating_sub(removed);
            Ok(vec![GameEvent::AdvancementCountersRemoved {
                install: installed.install_id,
                card: Some(installed.card.clone()),
                advancement_tokens: installed.advancement_tokens,
            }])
        }

        Effect::BoostStrength { amount, duration } => {
            let acting = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            require_encounter(state)?;
            let position = acting_rig_position(state, ctx)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: acting.clone() })?;
            // A hosted card can lengthen the boost (GAMEDRAGON™ Pro:
            // "abilities that increase its strength last for the remainder
            // of the run"). Only ever a lengthening: a `Turn` boost is
            // already longer and stays `Turn`.
            let host_install = state.runner.rig[position].install_id;
            let lasts_the_run = continuous::boosts_last_the_run(state, registry, host_install);
            let duration = match duration {
                EffectDuration::Encounter if lasts_the_run => EffectDuration::Run,
                other => *other,
            };
            let until = lingering::until(state, duration)?;
            state.lingering.push(LingeringEffect { what: Lingering::Strength(*amount as i32), on: On::Install(host_install), until, source: acting.clone() });
            let new_strength = lingering::rig_strength(state, &state.runner.rig[position]);
            Ok(vec![GameEvent::StrengthBoosted {
                card_id: acting.clone(),
                new_strength,
                delta: *amount as i32,
                duration,
            }])
        }

        Effect::BreakSubroutines { count, restrict_to } => {
            let acting = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
            if run.phase != RunPhase::EncounterIce {
                return Err(RulesError::NotInEncounter);
            }

            let breaker = acting_rig_card(state, ctx)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: acting.clone() })?;
            let breaker_strength = continuous::breaker_strength(state, registry, breaker);

            let run = state.active_run.as_ref().unwrap();
            let ice = &run.ice[run.position];
            let (ice_card_id, ice_strength, ice_type, ice_install) =
                (ice.card_id.clone(), continuous::ice_strength(state, registry, ice), ice.ice_type, ice.install_id);
            if let Some(expected) = restrict_to
                && *expected != ice_type
                && !continuous::ice_gains_subtype(state, registry, ice_install, *expected)
            {
                return Err(RulesError::InvalidBreakerSubtype {
                    breaker: acting.clone(),
                    ice: ice_card_id,
                    expected: *expected,
                });
            }
            if breaker_strength < ice_strength {
                return Err(RulesError::BreakerStrengthTooLow {
                    breaker: acting.clone(),
                    breaker_strength,
                    ice: ice_card_id,
                    ice_strength,
                });
            }

            // Collected/owned before any &mut state borrow below, so the
            // immutable `run`/`ice` reads above never overlap with
            // transition_subroutine's &mut state. A subroutine only a
            // printed subtype may break (Semak-samun) stays pending for a
            // breaker without it.
            let breaker_def = registry.get(acting);
            let pending = breakable_now(state, registry, ice, breaker_def);
            if pending.is_empty() {
                return Err(RulesError::NoBreakableSubroutine { ice: ice_card_id });
            }
            break_pending(state, registry, pending, count, ctx.acting_install)
        }

        Effect::BreakSubroutinesUnconditionally { count } => {
            let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
            if run.phase != RunPhase::EncounterIce {
                return Err(RulesError::NotInEncounter);
            }
            let ice = &run.ice[run.position];
            let breaker_def = acting_card.and_then(|card| registry.get(card));
            let pending = breakable_now(state, registry, ice, breaker_def);
            if pending.is_empty() {
                return Err(RulesError::NoBreakableSubroutine { ice: ice.card_id.clone() });
            }
            break_pending(state, registry, pending, count, ctx.acting_install)
        }

        Effect::Trace { base, on_success } => {
            if state.active_trace.is_some() {
                return Err(RulesError::TraceAlreadyActive);
            }
            state.active_trace = Some(TraceState {
                initiating_card: acting_card.cloned(),
                initiating_install: ctx.acting_install,
                base_strength: *base,
                corp_bid: None,
                effect_on_success: (**on_success).clone(),
                resume: TraceResume::None,
            });
            Ok(vec![GameEvent::TraceInitiated { base: *base, initiating_card: acting_card.cloned() }])
        }

        Effect::AddAdditionalAccess { server, count } => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            match server {
                ServerId::Hq => run.additional_hq_access = run.additional_hq_access.saturating_add(*count),
                ServerId::RnD => run.additional_rd_access = run.additional_rd_access.saturating_add(*count),
                // A rules no-op, not a modelling gap: a breach of Archives
                // accesses every card there and a breach of a remote every
                // card in its root, so "one additional card" changes
                // nothing. No event either — this used to emit
                // `AdditionalAccessGranted` here, recording a grant that
                // had no effect.
                ServerId::Archives | ServerId::Remote(_) => return Ok(Vec::new()),
            }
            Ok(vec![GameEvent::AdditionalAccessGranted { server: *server, count: *count }])
        }

        Effect::SetAccessReplacement { server, effect, optional } => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            run.access_replacement = Some((*server, (**effect).clone(), *optional));
            run.access_replacement_card = acting_card.cloned();
            run.access_replacement_install = ctx.acting_install;
            Ok(vec![GameEvent::AccessReplacementSet { server: *server }])
        }

        Effect::Sequence(effects) => evaluate_sequence(state, effects, ctx, registry),

        Effect::GainCreditsAmount(side, amount) => {
            let amount = resolve_amount(amount, ctx, state, registry);
            gain_credits_from_ability(state, registry, *side, amount, ctx)
        }

        Effect::LoseCredits(side, amount) => {
            let before = state.resources(*side).credits.0;
            // What was actually removed, not the printed amount — recorded
            // for `Amount::CreditsLostThisResolution` (Account Siphon's
            // "2[c] for each credit lost"), and emitted, since "loses 3"
            // against a 2-credit pool loses 2.
            // Aircheck's "you cannot lose … credits from your credit pool":
            // nothing is lost, and the event says so.
            let locked = *side == Side::Runner && continuous::cannot(state, registry, crate::dsl::Prohibition::SpendOrLoseCreditPool);
            let lost = if locked { 0 } else { (*amount).min(before) };
            state.resources_mut(*side).credits = Credits(before - lost);
            ctx.credits_lost = lost;
            Ok(vec![GameEvent::CreditsLost { side: *side, amount: lost }])
        }

        Effect::LoseCreditsAmount(side, amount) => {
            let amount = resolve_amount(amount, ctx, state, registry);
            evaluate_effect(state, &Effect::LoseCredits(*side, amount), ctx, registry)
        }

        // Dispatched where it happens, as the basic action's is:
        // "when the Corp purges virus counters" hears Flyswatter too.
        Effect::PurgeVirusCounters => {
            let purged = crate::rules::engine::purge_all_virus_counters(state, registry);
            let mut events = Vec::new();
            dispatcher::emit(state, registry, &mut events, purged)?;
            Ok(events)
        }

        Effect::TurnFaceupInArchives => {
            let card = acting_card.ok_or(RulesError::MissingActingCardContext)?;
            let mut events = Vec::new();
            if let Some(archived) = state.corp.archives.iter_mut().find(|archived| archived.facedown && &archived.card == card) {
                archived.facedown = false;
                dispatcher::emit(state, registry, &mut events, GameEvent::ArchivesTurnedFaceup { count: 1 })?;
            }
            Ok(events)
        }

        // Rewritten into the `PresentChoice` it is shorthand for: each option
        // followed by the same offer over the rest, one fewer to resolve.
        Effect::ResolveSomeOf { chooser, count, options, texts } => {
            if *count == 0 || options.is_empty() {
                return Ok(Vec::new());
            }
            let text_of = |index: usize| texts.get(index).cloned();
            let expanded: Vec<Effect> = options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    let remaining: Vec<Effect> =
                        options.iter().enumerate().filter(|(other, _)| *other != index).map(|(_, e)| e.clone()).collect();
                    let remaining_texts: Vec<String> =
                        (0..options.len()).filter(|other| *other != index).filter_map(text_of).collect();
                    Effect::Sequence(vec![
                        option.clone(),
                        Effect::ResolveSomeOf { chooser: *chooser, count: count - 1, options: remaining, texts: remaining_texts },
                    ])
                })
                .collect();
            // The printed clauses ride along one level: the option chosen
            // now is labelled by its own clause, the rest re-offered with
            // theirs.
            let expanded_texts: Vec<String> = (0..options.len()).map(|i| text_of(i).unwrap_or_default()).collect();
            let expanded_texts = if texts.is_empty() { Vec::new() } else { expanded_texts };
            evaluate_effect(state, &Effect::PresentChoice { chooser: *chooser, options: expanded, texts: expanded_texts }, ctx, registry)
        }

        Effect::LoseClicks(amount) => {
            state.runner.resources.clicks =
                Clicks(state.runner.resources.clicks.0.saturating_sub(*amount));
            Ok(vec![GameEvent::ClicksLost { side: Side::Runner, amount: *amount }])
        }

        Effect::GainClicks(side, amount) => {
            let resources = match side {
                Side::Corp => &mut state.corp.resources,
                Side::Runner => &mut state.runner.resources,
            };
            resources.clicks = Clicks(resources.clicks.0.saturating_add(*amount));
            Ok(vec![GameEvent::ClicksGained { side: *side, amount: *amount }])
        }

        Effect::InitiateRun(server) => {
            run::start_run(state, registry, *server)?;
            if let Some(run) = state.active_run.as_mut() {
                run.initiated_by = acting_card.cloned();
            }
            let run_initiated_event = GameEvent::RunInitiated { server: *server };
            let mut events = vec![run_initiated_event.clone()];
            events.extend(crate::rules::dispatcher::dispatch_event(state, registry, &run_initiated_event)?);
            Ok(events)
        }

        Effect::EffectIf { condition, effect } => {
            let side = acting_side(acting_card, registry);
            if check_requirement(state, condition, side, ctx, registry).is_ok() {
                evaluate_effect(state, effect, ctx, registry)
            } else {
                Ok(Vec::new())
            }
        }

        Effect::OfferPaidChoice { side, cost, if_paid, if_declined, text } => {
            state.pending_paid_choice = Some(PendingPaidChoice {
                side: *side,
                cost: cost.clone(),
                if_paid: (**if_paid).clone(),
                if_declined: (**if_declined).clone(),
                text: text.clone(),
                source_card: acting_card.cloned(),
                prompting_card: ctx.attributed_card(),
                source_install: ctx.acting_install,
                resume: PendingPaidChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingPaidChoiceOffered { side: *side }])
        }

        Effect::PresentChoice { chooser, options, texts } => {
            state.pending_decision = Some(PendingDecision::ChooseEffect {
                chooser: *chooser,
                options: options.clone(),
                option_texts: texts.clone(),
                source_card: acting_card.cloned(),
                prompting_card: ctx.attributed_card(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingChoicePresented { chooser: *chooser, option_count: options.len() }])
        }

        Effect::PsiGame { on_match, on_differ } => {
            // What each could spend on a bid (CR 10.14.3), fixed as the
            // game begins; a player who could bid only 0 is not asked.
            let most = |side| {
                crate::rules::payment::available(state, registry, side, crate::rules::payment::Purpose::Other)
                    .min(crate::rules::pending_choice::PSI_MAX_BID)
            };
            let (corp_max, runner_max) = (most(Side::Corp), most(Side::Runner));
            state.pending_decision = Some(PendingDecision::PsiGame {
                corp_bid: if corp_max == 0 { crate::rules::state::PsiBid::Bid(0) } else { crate::rules::state::PsiBid::Awaiting },
                corp_max,
                runner_max,
                on_match: on_match.clone(),
                on_differ: on_differ.clone(),
                source_card: acting_card.cloned(),
                prompting_card: ctx.attributed_card(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            match (corp_max, runner_max) {
                (0, 0) => crate::rules::pending_choice::finish_psi_game(state, registry, 0, Vec::new()),
                (0, _) => Ok(vec![GameEvent::NumberChoiceOffered { chooser: Side::Runner, min: 0, max: runner_max }]),
                _ => Ok(vec![GameEvent::NumberChoiceOffered { chooser: Side::Corp, min: 0, max: corp_max }]),
            }
        }

        Effect::ChooseNumber { chooser, min, max, of, then, text, secret } => {
            let mut most = resolve_amount(max, ctx, state, registry).min(crate::rules::action_mask::MAX_CHOSEN_NUMBER);
            if let Some(of) = of {
                most = most.min(resolve_amount(of, ctx, state, registry));
            }
            if most <= *min {
                // One number, or none the card allows: nobody is asked.
                // A range the state has emptied ("up to 2" of no tags)
                // resolves with what there is, as `RemoveTags` always
                // removed what it could.
                return evaluate_effect(state, &then.as_ref().clone().with_chosen_number(most), ctx, registry);
            }
            state.pending_decision = Some(PendingDecision::ChooseNumber {
                chooser: *chooser,
                min: *min,
                max: most,
                then: then.clone(),
                text: text.clone(),
                source_card: acting_card.cloned(),
                prompting_card: ctx.attributed_card(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
                secret: *secret,
            });
            Ok(vec![GameEvent::NumberChoiceOffered { chooser: *chooser, min: *min, max: most }])
        }

        Effect::PromptChooseCards { side, source, filter, min, max, reveal, shuffle_after, destination, then } => {
            let filter = &filter.clone().with_this_server(acting_server(state, ctx));
            let available = crate::rules::pending_choice::eligible_positions(state, registry, *side, source, filter, ctx.acting_install, ctx.acting_card);
            // An installed Runner card trashed by the text of ice whose
            // `TrashLimit` this encounter has spent (Sorocaban Blade): there
            // is nothing it may choose.
            let trash_spent = *side == Side::Corp
                && matches!(source, crate::dsl::CardZoneRef::OpponentInstalled)
                && matches!(destination, Some(crate::dsl::CardZoneRef::OpponentDiscard))
                && !continuous::may_trash_with(state, registry, ctx.acting_install);
            if trash_spent || available.is_empty() || available.len() < *min as usize {
                // Nothing to do — same "silently no-op" leniency
                // `DrawCards`/`TrashCard`'s "already gone" case establish.
                // e.g. Hansei Review's "if there are any cards in HQ".
                // Nothing eligible at all is the same case even for a
                // `min: 0` offer (Bumi 1.0 with no trojan in the rig):
                // parking a choice whose only resolution is an empty
                // confirmation costs the player a decision for nothing.
                // A search that finds nothing still shuffles (CR 8.7.3).
                if *shuffle_after {
                    crate::rules::pending_choice::shuffle_decks(state, *side, source, destination.as_ref());
                }
                return Ok(Vec::new());
            }
            state.pending_decision = Some(PendingDecision::ChooseCards {
                side: *side,
                source: source.clone(),
                filter: filter.clone(),
                min: *min,
                max: *max,
                reveal: *reveal,
                shuffle_after: *shuffle_after,
                destination: destination.clone(),
                then: then.clone(),
                selected: Vec::new(),
                source_card: acting_card.cloned(),
                prompting_card: ctx.attributed_card(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingCardSelectionOffered { side: *side, min: *min, max: *max, source: ctx.attributed_card() }])
        }

        Effect::PromptChooseServer {
            chooser,
            rez_cost_delta,
            bonus_run_credits,
            allowed_servers,
            on_success,
            on_start,
            exclude_servers_run_this_turn,
            only_protected_by_ice,
            only_in,
        } => {
            // A parked `ChooseServer` is only ever resolved by
            // `run::start_run`, which rejects a second concurrent run — so
            // parking one while a run is active creates a decision *nothing*
            // can resolve, and (since a parked decision blocks every other
            // action) deadlocks the game outright.
            //
            // Checking the precondition here rather than deferring it to
            // resolution is what makes it visible to `legal_actions`'
            // dry-run probe, which then filters out the whole activating
            // ability instead of offering a click-sink. Same reason
            // `Effect::InitiateRun` is safe already: it calls `start_run`
            // inline, so its error surfaces to the probe.
            //
            // Found by `no_panics_or_deadlocks_across_many_seeds_system_gateway`:
            // Red Team's `[click]: Run a central server…` was being offered
            // mid-run.
            //
            // Shares `run::check_run_may_begin` with `start_run` itself
            // rather than restating the condition — the two MUST agree, or
            // this parks a decision `start_run` will refuse. See that
            // function's doc comment; a narrower copy here is exactly what
            // caused the original deadlock.
            run::check_run_may_begin(state)?;
            // Narrow the offer here, for the same reason as the check
            // above: an offer with nothing in it is a decision nothing can
            // resolve, and refusing to park makes the probe drop the
            // ability. The narrowed list is what the decision carries, so
            // resolution's re-check and the candidate filter need no
            // knowledge of why a server is missing.
            let allowed_servers = if *exclude_servers_run_this_turn || *only_protected_by_ice || only_in.is_some() {
                let already_run = &state.runner.servers_run_this_turn;
                // `None` means every server — enumerated the way
                // `legal_actions` offers them, fresh remote included.
                let every_server = || {
                    let existing = crate::rules::legal_actions::existing_remote_ids(state);
                    let mut servers = vec![ServerId::Hq, ServerId::RnD, ServerId::Archives];
                    servers.extend(existing.iter().copied().map(ServerId::Remote));
                    servers.push(ServerId::Remote(crate::rules::legal_actions::fresh_remote_id(&existing)));
                    servers
                };
                let protected = |server: &ServerId| {
                    state.corp.installed.iter().any(|c| c.server == *server && c.slot == crate::rules::InstallSlot::Ice)
                };
                let offered: Vec<ServerId> = allowed_servers
                    .clone()
                    .unwrap_or_else(every_server)
                    .into_iter()
                    .filter(|server| !*exclude_servers_run_this_turn || !already_run.contains(server))
                    .filter(|server| !*only_protected_by_ice || protected(server))
                    .filter(|server| {
                        only_in.is_none_or(|kind| {
                            let exists = match server {
                                ServerId::Remote(id) => crate::rules::legal_actions::existing_remote_ids(state).contains(id),
                                _ => true,
                            };
                            kind.admits(*server) && exists
                        })
                    })
                    .collect();
                if offered.is_empty() {
                    return Err(RulesError::NoServerLeftToRun);
                }
                Some(offered)
            } else {
                allowed_servers.clone()
            };
            state.pending_decision = Some(PendingDecision::ChooseServer {
                chooser: *chooser,
                rez_cost_delta: *rez_cost_delta,
                bonus_run_credits: *bonus_run_credits,
                allowed_servers,
                on_success: on_success.clone(),
                on_start: on_start.clone(),
                install: None,
                move_to_root: false,
                source_card: acting_card.cloned(),
                prompting_card: ctx.attributed_card(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingServerChoiceOffered { chooser: *chooser }])
        }

        Effect::PromptMoveThisCardToAnotherRoot => {
            let Some(installed) = acting_corp_install(state, ctx) else { return Ok(Vec::new()) };
            if installed.slot != crate::rules::state::InstallSlot::Root {
                return Ok(Vec::new());
            }
            let own = installed.server;
            // "Central server only" holds at all times: such an upgrade is
            // not moved into a server it may not occupy (CR 8.5.12).
            let card = registry.get(&installed.card);
            let mut servers = vec![ServerId::Hq, ServerId::RnD, ServerId::Archives];
            servers.extend(crate::rules::legal_actions::existing_remote_ids(state).into_iter().map(ServerId::Remote));
            servers.retain(|server| *server != own && card.is_none_or(|card| card.may_be_installed_in(*server)));
            if servers.is_empty() {
                return Ok(Vec::new());
            }
            state.pending_decision = Some(PendingDecision::ChooseServer {
                chooser: Side::Corp,
                rez_cost_delta: 0,
                bonus_run_credits: 0,
                allowed_servers: Some(servers),
                on_success: None,
                on_start: None,
                install: None,
                move_to_root: true,
                source_card: acting_card.cloned(),
                prompting_card: ctx.attributed_card(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingServerChoiceOffered { chooser: Side::Corp }])
        }

        Effect::PromptInstallCorpCard { origin_zone, ignore_costs, discount, then, remote_only, another_server, rez, if_rezzed, if_installed } => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            // First match by position: two copies of one card in HQ are
            // indistinguishable and interchangeable, so "the copy the Corp
            // just selected" and "the first copy" are the same card.
            let position = match origin_zone {
                crate::dsl::CardZoneRef::OwnHq => state.corp.hq.iter().position(|c| c == &card_id),
                crate::dsl::CardZoneRef::OwnArchives => {
                    state.corp.archives.iter().position(|a| a.card == card_id)
                }
                // Poétrï Luxury Brands installs one of the top 3 cards of
                // R&D — the selection narrowed the offer to the top three
                // (`CardFilter::TopOfZone`), but the card is taken from
                // R&D as a whole, so this is a first-match lookup like
                // HQ's.
                crate::dsl::CardZoneRef::OwnRAndD => state.corp.r_and_d.iter().position(|c| c == &card_id),
                _ => return Err(RulesError::UnresolvedCardTarget),
            };
            // Card gone from the zone, an uninstallable type, or (for ICE)
            // no affordable destination: nothing to offer — the same
            // "nothing to do" leniency `PromptChooseCards` establishes. A
            // fresh remote is always free, so this only bites for a type
            // that cannot be installed at all.
            let Some(position) = position else { return Ok(Vec::new()) };
            let Some(card_def) = registry.get(&card_id) else { return Ok(Vec::new()) };
            let mut allowed = crate::rules::engine::corp_install_destinations(state, card_def, *ignore_costs, *discount);
            if *remote_only {
                allowed.retain(|server| matches!(server, crate::rules::run::ServerId::Remote(_)));
            }
            if *another_server {
                let own = acting_corp_position(state, ctx).map(|position| state.corp.installed[position].server);
                allowed.retain(|server| Some(*server) != own);
            }
            if allowed.is_empty() {
                return Ok(Vec::new());
            }
            state.pending_decision = Some(PendingDecision::ChooseServer {
                chooser: Side::Corp,
                rez_cost_delta: 0,
                bonus_run_credits: 0,
                allowed_servers: Some(allowed),
                on_success: None,
                on_start: None,
                move_to_root: false,
                install: Some(crate::rules::state::PendingInstallFromZone {
                    origin: origin_zone.clone(),
                    position,
                    pay_cost: !ignore_costs,
                    discount: *discount,
                    remote_only: *remote_only,
                    then: then.clone(),
                    rez: *rez,
                    if_rezzed: if_rezzed.clone(),
                    if_installed: if_installed.clone(),
                }),
                // Deliberately NOT the chosen card: `source_card` passes
                // through the masked view, and the pick out of HQ is
                // hidden information until it lands. The install payload
                // above names it by position instead.
                source_card: None,
                prompting_card: ctx.attributed_card(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingServerChoiceOffered { chooser: Side::Corp }])
        }

        Effect::MoveThisCardToRoot(server) => {
            let Some(position) = acting_corp_position(state, ctx) else { return Ok(Vec::new()) };
            let installed = &state.corp.installed[position];
            let may_occupy = registry.get(&installed.card).is_none_or(|card| card.may_be_installed_in(*server));
            if installed.slot != crate::rules::state::InstallSlot::Root || installed.server == *server || !may_occupy {
                return Ok(Vec::new());
            }
            let (card, from, install) = (installed.card.clone(), installed.server, installed.install_id);
            state.corp.installed[position].server = *server;
            // Heard by the card that moved (Isaac Liberdade).
            let mut events = Vec::new();
            dispatcher::emit(state, registry, &mut events, GameEvent::CardMoved { install, card: Some(card), from, to: *server })?;
            Ok(events)
        }

        Effect::ForceEncounter => {
            let Some(install) = ctx.acting_install else { return Ok(Vec::new()) };
            crate::rules::run::force_encounter(state, registry, install)
        }

        Effect::MoveThisIceToOutermost => {
            let Some(server) = state.active_run.as_ref().map(|run| run.server) else { return Ok(Vec::new()) };
            let Some(position) = acting_corp_position(state, ctx) else { return Ok(Vec::new()) };
            if state.corp.installed[position].slot != crate::rules::state::InstallSlot::Ice {
                return Ok(Vec::new());
            }
            let outermost = state.corp.installed.iter().position(|c| c.server == server && c.slot == crate::rules::state::InstallSlot::Ice);
            if outermost == Some(position) {
                return Ok(Vec::new());
            }
            // Out of the list and back in front of the attacked server's
            // ice: `corp.installed` is outermost-first per server
            // (`engine::place_corp_card`).
            let mut ice = state.corp.installed.remove(position);
            let from = ice.server;
            ice.server = server;
            let (card, install) = (ice.card.clone(), ice.install_id);
            match state.corp.installed.iter().position(|c| c.server == server && c.slot == crate::rules::state::InstallSlot::Ice) {
                Some(index) => state.corp.installed.insert(index, ice),
                None => state.corp.installed.push(ice),
            }
            let mut events = Vec::new();
            if from != server {
                dispatcher::emit(state, registry, &mut events, GameEvent::CardMoved { install, card: Some(card), from, to: server })?;
            }
            Ok(events)
        }

        Effect::PlayOperation { from } => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            let from_archives = matches!(from, crate::dsl::CardZoneRef::OwnArchives);
            // Lenient, like every other "you may" here: an operation whose
            // cost or `play_requirement` has stopped holding since the
            // offer resolves to nothing rather than failing the action
            // that got here.
            if !crate::rules::engine::can_play_operation(state, registry, &card_id, from_archives) {
                return Ok(Vec::new());
            }
            crate::rules::engine::play_operation_card(state, registry, card_id, from_archives)
        }

        Effect::RezInstalled { install, pay_cost, discount } => {
            // Shares `engine::rez_install` with `PlayerAction::RezIce`, so a
            // discounted rez is paid the same way (`rules::payment`: a region's
            // hosted rez credits before the credit pool) and fires `OnRez`
            // identically. Unaffordable is a no-op, not an error: Mycoweb's
            // "you may rez ... paying 2[c] less" and both branches of
            // Biawak's forfeit choice must resolve to *something*, and a
            // failing effect would leave the decision that parked them
            // unresolvable. `AlreadyRezzed`/`InstallNotFound` still error —
            // those mean the card was named wrongly, not priced wrongly.
            // The placeholder is "the card this resolves as": a selection's
            // `then` substitutes it when the rez is the whole `then` (Send a
            // Message), and inside a `Sequence` it is read here instead —
            // Unleash rezzes the chosen ice and then resolves one of its
            // subroutines, as the same card.
            let install = if *install == InstallId::PLACEHOLDER {
                ctx.acting_install.ok_or(RulesError::UnresolvedCardTarget)?
            } else {
                *install
            };
            match crate::rules::engine::rez_install(state, registry, install, *pay_cost, *discount) {
                Err(RulesError::NotEnoughCredits { .. }) => Ok(Vec::new()),
                other => other,
            }
        }

        Effect::ResolveSubroutineOfSelectedIce => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let Some(card_def) = registry.get(card_id) else { return Ok(Vec::new()) };
            let options: Vec<Effect> = card_def.subroutines.iter().map(|sub| sub.effect.clone()).collect();
            // A rezzed piece of ice with no subroutines resolves nothing
            // rather than parking an empty choice — the same leniency an
            // empty `PromptChooseCards` offer takes.
            if options.is_empty() {
                return Ok(Vec::new());
            }
            // One subroutine is not a choice; resolve it directly rather
            // than asking the Corp to confirm the only option.
            if let [only] = options.as_slice() {
                let only = only.clone();
                return evaluate_effect(state, &only, ctx, registry);
            }
            evaluate_effect(state, &Effect::PresentChoice { chooser: Side::Corp, options, texts: Vec::new() }, ctx, registry)
        }

        Effect::AllottedClicksNextTurn(side, delta) => {
            let source = acting_card.cloned().ok_or(RulesError::UnresolvedCardTarget)?;
            state.lingering.push(LingeringEffect {
                what: Lingering::AllottedClicks(*delta),
                on: On::Player(*side),
                until: Until::NextTurnOf(*side),
                source,
            });
            Ok(Vec::new())
        }

        Effect::InstallAgendaFromRunnerScoreArea => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            let Some(position) = state.runner.scored_agendas.iter().position(|scored| scored.card == card_id && scored.as_agenda.is_none()) else {
                return Ok(Vec::new());
            };
            let points = crate::rules::win::agenda_value_in(state, registry, &card_id, Side::Runner);
            // The tags are the price, and the offer was filtered so they
            // are there — but check anyway: a parked selection resolves
            // later than it was built, and paying half a cost is worse
            // than doing nothing.
            if state.runner.tags < points {
                return Ok(Vec::new());
            }
            state.runner.scored_agendas.remove(position);
            state.runner.resources.agenda_points = state.runner.resources.agenda_points.gain(-(points as i32));
            let mut events = evaluate_effect(state, &Effect::RemoveTags(Amount::Fixed(points)), ctx, registry)?;
            // A fresh remote: see the variant's doc comment for why the
            // Corp is not asked where.
            let existing = crate::rules::legal_actions::existing_remote_ids(state);
            let server = crate::rules::run::ServerId::Remote(crate::rules::legal_actions::fresh_remote_id(&existing));
            events.extend(crate::rules::engine::place_corp_card(
                state,
                registry,
                card_id,
                server,
                crate::rules::state::InstallSlot::Root,
                false,
                0,
                false,
                false,
            )?);
            Ok(events)
        }

        Effect::MoveRunToOutermost(server) => {
            crate::rules::run::move_run_to_outermost(state, registry, *server)
        }

        Effect::SwapApproachedIceWithCard { origin } => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            crate::rules::run::swap_approached_ice_with_card(state, registry, &card_id, origin)
        }

        Effect::DealDamageAmount(damage_type, amount) => {
            let resolved = resolve_amount(amount, ctx, state, registry);
            evaluate_effect(state, &Effect::DealDamage(*damage_type, resolved as usize), ctx, registry)
        }

        Effect::AddAdditionalAccessAmount { server, amount } => {
            let resolved = resolve_amount(amount, ctx, state, registry);
            evaluate_effect(state, &Effect::AddAdditionalAccess { server: *server, count: resolved }, ctx, registry)
        }

        Effect::BoostStrengthAmount { amount, duration } => {
            let resolved = resolve_amount(amount, ctx, state, registry);
            evaluate_effect(state, &Effect::BoostStrength { amount: resolved, duration: *duration }, ctx, registry)
        }
    }
}

/// The engine-level "you are encountering ICE right now" guard, shared by
/// the effects that only make sense mid-encounter.
///
/// The backstop half of a deliberate two-level split: `EffectRequirement::
/// DuringEncounter` on an icebreaker's `AbilityDef` gates whether the
/// ability is *offered* (soft, silent, and readable without evaluating
/// anything — which is what `paid_ability::has_usable_paid_ability` needs),
/// while this gates whether the effect can actually *resolve*, however it
/// was reached. `Effect::BreakSubroutines` and `ModifyStrength` already
/// enforced this inline; `BoostStrength` did not, which is why Cleaver's
/// "+1 strength" was a legal action on the Corp's turn.
fn require_encounter(state: &GameState) -> Result<(), RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    if run.phase != RunPhase::EncounterIce {
        return Err(RulesError::NotInEncounter);
    }
    Ok(())
}

/// Which side's context an `EffectRequirement` check runs under, when the
/// caller only has `acting_card` (not an explicit `Side`) to go on —
/// `Effect::EffectIf`'s only source of "whose turn/state is this." Falls
/// back to `Side::Corp` for an unregistered/absent card, matching
/// `owning_side_of_target`'s own precedent for `CardTarget::ThisCard`.
fn acting_side(acting_card: Option<&CardId>, registry: &CardRegistry) -> Side {
    acting_card.and_then(|id| registry.get(id)).map(|c| c.side).unwrap_or(Side::Corp)
}

/// Fires every still-`Pending` subroutine on the ICE currently being
/// encountered, lowest index first, stopping once none are left (or the
/// run/game ends out from under the loop — e.g. an `Effect::EndTheRun`
/// subroutine partway through). "Nothing left to resolve" is this
/// function's normal terminal condition, not a failure: it only returns
/// `Err` if `evaluate_effect` itself errors on one of the fired
/// subroutines' effects, in which case that error propagates immediately
/// and any already-fired subroutines stay fired (no rollback).
pub fn resolve_unbroken_subroutines(
    state: &mut GameState,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = Vec::new();

    loop {
        // Stop on a finished game, or when a subroutine we just fired parked
        // a Trace or a PendingPrevention — either spans future
        // PlayerActions, so the next pending subroutine must not fire
        // underneath it. `rules::trace::submit_runner_bid`/`paid_ability::
        // close_window`'s `Prevention` arm call this function again once
        // resolved, resuming the loop.
        if state.resolution_halted() {
            break;
        }

        // Immutable read only — ends before any mutation below, so it
        // never overlaps with the `&mut state` passed to transition_subroutine/evaluate_effect.
        let Some((index, install)) = state.active_run.as_ref().and_then(|run| {
            // A subroutine fired above may have removed this ICE from play
            // (or `run::reconcile_ice` moved the run for another reason):
            // the run then stands on the *next* ICE, in `ApproachIce`, with
            // all of its subroutines `Pending`. Firing those here would be
            // wrong twice over — and `transition_subroutine` would refuse
            // with `NotInEncounter`, failing the `PassPriority` that got
            // here and leaving the priority holder no legal action.
            if run.phase != RunPhase::EncounterIce {
                return None;
            }
            let ice = run.ice.get(run.position)?;
            let index = ice.subroutines.iter().position(|s| s.status == SubroutineStatus::Pending)?;
            Some((index, ice.install_id))
        }) else {
            break;
        };

        let (card_id, effect) = run::transition_subroutine(state, index, SubroutineStatus::Resolved)?;
        // Pass the ICE itself as `acting_card` — needed for a subroutine
        // effect that self-references its own installed position (e.g.
        // Ansel 1.0/Brân 1.0's "install ... directly inward from this
        // ice," resolved via `Effect::InstallFromZoneIgnoringCost`'s
        // `PendingDecision::ChooseCards::source_card` lookup). No existing
        // subroutine effect relied on `acting_card` being absent here.
        // The other site a subroutine resolves at (`run::engine::step_subroutine`
        // is the first) — `EffectRequirement::SubroutineResolvedThisRun`.
        if let Some(run) = state.active_run.as_mut() {
            run.subroutine_resolved = true;
        }
        // The *install* too, not just the card: Mycoweb's "another rezzed
        // code gate" (`CardFilter::NotSourceCard`) has to know which copy
        // is asking, and "this card" lookups inside a subroutine resolve
        // against the encountered ice rather than the first install
        // sharing its name — the same per-install exactness the Rules
        // Audit gave activated abilities.
        let fired_events = evaluate_effect(state, &effect, &mut ResolutionContext::for_install(install, &card_id), registry)?;
        events.push(GameEvent::SubroutineFired { card_id, index, effect });
        events.extend(fired_events);

        // Whatever that subroutine's effect parked — a trace, a
        // prevention, a paid choice, a decision (Ansel 1.0's first
        // subroutine parks a `ChooseCards`) — must resume this loop once
        // it resolves, so the later subroutines still fire.
        crate::rules::pending_choice::mark_parked_resume_subroutines(state);
    }

    Ok(events)
}

/// Fires every `TriggeredEffect` on `card_id`'s `CardRegistry` definition
/// matching `trigger`, in declaration order, evaluating each contained
/// `Effect` via `evaluate_effect` and collecting events. An unregistered
/// `card_id` (or one with no matching `TriggeredEffect`) is not an error —
/// yields `Ok(Vec::new())`, mirroring `run::access::compute_pending_choice`'s
/// existing "unrecognized card" default. Errors propagate immediately with
/// no rollback (already-fired effects/triggers stay applied), matching
/// `resolve_unbroken_subroutines`'s convention.
pub fn process_card_triggers(
    state: &mut GameState,
    registry: &CardRegistry,
    card_id: &CardId,
    trigger: Trigger,
    triggering_event: Option<&GameEvent>,
) -> Result<Vec<GameEvent>, RulesError> {
    let due = DeferredTrigger { announce: None,
        card: card_id.clone(),
        trigger,
        target: None,
        install: None,
        target_install: None,
        event: triggering_event.cloned(),
        continuation: None,
        heard: Default::default(),
        not_the_first_this_turn: false,
        fired: 0,
    };
    fire_card_triggers(state, registry, &due, false)
}

/// The one loop behind `process_card_triggers` and every firing in
/// `dispatcher`. `target` is the card the moment was about, carried for a
/// `TriggeredEffect` that says its effects act on it
/// (`acts_on_subject`) — the one case where "who reacts" and "what the
/// effect acts on" differ: Cookbook's "whenever you install a virus
/// program, you may place 1 virus counter on it" reacts as Cookbook but
/// acts on the just-installed program. The requirement is checked and
/// consumed as `card_id`; only the effects' context changes. With `announce`, a
/// `GameEvent::TriggerFired { card, trigger }` precedes each
/// `TriggeredEffect` that actually fires — after its requirement passed,
/// before its effects — which is the exact record the coverage harness
/// counts (`netrunner_session::Coverage::triggers_fired`). It used to
/// *infer* firings from the event that would have offered the trigger,
/// which could not see a failed requirement or a `still_applies` bail-out.
/// The two plain entry points do not announce: they are what a unit test
/// drives directly, and dozens of them assert exact event vectors that
/// gain nothing from the marker.
pub(crate) fn fire_card_triggers(
    state: &mut GameState,
    registry: &CardRegistry,
    due: &DeferredTrigger,
    announce: bool,
) -> Result<Vec<GameEvent>, RulesError> {
    let card_id = &due.card;
    let trigger = due.trigger;
    let triggering_event = due.event.as_ref();
    let Some(card) = registry.get(card_id) else {
        return Ok(Vec::new());
    };
    let mut events = Vec::new();
    let card_side = card.side;
    // Read before the loop: what the event was about does not change, and
    // the loop needs `state` mutably.
    let meant: Vec<bool> = card
        .triggers
        .iter()
        .map(|t| {
            t.trigger == trigger
                && due.heard.admits(t.subject)
                // A heap ability resolves only as heard from the heap, and
                // a card in play never resolves one (`Heard::FromHeap`).
                && t.from_heap == (due.heard == crate::rules::state::Heard::FromHeap)
                && !(t.first_each_turn && due.not_the_first_this_turn)
                && listeners::when_admits(state, registry, t, card_side, due.install, triggering_event)
        })
        .collect();
    let fired_before = usize::from(due.fired);
    let meant: Vec<&crate::dsl::TriggeredEffect> =
        card.triggers.iter().zip(meant).filter(|(_, meant)| *meant).map(|(triggered, _)| triggered).collect();
    for (index, triggered) in meant.iter().enumerate().skip(fired_before) {
        // The one before parked a decision: the rest wait behind it on the
        // queue, as the rest of a `Sequence` does, rather than park a
        // second over it (`DeferredTrigger::fired`).
        if index > fired_before && state.is_resolution_blocked() {
            state.deferred_triggers.push(DeferredTrigger { fired: index as u8, ..due.clone() });
            break;
        }
        // The requirement is checked as the *reacting* card, the effects
        // resolve as the target (the card itself, unless `target` says
        // otherwise) — separate contexts, and one pair per
        // `TriggeredEffect`, so nothing a trigger's own effects accumulate
        // leaks into the next trigger on the same card.
        let owner_ctx = ResolutionContext::for_install_trigger(due.install, Some(card_id), triggering_event);
        if let Some(requirement) = &triggered.requirement
            && check_requirement(state, requirement, card_side, &owner_ctx, registry).is_err()
        {
            // Soft gate (see `TriggeredEffect::requirement`'s doc comment):
            // unmet just means no bonus this time, not an error propagated
            // to the caller — and no per-turn flag is consumed, since it was
            // never available to begin with.
            continue;
        }
        if announce {
            events.push(GameEvent::TriggerFired { card: card_id.clone(), trigger });
        }
        let mut effect_ctx = match &due.target {
            Some(target) if triggered.acts_on_subject => ResolutionContext::for_install_trigger(due.target_install, Some(target), triggering_event),
            _ => ResolutionContext::for_install_trigger(due.install, Some(card_id), triggering_event),
        };
        // A trigger's effect list is a `Sequence` in all but name.
        events.extend(evaluate_sequence(state, &triggered.effects, &mut effect_ctx, registry)?);
        if let Some(requirement) = &triggered.requirement {
            consume_requirement(state, requirement, card_side, &owner_ctx);
        }
    }
    Ok(events)
}

/// Resolves `effects` in order, as one printed sentence: stops at a
/// finished game, and when one of them parks something that spans future
/// `PlayerAction`s, queues the rest as a continuation pinned to the acting
/// card (see `Effect::Sequence`).
///
/// A slice, so that the two lists that are a `Sequence` in all but name —
/// a trigger's `effects` and an access interaction's — resolve through it
/// too. They each had a loop of their own: the trigger's stopped at a
/// parked decision and *dropped* the rest, and the access interaction's
/// did not stop at all, so Snare!'s "give the Runner 1 tag and do 3 net
/// damage" would have dealt the damage underneath the window its tag
/// opened — unannounced, since one thing is parked at a time, so a Net
/// Shield was never asked.
pub(crate) fn evaluate_sequence(
    state: &mut GameState,
    effects: &[Effect],
    ctx: &mut ResolutionContext<'_>,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = Vec::new();
    for (index, inner) in effects.iter().enumerate() {
        events.extend(evaluate_effect(state, inner, ctx, registry)?);
        // Stops at `GameOver`: Clearinghouse's `[DealDamage,
        // TrashCard(ThisCard)]` used to run its trash against a
        // finished game.
        if state.is_over() {
            break;
        }
        // That effect parked something spanning future
        // `PlayerAction`s: the rest of the sequence is queued as a
        // continuation on the deferred-trigger queue, pinned to the
        // acting card, and drains once the decision resolves — see
        // `Effect::Sequence`'s doc comment. Only a resolution with an
        // acting card can be pinned; a bare effect evaluation stops
        // here as it always did.
        if state.is_resolution_blocked() {
            let rest = &effects[index + 1..];
            if let (Some(card), false) = (ctx.acting_card, rest.is_empty()) {
                state.deferred_triggers.push(crate::rules::state::DeferredTrigger { announce: None,
                    card: card.clone(),
                    trigger: Trigger::OnPlay,
                    target: None,
                    install: ctx.acting_install,
                    target_install: None,
                    event: ctx.triggering_event.cloned(),
                    continuation: Some(Effect::Sequence(rest.to_vec())),
                    heard: Default::default(),
                    not_the_first_this_turn: false,
                    fired: 0,
                });
            }
            break;
        }
    }
    Ok(events)
}

/// Whether firing `due` right now would resolve anything: some
/// `TriggeredEffect` it names has no requirement, or one that passes.
///
/// A read, never a dry run — it consumes no per-turn flag and resolves no
/// effect. `dispatcher::offer_trigger_order` uses it so that a trigger whose
/// condition is already false ("from the root of **this server**", scored
/// elsewhere) is not something a player is asked to order.
pub(crate) fn would_fire(state: &GameState, registry: &CardRegistry, due: &DeferredTrigger) -> bool {
    if due.continuation.is_some() {
        return true;
    }
    let Some(card) = registry.get(&due.card) else { return false };
    let ctx = ResolutionContext::for_install_trigger(due.install, Some(&due.card), due.event.as_ref());
    let meant = |t: &&TriggeredEffect| {
        t.trigger == due.trigger
            && due.heard.admits(t.subject)
            && t.from_heap == (due.heard == crate::rules::state::Heard::FromHeap)
            && !(t.first_each_turn && due.not_the_first_this_turn)
            && listeners::when_admits(state, registry, t, card.side, due.install, due.event.as_ref())
    };
    // What already fired from this entry is not still to come.
    card.triggers.iter().filter(meant).skip(usize::from(due.fired)).any(|triggered| {
        triggered.requirement.as_ref().is_none_or(|requirement| check_requirement(state, requirement, card.side, &ctx, registry).is_ok())
    })
}

/// The installed card an `Effect::TrashCard` target names, as the thing
/// `rules::prevention` would park — `None` for a target that is not an
/// install (the top of a deck, the cards hosted on a host, a card trashing
/// itself out of a hand), whose trash no card in the pool can prevent.
fn installed_target(state: &GameState, registry: &CardRegistry, target: &CardTarget, ctx: &ResolutionContext<'_>) -> Option<WouldHappen> {
    let by = carried_out_by(registry, ctx);
    let corp = |position: usize| {
        let install = &state.corp.installed[position];
        WouldHappen::Trash { owner: Side::Corp, install: install.install_id, by }
    };
    let rig = |position: usize| {
        let install = &state.runner.rig[position];
        WouldHappen::Trash { owner: Side::Runner, install: install.install_id, by }
    };
    match target {
        CardTarget::ThisCard => acting_corp_position(state, ctx).map(corp).or_else(|| acting_rig_position(state, ctx).map(rig)),
        CardTarget::CorpInstalled { card, server } => {
            state.corp.installed.iter().position(|installed| installed.card == *card && installed.server == *server).map(corp)
        }
        CardTarget::HostIce | CardTarget::EncounteredIce => {
            let (host, _, _) = resolve_corp_installed_target(state, target, ctx).ok()?;
            state.corp.installed.iter().position(|installed| installed.install_id == host).map(corp)
        }
        CardTarget::RunnerRig(card) => state.runner.rig.iter().position(|installed| &installed.card == card).map(rig),
        CardTarget::TopOfStack { .. } | CardTarget::HostedOnThisCard | CardTarget::RandomFromHq => None,
    }
}

/// Trashes one installed card by its handle — what a parked trash resolves
/// to once nobody prevented it, and what a selection-trash the players were
/// asked about does. Exact with two copies installed, where
/// `trash_card`'s `CardTarget`s name a card and take the first. Nothing
/// happens if the install left play while the trash was parked.
pub(crate) fn trash_install(
    state: &mut GameState,
    registry: &CardRegistry,
    owner: Side,
    install: InstallId,
    by: Option<Side>,
) -> Result<Vec<GameEvent>, RulesError> {
    match owner {
        Side::Corp => {
            let Some((removed, mut events)) = uninstall::corp_install(state, registry, install)? else { return Ok(Vec::new()) };
            // A rezzed install was faceup on the table, and a card the
            // Runner is accessing has been seen whatever its rez state.
            let seen = removed.rezzed || runner_is_accessing(state, &removed.card);
            state.corp.archives.push(orient(removed.card.clone(), seen));
            events.push(GameEvent::CardTrashed { side: Side::Corp, card: removed.card, installed: true, by });
            events.extend(cascade_trash_hosted_programs(state, install));
            Ok(events)
        }
        Side::Runner => {
            let Some(position) = state.runner.rig.iter().position(|c| c.install_id == install) else { return Ok(Vec::new()) };
            let removed = state.runner.rig.remove(position);
            state.runner.heap.push(removed.card.clone());
            let mut events = vec![GameEvent::CardTrashed { side: Side::Runner, card: removed.card.clone(), installed: true, by }];
            events.extend(cascade_trash_hosted_on_rig_card(state, registry, &removed));
            Ok(events)
        }
    }
}

/// Resolves a `CardTarget` to a concrete Corp installed `(CardId,
/// ServerId)` — for `Effect::DerezCard`, which (unlike `TrashCard`) only
/// ever makes sense against a Corp install. `CorpInstalled`/`HostIce`
/// resolve directly (mirroring `trash_card`'s own `HostIce` resolution);
/// every other `CardTarget` variant errors `UnresolvedCardTarget`, since
/// none of them can ever name a Corp installed card.
fn resolve_corp_installed_target(
    state: &GameState,
    target: &CardTarget,
    ctx: &ResolutionContext<'_>,
) -> Result<(InstallId, CardId, ServerId), RulesError> {
    match target {
        CardTarget::CorpInstalled { card, server } => {
            let installed = state
                .corp
                .installed
                .iter()
                .find(|c| &c.card == card && c.server == *server)
                .ok_or_else(|| RulesError::CardNotInstalled { card: card.clone() })?;
            Ok((installed.install_id, card.clone(), *server))
        }
        CardTarget::HostIce => {
            // The acting trojan's host, by install — exact even with two
            // copies of the host ICE on the table.
            let host = acting_rig_card(state, ctx).and_then(|c| c.hosted_on_ice).ok_or(RulesError::UnresolvedCardTarget)?;
            let installed = state.find_corp_install(host).ok_or(RulesError::InstallNotFound(host))?;
            Ok((host, installed.card.clone(), installed.server))
        }
        // By install, off the run: two copies of one ice on a server are
        // two targets.
        CardTarget::EncounteredIce => {
            let run = state.active_run.as_ref().filter(|run| run.phase == crate::rules::run::RunPhase::EncounterIce).ok_or(RulesError::NotInEncounter)?;
            let ice = run.ice.get(run.position).ok_or(RulesError::NotInEncounter)?;
            let installed = state.find_corp_install(ice.install_id).ok_or(RulesError::InstallNotFound(ice.install_id))?;
            Ok((ice.install_id, installed.card.clone(), installed.server))
        }
        // A `PromptChooseCards::then` over the Corp's installs runs with the
        // chosen install as the acting one — Maglectric Rapid's "derez 1
        // installed Corp card" names it as `ThisCard`, the convention
        // `TrashCard(ThisCard)` already follows.
        CardTarget::ThisCard => {
            let install = ctx.acting_install.ok_or(RulesError::UnresolvedCardTarget)?;
            let installed = state.find_corp_install(install).ok_or(RulesError::UnresolvedCardTarget)?;
            Ok((install, installed.card.clone(), installed.server))
        }
        CardTarget::RunnerRig(_) | CardTarget::TopOfStack { .. } | CardTarget::HostedOnThisCard | CardTarget::RandomFromHq => {
            Err(RulesError::UnresolvedCardTarget)
        }
    }
}

/// Fires `Trigger::OnDamageDealt` for each `DamageTaken` among `events` —
/// the hook AU Co.: The Gold Standard in Clones counts damage with.
///
/// Called where the damage actually lands rather than from
/// `dispatch_event`'s own table, because `DamageTaken` is produced deep
/// inside `damage::apply_damage` and returned, never dispatched: only
/// `DamageAboutToResolve` (the prevention window) goes through the
/// dispatcher. Nothing fires once the damage has ended the game.
///
/// Dispatching a recorded event after the fact, rather than emitting it
/// where it is made, is fine for as long as nobody forgets to:
/// `dispatcher::audit` fails any action whose record holds a `DamageTaken`
/// that did not come through here. `damage::apply_damage` keeps returning
/// plain data because it is public API with no registry to dispatch with,
/// and moving the dispatch inside it would reorder the record.
pub(crate) fn dispatch_damage_taken(
    state: &mut GameState,
    registry: &CardRegistry,
    events: &[GameEvent],
) -> Result<Vec<GameEvent>, RulesError> {
    let mut fired = Vec::new();
    for event in events {
        if matches!(event, GameEvent::DamageTaken { .. }) && !state.is_over() {
            fired.extend(dispatcher::dispatch_event(state, registry, event)?);
        }
    }
    Ok(fired)
}

/// Credits gained by a resolving card, as opposed to by a click or a
/// trace payout: pays them, emits `CreditsGained`, and — when there is an
/// acting card to name — `GameEvent::AbilityGainedCredits` with its
/// dispatch, which is what The Zwicky Group: Invisible Hands draws off.
///
/// Every credit-gaining `Effect` goes through here, so a card that gains
/// through a counter (Regolith Mining License) or a formula (Ritual)
/// counts the same as a flat `GainCredits`. The dispatch is re-entrant in
/// principle — a reaction that itself gained credits would come back
/// round — and safe in practice because the one reader is gated
/// `OncePerTurn`; a second reader that gains credits must carry the same
/// gate.
///
/// A gain of 0 does not take place: "If a value aggregated in this way is
/// less than or equal to 0, instead the part of the effect associated with
/// that value does not take place at all" (CR 9.12.2b). It used to emit a
/// gain of 0, which The Zwicky Group heard as the card gaining credits and
/// drew for: Bigger Picture against a Runner with no credits to lose.
fn gain_credits_from_ability(
    state: &mut GameState,
    registry: &CardRegistry,
    side: Side,
    amount: u32,
    ctx: &ResolutionContext<'_>,
) -> Result<Vec<GameEvent>, RulesError> {
    if amount == 0 {
        return Ok(Vec::new());
    }
    state.resources_mut(side).credits = state.resources(side).credits.gain(amount);
    let mut events = vec![GameEvent::CreditsGained { side, amount }];
    // The card whose text it is: a selection's `then` resolves *as* the
    // card chosen, and realloc()'s credits are an operation's, not the
    // ice's it derezzes (The Zwicky Group hears "an agenda or operation").
    if let Some(card) = ctx.prompting_card.or(ctx.acting_card) {
        let gained = GameEvent::AbilityGainedCredits { side, card: card.clone() };
        dispatcher::emit(state, registry, &mut events, gained)?;
    }
    Ok(events)
}

/// `Effect::AddCounters`/`RemoveCounters`'s shared implementation:
/// saturating-applies `delta` (negative to remove) to `acting_card`'s
/// `counters` field, wherever it's currently installed/rigged. Mirrors
/// `trash_this_card`'s "try Corp installed, then Runner rig" search order,
/// but doesn't need `trash_this_card`'s hand/deck arms — counters only ever
/// live on an installed/rigged card, never in a hand or deck zone.
pub(crate) fn modify_counters(
    state: &mut GameState,
    ctx: &ResolutionContext<'_>,
    delta: i64,
) -> Result<Vec<GameEvent>, RulesError> {
    let card_id = ctx.acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();

    let counters = if let Some(position) = acting_corp_position(state, ctx) {
        &mut state.corp.installed[position].counters
    } else if let Some(position) = acting_rig_position(state, ctx) {
        &mut state.runner.rig[position].counters
    } else if let Some(position) = acting_scored_position(state, ctx) {
        &mut state.corp.scored_agendas[position].agenda_counters
    } else if acting_is_corp_identity(state, ctx) {
        &mut state.corp.identity_counters
    } else if let Some(run) = state.active_run.as_mut().filter(|run| acting_is_run_event(run, ctx)) {
        &mut run.event_counters
    } else {
        return Err(RulesError::CardNotEligibleForCounters(card_id));
    };
    *counters = (i64::from(*counters) + delta).max(0) as u32;

    let event = if delta >= 0 {
        GameEvent::CountersAdded { card: card_id.clone(), amount: delta as u32 }
    } else {
        GameEvent::CountersRemoved { card: card_id.clone(), amount: (-delta) as u32 }
    };
    Ok(vec![event])
}

pub(crate) fn trash_card(
    state: &mut GameState,
    registry: &CardRegistry,
    target: &CardTarget,
    ctx: &ResolutionContext<'_>,
) -> Result<Vec<GameEvent>, RulesError> {
    let by = carried_out_by(registry, ctx);
    match target {
        CardTarget::ThisCard => {
            ctx.acting_card.ok_or(RulesError::MissingActingCardContext)?;
            trash_this_card(state, registry, ctx, by)
        }

        CardTarget::CorpInstalled { card, server } => {
            let position = state
                .corp
                .installed
                .iter()
                .position(|installed| installed.card == *card && installed.server == *server)
                .ok_or_else(|| RulesError::CardNotInstalled { card: card.clone() })?;
            let install = state.corp.installed[position].install_id;
            let Some((removed, mut events)) = uninstall::corp_install(state, registry, install)? else { return Ok(Vec::new()) };
            // A rezzed install was face-up on the table, so the Runner has
            // already seen it; an unrezzed one they never did.
            let seen = removed.rezzed || runner_is_accessing(state, card);
            state.corp.archives.push(orient(card.clone(), seen));
            events.push(GameEvent::CardTrashed { side: Side::Corp, card: card.clone(), installed: true, by });
            events.extend(cascade_trash_hosted_programs(state, install));
            Ok(events)
        }

        // "The ICE I'm hosted on" — resolve `acting_card`'s host, then
        // trash it exactly like `CorpInstalled` (cascade included, since
        // it recurses into that same arm).
        CardTarget::HostIce => {
            let (_, host, server) = resolve_corp_installed_target(state, target, ctx)?;
            trash_card(state, registry, &CardTarget::CorpInstalled { card: host, server }, ctx)
        }

        // By its handle, which the run holds.
        CardTarget::EncounteredIce => {
            let (install, _, _) = resolve_corp_installed_target(state, target, ctx)?;
            trash_install(state, registry, Side::Corp, install, by)
        }

        // Handled by `evaluate_effect`'s `TrashCard` arm before it gets
        // here (it needs the registry to route each card home).
        CardTarget::HostedOnThisCard => Err(RulesError::UnresolvedCardTarget),

        CardTarget::RunnerRig(card) => {
            let position = state
                .runner
                .rig
                .iter()
                .position(|c| &c.card == card)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: card.clone() })?;
            let removed = state.runner.rig.remove(position);
            state.runner.heap.push(removed.card.clone());
            let mut events = vec![GameEvent::CardTrashed { side: Side::Runner, card: card.clone(), installed: true, by }];
            events.extend(cascade_trash_hosted_on_rig_card(state, registry, &removed));
            Ok(events)
        }

        CardTarget::TopOfStack { side, zone } => {
            // Split by side rather than sharing one `(deck, pile)` pair:
            // Archives carries a facedown flag (`ArchivedCard`) while the
            // Heap is a plain `Vec<CardId>`, so the two piles no longer have
            // the same type. A card milled off R&D was never seen by the
            // Runner, hence facedown.
            let popped = match (side, zone) {
                (Side::Corp, StackZone::RAndD) => state.corp.r_and_d.pop().inspect(|card| {
                    state.corp.archives.push(ArchivedCard::facedown(card.clone()));
                }),
                (Side::Runner, StackZone::Stack) => state.runner.stack.pop().inspect(|card| {
                    state.runner.heap.push(card.clone());
                }),
                // Corp has no Stack, Runner has no R&D — no card ever
                // occupies this mismatched combination's "top".
                _ => return Err(RulesError::EmptyZone { side: *side, zone: *zone }),
            };
            // An empty pile is nothing to trash, not a failure — what
            // `MillRnDAmount` always said. As an error it refused the whole
            // resolution: Noise installing a virus against an empty R&D
            // parked a trigger order neither choice of which could be
            // applied, and the Runner had no legal action (seed 181 of
            // Hostile Bid against Pay As You Go, the first deck on Noise).
            Ok(popped.map(|card| GameEvent::CardTrashed { side: *side, card, installed: false, by }).into_iter().collect())
        }

        // Drawn with the state's own PRNG, as `Cost::TrashRandomFromHq` is,
        // and facedown: nobody chose it and the Runner has not seen it.
        CardTarget::RandomFromHq => {
            if state.corp.hq.is_empty() {
                return Ok(Vec::new());
            }
            let index = (state.next_u64() % state.corp.hq.len() as u64) as usize;
            let card = state.corp.hq.remove(index);
            state.corp.archives.push(ArchivedCard::facedown(card.clone()));
            Ok(vec![GameEvent::CardTrashed { side: Side::Corp, card, installed: false, by }])
        }
    }
}

/// Whenever a Corp installed card (`host_card_id`) leaves play, any
/// Trojan Program hosted on it (`InstalledRunnerCard::hosted_on_ice ==
/// Some(host_card_id)`) is trashed too — e.g. trashing the ICE Botulus is
/// hosted on takes Botulus with it. Called from every site that removes a
/// piece of ICE from `CorpState::installed` — `trash_card`, `trash_this_card`
/// and `pending_choice::remove_installed_card` (a selection-trash used to
/// skip it, stranding the trojan with a dangling `hosted_on_ice` that
/// Tranquilizer's `DerezCard(HostIce)` then failed on). A no-op (returns no
/// events) if nothing is hosted on `host_card_id`, so this is zero-overhead
/// for the overwhelming majority of Corp cards that never host anything.
///
/// Keyed by the host's `InstallId`, so with two copies of one ICE installed
/// only the trojans on the copy that left go — `hosted_on_ice` used to be a
/// `CardId` and both copies' trojans went together.
/// `Effect::TrashCard(CardTarget::HostedOnThisCard)` — Bling's "trash all
/// hosted cards". Each card goes to its owner's discard pile
/// (`trash_hosted_card`).
fn trash_hosted_cards(state: &mut GameState, registry: &CardRegistry, ctx: &ResolutionContext<'_>) -> Result<Vec<GameEvent>, RulesError> {
    let position = acting_rig_position(state, ctx).ok_or(RulesError::UnresolvedCardTarget)?;
    let hosted = std::mem::take(&mut state.runner.rig[position].hosted_cards);
    let by = carried_out_by(registry, ctx);
    Ok(hosted.into_iter().map(|card| trash_hosted_card(state, registry, card, by)).collect())
}

/// Trashes one card hosted uninstalled on a rig card to its *owner's*
/// discard pile: "Trashing is the act of moving an object to its owner's
/// discard pile" (CR 1.19.1). A Runner card goes to the heap; a Corp card
/// — Detente hosts one from HQ — goes faceup to Archives, since it sat
/// faceup on the table. One function for Bling's "trash all hosted cards"
/// and for the host leaving the rig, which sent a Corp card to the heap.
fn trash_hosted_card(state: &mut GameState, registry: &CardRegistry, card: CardId, by: Option<Side>) -> GameEvent {
    let side = registry.get(&card).map_or(Side::Runner, |def| def.side);
    match side {
        Side::Runner => state.runner.heap.push(card.clone()),
        Side::Corp => state.corp.archives.push(ArchivedCard::faceup(card.clone())),
    }
    GameEvent::CardTrashed { side, card, installed: false, by }
}

/// The rig-side twin of `cascade_trash_hosted_programs`: when the rig card
/// `host` leaves the rig, every card hosted on it
/// (`InstalledRunnerCard::hosted_on_rig_card == Some(host)`) is trashed too
/// — GAMEDRAGON™ Pro goes with the icebreaker it sits on. Called from
/// every site that removes a rig card. A no-op for the overwhelming
/// majority of rig cards, which host nothing.
pub(crate) fn cascade_trash_hosted_on_rig_card(state: &mut GameState, registry: &CardRegistry, removed: &InstalledRunnerCard) -> Vec<GameEvent> {
    let host = removed.install_id;
    let mut events = Vec::new();
    // Cards hosted *uninstalled* on the host (Madani's programs, Detente's
    // Corp cards) are trashed with it too — they were in no other zone,
    // and they left the rig inside `removed`, which is why this takes the
    // card and not its id.
    for hosted in &removed.hosted_cards {
        // The rules trash what its host took with it, not a player.
        events.push(trash_hosted_card(state, registry, hosted.clone(), None));
    }
    while let Some(position) = state.runner.rig.iter().position(|c| c.hosted_on_rig_card == Some(host)) {
        let removed = state.runner.rig.remove(position);
        state.runner.heap.push(removed.card.clone());
        events.push(GameEvent::CardTrashed { side: Side::Runner, card: removed.card, installed: true, by: None });
    }
    events
}

pub(crate) fn cascade_trash_hosted_programs(state: &mut GameState, host: InstallId) -> Vec<GameEvent> {
    let mut events = Vec::new();
    while let Some(position) = state.runner.rig.iter().position(|c| c.hosted_on_ice == Some(host)) {
        let removed = state.runner.rig.remove(position);
        state.runner.heap.push(removed.card.clone());
        events.push(GameEvent::CardTrashed { side: Side::Runner, card: removed.card, installed: true, by: None });
    }
    events
}

/// Locates `card_id` wherever it currently sits — Corp installed, HQ, R&D,
/// or Runner Rig/Grip — and moves it to that side's discard pile, for
/// `CardTarget::ThisCard`/`Cost::TrashSelf` self-reference resolution
/// (unlike `CardTarget::CorpInstalled`/`RunnerRig`, the zone isn't known
/// ahead of time). Not found in any of those zones (e.g. already trashed by
/// an earlier effect in the same resolution, or accessed straight from
/// Archives) is a no-op, mirroring `run::access::move_to_archives`'s
/// existing "already there" leniency, rather than erroring.
/// `ArchivedCard::faceup`/`facedown` selected by whether the Runner has
/// seen the card — the one place that decision is spelled out, so every
/// trash path reads the same way.
fn orient(card: CardId, seen_by_runner: bool) -> ArchivedCard {
    if seen_by_runner { ArchivedCard::faceup(card) } else { ArchivedCard::facedown(card) }
}

/// Whether the Runner is currently accessing `card_id` (it's the pending
/// choice, or already resolved earlier in this same access). Such a card has
/// been seen even if it was never rezzed — e.g. an unrezzed ambush that
/// trashes itself on access — so it lands faceup in Archives rather than
/// following the plain rezzed-or-not rule.
fn runner_is_accessing(state: &GameState, card_id: &CardId) -> bool {
    state
        .active_run
        .as_ref()
        .and_then(|run| run.access_state.as_ref())
        .is_some_and(|access| {
            access.currently_accessing.as_ref() == Some(card_id)
                || access.resolved_cards.contains(card_id)
                || matches!(&access.phase, AccessPhase::PendingChoice { card_id: pending, .. } if pending == card_id)
        })
}

/// Puts `card` into the Corp's score area "as an agenda" (CR 10.1.3) — the
/// one place both ways in go through: Myōshu's own text and Word on the
/// Street's additional cost. The caller has taken the card out of wherever
/// it was. A fresh handle, since the card is a new object where it lands;
/// the stored tally moves as a score does, by the points the addition gave
/// it, which may be negative.
fn add_to_score_area_as_agenda(state: &mut GameState, side: Side, card: CardId, as_agenda: crate::dsl::AsAgenda) -> GameEvent {
    let install_id = state.allocate_install_id();
    let entry = crate::rules::state::ScoredAgenda {
        card: card.clone(),
        install_id,
        agenda_counters: 0,
        scored_on_turn: state.turn,
        installed_on_scoring_turn: false,
        as_agenda: Some(as_agenda),
    };
    let (scored, resources) = match side {
        Side::Corp => (&mut state.corp.scored_agendas, &mut state.corp.resources),
        Side::Runner => (&mut state.runner.scored_agendas, &mut state.runner.resources),
    };
    scored.push(entry);
    resources.agenda_points = resources.agenda_points.gain(as_agenda.points);
    GameEvent::AddedToScoreAreaAsAgenda { side, card, points: as_agenda.points, subtype: as_agenda.subtype }
}

/// Adds `card`, an agenda out of whatever Corp zone it just left, to the
/// Corp's score area as itself (CR 1.17.3e): worth what it prints, never
/// scored (`ScoredAgenda::scored_on_turn` 0), so nothing that hears a score
/// hears it. The checkpoint after the action is what a win by it waits for.
pub(crate) fn add_agenda_to_score_area(state: &mut GameState, registry: &CardRegistry, card: CardId) -> GameEvent {
    let agenda_points = registry.get(&card).map_or(0, |def| crate::rules::continuous::agenda_points_in(state, registry, def, Side::Corp));
    let install_id = state.allocate_install_id();
    state.corp.scored_agendas.push(crate::rules::state::ScoredAgenda {
        card: card.clone(),
        install_id,
        agenda_counters: 0,
        scored_on_turn: 0,
        installed_on_scoring_turn: false,
        as_agenda: None,
    });
    state.corp.resources.agenda_points = state.corp.resources.agenda_points.gain(agenda_points as i32);
    GameEvent::AgendaAddedToScoreArea { card, agenda_points }
}

/// The positions in the Corp's score area that may be forfeited: every one
/// but a card added "as an agenda" with "You cannot forfeit this agenda."
/// (Word on the Street). The one list `Cost::Forfeit`'s affordability and
/// its payment read.
fn forfeitable(state: &GameState) -> Vec<usize> {
    (0..state.corp.scored_agendas.len())
        .filter(|&position| state.corp.scored_agendas[position].as_agenda.is_none_or(|as_agenda| !as_agenda.cannot_forfeit))
        .collect()
}

/// Removes the acting install from the game — `Cost::RemoveSelfFromGame`
/// (Spin Doctor, Malandragem) and `Effect::RemoveFromGame(ThisCard)`
/// (Malandragem's "when it is empty"). Deliberately not a discard pile: a
/// removed card is gone for good. What a rig card hosted is trashed with it,
/// as when a host is trashed.
fn remove_this_card_from_game(state: &mut GameState, registry: &CardRegistry, ctx: &ResolutionContext<'_>) -> Result<Vec<GameEvent>, RulesError> {
    let card_id = ctx.acting_card.ok_or(RulesError::MissingActingCardContext)?.clone();
    if let Some(position) = acting_corp_position(state, ctx) {
        let install = state.corp.installed[position].install_id;
        let (_, mut events) = uninstall::corp_install(state, registry, install)?.ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
        state.corp.removed_from_game.push(card_id.clone());
        events.push(GameEvent::CardRemovedFromGame { side: Side::Corp, card: card_id });
        return Ok(events);
    }
    let position = acting_rig_position(state, ctx).ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
    let removed = state.runner.rig.remove(position);
    state.runner.removed_from_game.push(card_id.clone());
    let mut events = vec![GameEvent::CardRemovedFromGame { side: Side::Runner, card: card_id }];
    events.extend(cascade_trash_hosted_on_rig_card(state, registry, &removed));
    Ok(events)
}

pub(crate) fn trash_this_card(state: &mut GameState, registry: &CardRegistry, ctx: &ResolutionContext<'_>, by: Option<Side>) -> Result<Vec<GameEvent>, RulesError> {
    let card_id = &ctx.acting_card.ok_or(RulesError::MissingActingCardContext)?.clone();
    if let Some(position) = acting_corp_position(state, ctx) {
        // Same rezzed-or-not rule as `CardTarget::CorpInstalled` above,
        // widened for the access case: a card the Runner is accessing right
        // now has been seen regardless of rez state.
        let install = state.corp.installed[position].install_id;
        let Some((removed, mut events)) = uninstall::corp_install(state, registry, install)? else { return Ok(Vec::new()) };
        let seen = removed.rezzed || runner_is_accessing(state, card_id);
        state.corp.archives.push(orient(card_id.clone(), seen));
        events.push(GameEvent::CardTrashed { side: Side::Corp, card: card_id.clone(), installed: true, by });
        events.extend(cascade_trash_hosted_programs(state, install));
        return Ok(events);
    }
    if ctx.acting_install.is_some() && acting_rig_position(state, ctx).is_none() {
        // See the matching guard below the rig arm.
        return Ok(Vec::new());
    }
    if let Some(position) = state.corp.hq.iter().position(|c| c == card_id) {
        // Trashed straight out of the Corp's hand — facedown, unless the
        // Runner is accessing it right now (an HQ ambush that trashes
        // itself on access, say), in which case they have seen it.
        let seen = runner_is_accessing(state, card_id);
        state.corp.hq.remove(position);
        state.corp.archives.push(orient(card_id.clone(), seen));
        return Ok(vec![GameEvent::CardTrashed { side: Side::Corp, card: card_id.clone(), installed: false, by }]);
    }
    if let Some(position) = state.corp.r_and_d.iter().position(|c| c == card_id) {
        // Milled off R&D — facedown on the same reasoning as HQ above.
        let seen = runner_is_accessing(state, card_id);
        state.corp.r_and_d.remove(position);
        state.corp.archives.push(orient(card_id.clone(), seen));
        return Ok(vec![GameEvent::CardTrashed { side: Side::Corp, card: card_id.clone(), installed: false, by }]);
    }
    if let Some(position) = acting_rig_position(state, ctx) {
        let removed = state.runner.rig.remove(position);
        state.runner.heap.push(removed.card.clone());
        let mut events = vec![GameEvent::CardTrashed { side: Side::Runner, card: card_id.clone(), installed: true, by }];
        events.extend(cascade_trash_hosted_on_rig_card(state, registry, &removed));
        return Ok(events);
    }
    // An install that has already left play is gone: its hand/deck
    // namesakes are other cards, and trashing one of them would be exactly
    // the sibling-copy aliasing this context exists to end.
    if ctx.acting_install.is_some() {
        return Ok(Vec::new());
    }
    if let Some(position) = state.runner.grip.iter().position(|c| c == card_id) {
        state.runner.grip.remove(position);
        state.runner.heap.push(card_id.clone());
        return Ok(vec![GameEvent::CardTrashed { side: Side::Runner, card: card_id.clone(), installed: false, by }]);
    }
    Ok(Vec::new())
}

/// Pays `cost` on `side`'s behalf, mutating `state` in place. Kept as a
/// function separate from `evaluate_effect` — mirroring `AbilityDef`
/// itself already modeling cost and effect as two separate fields — so a
/// future dispatch path calls `pay_cost` then, only on success,
/// `evaluate_effect`, matching real Netrunner's "costs are paid first,
/// then the ability resolves" structure.
///
/// `acting_card` identifies the card whose cost this is — only read by
/// `Cost::TrashSelf`, resolved via `trash_this_card`
/// (`RulesError::MissingActingCardContext` if `None`); every other `Cost`
/// variant ignores it.
/// Whether `side` could pay `cost` right now, without paying it.
///
/// A non-mutating mirror of [`pay_cost`]'s preconditions. **The two must
/// agree** — same pairing discipline as `run::check_run_may_begin` and
/// `Effect::PromptChooseServer`'s park-time check. Disagreement here is
/// only wasteful rather than fatal (a `WindowCheckpoint::PostAction`
/// opening with nothing to do still closes on two passes), but it is the
/// kind of drift that gets expensive later, so add new `Cost` variants to
/// both or neither.
///
/// Costs with no resource precondition (`TrashSelf`, `RemoveSelfFromGame`,
/// `TakeTags`, `ClearTags`) are always payable and answer `true`.
pub(crate) fn cost_is_affordable(
    state: &GameState,
    registry: &CardRegistry,
    side: Side,
    cost: &Cost,
    purpose: Purpose<'_>,
    ctx: &ResolutionContext<'_>,
) -> bool {
    match cost {
        // The scan the payment spends from — it used to be a sum of its
        // own, which counted bad publicity and forgot the run's credits.
        Cost::Credits(amount) => payment::available(state, registry, side, purpose) >= *amount,
        Cost::CreditsFrom { amount, from } => {
            payment::available_from(state, registry, side, purpose, Some(from)) >= resolve_amount(amount, ctx, state, registry)
        }
        Cost::CreditsAmount(amount) => payment::available(state, registry, side, purpose) >= resolve_amount(amount, ctx, state, registry),
        // X may be 0.
        Cost::CreditsX { .. } => true,
        Cost::Clicks(amount) | Cost::LoseClicks(amount) => state.resources(side).clicks.0 >= *amount,
        // A run the Runner is in, and nothing else: there is no "cannot
        // jack out" in the pool.
        Cost::JackOut => side == Side::Runner && state.active_run.is_some(),
        Cost::RemoveCounters(amount) => counters_of(state, ctx).is_some_and(|counters| counters >= *amount),
        Cost::RemoveAdvancementCounters(amount) => acting_corp_install(state, ctx).is_some_and(|installed| installed.advancement_tokens >= *amount),
        // Word on the Street's card is its own price: payable while it is
        // in the rig.
        Cost::AddToScoreAreaAsAgenda(_) => acting_rig_position(state, ctx).is_some(),
        // Any one alternative being payable is enough — the payer picks.
        Cost::AnyOf(options) => options.iter().any(|option| cost_is_affordable(state, registry, side, option, purpose, ctx)),
        // Every part must be payable — read against the same state, which
        // is exact for the shapes in the pool (clicks plus a self-trash
        // draw on different resources).
        Cost::AllOf(parts) => parts.iter().all(|part| cost_is_affordable(state, registry, side, part, purpose, ctx)),
        Cost::RemoveTags(amount) => state.runner.tags >= *amount,
        Cost::SufferDamage(_, amount) => state.runner.grip.len() >= *amount as usize,
        Cost::Forfeit(count) => side == Side::Corp && forfeitable(state).len() >= *count as usize,
        // The same scan the payment picks from.
        Cost::Trash { from, filter, count, .. } => {
            crate::rules::pending_choice::eligible_positions(state, registry, side, from, filter, ctx.acting_install, ctx.acting_card).len() >= *count as usize
        }
        Cost::Derez { filter, count } => derez_eligible(state, registry, side, filter, ctx).len() >= *count as usize,
        Cost::TrashSelf | Cost::RemoveSelfFromGame | Cost::TakeTags(_) | Cost::ClearTags => true,
        Cost::TakeBadPublicity(_) => side == Side::Corp,
        Cost::TrashRandomFromHq(count) => state.corp.hq.len() as u32 >= *count,
        Cost::RevealSelf | Cost::AddSelfToHq => side == Side::Corp && acting_corp_install(state, ctx).is_some(),
        Cost::DerezSelf => side == Side::Corp && acting_corp_install(state, ctx).is_some_and(|installed| installed.is_rezzed(registry)),
        // Payable while the card is in its owner's hand.
        Cost::RevealAndTrashSelf => ctx.acting_card.is_some_and(|card| match side {
            Side::Corp => state.corp.hq.contains(card),
            Side::Runner => state.runner.grip.contains(card),
        }),
    }
}

/// The positions in the Corp's installs a `Cost::Derez` may take: those
/// `filter` admits that are rezzed. The Corp's own, so a Runner payer has
/// none.
fn derez_eligible(state: &GameState, registry: &CardRegistry, side: Side, filter: &crate::dsl::CardFilter, ctx: &ResolutionContext<'_>) -> Vec<usize> {
    if side != Side::Corp {
        return Vec::new();
    }
    crate::rules::pending_choice::eligible_positions(state, registry, side, &crate::dsl::CardZoneRef::OwnInstalled, filter, ctx.acting_install, ctx.acting_card)
        .into_iter()
        .filter(|&position| state.corp.installed[position].is_rezzed(registry))
        .collect()
}

/// `pay_cost_ctx` for a payer with no install to name — an operation or
/// event being played, an install being paid for. Anything paying *as an
/// installed card* (an activated ability, a parked choice's cost) must use
/// `pay_cost_ctx` with the install, or `Cost::RemoveCounters`/`TrashSelf`
/// act on the first copy of the card.
///
/// `purpose` is what the cost is being paid *for*, which only the caller
/// knows and which decides where credits may come from — see
/// `rules::payment`. It reaches nothing but `Cost::Credits`.
pub(crate) fn pay_cost(
    state: &mut GameState,
    registry: &CardRegistry,
    side: Side,
    cost: &Cost,
    purpose: Purpose<'_>,
    acting_card: Option<&CardId>,
) -> Result<Vec<GameEvent>, RulesError> {
    pay_cost_ctx(state, registry, side, cost, purpose, &ResolutionContext::for_card(acting_card))
}

pub(crate) fn pay_cost_ctx(
    state: &mut GameState,
    registry: &CardRegistry,
    side: Side,
    cost: &Cost,
    purpose: Purpose<'_>,
    ctx: &ResolutionContext<'_>,
) -> Result<Vec<GameEvent>, RulesError> {
    let acting_card = ctx.acting_card;
    match cost {
        // Where the credits come from is `rules::payment`'s: the run's
        // pools, an identity's and a card's hosted credits are sources
        // beside the credit pool, and which of them this payment may use
        // is read off `purpose`.
        Cost::Credits(amount) => payment::pay(state, registry, side, *amount, purpose),
        Cost::CreditsFrom { amount, from } => {
            let amount = resolve_amount(amount, ctx, state, registry);
            payment::pay_from(state, registry, side, amount, purpose, Some(from))
        }
        Cost::CreditsAmount(amount) => {
            let amount = resolve_amount(amount, ctx, state, registry);
            payment::pay(state, registry, side, amount, purpose)
        }

        // X is named before it is paid (CR 1.16.2c): asked by replay,
        // no more than the printed bound or what could be spent.
        Cost::CreditsX { max } => {
            let most = resolve_amount(max, ctx, state, registry)
                .min(payment::available(state, registry, side, purpose))
                .min(crate::rules::action_mask::MAX_CHOSEN_NUMBER);
            if state.payment_answers.is_empty() {
                return Err(RulesError::PaymentChoiceNeeded { side, amount: most, question: payment::Ask::X { max: most } });
            }
            let x = state.payment_answers.remove(0);
            if x > most {
                return Err(RulesError::ChosenNumberOutOfRange { amount: x, min: 0, max: most });
            }
            payment::pay(state, registry, side, x, purpose)
        }

        Cost::Clicks(amount) => {
            let clicks = state.resources(side).clicks;
            let spent = clicks.spend(*amount).ok_or(RulesError::NotEnoughClicks {
                side,
                available: clicks.0,
                requested: *amount,
            })?;
            state.resources_mut(side).clicks = spent;
            Ok(std::iter::repeat_n(GameEvent::ClickSpent { side }, *amount as usize).collect())
        }

        // Lost, not spent: nothing about the ability is an action.
        Cost::LoseClicks(amount) => {
            let clicks = state.resources(side).clicks;
            let left = clicks.spend(*amount).ok_or(RulesError::NotEnoughClicks { side, available: clicks.0, requested: *amount })?;
            state.resources_mut(side).clicks = left;
            Ok(vec![GameEvent::ClicksLost { side, amount: *amount }])
        }

        // The run ends as a jack-out, which the payer dispatches with the
        // rest of its cost's events (`dispatch_cost_events`): "when a run
        // ends" hears it.
        Cost::JackOut => {
            let encounter = state.active_run.as_ref().and_then(run::encounter_ends);
            let run = run::end_run(state).ok_or(RulesError::NoActiveRun)?;
            Ok(encounter.into_iter().chain([GameEvent::RunJackedOut { server: run.server }]).collect())
        }

        // The payer trashes it, and dispatches that with the rest of its
        // cost's events (`dispatch_cost_events`).
        Cost::TrashSelf => {
            acting_card.ok_or(RulesError::MissingActingCardContext)?;
            trash_this_card(state, registry, ctx, Some(side))
        }

        // Out of the hand, faceup: it was shown as it went (CR 4.4.6b). The
        // payer dispatches the trash with the rest of its cost's events.
        Cost::RevealAndTrashSelf => {
            let card = acting_card.ok_or(RulesError::MissingActingCardContext)?.clone();
            let hand = match side {
                Side::Corp => &mut state.corp.hq,
                Side::Runner => &mut state.runner.grip,
            };
            let position = hand.iter().position(|c| *c == card).ok_or_else(|| RulesError::CardNotInHand { side, card: card.clone() })?;
            hand.remove(position);
            match side {
                Side::Corp => state.corp.archives.push(ArchivedCard::faceup(card.clone())),
                Side::Runner => state.runner.heap.push(card.clone()),
            }
            Ok(vec![GameEvent::CardRevealed { side, card: card.clone() }, GameEvent::CardTrashed { side, card, installed: false, by: Some(side) }])
        }

        Cost::TrashRandomFromHq(count) => {
            if (state.corp.hq.len() as u32) < *count {
                return Err(RulesError::NotEnoughCardsInHq { required: *count, available: state.corp.hq.len() as u32 });
            }
            let mut events = Vec::new();
            for _ in 0..*count {
                let index = (state.next_u64() % state.corp.hq.len() as u64) as usize;
                let card = state.corp.hq.remove(index);
                // Revealed as it is trashed, so it lands faceup.
                state.corp.archives.push(ArchivedCard::faceup(card.clone()));
                events.push(GameEvent::CardTrashed { side: Side::Corp, card, installed: false, by: Some(side) });
            }
            Ok(events)
        }

        Cost::RemoveSelfFromGame => remove_this_card_from_game(state, registry, ctx),

        // Shown and left as it was (CR 1.21.3a): still facedown, and
        // remembered by the Runner as an accessed card is.
        Cost::RevealSelf => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?.clone();
            let position = acting_corp_position(state, ctx).ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            state.corp.installed[position].seen_by_runner = true;
            Ok(vec![GameEvent::CardRevealed { side: Side::Corp, card: card_id }])
        }

        Cost::DerezSelf => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?.clone();
            let installed = acting_corp_install_mut(state, ctx).filter(|installed| installed.rezzed).ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            installed.rezzed = false;
            Ok(vec![GameEvent::CardDerezzed { install: installed.install_id, card: Some(card_id) }])
        }

        Cost::AddSelfToHq => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?.clone();
            let install = acting_corp_install(state, ctx).map(|installed| installed.install_id).ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            let (removed, mut events) = uninstall::corp_install(state, registry, install)?.ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            state.corp.hq.push(removed.card.clone());
            events.push(GameEvent::CardAddedToHand { side: Side::Corp, card: Some(removed.card), install, faceup: removed.rezzed });
            Ok(events)
        }

        Cost::ClearTags => {
            state.runner.tags = 0;
            Ok(vec![GameEvent::TagsCleared { side: Side::Runner, by: side }])
        }

        Cost::RemoveTags(amount) => {
            if state.runner.tags < *amount {
                return Err(RulesError::RunnerNotTagged);
            }
            state.runner.tags -= *amount;
            // Returned, not dispatched: the payer dispatches its cost's
            // events after the effect (`dispatch_cost_events`).
            Ok(vec![GameEvent::TagsRemoved { side: Side::Runner, amount: *amount, by: side }])
        }

        Cost::SufferDamage(damage_type, amount) => {
            if state.runner.grip.len() < *amount as usize {
                return Err(RulesError::NotEnoughCardsInGrip { required: *amount, available: state.runner.grip.len() as u32 });
            }
            // Straight to the damage, never through `prevention::would`: a
            // cost is not prevented (1.16.1a). Its `DamageTaken` is
            // dispatched by the payer (`dispatch_cost_events`).
            // The Runner chose to suffer it, so it is theirs (CR 10.4.1).
            Ok(crate::rules::damage::apply_damage(state, *damage_type, *amount as usize, Some(Side::Runner)).0)
        }

        Cost::Forfeit(count) => {
            // Only the Corp's score area holds agendas with a handle each;
            // no Runner card in the pool forfeits.
            let eligible = forfeitable(state);
            if side != Side::Corp || eligible.len() < *count as usize {
                return Err(RulesError::NotEnoughAgendasToForfeit { required: *count, available: eligible.len() as u32 });
            }
            let zone = crate::dsl::CardZoneRef::OwnScoreArea;
            let picked = crate::rules::pending_choice::pick_for_cost(state, side, &zone, &eligible, *count, None)?;
            // Resolved to handles before any agenda leaves, which would
            // shift the positions still to be read.
            let installs: Vec<InstallId> = picked.iter().map(|&p| state.corp.scored_agendas[p].install_id).collect();
            let mut events = Vec::new();
            for install in installs {
                let Some(position) = state.corp.scored_agendas.iter().position(|s| s.install_id == install) else { continue };
                let forfeited = state.corp.scored_agendas.remove(position);
                // "The sum of all agenda points on agendas in a player's
                // score area is that player's score" (CR 1.17.1), so a
                // forfeited agenda takes its points with it. The win check
                // recounts the score area and was right; this is the
                // number the view, the HUD and the bots read, which kept
                // the forfeited points.
                let points = crate::rules::win::scored_value(state, registry, &forfeited, Side::Corp);
                state.corp.resources.agenda_points = state.corp.resources.agenda_points.gain(-points);
                // Out of the game rather than to Archives (it was never on
                // the table), taking its counters with it. Returned, not
                // dispatched: the payer dispatches (`dispatch_cost_events`),
                // which is how Greenmail hears its own forfeit.
                state.corp.removed_from_game.push(forfeited.card.clone());
                events.push(GameEvent::AgendaForfeited { card: forfeited.card.clone() });
                events.push(GameEvent::CardRemovedFromGame { side: Side::Corp, card: forfeited.card });
            }
            Ok(events)
        }

        Cost::Trash { from, filter, count, reveal } => {
            let eligible = crate::rules::pending_choice::eligible_positions(state, registry, side, from, filter, ctx.acting_install, ctx.acting_card);
            if eligible.len() < *count as usize {
                return Err(RulesError::NotEnoughCardsToTrash { required: *count, available: eligible.len() as u32 });
            }
            let picked = crate::rules::pending_choice::pick_for_cost(state, side, from, &eligible, *count, ctx.acting_install)?;
            crate::rules::pending_choice::trash_as_cost(state, registry, side, from, &picked, *reveal, ctx.acting_install)
        }

        Cost::Derez { filter, count } => {
            let eligible = derez_eligible(state, registry, side, filter, ctx);
            if eligible.len() < *count as usize {
                return Err(RulesError::NotEnoughCardsToDerez { required: *count, available: eligible.len() as u32 });
            }
            let zone = crate::dsl::CardZoneRef::OwnInstalled;
            let picked = crate::rules::pending_choice::pick_for_cost(state, side, &zone, &eligible, *count, ctx.acting_install)?;
            // Resolved to handles first, as `Forfeit` does: nothing leaves
            // the list here, but a position is only good against the list
            // it was read from.
            let installs: Vec<InstallId> = picked.iter().map(|&p| state.corp.installed[p].install_id).collect();
            let mut events = Vec::new();
            for install in installs {
                let installed = state.corp.installed.iter_mut().find(|c| c.install_id == install).expect("picked from this list");
                installed.rezzed = false;
                events.push(GameEvent::CardDerezzed { install, card: Some(installed.card.clone()) });
            }
            Ok(events)
        }

        Cost::TakeBadPublicity(amount) => {
            state.corp.bad_publicity = state.corp.bad_publicity.saturating_add(*amount);
            Ok(vec![GameEvent::BadPublicityGiven { amount: *amount }])
        }

        Cost::TakeTags(amount) => {
            let had = state.runner.tags;
            state.runner.tags = state.runner.tags.saturating_add(*amount);
            Ok(vec![GameEvent::TagsGiven { side: Side::Runner, amount: *amount, had }])
        }

        // `AnyOf`'s choice is resolved by the caller before `pay_cost` is
        // ever invoked (see `pending_choice::resolve_accept_pending_paid_choice`,
        // the only production caller that can encounter one) — reaching
        // here means something handed `pay_cost` a raw, unresolved `AnyOf`.
        Cost::AnyOf(_) => Err(RulesError::CostRequiresChoice),

        // Paid part by part, in authored order. `cost_is_affordable` has
        // already vouched for every part where a probe precedes payment;
        // a direct submission that can pay the first part but not a later
        // one errors there, and `apply_action`'s clone-on-write discards
        // the partial payment.
        Cost::AllOf(parts) => {
            let mut events = Vec::new();
            for part in parts {
                events.extend(pay_cost_ctx(state, registry, side, part, purpose, ctx)?);
            }
            Ok(events)
        }

        Cost::RemoveCounters(amount) => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?;
            let available = counters_of(state, ctx).unwrap_or(0);
            if available < *amount {
                return Err(RulesError::InsufficientCounters { card: card_id.clone(), required: *amount, available });
            }
            modify_counters(state, ctx, -i64::from(*amount))
        }
        Cost::RemoveAdvancementCounters(amount) => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?;
            let position = acting_corp_position(state, ctx).ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            let installed = &mut state.corp.installed[position];
            if installed.advancement_tokens < *amount {
                return Err(RulesError::InsufficientCounters { card: card_id.clone(), required: *amount, available: installed.advancement_tokens });
            }
            installed.advancement_tokens -= amount;
            Ok(vec![GameEvent::AdvancementCountersRemoved {
                install: installed.install_id,
                card: Some(installed.card.clone()),
                advancement_tokens: installed.advancement_tokens,
            }])
        }
        // Out of the rig and into the Corp's score area. Leaving the rig is
        // leaving play, so what it hosts is trashed with it, as a trash
        // would (`cascade_trash_hosted_on_rig_card`).
        Cost::AddToScoreAreaAsAgenda(as_agenda) => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?;
            let position = acting_rig_position(state, ctx).ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            let removed = state.runner.rig.remove(position);
            let mut events = cascade_trash_hosted_on_rig_card(state, registry, &removed);
            events.push(add_to_score_area_as_agenda(state, Side::Corp, removed.card, *as_agenda));
            Ok(events)
        }
    }
}

/// Who carries out a trash a card's text makes: its controller (CR
/// 1.14.5), read off the card resolving. A card that names another
/// player to do it — Noise's "the Corp trashes the top card of R&D" —
/// is still read as its controller's; nothing in the pool hears the
/// difference yet.
/// Whether `card` is one revealed in `side`'s hand (`GameState::revealed`),
/// taking it off the list if so: a revealed card that moves is revealed no
/// longer (CR 1.21.6). `AddToDeck` asks, so it takes the card from the hand
/// it was revealed in — not the heap, which it otherwise searches first —
/// and moves it in the open.
fn take_revealed(state: &mut GameState, side: Side, card: &CardId) -> bool {
    let Some(at) = state.revealed.iter().position(|revealed| revealed.side == side && &revealed.card == card) else { return false };
    state.revealed.remove(at);
    true
}

fn carried_out_by(registry: &CardRegistry, ctx: &ResolutionContext<'_>) -> Option<Side> {
    ctx.acting_card.and_then(|card| registry.get(card)).map(|definition| definition.side)
}

/// Dispatches the trashes among `events` a player carried out
/// (`GameEvent::CardTrashed::by`), for a site that trashes by a card's
/// text and is not paying a cost — a cost's are its payer's to dispatch
/// (`dispatch_cost_events`). Hiram "0mission" Svensson hears them.
pub(crate) fn dispatch_trashes(state: &mut GameState, registry: &CardRegistry, events: &[GameEvent]) -> Result<Vec<GameEvent>, RulesError> {
    let mut fired = Vec::new();
    for event in events.iter().filter(|event| matches!(event, GameEvent::CardTrashed { by: Some(_), .. })) {
        if !state.is_over() {
            fired.extend(dispatcher::dispatch_event(state, registry, event)?);
        }
    }
    Ok(fired)
}

/// Dispatches the events a cost produced that a card can hear — a tag
/// taken (`Cost::TakeTags`: NBN: Reality Plus hears Funhouse's), a tag
/// removed (`Cost::RemoveTags`: Synapse Global hears its own ability's) —
/// for a payer to call once the effect the cost paid for has resolved.
///
/// After the effect and not between the cost and it, which is the order
/// `resolve_accept` always had for a tag paid as a cost: if the effect
/// parks something, a reaction dispatched here is queued behind it rather
/// than fired underneath. Comprehensive Rules 1.16.3 puts a checkpoint
/// after the payment, which would resolve such a reaction *before* the
/// effect; no card in the pool can tell the two orders apart (Synapse's
/// credits land before its install prompt rather than after), and a
/// dispatch ahead of the effect would have to hold the effect behind
/// whatever the reaction parks, which only a `Sequence` knows how to do.
///
/// `pay_cost_ctx` dispatches nothing itself, so every site that pays a
/// data-driven cost calls this; `dispatcher::audit` names one that does
/// not, in every test and both sweeps.
pub(crate) fn dispatch_cost_events(
    state: &mut GameState,
    registry: &CardRegistry,
    cost_events: &[GameEvent],
) -> Result<Vec<GameEvent>, RulesError> {
    let mut fired = Vec::new();
    for event in cost_events {
        if !state.is_over() && !crate::rules::listeners::moments(state, event).is_empty() {
            fired.extend(dispatcher::dispatch_event(state, registry, event)?);
        }
    }
    Ok(fired)
}

/// Checks an `AbilityDef::requirement` gate before its cost/effect resolve —
/// same "checked before resolution" role as `pay_cost`, but for a
/// precondition rather than a payment. Called from `engine::activate_ability`.
///
/// `side` is whose turn/ability this check is running for — only
/// `OncePerTurn` reads it (to pick which side's `once_per_turn_used` set to
/// consult); every other variant ignores it. Callers pass the side that owns
/// the card/ability/play in question (`AbilityDef`'s activating side,
/// `CardDefinition::play_requirement`'s playing side, or — from
/// `process_card_triggers` — the triggered card's own registry `side`).
pub fn check_requirement(
    state: &GameState,
    requirement: &EffectRequirement,
    side: Side,
    ctx: &ResolutionContext<'_>,
    registry: &CardRegistry,
) -> Result<(), RulesError> {
    match requirement {
        EffectRequirement::IsTagged => {
            if !state.runner.is_tagged() {
                return Err(RulesError::RunnerNotTagged);
            }
            Ok(())
        }
        EffectRequirement::CurrentlyAccessingNonAgenda => {
            let accessing = state.active_run.as_ref().and_then(|run| run.access_state.as_ref()).and_then(|access| {
                match &access.phase {
                    run::AccessPhase::PendingChoice { card_id, .. } => Some(card_id.clone()),
                    _ => None,
                }
            });
            let Some(accessing) = accessing else { return Err(RulesError::RequirementNotMet) };
            let is_agenda = registry.get(&accessing).is_some_and(|def| def.card_type == crate::dsl::CardType::Agenda);
            if is_agenda { Err(RulesError::RequirementNotMet) } else { Ok(()) }
        }
        EffectRequirement::SubroutineResolvedThisRun => {
            if state.active_run.as_ref().is_some_and(|run| run.subroutine_resolved) { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::IdentityFlipped => {
            // The asking side's own flip state — `side` is the resolving
            // card's controller, so a Corp identity reads the Corp's.
            let flipped = match side {
                Side::Corp => state.corp.identity_flipped,
                Side::Runner => state.runner.identity_flipped,
            };
            if flipped { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::IdentityCopy(copy) => {
            // Only the Corp's identity comes in copies (CR 1.5.2).
            if side == Side::Corp && state.corp.identity_copy == *copy { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::DuringRunOn(server) => {
            let on = state.run_in_progress().is_some_and(|run| run.server == *server && !matches!(run.phase, RunPhase::Ended));
            if on { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::MemoryFull => {
            if crate::rules::memory::available_memory(state, registry) == 0 { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::OncePerTurn => {
            let used = match side {
                Side::Corp => &state.corp.once_per_turn_used,
                Side::Runner => &state.runner.once_per_turn_used,
            };
            if used.contains(&OncePerTurnKey { card: ctx.acting_card.cloned(), install: ctx.acting_install }) {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::OncePerRun => {
            let key = OncePerTurnKey { card: ctx.acting_card.cloned(), install: ctx.acting_install };
            match state.run_in_progress() {
                Some(run) if !run.once_per_run_used.contains(&key) => Ok(()),
                _ => Err(RulesError::RequirementNotMet),
            }
        }
        EffectRequirement::DuringYourTurn => {
            if crate::rules::listeners::active_side(state) == side { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::RunnerCreditsAtMost(amount) => {
            if state.runner.resources.credits.0 > *amount {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::RunnerClicksAtLeast(amount) => {
            if state.runner.resources.clicks.0 < *amount {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::ZoneHasAtLeast { zone, count, filter } => {
            let found = match filter {
                Some(filter) => {
                    crate::rules::pending_choice::eligible_positions(state, registry, side, zone, filter, ctx.acting_install, ctx.acting_card).len()
                }
                None => crate::rules::pending_choice::zone_card_ids(state, side, zone, ctx.acting_install).len(),
            };
            if found < *count as usize {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::Not(inner) => match check_requirement(state, inner, side, ctx, registry) {
            Ok(()) => Err(RulesError::RequirementNotMet),
            Err(_) => Ok(()),
        },
        EffectRequirement::And(a, b) => {
            check_requirement(state, a, side, ctx, registry)?;
            check_requirement(state, b, side, ctx, registry)
        }
        EffectRequirement::RezzedDuringRunAgainstThisServer => {
            let own_server = acting_corp_install(state, ctx).map(|c| c.server).ok_or(RulesError::RequirementNotMet)?;
            let matches = state.active_run.as_ref().is_some_and(|run| {
                run.server == own_server && matches!(run.phase, RunPhase::ApproachIce | RunPhase::EncounterIce)
            });
            if matches { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::RunAgainstThisServer => {
            let own_server = acting_corp_install(state, ctx).map(|c| c.server);
            let matches = own_server.is_some_and(|own| state.run_in_progress().is_some_and(|run| run.server == own));
            if matches { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::LastDamageTrashedOddCostCard => {
            // Read from the resolution in flight, not from `GameState`:
            // *Diviner* asks about the `DealDamage` immediately preceding
            // it in its own `Sequence`. A `ctx` with nothing recorded means
            // no damage was dealt in this resolution, which is correctly
            // "requirement not met" rather than a stale answer from some
            // earlier action's damage.
            let trashed_odd_cost = ctx
                .damage_discarded
                .iter()
                .any(|card_id| registry.get(card_id).is_some_and(|card| card.cost % 2 == 1));
            if trashed_odd_cost { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::LastRunWasOnHqOrRnD => match state.last_completed_run.as_ref().map(|run| run.server) {
            Some(ServerId::Hq | ServerId::RnD) => Ok(()),
            _ => Err(RulesError::RequirementNotMet),
        },
        EffectRequirement::ArchivesHasFacedownCard => {
            if state.corp.has_facedown_in_archives() { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::AccessingIn(server) => {
            let accessing_there = state
                .active_run
                .as_ref()
                .and_then(|run| run.access_state.as_ref())
                .is_some_and(|access| access.server == *server);
            if accessing_there { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::StoleAgendaDuringLastRun => {
            let stole = state.last_completed_run.as_ref().is_some_and(|run| run.agendas_stolen > 0);
            if stole { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::ThisCardCountersAtMost(amount) => {
            let current = counters_of(state, ctx).unwrap_or(0);
            if current > *amount {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::ThisCardCountersAtLeast(amount) => {
            let current = counters_of(state, ctx).unwrap_or(0);
            if current < *amount {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::EncounteringHostIce => {
            let Some(host) = acting_rig_card(state, ctx).and_then(|c| c.hosted_on_ice) else {
                return Err(RulesError::RequirementNotMet);
            };
            let matches = state.active_run.as_ref().is_some_and(|run| {
                run.phase == RunPhase::EncounterIce && run.ice.get(run.position).is_some_and(|ice| ice.install_id == host)
            });
            if matches { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::EncounteringThisIce => {
            let matches = ctx.acting_install.is_some_and(|this| {
                state.active_run.as_ref().is_some_and(|run| {
                    run.phase == RunPhase::EncounterIce && run.ice.get(run.position).is_some_and(|ice| ice.install_id == this)
                })
            });
            if matches { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::DuringRun => {
            // Not merely `active_run.is_some()`: once the Runner is
            // accessing, or the run has ended but not been cleared, there
            // is nothing left to move.
            let running = state
                .active_run
                .as_ref()
                .is_some_and(|run| !matches!(run.phase, RunPhase::AccessingCard | RunPhase::Ended));
            if running { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::DuringEncounter => {
            let encountering =
                state.active_run.as_ref().is_some_and(|run| run.phase == RunPhase::EncounterIce);
            if encountering { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::Encountering(ice_type) => {
            let encountering = state.active_run.as_ref().filter(|run| run.phase == RunPhase::EncounterIce).and_then(|run| run.ice.get(run.position)).is_some_and(|ice| {
                ice.ice_type == *ice_type || continuous::ice_gains_subtype(state, registry, ice.install_id, *ice_type)
            });
            if encountering { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::ThisCardStartedTheRun => {
            let started = ctx.acting_card.is_some_and(|this| state.active_run.as_ref().and_then(|run| run.initiated_by.as_ref()) == Some(this));
            if started { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::AboutToApproach(server) => {
            let next = state.active_run.as_ref().is_some_and(|run| {
                run.phase == RunPhase::Movement && run.position >= run.ice.len() && run.server == *server && run.redirect_on_approach.is_none()
            });
            if next { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::AccessedAnyCardDuringLastRun => {
            let accessed = state.last_completed_run.as_ref().is_some_and(|run| run.cards_accessed > 0);
            if accessed { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::ThisCardIsInstalled => {
            // Deliberately off the *install* the dispatch named, never a
            // first-match CardId fallback — see the variant's doc comment.
            let installed = ctx.acting_install.is_some_and(|install| {
                state.find_corp_install(install).is_some() || state.find_rig_install(install).is_some()
            });
            if installed { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::HostsInstalled(filter) => {
            let hosts = ctx.acting_install.is_some_and(|host| {
                state.runner.rig.iter().filter(|card| card.hosted_on_rig_card == Some(host)).any(|card| {
                    registry.get(&card.card).is_some_and(|definition| card_matches_filter(definition, filter))
                })
            });
            if hosts { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::WasFirstAdvancementThisCard => {
            // Answered from the triggering event itself: `CardAdvanced`
            // already carries the running total, and `== 1` *is* "this was
            // the first advancement". The `GameState` field this replaced
            // held nothing the event didn't.
            let was_first = matches!(
                ctx.triggering_event,
                Some(GameEvent::CardAdvanced { advancement_tokens: 1, .. })
            );
            if was_first { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::HadNoTags => {
            // Off the triggering event, as `WasFirstAdvancementThisCard`
            // is: the state has the tags just taken.
            let had_none = matches!(ctx.triggering_event, Some(GameEvent::TagsGiven { side: Side::Runner, had: 0, .. }));
            if had_none { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::CorpCreditsAtLeast(amount) => {
            if state.corp.resources.credits.0 >= *amount { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::RunEventActive => {
            let active = crate::rules::run::run_event(state, registry)
                .and_then(|card| registry.get(card))
                .is_some_and(|def| def.subtypes.contains(&CardSubtype::Run));
            if active { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::InstalledWithoutSpendingCredits => {
            let free = matches!(
                ctx.triggering_event,
                Some(
                    GameEvent::ProgramInstalled { credits_paid: 0, .. }
                        | GameEvent::HardwareInstalled { credits_paid: 0, .. }
                        | GameEvent::ResourceInstalled { credits_paid: 0, .. }
                )
            );
            if free { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::AmountAtLeast(amount, min) => {
            if resolve_amount(amount, ctx, state, registry) >= *min { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::MoreThan(more, than) => {
            if resolve_amount(more, ctx, state, registry) > resolve_amount(than, ctx, state, registry) { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::NoActionTakenThisTurn => {
            if state.this_turn.actions_finished() == 0 { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::ActingCardMatches(filter) => {
            let matches = ctx.acting_card.and_then(|card| registry.get(card)).is_some_and(|card| card_matches_filter(card, filter));
            if matches { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::ThisAgendaScoredThisTurn => {
            let scored_now = ctx.acting_install.is_some_and(|install| {
                state.corp.scored_agendas.iter().any(|scored| scored.install_id == install && scored.scored_on_turn == state.turn)
            });
            if scored_now { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::ProtectingRemote => {
            let protecting = acting_corp_install(state, ctx)
                .is_some_and(|installed| installed.slot == InstallSlot::Ice && matches!(installed.server, ServerId::Remote(_)));
            if protecting { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::AgendaCameFromThisCardsServer => {
            // `AgendaScored` names the server; a steal names none, so it
            // comes off the run the steal necessarily happened during.
            let from = match ctx.triggering_event {
                Some(GameEvent::AgendaScored { server, .. }) => Some(*server),
                Some(GameEvent::AgendaStolen { .. }) => state.active_run.as_ref().map(|run| run.server),
                _ => None,
            };
            let here = acting_corp_install(state, ctx).map(|installed| installed.server);
            if from.is_some() && from == here { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::CurrentlyAccessingInstalledCard { rezzed_only } => {
            // `AccessState::pending_install` is set when the card being
            // accessed is a root install, and is still set while its
            // `OnAccessed` and `OnTrashedFromAccess` triggers dispatch —
            // which is what lets Aggressive Trendsetting tell "trashed an
            // installed card" from "trashed a card found in HQ".
            let access = state.active_run.as_ref().and_then(|run| run.access_state.as_ref());
            let install = access.and_then(|access| access.pending_install);
            // Rezzed-ness comes off the access, not the table: the card may
            // already be in Archives by the time the question is asked
            // (Public Access Plaza, from `OnTrashedFromAccess`).
            let ok = match install {
                None => false,
                Some(_) if *rezzed_only => access.is_some_and(|access| access.pending_install_rezzed),
                Some(_) => true,
            };
            if ok { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::PlayedFromArchives => {
            if matches!(ctx.triggering_event, Some(GameEvent::OperationPlayed { from_archives: true, .. })) {
                Ok(())
            } else {
                Err(RulesError::RequirementNotMet)
            }
        }
    }
}

/// `acting_card`'s current generic counter total, wherever it's currently
/// installed/rigged — `None` if it's neither (already trashed, or never
/// resolvable), mirroring `modify_counters`'s "try Corp installed, then
/// Runner rig" search order but read-only.
fn counters_of(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<u32> {
    acting_corp_install(state, ctx)
        .map(|c| c.counters)
        .or_else(|| acting_rig_card(state, ctx).map(|c| c.counters))
        .or_else(|| acting_scored_position(state, ctx).map(|position| state.corp.scored_agendas[position].agenda_counters))
        .or_else(|| acting_is_corp_identity(state, ctx).then_some(state.corp.identity_counters))
        .or_else(|| state.active_run.as_ref().filter(|run| acting_is_run_event(run, ctx)).map(|run| run.event_counters))
        .or_else(|| remembered(state, ctx).map(|known| known.counters))
}

/// Whether the resolution is the run's event's own — its paid ability
/// (`InstallId::RUN_EVENT`), or a rider the event left on the run as it
/// began it (`PromptChooseServer::on_start`, resolved as the card with no
/// install) — so its counters are `RunState::event_counters`.
fn acting_is_run_event(run: &crate::rules::RunState, ctx: &ResolutionContext<'_>) -> bool {
    match ctx.acting_install {
        Some(install) => install == InstallId::RUN_EVENT,
        None => ctx.acting_card.is_some() && run.initiated_by.as_ref() == ctx.acting_card,
    }
}

/// `acting_card`'s current advancement token total, if it's a Corp
/// installed card — `None` otherwise (a Runner rig card, or already
/// trashed). Read-only counterpart to `advance_card`'s mutation.
fn advancement_tokens_of(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<u32> {
    acting_corp_install(state, ctx).map(|c| c.advancement_tokens).or_else(|| remembered(state, ctx).map(|known| known.advancement_tokens))
}

/// `ctx.last_known`, when its install has left play and only then.
fn remembered(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<LastKnown> {
    ctx.last_known.filter(|_| acting_install_has_left(state, ctx))
}

/// Number of Runner rig cards matching `dsl::zone::CardFilter::Icebreaker`'s
/// heuristic — used by `Amount::InstalledIcebreakerCount`.
fn installed_icebreaker_count(state: &GameState, registry: &CardRegistry) -> u32 {
    state
        .runner
        .rig
        .iter()
        .filter(|c| registry.get(&c.card).is_some_and(|def| card_matches_filter(def, &CardFilter::Icebreaker)))
        .count() as u32
}

/// Resolves a `dsl::effect::Amount` to a concrete `u32` against the current
/// `state`/`acting_card` context — the shared computation every
/// `Amount`-typed `Effect` field (`DealDamageAmount`, `AddAdditionalAccessAmount`,
/// `BoostStrengthAmount`) delegates to before falling through to its
/// fixed-amount sibling's existing mutation logic. Every non-`Fixed`
/// variant that can't be resolved (no `acting_card`, card not currently
/// installed/rigged) resolves to `0` rather than erroring — the same
/// "nothing to do" leniency `Effect::DrawCards`/`TakeAllCountersAsCredits`
/// already establish for an empty/absent source.
/// Whether `breaker` (its definition, if it has one — a click has none)
/// may break `subroutine` under `SubroutineDef::only_breakable_by`.
fn subroutine_breakable_by(subroutine: &crate::rules::run::EncounteredSubroutine, breaker: Option<&crate::dsl::CardDefinition>) -> bool {
    match subroutine.definition.only_breakable_by {
        None => true,
        Some(subtype) => breaker.is_some_and(|def| def.subtypes.contains(&subtype)),
    }
}

/// The subroutines on the encountered `ice` that `breaker` may break now,
/// in the order a break takes them, each with whether its break counts
/// against the ice's `BreakLimit`: pending, breakable by this breaker
/// (`only_breakable_by`), and no more of the printed ones than the limit
/// leaves (`continuous::breaks_left`). A gained subroutine is not printed,
/// so it is never limited.
fn breakable_now(
    state: &GameState,
    registry: &CardRegistry,
    ice: &crate::rules::run::RunIce,
    breaker: Option<&crate::dsl::CardDefinition>,
) -> Vec<(usize, bool)> {
    let mut left = continuous::breaks_left(state, registry, ice, breaker);
    let mut breakable = Vec::new();
    for subroutine in ice.subroutines.iter().filter(|s| s.status == SubroutineStatus::Pending && subroutine_breakable_by(s, breaker)) {
        let limited = left.is_some() && !subroutine.gained;
        if limited {
            match left.as_mut() {
                Some(0) => continue,
                Some(room) => *room -= 1,
                None => {}
            }
        }
        breakable.push((subroutine.id, limited));
    }
    breakable
}

/// Breaks the first `count` of `pending` (`breakable_now`), counting each
/// limited break for the rest of the encounter.
fn break_pending(
    state: &mut GameState,
    registry: &CardRegistry,
    pending: Vec<(usize, bool)>,
    count: &SubroutineBreakCount,
    by: Option<InstallId>,
) -> Result<Vec<GameEvent>, RulesError> {
    let take = match count {
        SubroutineBreakCount::All => pending.len(),
        SubroutineBreakCount::Fixed(n) => (*n as usize).min(pending.len()),
        SubroutineBreakCount::ChosenNumber => 0,
    };
    let mut events = Vec::new();
    for (idx, limited) in pending.into_iter().take(take) {
        if limited && let Some(run) = state.active_run.as_mut() {
            run.this_encounter.limited_breaks += 1;
        }
        for event in run::break_subroutine(state, registry, idx, by)? {
            dispatcher::emit(state, registry, &mut events, event)?;
        }
    }
    Ok(events)
}

/// An `Amount` read off the table alone, with no card resolving it — what
/// a bot's evaluator asks of a card text it has not played yet (Flood the
/// Market's count of protected remotes). An amount that reads the
/// resolving card (its counters, its printed cost) or the resolution (a
/// chosen number, the credits just lost) is 0 here.
pub fn amount_on_table(amount: &Amount, state: &GameState, registry: &CardRegistry) -> u32 {
    resolve_amount(amount, &ResolutionContext::default(), state, registry)
}

pub(crate) fn resolve_amount(amount: &Amount, ctx: &ResolutionContext<'_>, state: &GameState, registry: &CardRegistry) -> u32 {
    match amount {
        Amount::ClicksRemaining => match state.phase {
            crate::rules::GamePhase::Action(side) => state.resources(side).clicks.0,
            _ => 0,
        },
        Amount::PrintedCost => ctx.acting_card.and_then(|card| registry.get(card)).map_or(0, |def| def.cost),
        Amount::AccessedCardPrintedCost => state
            .active_run
            .as_ref()
            .and_then(|run| run.access_state.as_ref())
            .and_then(|access| access.phase.card())
            .and_then(|card| registry.get(card))
            .map_or(0, |def| def.cost),
        Amount::RemainingAfterSelection(total) => total.saturating_sub(ctx.selected_count),
        Amount::CardsSelected => ctx.selected_count,
        Amount::RunCreditsLeftLastRun => state.last_completed_run.as_ref().map_or(0, |run| run.run_credits_left),
        Amount::AccessLimit(server) => state.active_run.as_ref().map_or(0, |run| match server {
            ServerId::Hq => 1 + run.additional_hq_access,
            ServerId::RnD => 1 + run.additional_rd_access,
            ServerId::Archives | ServerId::Remote(_) => 0,
        }),
        Amount::Fixed(n) => *n,
        // A placeholder `Effect::with_chosen_number` writes over before a
        // `then` resolves; read anywhere else it is nothing.
        Amount::ChosenNumber => 0,
        Amount::AgendaPointsScoredThisTurn => state.this_turn.agenda_points_scored(),
        Amount::CardsInstalledFromHqThisTurn => state.this_turn.installed_from_hq(),
        Amount::TimesThisTurn(trigger) => state.this_turn.times(*trigger),
        Amount::TimesThisTurnWhen { trigger, when } => {
            let controller = ctx.acting_card.and_then(|card| registry.get(card)).map_or(Side::Runner, |card| card.side);
            state.this_turn.times_when(*trigger, when, controller)
        }
        Amount::TimesLastTurn(trigger) => state.last_turn.times(*trigger),
        Amount::TimesLastTurnWhen { trigger, when } => {
            let controller = ctx.acting_card.and_then(|card| registry.get(card)).map_or(Side::Runner, |card| card.side);
            state.last_turn.times_when(*trigger, when, controller)
        }
        Amount::HostedCounters => counters_of(state, ctx).unwrap_or(0),
        Amount::HostedAdvancementTokens => advancement_tokens_of(state, ctx).unwrap_or(0),
        Amount::HostedCards => acting_rig_card(state, ctx).map_or(0, |card| card.hosted_cards.len() as u32),
        Amount::InstalledIcebreakerCount => installed_icebreaker_count(state, registry),
        Amount::FacedownCardsInArchives => state.corp.archives.iter().filter(|a| a.facedown).count() as u32,
        // By discriminant: ice is one card type (CR 2.15.2) whatever its
        // `IceType`, which is a subtype.
        Amount::CardTypesAmongFaceupInArchives => {
            let mut types: Vec<std::mem::Discriminant<crate::dsl::CardType>> = Vec::new();
            for def in state.corp.archives.iter().filter(|archived| !archived.facedown).filter_map(|archived| registry.get(&archived.card)) {
                let kind = std::mem::discriminant(&def.card_type);
                if !types.contains(&kind) {
                    types.push(kind);
                }
            }
            types.len() as u32
        }
        Amount::CreditsLostThisResolution => ctx.credits_lost,
        // The greater of the two scores, in agenda points — Null Signal
        // Games' *Elevation* threat-level rule. Read off the score areas
        // through the registry, the same way `win::check_for_winner` does,
        // rather than a running counter that a forfeit would have to
        // decrement.
        Amount::RunnerTags => state.runner.tags,
        Amount::BadPublicity => state.corp.bad_publicity,
        Amount::IceProtectingThisServer => acting_corp_install(state, ctx).map_or(0, |installed| {
            state.corp.installed.iter().filter(|other| other.server == installed.server && other.slot == InstallSlot::Ice).count() as u32
        }),
        Amount::ProtectedRemotesWithRootCards => {
            let mut remotes: Vec<ServerId> = state
                .corp
                .installed
                .iter()
                .filter(|c| matches!(c.server, ServerId::Remote(_)) && c.slot == InstallSlot::Root)
                .map(|c| c.server)
                .collect();
            remotes.sort_by_key(|server| match server {
                ServerId::Remote(n) => *n,
                _ => 0,
            });
            remotes.dedup();
            remotes
                .into_iter()
                .filter(|server| state.corp.installed.iter().any(|c| c.server == *server && c.slot == InstallSlot::Ice))
                .count() as u32
        }
        Amount::IceProtecting(server) => {
            state.corp.installed.iter().filter(|c| c.server == *server && c.slot == InstallSlot::Ice).count() as u32
        }
        Amount::OtherUnrezzedIce => state
            .corp
            .installed
            .iter()
            .filter(|c| c.slot == InstallSlot::Ice && !c.rezzed && Some(c.install_id) != ctx.acting_install)
            .count() as u32,
        Amount::InScoreAreaWithSubtype(subtype) => {
            let side = ctx.acting_card.and_then(|card| registry.get(card)).map_or(Side::Runner, |definition| definition.side);
            let scored = match side {
                Side::Corp => &state.corp.scored_agendas,
                Side::Runner => &state.runner.scored_agendas,
            };
            scored
                .iter()
                .filter(|entry| match entry.as_agenda {
                    Some(as_agenda) => as_agenda.subtype == Some(*subtype),
                    None => registry.get(&entry.card).is_some_and(|definition| definition.subtypes.contains(subtype)),
                })
                .count() as u32
        }
        Amount::InHeapWithSubtype(subtype) => {
            state.runner.heap.iter().filter(|card| registry.get(card).is_some_and(|def| def.subtypes.contains(subtype))).count() as u32
        }
        // "The greatest score of any player" (CR 1.17.1a), never below 0.
        Amount::EncounteredIceSubroutines => state
            .active_run
            .as_ref()
            .filter(|run| run.phase == crate::rules::run::RunPhase::EncounterIce)
            .and_then(|run| run.ice.get(run.position))
            .map_or(0, |ice| ice.subroutines.len() as u32),
        Amount::CardsAccessedLastRun => state.last_completed_run.as_ref().map_or(0, |run| run.cards_accessed),
        Amount::EncounteredIceStrength => state
            .active_run
            .as_ref()
            .filter(|run| run.phase == crate::rules::run::RunPhase::EncounterIce)
            .and_then(|run| run.ice.get(run.position))
            .map_or(0, |ice| crate::rules::continuous::ice_strength(state, registry, ice).max(0) as u32),
        Amount::ThreatLevel => {
            let score = |side| crate::rules::win::score(state, registry, side);
            score(Side::Corp).max(score(Side::Runner)).max(0) as u32
        }
        Amount::CardsInHand(Side::Corp) => state.corp.hq.len() as u32,
        Amount::CardsInHand(Side::Runner) => state.runner.grip.len() as u32,
        Amount::TimesThisTurnOnThisCopy(trigger) => ctx
            .acting_install
            .and_then(|install| state.corp.installed.iter().find(|installed| installed.install_id == install))
            .map_or(0, |installed| installed.this_turn.count(state.turn, *trigger)),
    }
}

/// Spends what `requirement` gates — a `OncePerTurn`, anywhere under an
/// `And` — once the thing it gated has actually been used: a trigger whose
/// effects resolved, a paid ability that resolved (`engine::
/// activate_ability`). Kept apart from `check_requirement`,
/// which only reads, because the same requirement is also asked where
/// nothing is used: a legal-action probe, a price shown, `would_fire`.
pub(crate) fn consume_requirement(
    state: &mut GameState,
    requirement: &EffectRequirement,
    side: Side,
    ctx: &ResolutionContext<'_>,
) {
    match requirement {
        EffectRequirement::IsTagged => {}
        EffectRequirement::OncePerTurn => {
            let used = match side {
                Side::Corp => &mut state.corp.once_per_turn_used,
                Side::Runner => &mut state.runner.once_per_turn_used,
            };
            used.insert(OncePerTurnKey { card: ctx.acting_card.cloned(), install: ctx.acting_install });
        }
        EffectRequirement::OncePerRun => {
            if let Some(run) = state.active_run.as_mut() {
                run.once_per_run_used.insert(OncePerTurnKey { card: ctx.acting_card.cloned(), install: ctx.acting_install });
            }
        }
        EffectRequirement::And(a, b) => {
            consume_requirement(state, a, side, ctx);
            consume_requirement(state, b, side, ctx);
        }
        EffectRequirement::RunnerCreditsAtMost(_)
        | EffectRequirement::DuringYourTurn
        | EffectRequirement::IdentityFlipped
        | EffectRequirement::IdentityCopy(_)
        | EffectRequirement::DuringRunOn(_)
        | EffectRequirement::CurrentlyAccessingNonAgenda
        | EffectRequirement::CurrentlyAccessingInstalledCard { .. }
        | EffectRequirement::AgendaCameFromThisCardsServer
        | EffectRequirement::ProtectingRemote
        | EffectRequirement::ActingCardMatches(_)
        | EffectRequirement::ThisAgendaScoredThisTurn
        | EffectRequirement::SubroutineResolvedThisRun
        | EffectRequirement::MemoryFull
        | EffectRequirement::RunnerClicksAtLeast(_)
        | EffectRequirement::ZoneHasAtLeast { .. }
        | EffectRequirement::Not(_)
        | EffectRequirement::RezzedDuringRunAgainstThisServer
        | EffectRequirement::RunAgainstThisServer
        | EffectRequirement::LastDamageTrashedOddCostCard
        | EffectRequirement::LastRunWasOnHqOrRnD
        | EffectRequirement::StoleAgendaDuringLastRun
        | EffectRequirement::ArchivesHasFacedownCard
        | EffectRequirement::AccessingIn(_)
        | EffectRequirement::AccessedAnyCardDuringLastRun
        | EffectRequirement::ThisCardIsInstalled
        | EffectRequirement::HostsInstalled(_)
        | EffectRequirement::ThisCardCountersAtMost(_)
        | EffectRequirement::ThisCardCountersAtLeast(_)
        | EffectRequirement::EncounteringHostIce
        | EffectRequirement::EncounteringThisIce
        | EffectRequirement::DuringEncounter
        | EffectRequirement::Encountering(_)
        | EffectRequirement::ThisCardStartedTheRun
        | EffectRequirement::AboutToApproach(_)
        | EffectRequirement::DuringRun
        | EffectRequirement::WasFirstAdvancementThisCard
        | EffectRequirement::HadNoTags
        | EffectRequirement::CorpCreditsAtLeast(_)
        | EffectRequirement::RunEventActive
        | EffectRequirement::InstalledWithoutSpendingCredits
        | EffectRequirement::AmountAtLeast(..)
        | EffectRequirement::MoreThan(..)
        | EffectRequirement::NoActionTakenThisTurn
        | EffectRequirement::PlayedFromArchives => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::CompletedRun;
    use crate::rules::state::InstallId;
    use crate::rules::test_support::fixture_install_id;
    use crate::dsl::{CardDefinition, CardId, CardType, DamageType, IceType, SubroutineDef, TriggeredEffect};
    use crate::rules::run::{EncounteredSubroutine, RunIce, RunPhase as RP, RunState, ServerId, SubroutineStatus};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, GamePhase, InstalledCard, InstalledRunnerCard,
        MemoryUnits, PlayerResources, RunnerState, ScoredAgenda,
    };

    fn installed_runner_card(id: &str, base_strength: i32) -> InstalledRunnerCard {
        InstalledRunnerCard {
            install_id: fixture_install_id(id),
            card: CardId(id.to_string()),
            base_strength,
            ..Default::default()
        }
    }

    /// A card added as an agenda with "You cannot forfeit this agenda."
    /// is no forfeit's to take; one without the sentence is.
    #[test]
    fn a_card_that_cannot_be_forfeited_is_not_counted_toward_a_forfeit() {
        let registry = CardRegistry::default();
        let as_agenda = |points, cannot_forfeit| {
            Some(crate::dsl::AsAgenda { points, cannot_forfeit, subtype: None })
        };
        let mut state = game_state();
        state.corp.scored_agendas =
            vec![ScoredAgenda { install_id: InstallId(1), as_agenda: as_agenda(-1, true), ..ScoredAgenda::plain(CardId("word_on_the_street".to_string())) }];
        let ctx = ResolutionContext::for_card(None);
        let forfeit = Cost::Forfeit(1);
        assert!(!cost_is_affordable(&state, &registry, Side::Corp, &forfeit, Purpose::Other, &ctx));
        assert!(pay_cost_ctx(&mut state.clone(), &registry, Side::Corp, &forfeit, Purpose::Other, &ctx).is_err());
        state.corp.scored_agendas.push(ScoredAgenda { install_id: InstallId(2), as_agenda: as_agenda(2, false), ..ScoredAgenda::plain(CardId("myoshu".to_string())) });
        assert!(cost_is_affordable(&state, &registry, Side::Corp, &forfeit, Purpose::Other, &ctx));
        pay_cost_ctx(&mut state, &registry, Side::Corp, &forfeit, Purpose::Other, &ctx).expect("forfeit Myōshu");
        assert_eq!(state.corp.scored_agendas.len(), 1, "Word on the Street stays");
        assert_eq!(state.corp.removed_from_game, vec![CardId("myoshu".to_string())]);
    }

    fn game_state() -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources {
                    credits: Credits(5),
                    clicks: Clicks(3),
                    agenda_points: AgendaPoints(0),
                },
                ..Default::default()
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(5),
                    clicks: Clicks(4),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Runner),
            ..Default::default()
        }
    }

    #[test]
    fn gain_credits_targets_the_named_side() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GainCredits(Side::Corp, 3), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(8));
        assert_eq!(state.runner.resources.credits, Credits(5));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Corp, amount: 3 }]);
    }

    #[test]
    fn deal_damage_delegates_to_apply_damage() {
        let mut state = game_state();
        state.runner.grip = vec![CardId("card_0".to_string()), CardId("card_1".to_string())];

        let events = evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 1), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.grip.len(), 1);
        assert_eq!(state.runner.heap.len(), 1);
        // Announced, then taken: damage about to be suffered is a moment a
        // card can hear, so it is always in the record.
        assert_eq!(events[0], GameEvent::AboutToResolve { what: WouldHappen::Damage { kind: DamageType::Net, amount: 1 } });
        assert!(matches!(events[1], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 1, responsible: None }), "no card dealt it, so nobody did");
    }

    #[test]
    fn draw_cards_stops_silently_on_an_empty_deck() {
        let mut state = game_state();
        state.runner.stack = vec![CardId("only_card".to_string())];

        let events = evaluate_effect(&mut state, &Effect::DrawCards(Side::Runner, 3), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.grip, vec![CardId("only_card".to_string())]);
        assert!(state.runner.stack.is_empty());
        assert_eq!(events, vec![GameEvent::CardDrawn { side: Side::Runner }]);
    }

    /// CR 1.19.1: "Trashing is the act of moving an object to its owner's
    /// discard pile." A Corp card Detente hosts goes to Archives when
    /// Detente is trashed; it went to the Runner's heap, where the Corp's
    /// Archives-reading cards could never find it.
    #[test]
    fn a_corp_card_hosted_on_detente_goes_to_archives_when_detente_is_trashed() {
        let card = |id: &str, side: Side, card_type: CardType| CardDefinition { id: CardId(id.to_string()), side, card_type, ..CardDefinition::default() };
        let registry = CardRegistry::from_cards(vec![
            card("detente", Side::Runner, CardType::Hardware),
            card("hedge_fund", Side::Corp, CardType::Operation),
        ]);
        let mut state = game_state();
        state.runner.rig = vec![crate::rules::state::InstalledRunnerCard {
            install_id: fixture_install_id("detente"),
            card: CardId("detente".to_string()),
            hosted_cards: vec![CardId("hedge_fund".to_string())],
            ..Default::default()
        }];
        let install = state.runner.rig[0].install_id;

        let events = trash_install(&mut state, &registry, Side::Runner, install, Some(Side::Corp)).unwrap();

        assert_eq!(state.runner.heap, vec![CardId("detente".to_string())], "only Detente is the Runner's");
        assert_eq!(state.corp.archives, vec![ArchivedCard::faceup(CardId("hedge_fund".to_string()))]);
        assert!(events.iter().any(|e| matches!(e, GameEvent::CardTrashed { side: Side::Corp, card, .. } if *card == CardId("hedge_fund".to_string()))));
    }

    /// CR 1.7.2c: "The Runner wins if the Corp is required to draw a card
    /// from R&D but cannot because R&D is empty." Drawing what there is
    /// comes first, so a draw of two from one card takes the card and
    /// then loses.
    #[test]
    fn a_corp_required_to_draw_from_an_empty_r_and_d_loses() {
        let mut state = game_state();
        state.corp.r_and_d = vec![CardId("only_card".to_string())];

        let events = evaluate_effect(&mut state, &Effect::DrawCards(Side::Corp, 2), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.hq.last(), Some(&CardId("only_card".to_string())));
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(events, vec![GameEvent::CardDrawn { side: Side::Corp }, GameEvent::GameOver { winner: Side::Runner }]);
    }

    #[test]
    fn end_the_run_clears_active_run_and_emits_event() {
        let mut state = game_state();
        state.active_run = Some(RunState {
            phase: RP::ApproachIce,
            jack_out_permitted: true,
            ..Default::default()
        });

        let events = evaluate_effect(&mut state, &Effect::EndTheRun, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert!(state.active_run.is_none());
        assert_eq!(events, vec![GameEvent::RunEndedByEffect { server: ServerId::Hq }]);
    }

    /// Ending a run that is already over does nothing rather than
    /// erroring: this used to be `Err(NoActiveRun)`, which made *Biawak*'s
    /// second subroutine fail the whole action after its first had ended
    /// the run — and, when that action was a parked selection's
    /// confirmation, left a decision nothing could resolve (the view-path
    /// sweep's seed-19 stall).
    #[test]
    fn end_the_run_with_no_active_run_does_nothing() {
        let mut state = game_state();
        assert_eq!(evaluate_effect(&mut state, &Effect::EndTheRun, &mut ResolutionContext::for_card(None), &CardRegistry::new()), Ok(Vec::new()));
    }

    fn active_run_state() -> RunState {
        RunState {
            phase: RP::ApproachIce,
            jack_out_permitted: true,
            ..Default::default()
        }
    }

    #[test]
    fn add_additional_access_increments_the_matching_field() {
        let mut state = game_state();
        state.active_run = Some(active_run_state());

        let events =
            evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, &mut ResolutionContext::for_card(None), &CardRegistry::new())
                .unwrap();
        assert_eq!(state.active_run.as_ref().unwrap().additional_hq_access, 1);
        assert_eq!(events, vec![GameEvent::AdditionalAccessGranted { server: ServerId::Hq, count: 1 }]);

        let events =
            evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::RnD, count: 2 }, &mut ResolutionContext::for_card(None), &CardRegistry::new())
                .unwrap();
        assert_eq!(state.active_run.as_ref().unwrap().additional_rd_access, 2);
        assert_eq!(events, vec![GameEvent::AdditionalAccessGranted { server: ServerId::RnD, count: 2 }]);
    }

    #[test]
    fn add_additional_access_stacks_additively() {
        let mut state = game_state();
        state.active_run = Some(active_run_state());

        evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();
        evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.active_run.as_ref().unwrap().additional_hq_access, 2);
    }

    #[test]
    fn add_additional_access_no_ops_for_archives_and_remote() {
        let mut state = game_state();
        state.active_run = Some(active_run_state());

        let archives = evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Archives, count: 3 }, &mut ResolutionContext::for_card(None), &CardRegistry::new())
            .unwrap();
        let remote = evaluate_effect(
            &mut state,
            &Effect::AddAdditionalAccess { server: ServerId::Remote(0), count: 3 }, &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        let run = state.active_run.as_ref().unwrap();
        assert_eq!(run.additional_hq_access, 0);
        assert_eq!(run.additional_rd_access, 0);
        // A breach of either server accesses everything there already, so
        // the grant changes nothing — and records nothing. It used to emit
        // `AdditionalAccessGranted` for a no-op.
        assert!(archives.is_empty() && remote.is_empty(), "{archives:?} {remote:?}");
    }

    #[test]
    fn add_additional_access_without_an_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn set_access_replacement_stores_the_pending_effect() {
        let mut state = game_state();
        state.active_run = Some(active_run_state());
        let replacement = Effect::GainCredits(Side::Runner, 8);

        let events = evaluate_effect(
            &mut state,
            &Effect::SetAccessReplacement { server: ServerId::Hq, effect: Box::new(replacement.clone()), optional: false }, &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(
            state.active_run.as_ref().unwrap().access_replacement,
            Some((ServerId::Hq, replacement, false))
        );
        assert_eq!(events, vec![GameEvent::AccessReplacementSet { server: ServerId::Hq }]);
    }

    #[test]
    fn set_access_replacement_without_an_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::SetAccessReplacement { optional: false,
                    server: ServerId::Hq,
                    effect: Box::new(Effect::GainCredits(Side::Runner, 8)),
                }, &mut ResolutionContext::for_card(None),
                &CardRegistry::new()),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn give_tags_always_targets_the_runner() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GiveTags(Amount::Fixed(2)), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.tags, 2);
        assert_eq!(events, vec![GameEvent::TagsGiven { side: Side::Runner, amount: 2, had: 0 }]);
    }

    #[test]
    fn remove_tags_saturates_at_zero() {
        let mut state = game_state();
        state.runner.tags = 1;
        let events = evaluate_effect(&mut state, &Effect::RemoveTags(Amount::Fixed(5)), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.tags, 0);
        // The event reports the tag that actually came off, not the five
        // asked for — Synapse Global: Faster than Thought reacts to a
        // removal, and must not react to a request that removed nothing.
        assert_eq!(events, vec![GameEvent::TagsRemoved { side: Side::Runner, amount: 1, by: Side::Corp }]);
    }

    #[test]
    fn give_bad_publicity_increases_the_counter() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GiveBadPublicity(Amount::Fixed(2)), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.bad_publicity, 2);
        assert_eq!(events, vec![GameEvent::BadPublicityGiven { amount: 2 }]);
    }

    #[test]
    fn remove_bad_publicity_saturates_at_zero() {
        let mut state = game_state();
        state.corp.bad_publicity = 1;
        let events = evaluate_effect(&mut state, &Effect::RemoveBadPublicity(5), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.bad_publicity, 0);
        assert_eq!(events, vec![GameEvent::BadPublicityRemoved { amount: 5 }]);
    }

    #[test]
    fn is_tagged_requirement_fails_with_zero_tags_and_succeeds_with_a_tag() {
        let mut state = game_state();
        assert_eq!(
            check_requirement(&state, &EffectRequirement::IsTagged, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RunnerNotTagged)
        );

        state.runner.tags = 1;
        assert_eq!(check_requirement(&state, &EffectRequirement::IsTagged, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn once_per_turn_requirement_fires_once_then_is_silently_skipped_on_a_second_attempt() {
        let mut state = game_state();
        let requirement = EffectRequirement::OncePerTurn;

        assert_eq!(check_requirement(&state, &requirement, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));
        consume_requirement(&mut state, &requirement, Side::Runner, &ResolutionContext::default());

        assert_eq!(
            check_requirement(&state, &requirement, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );
        // The Corp's own set is untouched — OncePerTurn is per-side.
        assert_eq!(check_requirement(&state, &requirement, Side::Corp, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn once_per_turn_requirement_resets_at_the_next_turn_start() {
        let mut state = game_state();
        state.phase = GamePhase::Action(Side::Runner);
        let requirement = EffectRequirement::OncePerTurn;
        state.runner.once_per_turn_used.insert(OncePerTurnKey { card: None, install: None });
        assert_eq!(check_requirement(&state, &requirement, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Err(RulesError::RequirementNotMet));

        crate::rules::turn::enter_start_of_turn(&mut state, &mut Vec::new(), Side::Runner).unwrap();

        assert_eq!(check_requirement(&state, &requirement, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn zone_has_at_least_requirement_counts_the_acting_sides_zone() {
        let mut state = game_state();
        state.corp.hq = vec![CardId("a".to_string())];
        let requirement = EffectRequirement::ZoneHasAtLeast { zone: crate::dsl::CardZoneRef::OwnHq, count: 2, filter: None };
        assert_eq!(
            check_requirement(&state, &requirement, Side::Corp, &ResolutionContext::default(), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );
        state.corp.hq.push(CardId("b".to_string()));
        assert_eq!(check_requirement(&state, &requirement, Side::Corp, &ResolutionContext::default(), &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn runner_credits_at_most_requirement() {
        let mut state = game_state();
        state.runner.resources.credits = Credits(7);
        assert_eq!(
            check_requirement(&state, &EffectRequirement::RunnerCreditsAtMost(6), Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );

        state.runner.resources.credits = Credits(6);
        assert_eq!(check_requirement(&state, &EffectRequirement::RunnerCreditsAtMost(6), Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn not_requirement_inverts_the_inner_result() {
        let state = game_state();
        assert_eq!(
            check_requirement(&state, &EffectRequirement::Not(Box::new(EffectRequirement::IsTagged)), Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Ok(())
        );

        let mut tagged = game_state();
        tagged.runner.tags = 1;
        assert_eq!(
            check_requirement(&tagged, &EffectRequirement::Not(Box::new(EffectRequirement::IsTagged)), Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );
    }

    fn test_ice(card_id: &str, subroutine_count: usize, rezzed: bool) -> RunIce {
        test_ice_of_type(card_id, subroutine_count, rezzed, IceType::Barrier)
    }

    fn test_ice_of_type(
        card_id: &str,
        subroutine_count: usize,
        rezzed: bool,
        ice_type: IceType,
    ) -> RunIce {
        RunIce {
            install_id: crate::rules::InstallId::PLACEHOLDER,
            card_id: CardId(card_id.to_string()),
            ice_type,
            subroutines: (0..subroutine_count)
                .map(|id| EncounteredSubroutine {
                    id,
                    definition: SubroutineDef {
                        text: format!("Subroutine {id}"),
                        effect: Effect::EndTheRun,
                        only_breakable_by: None,
                    },
                    status: SubroutineStatus::Pending,
                    gained: false,
                })
                .collect(),
            rezzed,
        }
    }

    #[test]
    fn resolve_unbroken_subroutines_resolves_each_pending_subroutine_in_order() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 2, true);
        ice.subroutines[0].definition.effect = Effect::GiveTags(Amount::Fixed(2));
        ice.subroutines[1].definition.effect = Effect::GainCredits(Side::Corp, 3);
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![ice],
            jack_out_permitted: true,
            ..Default::default()
        });

        let events = resolve_unbroken_subroutines(&mut state, &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.tags, 2);
        assert_eq!(state.corp.resources.credits, Credits(8));

        let run = state.active_run.unwrap();
        assert_eq!(run.ice[0].subroutines[0].status, SubroutineStatus::Resolved);
        assert_eq!(run.ice[0].subroutines[1].status, SubroutineStatus::Resolved);

        assert_eq!(
            events,
            vec![
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 0,
                    effect: Effect::GiveTags(Amount::Fixed(2)),
                },
                GameEvent::TagsGiven { side: Side::Runner, amount: 2, had: 0 },
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 1,
                    effect: Effect::GainCredits(Side::Corp, 3),
                },
                GameEvent::CreditsGained { side: Side::Corp, amount: 3 },
                GameEvent::AbilityGainedCredits { side: Side::Corp, card: CardId("ice_wall".to_string()) },
            ]
        );
    }

    #[test]
    fn resolve_unbroken_subroutines_stops_at_end_the_run() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 2, true);
        ice.subroutines[0].definition.effect = Effect::EndTheRun;
        ice.subroutines[1].definition.effect = Effect::GiveTags(Amount::Fixed(5));
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![ice],
            jack_out_permitted: true,
            ..Default::default()
        });

        let events = resolve_unbroken_subroutines(&mut state, &CardRegistry::new()).unwrap();

        assert!(state.active_run.is_none());
        assert_eq!(state.runner.tags, 0);
        assert_eq!(
            events,
            vec![
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 0,
                    effect: Effect::EndTheRun,
                },
                GameEvent::EncounterEnded { card_id: CardId("ice_wall".to_string()), install: crate::rules::state::InstallId(0) },
                GameEvent::RunEndedByEffect { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn resolve_unbroken_subroutines_skips_already_handled_subroutines() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 2, true);
        ice.subroutines[0].status = SubroutineStatus::Broken;
        ice.subroutines[1].definition.effect = Effect::GiveTags(Amount::Fixed(1));
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![ice],
            jack_out_permitted: true,
            ..Default::default()
        });

        let events = resolve_unbroken_subroutines(&mut state, &CardRegistry::new()).unwrap();

        assert_eq!(events.len(), 2);
        let run = state.active_run.unwrap();
        assert_eq!(run.ice[0].subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(run.ice[0].subroutines[1].status, SubroutineStatus::Resolved);
    }

    #[test]
    fn modify_strength_lasts_the_encounter_and_emits_event() {
        let mut state = game_state();
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let mut registry = CardRegistry::new();
        registry.insert(crate::rules::test_support::ice_printing("ice_wall", 3));

        let events = evaluate_effect(&mut state, &Effect::ModifyStrength { delta: 2, ice: crate::dsl::StrengthOf::Encountered, duration: crate::dsl::EffectDuration::Encounter }, &mut ResolutionContext::for_card(None), &registry).unwrap();

        let ice = state.active_run.as_ref().unwrap().ice[0].clone();
        assert_eq!(continuous::ice_strength(&state, &registry, &ice), 5);
        state.active_run.as_mut().unwrap().phase = RP::ApproachIce;
        assert_eq!(continuous::ice_strength(&state, &registry, &ice), 3, "\"for the remainder of this encounter\"");
        assert_eq!(
            events,
            vec![GameEvent::IceStrengthModified {
                card_id: CardId("ice_wall".to_string()),
                new_strength: 5,
                delta: 2,
            }]
        );
    }

    #[test]
    fn modify_strength_outside_encounter_ice_errors() {
        let mut state = game_state();
        state.active_run =
            Some(RunState {
                phase: RP::ApproachIce,
                jack_out_permitted: true,
                ..Default::default()
            });

        assert_eq!(
            evaluate_effect(&mut state, &Effect::ModifyStrength { delta: 2, ice: crate::dsl::StrengthOf::Encountered, duration: crate::dsl::EffectDuration::Encounter }, &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::NotInEncounter)
        );
    }

    #[test]
    fn modify_strength_with_no_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::ModifyStrength { delta: 2, ice: crate::dsl::StrengthOf::Encountered, duration: crate::dsl::EffectDuration::Encounter }, &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn trash_card_this_card_without_acting_card_is_rejected_not_panicked() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::ThisCard), &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::MissingActingCardContext)
        );
    }

    #[test]
    fn trash_card_this_card_with_acting_card_moves_it_to_the_heap() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gordian_blade", 2)];
        let acting = CardId("gordian_blade".to_string());

        let events =
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::ThisCard), &mut ResolutionContext::for_card(Some(&acting)), &CardRegistry::new()).unwrap();

        assert!(state.runner.rig.is_empty());
        assert_eq!(state.runner.heap, vec![acting.clone()]);
        assert_eq!(events, vec![GameEvent::CardTrashed { side: Side::Runner, card: acting, installed: true, by: None }]);
    }

    #[test]
    fn trash_card_corp_installed_moves_card_to_archives() {
        let mut state = game_state();
        state.corp.installed.push(InstalledCard {
            install_id: InstallId(1082),
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        });

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::CorpInstalled {
                card: CardId("pad_campaign".to_string()),
                server: ServerId::Remote(0),
            }), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        assert!(state.corp.installed.is_empty());
        // The install was rezzed, so the Runner had already seen it.
        assert_eq!(state.corp.archives, vec![ArchivedCard::faceup(CardId("pad_campaign".to_string()))]);
        assert_eq!(
            events,
            vec![GameEvent::CardTrashed { side: Side::Corp, card: CardId("pad_campaign".to_string()), installed: true, by: None }]
        );
    }

    #[test]
    fn trash_card_runner_rig_not_found_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::RunnerRig(CardId("gordian_blade".to_string()))), &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::CardNotInRig { side: Side::Runner, card: CardId("gordian_blade".to_string()) })
        );
    }

    #[test]
    fn trash_card_runner_rig_moves_card_to_heap() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gordian_blade", 2)];

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::RunnerRig(CardId("gordian_blade".to_string()))), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        assert!(state.runner.rig.is_empty());
        assert_eq!(state.runner.heap, vec![CardId("gordian_blade".to_string())]);
        assert_eq!(
            events,
            vec![GameEvent::CardTrashed { side: Side::Runner, card: CardId("gordian_blade".to_string()), installed: true, by: None }]
        );
    }

    #[test]
    fn trash_card_top_of_stack_mills_from_the_correct_zone() {
        let mut state = game_state();
        state.corp.r_and_d = vec![CardId("ice_wall".to_string()), CardId("hedge_fund".to_string())];

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::RAndD }), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(state.corp.r_and_d, vec![CardId("ice_wall".to_string())]);
        // Milled off R&D — the Runner never saw it.
        assert_eq!(state.corp.archives, vec![ArchivedCard::facedown(CardId("hedge_fund".to_string()))]);
        assert_eq!(
            events,
            vec![GameEvent::CardTrashed { side: Side::Corp, card: CardId("hedge_fund".to_string()), installed: false, by: None }]
        );
    }

    #[test]
    fn trash_card_top_of_stack_mismatched_zone_errors() {
        let mut state = game_state();
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];

        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::Stack }), &mut ResolutionContext::for_card(None),
                &CardRegistry::new()),
            Err(RulesError::EmptyZone { side: Side::Corp, zone: StackZone::Stack })
        );
    }

    #[test]
    fn trash_card_top_of_stack_runner_mills_from_the_stack() {
        let mut state = game_state();
        state.runner.stack = vec![CardId("clone_chip".to_string()), CardId("sure_gamble".to_string())];

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Runner, zone: StackZone::Stack }), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(state.runner.stack, vec![CardId("clone_chip".to_string())]);
        assert_eq!(state.runner.heap, vec![CardId("sure_gamble".to_string())]);
        assert_eq!(
            events,
            vec![GameEvent::CardTrashed { side: Side::Runner, card: CardId("sure_gamble".to_string()), installed: false, by: None }]
        );
    }

    #[test]
    fn trash_card_top_of_stack_with_empty_deck_trashes_nothing() {
        let mut state = game_state();
        // corp.r_and_d is empty by default in game_state() — a valid
        // side/zone combo, unlike the mismatched-combo case above, so
        // there is nothing to trash rather than something wrong.
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::RAndD }), &mut ResolutionContext::for_card(None),
                &CardRegistry::new()),
            Ok(Vec::new())
        );
        assert!(state.corp.archives.is_empty());
    }

    #[test]
    fn pay_credits_deducts_and_errors_when_insufficient() {
        let mut state = game_state();
        let events = pay_cost(&mut state, &CardRegistry::new(), Side::Corp, &Cost::Credits(3), Purpose::Other, None).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(2));
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Corp, amount: 3 }]);

        assert_eq!(
            pay_cost(&mut state, &CardRegistry::new(), Side::Corp, &Cost::Credits(10), Purpose::Other, None),
            Err(RulesError::NotEnoughCredits { side: Side::Corp, available: 2, requested: 10 })
        );
    }

    fn run_with_bad_publicity_credits(amount: u32) -> RunState {
        RunState {
            bad_publicity_credits: amount,
            phase: RP::ApproachIce,
            jack_out_permitted: true,
            ..Default::default()
        }
    }

    #[test]
    fn pay_credits_draws_from_bad_publicity_pool_before_the_wallet_during_a_run() {
        let mut state = game_state();
        state.active_run = Some(run_with_bad_publicity_credits(3));

        let events = pay_cost(&mut state, &CardRegistry::new(), Side::Runner, &Cost::Credits(5), Purpose::Other, None).unwrap();

        assert_eq!(state.active_run.as_ref().unwrap().bad_publicity_credits, 0);
        assert_eq!(state.runner.resources.credits, Credits(3)); // 5 wallet - (5 - 3 from BP)
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpentFromOutsidePool { side: Side::Runner, amount: 3, run_against: Some(state.active_run.as_ref().unwrap().server) },
                GameEvent::BadPublicityCreditsSpent { amount: 3 },
                GameEvent::CreditsSpent { side: Side::Runner, amount: 5 },
            ]
        );
    }

    #[test]
    fn pay_credits_with_no_active_run_ignores_bad_publicity_pool() {
        // Regression guard: behavior/events must be byte-identical to the
        // pre-Bad-Publicity path when there's no run to draw from.
        let mut state = game_state();
        let events = pay_cost(&mut state, &CardRegistry::new(), Side::Runner, &Cost::Credits(2), Purpose::Other, None).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(3));
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Runner, amount: 2 }]);
    }

    #[test]
    fn pay_credits_with_zero_bad_publicity_credits_behaves_like_wallet_only() {
        let mut state = game_state();
        state.active_run = Some(run_with_bad_publicity_credits(0));

        let events = pay_cost(&mut state, &CardRegistry::new(), Side::Runner, &Cost::Credits(2), Purpose::Other, None).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(3));
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Runner, amount: 2 }]);
    }

    #[test]
    fn pay_credits_insufficient_across_both_pools_reports_combined_available_and_leaves_state_untouched() {
        let mut state = game_state();
        state.runner.resources.credits = Credits(1);
        state.active_run = Some(run_with_bad_publicity_credits(1));

        let result = pay_cost(&mut state, &CardRegistry::new(), Side::Runner, &Cost::Credits(5), Purpose::Other, None);

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Runner, available: 2, requested: 5 })
        );
        assert_eq!(state.runner.resources.credits, Credits(1));
        assert_eq!(state.active_run.as_ref().unwrap().bad_publicity_credits, 1);
    }

    #[test]
    fn pay_credits_corp_side_is_unaffected_by_runner_bad_publicity_pool() {
        let mut state = game_state();
        state.active_run = Some(run_with_bad_publicity_credits(10));

        let events = pay_cost(&mut state, &CardRegistry::new(), Side::Corp, &Cost::Credits(3), Purpose::Other, None).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(2));
        assert_eq!(state.active_run.as_ref().unwrap().bad_publicity_credits, 10);
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Corp, amount: 3 }]);
    }

    #[test]
    fn pay_clicks_spends_the_requested_amount() {
        let mut state = game_state();
        let events = pay_cost(&mut state, &CardRegistry::new(), Side::Runner, &Cost::Clicks(2), Purpose::Other, None).unwrap();

        assert_eq!(state.runner.resources.clicks, Clicks(2));
        assert_eq!(events, vec![GameEvent::ClickSpent { side: Side::Runner }, GameEvent::ClickSpent { side: Side::Runner }]);
    }

    #[test]
    fn pay_clear_tags_zeroes_the_counter() {
        let mut state = game_state();
        state.runner.tags = 3;

        let events = pay_cost(&mut state, &CardRegistry::new(), Side::Runner, &Cost::ClearTags, Purpose::Other, None).unwrap();

        assert_eq!(state.runner.tags, 0);
        assert_eq!(events, vec![GameEvent::TagsCleared { side: Side::Runner, by: Side::Runner }]);
    }

    #[test]
    fn pay_trash_self_without_acting_card_is_rejected_not_panicked() {
        let mut state = game_state();
        assert_eq!(
            pay_cost(&mut state, &CardRegistry::new(), Side::Runner, &Cost::TrashSelf, Purpose::Other, None),
            Err(RulesError::MissingActingCardContext)
        );
    }

    #[test]
    fn pay_trash_self_with_acting_card_trashes_it_to_the_heap() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("self_modifying_code", 0)];
        let acting = CardId("self_modifying_code".to_string());

        let events = pay_cost(&mut state, &CardRegistry::new(), Side::Runner, &Cost::TrashSelf, Purpose::Other, Some(&acting)).unwrap();

        assert!(state.runner.rig.is_empty());
        assert_eq!(state.runner.heap, vec![acting.clone()]);
        assert_eq!(events, vec![GameEvent::CardTrashed { side: Side::Runner, card: acting, installed: true, by: Some(Side::Runner) }]);
    }

    /// A minimal `CardDefinition` carrying exactly the given `triggers` — everything
    /// else is irrelevant to `process_card_triggers` tests.
    fn card_with_triggers(id: &str, triggers: Vec<TriggeredEffect>) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Asset,
            triggers,
            is_playable: true,
            ..Default::default()
        }
    }

    #[test]
    fn process_card_triggers_fires_all_effects_of_a_matching_trigger_in_order() {
        let mut state = game_state();
        let registry = CardRegistry::from_cards(vec![card_with_triggers(
            "snare",
            vec![TriggeredEffect {
                subject: None, when: None, acts_on_subject: false, first_each_turn: false, from_heap: false,
                text: None,
                trigger: Trigger::OnAccessed,
                effects: vec![Effect::GiveTags(Amount::Fixed(1)), Effect::GainCredits(Side::Corp, 2)],
                requirement: None,
            }],
        )]);

        let events = process_card_triggers(
            &mut state,
            &registry,
            &CardId("snare".to_string()),
            Trigger::OnAccessed,
            None,
        )
        .unwrap();

        assert_eq!(state.runner.tags, 1);
        assert_eq!(state.corp.resources.credits, Credits(7));
        assert_eq!(
            events,
            vec![
                GameEvent::TagsGiven { side: Side::Runner, amount: 1, had: 0 },
                GameEvent::CreditsGained { side: Side::Corp, amount: 2 },
                GameEvent::AbilityGainedCredits { side: Side::Corp, card: CardId("snare".to_string()) },
            ]
        );
    }

    /// One queued trigger stands for every `TriggeredEffect` of its card
    /// that names the trigger, so the filter the scan applied is asked
    /// again per effect — a card printed "on HQ, … / on R&D, …" resolves
    /// only the half the run was about.
    #[test]
    fn a_cards_filtered_triggers_fire_only_for_the_occurrence_each_means() {
        let on = |server: ServerId, credits: u32| TriggeredEffect {
            subject: Some(crate::dsl::Subject::Any),
            when: Some(crate::dsl::EventFilter::Server(vec![server])),
            acts_on_subject: false, first_each_turn: false, from_heap: false,
            text: None,
            trigger: Trigger::OnSuccessfulRun,
            effects: vec![Effect::GainCredits(Side::Runner, credits)],
            requirement: None,
        };
        let registry = CardRegistry::from_cards(vec![card_with_triggers("two_doors", vec![on(ServerId::Hq, 1), on(ServerId::RnD, 3)])]);
        let card = CardId("two_doors".to_string());

        let mut state = game_state();
        let succeeded = GameEvent::RunSucceeded { server: ServerId::RnD };
        let events = process_card_triggers(&mut state, &registry, &card, Trigger::OnSuccessfulRun, Some(&succeeded)).unwrap();
        assert!(events.contains(&GameEvent::CreditsGained { side: Side::Runner, amount: 3 }), "{events:?}");
        assert!(!events.contains(&GameEvent::CreditsGained { side: Side::Runner, amount: 1 }), "{events:?}");

        // A filter with no event to read admits nothing, rather than
        // everything.
        assert!(process_card_triggers(&mut state, &registry, &card, Trigger::OnSuccessfulRun, None).unwrap().is_empty());
    }

    #[test]
    fn process_card_triggers_ignores_non_matching_triggers() {
        let mut state = game_state();
        let registry = CardRegistry::from_cards(vec![card_with_triggers(
            "hedge_fund",
            vec![TriggeredEffect {
                subject: None, when: None, acts_on_subject: false, first_each_turn: false, from_heap: false,
                text: None,
                trigger: Trigger::OnPlay,
                effects: vec![Effect::GainCredits(Side::Corp, 9)],
                requirement: None,
            }],
        )]);

        let events = process_card_triggers(
            &mut state,
            &registry,
            &CardId("hedge_fund".to_string()),
            Trigger::OnAccessed,
            None,
        )
        .unwrap();

        assert!(events.is_empty());
        assert_eq!(state.corp.resources.credits, Credits(5));
    }

    #[test]
    fn process_card_triggers_with_unregistered_card_yields_no_events() {
        let mut state = game_state();
        let registry = CardRegistry::new();

        let events = process_card_triggers(
            &mut state,
            &registry,
            &CardId("unregistered".to_string()),
            Trigger::OnAccessed,
            None,
        )
        .unwrap();

        assert!(events.is_empty());
    }

    #[test]
    fn add_counters_on_a_runner_rig_card_increments_its_counters() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gorman_drip", 0)];
        let acting = CardId("gorman_drip".to_string());

        let events = evaluate_effect(&mut state, &Effect::AddCounters(2), &mut ResolutionContext::for_card(Some(&acting)), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.rig[0].counters, 2);
        assert_eq!(events, vec![GameEvent::CountersAdded { card: acting, amount: 2 }]);
    }

    #[test]
    fn remove_counters_on_a_runner_rig_card_decrements_and_saturates_at_zero() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gorman_drip", 0)];
        state.runner.rig[0].counters = 1;
        let acting = CardId("gorman_drip".to_string());

        let events = evaluate_effect(&mut state, &Effect::RemoveCounters(Amount::Fixed(3)), &mut ResolutionContext::for_card(Some(&acting)), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.rig[0].counters, 0);
        assert_eq!(events, vec![GameEvent::CountersRemoved { card: acting, amount: 3 }]);
    }

    #[test]
    fn add_counters_on_a_corp_installed_card_increments_its_counters() {
        let mut state = game_state();
        state.corp.installed = vec![InstalledCard {
            install_id: InstallId(1083),
            card: CardId("some_asset".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];
        let acting = CardId("some_asset".to_string());

        evaluate_effect(&mut state, &Effect::AddCounters(3), &mut ResolutionContext::for_card(Some(&acting)), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.installed[0].counters, 3);
    }

    #[test]
    fn add_counters_without_acting_card_errors_unresolved_card_target() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::AddCounters(1), &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::UnresolvedCardTarget)
        );
    }

    #[test]
    fn add_counters_for_a_card_neither_installed_nor_rigged_errors() {
        let mut state = game_state();
        let acting = CardId("nowhere".to_string());
        assert_eq!(
            evaluate_effect(&mut state, &Effect::AddCounters(1), &mut ResolutionContext::for_card(Some(&acting)), &CardRegistry::new()),
            Err(RulesError::CardNotEligibleForCounters(acting))
        );
    }

    #[test]
    fn boost_strength_for_the_encounter_lingers_until_it_ends() {
        // Boosting requires an encounter (`require_encounter`) — an
        // icebreaker's abilities are only usable while encountering ICE.
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 1);
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BoostStrength { amount: 1, duration: EffectDuration::Encounter }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
        .unwrap();

        let encountered = state.active_run.as_ref().unwrap().ice[0].install_id;
        assert_eq!(state.lingering.len(), 1);
        assert_eq!(state.lingering[0].until, Until::EndOfEncounter(encountered));
        assert_eq!(lingering::rig_strength(&state, &state.runner.rig[0]), 3);
        assert_eq!(
            events,
            vec![GameEvent::StrengthBoosted {
                card_id: acting,
                new_strength: 3,
                delta: 1,
                duration: EffectDuration::Encounter,
            }]
        );
    }

    /// ROADMAP Rules Audit T8: "until the end of this encounter" buffs used
    /// to survive an unbroken "end the run" subroutine, because only the
    /// normal ICE pass reset them. Whether one holds is read off the state
    /// now (`rules::lingering`), so there is no way for a run to end that
    /// leaves one running; a `Turn` pump is the turn's business and stays.
    #[test]
    fn end_the_run_ends_encounter_pumps_but_not_turn_pumps() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 1);
        let breaker = state.runner.rig[0].install_id;
        let encountered = state.active_run.as_ref().unwrap().ice[0].install_id;
        let pump = |amount, until| LingeringEffect { what: Lingering::Strength(amount), on: On::Install(breaker), until, source: CardId("corroder".to_string()) };
        state.lingering = vec![pump(3, Until::EndOfEncounter(encountered)), pump(1, Until::EndOfTurn(state.turn))];

        evaluate_effect(&mut state, &Effect::EndTheRun, &mut ResolutionContext::for_card(None), &CardRegistry::new())
            .expect("a run is active");

        assert!(state.active_run.is_none());
        assert!(state.last_completed_run.is_some(), "the ended run is still snapshotted for OnRunEnded");
        assert_eq!(lingering::strength(&state, breaker), 1);
        assert_eq!(state.lingering.len(), 1, "`end_run` swept what ended with it");
    }

    #[test]
    fn boost_strength_for_the_turn_lasts_the_turn() {
        // Boosting requires an encounter (`require_encounter`) — an
        // icebreaker's abilities are only usable while encountering ICE.
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 1);
        let acting = CardId("corroder".to_string());

        evaluate_effect(
            &mut state,
            &Effect::BoostStrength { amount: 2, duration: EffectDuration::Turn }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(state.lingering[0].until, Until::EndOfTurn(state.turn));
        assert_eq!(lingering::rig_strength(&state, &state.runner.rig[0]), 4);
        state.active_run = None;
        assert_eq!(lingering::rig_strength(&state, &state.runner.rig[0]), 4, "past the run");
        state.turn += 1;
        assert_eq!(lingering::rig_strength(&state, &state.runner.rig[0]), 2, "and not past the turn");
    }

    #[test]
    fn boost_strength_without_acting_card_errors_unresolved_card_target() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BoostStrength { amount: 1, duration: EffectDuration::Encounter }, &mut ResolutionContext::for_card(None),
                &CardRegistry::new()),
            Err(RulesError::UnresolvedCardTarget)
        );
    }

    #[test]
    fn boost_strength_acting_card_not_in_rig_errors_card_not_in_rig() {
        // In an encounter, so the rig lookup is the operative check rather
        // than `require_encounter` short-circuiting first.
        let mut state = ice_encounter_state(Vec::new(), 1);
        let acting = CardId("corroder".to_string());
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BoostStrength { amount: 1, duration: EffectDuration::Encounter }, &mut ResolutionContext::for_card(Some(&acting)),
                &CardRegistry::new()),
            Err(RulesError::CardNotInRig { side: Side::Runner, card: acting })
        );
    }

    /// The engine-level half of gating icebreaker abilities to encounters.
    /// `EffectRequirement::DuringEncounter` on the ability stops it being
    /// *offered*; this stops the effect *resolving* however it was reached.
    /// Before both, Cleaver's "+1 strength" was a legal action on the
    /// Corp's turn — affordable, permitted, and pointless.
    #[test]
    fn boost_strength_outside_an_encounter_errors_not_in_encounter() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("corroder", 2)];
        let acting = CardId("corroder".to_string());

        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BoostStrength { amount: 1, duration: EffectDuration::Encounter }, &mut ResolutionContext::for_card(Some(&acting)),
                &CardRegistry::new()),
            Err(RulesError::NoActiveRun)
        );
        assert!(state.lingering.is_empty(), "and nothing was mutated");
    }

    fn ice_encounter_state(rig: Vec<InstalledRunnerCard>, subroutine_count: usize) -> GameState {
        let mut state = game_state();
        state.runner.rig = rig;
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", subroutine_count, true)],
            jack_out_permitted: true,
            ..Default::default()
        });
        state
    }

    #[test]
    fn break_subroutines_fixed_breaks_up_to_count_pending_lowest_id_first() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 3);
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(2), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
        .unwrap();

        let ice = &state.active_run.unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(ice.subroutines[1].status, SubroutineStatus::Broken);
        assert_eq!(ice.subroutines[2].status, SubroutineStatus::Pending);
        assert_eq!(
            events,
            vec![
                GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0, strength: 0 },
                GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 1, strength: 0 },
            ]
        );
    }

    #[test]
    fn break_subroutines_fixed_breaks_fewer_when_fewer_are_pending() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 1);
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(2), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(events.len(), 2, "the break and the full break");
        let ice = &state.active_run.unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Broken);
    }

    #[test]
    fn break_subroutines_all_breaks_every_pending_subroutine() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 3);
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(events.len(), 4, "three breaks and the full break");
        let ice = state.active_run.unwrap().ice;
        assert!(ice[0].subroutines.iter().all(|s| s.status == SubroutineStatus::Broken));
    }

    #[test]
    fn break_subroutines_outside_encounter_ice_errors_not_in_encounter() {
        let mut state = game_state();
        state.active_run = Some(RunState {
            phase: RP::ApproachIce,
            jack_out_permitted: true,
            ..Default::default()
        });

        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None }, &mut ResolutionContext::for_card(Some(&CardId("corroder".to_string()))),
                &CardRegistry::new()),
            Err(RulesError::NotInEncounter)
        );
    }

    #[test]
    fn break_subroutines_with_insufficient_breaker_strength_errors_breaker_strength_too_low() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 1)], 1);
        let acting = CardId("corroder".to_string());
        let registry = CardRegistry::from_cards(vec![crate::rules::test_support::ice_printing("ice_wall", 3)]);

        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
                &registry),
            Err(RulesError::BreakerStrengthTooLow {
                breaker: acting,
                breaker_strength: 1,
                ice: CardId("ice_wall".to_string()),
                ice_strength: 3,
            })
        );
        let ice = &state.active_run.unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Pending);
    }

    #[test]
    fn break_subroutines_after_boost_succeeds_and_marks_subroutines_broken() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 1)], 1);
        let acting = CardId("corroder".to_string());
        let registry = CardRegistry::from_cards(vec![crate::rules::test_support::ice_printing("ice_wall", 2)]);

        // Too weak before boosting.
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
                &registry),
            Err(RulesError::BreakerStrengthTooLow {
                breaker: acting.clone(),
                breaker_strength: 1,
                ice: CardId("ice_wall".to_string()),
                ice_strength: 2,
            })
        );

        evaluate_effect(
            &mut state,
            &Effect::BoostStrength { amount: 1, duration: EffectDuration::Encounter }, &mut ResolutionContext::for_card(Some(&acting)),
            &registry)
        .unwrap();

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
            &registry)
        .unwrap();

        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0, strength: 2 }, GameEvent::IceFullyBroken { card_id: CardId("ice_wall".to_string()), position: 0, by: None }]
        );
        let ice = &state.active_run.unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Broken);
    }

    #[test]
    fn break_subroutines_skips_already_broken_subroutines() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 2);
        state.active_run.as_mut().unwrap().ice[0].subroutines[0].status = SubroutineStatus::Broken;
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(events, vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 1, strength: 0 }, GameEvent::IceFullyBroken { card_id: CardId("ice_wall".to_string()), position: 0, by: None }]);
    }

    fn ice_encounter_state_of_type(
        rig: Vec<InstalledRunnerCard>,
        subroutine_count: usize,
        ice_type: IceType,
    ) -> GameState {
        let mut state = game_state();
        state.runner.rig = rig;
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![test_ice_of_type("ice_wall", subroutine_count, true, ice_type)],
            jack_out_permitted: true,
            ..Default::default()
        });
        state
    }

    #[test]
    fn break_subroutines_restrict_to_matching_ice_type_succeeds() {
        let mut state =
            ice_encounter_state_of_type(vec![installed_runner_card("corroder", 2)], 1, IceType::Barrier);
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines {
                count: SubroutineBreakCount::Fixed(1),
                restrict_to: Some(IceType::Barrier),
            }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0, strength: 0 }, GameEvent::IceFullyBroken { card_id: CardId("ice_wall".to_string()), position: 0, by: None }]
        );
    }

    #[test]
    fn break_subroutines_restrict_to_mismatched_ice_type_errors_invalid_breaker_subtype() {
        let mut state =
            ice_encounter_state_of_type(vec![installed_runner_card("corroder", 2)], 1, IceType::CodeGate);
        let acting = CardId("corroder".to_string());

        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BreakSubroutines {
                    count: SubroutineBreakCount::Fixed(1),
                    restrict_to: Some(IceType::Barrier),
                }, &mut ResolutionContext::for_card(Some(&acting)),
                &CardRegistry::new()),
            Err(RulesError::InvalidBreakerSubtype {
                breaker: acting,
                ice: CardId("ice_wall".to_string()),
                expected: IceType::Barrier,
            })
        );
        let ice = &state.active_run.unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Pending);
    }

    #[test]
    fn break_subroutines_with_no_restrict_to_breaks_any_ice_type() {
        for ice_type in [IceType::Barrier, IceType::CodeGate, IceType::Sentry] {
            let mut state =
                ice_encounter_state_of_type(vec![installed_runner_card("mimic", 2)], 1, ice_type);
            let acting = CardId("mimic".to_string());

            let events = evaluate_effect(
                &mut state,
                &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
                &CardRegistry::new())
            .unwrap();

            assert_eq!(
                events,
                vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0, strength: 0 }, GameEvent::IceFullyBroken { card_id: CardId("ice_wall".to_string()), position: 0, by: None }]
            );
        }
    }

    #[test]
    fn trace_effect_parks_pending_state_and_does_not_resolve_immediately() {
        let mut state = game_state();
        let effect = Effect::Trace { base: 3, on_success: Box::new(Effect::GiveTags(Amount::Fixed(1))) };

        let events = evaluate_effect(&mut state, &effect, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(events, vec![GameEvent::TraceInitiated { base: 3, initiating_card: None }]);
        assert_eq!(state.runner.tags, 0, "on_success must not fire yet");
        let trace = state.active_trace.expect("trace should be parked");
        assert_eq!(trace.base_strength, 3);
        assert_eq!(trace.corp_bid, None);
        assert_eq!(trace.effect_on_success, Effect::GiveTags(Amount::Fixed(1)));
        assert_eq!(trace.resume, TraceResume::None);
    }

    #[test]
    fn trace_effect_while_already_active_errors() {
        let mut state = game_state();
        evaluate_effect(&mut state, &Effect::Trace { base: 3, on_success: Box::new(Effect::GiveTags(Amount::Fixed(1))) }, &mut ResolutionContext::for_card(None), &CardRegistry::new())
            .unwrap();

        let result =
            evaluate_effect(&mut state, &Effect::Trace { base: 5, on_success: Box::new(Effect::GiveTags(Amount::Fixed(2))) }, &mut ResolutionContext::for_card(None), &CardRegistry::new());

        assert_eq!(result, Err(RulesError::TraceAlreadyActive));
        assert_eq!(state.active_trace.unwrap().base_strength, 3, "original trace must be untouched");
    }

    #[test]
    fn resolve_unbroken_subroutines_stops_at_a_trace_subroutine_and_marks_resume() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 2, true);
        ice.subroutines[0].definition.effect =
            Effect::Trace { base: 2, on_success: Box::new(Effect::EndTheRun) };
        ice.subroutines[1].definition.effect = Effect::GiveTags(Amount::Fixed(5));
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![ice],
            jack_out_permitted: true,
            ..Default::default()
        });

        let events = resolve_unbroken_subroutines(&mut state, &CardRegistry::new()).unwrap();

        let run = state.active_run.as_ref().unwrap();
        assert_eq!(run.ice[0].subroutines[0].status, SubroutineStatus::Resolved);
        assert_eq!(run.ice[0].subroutines[1].status, SubroutineStatus::Pending, "must not fire while trace pending");
        let trace = state.active_trace.expect("trace should be parked");
        assert_eq!(trace.resume, TraceResume::ResumeSubroutines);
        assert_eq!(
            events,
            vec![
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 0,
                    effect: Effect::Trace { base: 2, on_success: Box::new(Effect::EndTheRun) },
                },
                // `resolve_unbroken_subroutines` now passes the firing ICE
                // itself as `acting_card` (needed for a subroutine effect
                // that self-references its own installed position, e.g.
                // Ansel 1.0/Brân 1.0's "directly inward from this ice") —
                // so a subroutine-initiated trace correctly records its
                // initiating card instead of always `None`.
                GameEvent::TraceInitiated { base: 2, initiating_card: Some(CardId("ice_wall".to_string())) },
            ]
        );
    }

    #[test]
    fn effect_if_evaluates_the_inner_effect_when_the_condition_holds() {
        let mut state = game_state();
        state.runner.tags = 1;
        let effect = Effect::EffectIf {
            condition: EffectRequirement::IsTagged,
            effect: Box::new(Effect::GainCredits(Side::Runner, 3)),
        };

        let events = evaluate_effect(&mut state, &effect, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(8));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Runner, amount: 3 }]);
    }

    #[test]
    fn effect_if_silently_no_ops_when_the_condition_fails() {
        let mut state = game_state();
        let effect = Effect::EffectIf {
            condition: EffectRequirement::IsTagged,
            effect: Box::new(Effect::GainCredits(Side::Runner, 3)),
        };

        let events = evaluate_effect(&mut state, &effect, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(5), "no credits gained — condition wasn't met");
        assert!(events.is_empty());
    }

    #[test]
    fn offer_paid_choice_parks_pending_state_and_does_not_resolve_immediately() {
        let mut state = game_state();
        let effect = Effect::OfferPaidChoice {
            side: Side::Runner,
            cost: Cost::Credits(4),
            if_paid: Box::new(Effect::Sequence(Vec::new())),
            if_declined: Box::new(Effect::GiveTags(Amount::Fixed(1))),
            text: None,
        };

        let events = evaluate_effect(&mut state, &effect, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.tags, 0, "not resolved yet");
        assert_eq!(state.runner.resources.credits, Credits(5), "not paid yet");
        let pending = state.pending_paid_choice.expect("should be parked");
        assert_eq!(pending.side, Side::Runner);
        assert_eq!(pending.cost, Cost::Credits(4));
        assert_eq!(events, vec![GameEvent::PendingPaidChoiceOffered { side: Side::Runner }]);
    }

    #[test]
    fn present_choice_parks_pending_decision_and_does_not_resolve_immediately() {
        let mut state = game_state();
        let effect = Effect::PresentChoice {
            chooser: Side::Corp,
            options: vec![Effect::GainCredits(Side::Corp, 2), Effect::DrawCards(Side::Corp, 2)],
            texts: Vec::new(),
        };

        let events = evaluate_effect(&mut state, &effect, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(5), "not resolved yet");
        let PendingDecision::ChooseEffect { chooser, options, .. } = state.pending_decision.expect("should be parked")
        else {
            panic!("expected a parked ChooseEffect");
        };
        assert_eq!(chooser, Side::Corp);
        assert_eq!(options.len(), 2);
        assert_eq!(events, vec![GameEvent::PendingChoicePresented { chooser: Side::Corp, option_count: 2 }]);
    }

    #[test]
    fn gain_credits_per_card_accessed_this_run_reads_the_last_completed_run() {
        let mut state = game_state();
        state.last_completed_run = Some(CompletedRun { server: ServerId::Hq, cards_accessed: 3, agendas_stolen: 0, persistent_trashed_upgrades: Vec::new(), accessed_cards: Vec::new(), on_end_effect: None, on_end_card: None, on_end_install: None, run_credits_left: 0 });

        let events = evaluate_effect(
            &mut state,
            &Effect::GainCreditsAmount(Side::Runner, Amount::CardsAccessedLastRun), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(state.runner.resources.credits, Credits(8));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Runner, amount: 3 }]);
    }

    #[test]
    fn gain_credits_per_card_accessed_this_run_is_zero_with_no_completed_run() {
        let mut state = game_state();

        let events = evaluate_effect(
            &mut state,
            &Effect::GainCreditsAmount(Side::Runner, Amount::CardsAccessedLastRun), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(state.runner.resources.credits, Credits(5));
        // CR 9.12.2b: a "for each" that comes to 0 does not take place.
        assert!(events.is_empty(), "{events:?}");
    }

    #[test]
    fn pay_cost_take_tags_gives_the_runner_tags() {
        let mut state = game_state();

        let events = pay_cost(&mut state, &CardRegistry::new(), Side::Runner, &Cost::TakeTags(1), Purpose::Other, None).unwrap();

        assert_eq!(state.runner.tags, 1);
        assert_eq!(events, vec![GameEvent::TagsGiven { side: Side::Runner, amount: 1, had: 0 }]);
    }

    #[test]
    fn pay_cost_any_of_directly_errors_cost_requires_choice() {
        let mut state = game_state();

        let result = pay_cost(&mut state, &CardRegistry::new(), Side::Runner, &Cost::AnyOf(vec![Cost::Clicks(1)]), Purpose::Other, None);

        assert_eq!(result, Err(RulesError::CostRequiresChoice));
    }

    #[test]
    fn rezzed_during_run_against_this_server_requirement_matches_only_the_active_run_server() {
        let mut state = game_state();
        state.corp.installed = vec![crate::rules::state::InstalledCard {
            install_id: InstallId(1084),
            card: CardId("ping".to_string()),
            slot: crate::rules::state::InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }];
        let ping = CardId("ping".to_string());

        assert_eq!(
            check_requirement(
                &state,
                &EffectRequirement::RezzedDuringRunAgainstThisServer,
                Side::Corp, &ResolutionContext::for_card(Some(&ping)),
                &CardRegistry::new()
            ),
            Err(RulesError::RequirementNotMet),
            "no active run at all"
        );

        state.active_run = Some(active_run_state());
        state.active_run.as_mut().unwrap().server = ServerId::RnD;
        assert_eq!(
            check_requirement(
                &state,
                &EffectRequirement::RezzedDuringRunAgainstThisServer,
                Side::Corp, &ResolutionContext::for_card(Some(&ping)),
                &CardRegistry::new()
            ),
            Err(RulesError::RequirementNotMet),
            "run against a different server"
        );

        state.active_run.as_mut().unwrap().server = ServerId::Hq;
        assert_eq!(
            check_requirement(
                &state,
                &EffectRequirement::RezzedDuringRunAgainstThisServer,
                Side::Corp, &ResolutionContext::for_card(Some(&ping)),
                &CardRegistry::new()
            ),
            Ok(())
        );
    }

    /// A second `DealDamage` in the same `Sequence` overwrites the first's
    /// discards rather than accumulating them, so
    /// `LastDamageTrashedOddCostCard` always answers about the most recent
    /// damage — the "last" its name promises.
    #[test]
    fn a_second_deal_damage_replaces_the_first_ones_discards_in_the_context() {
        let mut registry = CardRegistry::new();
        for (id, cost) in [("odd_cost", 3u32), ("even_a", 2), ("even_b", 4)] {
            registry.insert(crate::cards::common::base_card(id, id, Side::Runner, crate::dsl::CardType::Event, cost));
        }

        let mut state = game_state();
        // Grip is drawn from randomly, so stack it with one card per hit to
        // make which card each `DealDamage` discards deterministic.
        state.runner.grip = vec![CardId("odd_cost".to_string())];
        let mut ctx = ResolutionContext::default();

        evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 1), &mut ctx, &registry).unwrap();
        assert_eq!(ctx.damage_discarded, vec![CardId("odd_cost".to_string())]);
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastDamageTrashedOddCostCard, Side::Corp, &ctx, &registry),
            Ok(()),
            "the odd-cost card was just discarded"
        );

        state.runner.grip = vec![CardId("even_a".to_string())];
        evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 1), &mut ctx, &registry).unwrap();
        assert_eq!(ctx.damage_discarded, vec![CardId("even_a".to_string())], "replaced, not appended");
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastDamageTrashedOddCostCard, Side::Corp, &ctx, &registry),
            Err(RulesError::RequirementNotMet),
            "the *last* damage trashed an even-cost card, so the earlier odd one must not carry over"
        );
    }

    #[test]
    fn last_damage_trashed_odd_cost_card_requirement_checks_registry_cost() {
        let state = game_state();
        let mut registry = CardRegistry::new();
        registry.insert(crate::cards::common::base_card(
            "odd_cost",
            "Odd Cost",
            Side::Runner,
            crate::dsl::CardType::Event,
            3,
        ));
        registry.insert(crate::cards::common::base_card(
            "even_cost",
            "Even Cost",
            Side::Runner,
            crate::dsl::CardType::Event,
            2,
        ));

        // The discards now live on the resolution in flight, not on
        // `GameState` — same assertions, read from the new home.
        let mut ctx = ResolutionContext {
            damage_discarded: vec![CardId("even_cost".to_string())],
            ..ResolutionContext::default()
        };
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastDamageTrashedOddCostCard, Side::Corp, &ctx, &registry),
            Err(RulesError::RequirementNotMet)
        );

        ctx.damage_discarded = vec![CardId("even_cost".to_string()), CardId("odd_cost".to_string())];
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastDamageTrashedOddCostCard, Side::Corp, &ctx, &registry),
            Ok(())
        );

        // A resolution that dealt no damage answers "not met" rather than
        // inheriting some earlier action's discards — the stale read the
        // old `GameState` field allowed.
        assert_eq!(
            check_requirement(
                &state,
                &EffectRequirement::LastDamageTrashedOddCostCard,
                Side::Corp,
                &ResolutionContext::default(),
                &registry
            ),
            Err(RulesError::RequirementNotMet)
        );
    }

    #[test]
    fn last_run_was_on_hq_or_rnd_requirement() {
        let mut state = game_state();
        state.last_completed_run = Some(CompletedRun { server: ServerId::Archives, cards_accessed: 0, agendas_stolen: 0, persistent_trashed_upgrades: Vec::new(), accessed_cards: Vec::new(), on_end_effect: None, on_end_card: None, on_end_install: None, run_credits_left: 0 });
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastRunWasOnHqOrRnD, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );

        state.last_completed_run = Some(CompletedRun { server: ServerId::Hq, cards_accessed: 2, agendas_stolen: 0, persistent_trashed_upgrades: Vec::new(), accessed_cards: Vec::new(), on_end_effect: None, on_end_card: None, on_end_install: None, run_credits_left: 0 });
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastRunWasOnHqOrRnD, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Ok(())
        );
    }

    #[test]
    fn and_requirement_requires_both_sides() {
        let mut state = game_state();
        state.runner.tags = 1;
        let req = EffectRequirement::And(
            Box::new(EffectRequirement::IsTagged),
            Box::new(EffectRequirement::RunnerCreditsAtMost(10)),
        );
        assert_eq!(check_requirement(&state, &req, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));

        state.runner.resources.credits = Credits(11);
        assert_eq!(
            check_requirement(&state, &req, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );
    }
}
