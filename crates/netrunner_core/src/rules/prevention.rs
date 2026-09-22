//! Prevention: the one door everything a card can prevent goes through.
//!
//! Damage, a tag and the trashing of an installed card by a card's text
//! are each about to happen before they happen. `would` is what the effect
//! that does one calls instead of doing it: it parks the thing in
//! `GameState::pending_prevention`, lets both players use the abilities
//! that prevent some of it, and makes what is left happen.
//!
//! **What is prevented is a word (`dsl::Preventable`), never a mechanism of
//! its own.** Prevention was two special cases — `Effect::PreventDamage`
//! and `PreventTrash`, a two-variant enum with a field per kind, an error
//! for mixing them up — and tags would have been a third; no card in the
//! pool used either, so neither sweep had ever opened the window, and five
//! things about it were wrong in ways only an agent would have found:
//!
//! - **The window took the one window slot and never gave it back.** A
//!   prevention opened while a run's window was open replaced it
//!   (`paid_ability_window` is one `Option`), and whatever window the
//!   game's own flow opened next replaced the prevention. It nests now:
//!   the window it opens over, and any the flow opens under it, wait in
//!   `PendingPrevention::interrupted` (`paid_ability::open_window_for`).
//! - **Anything could be done in it.** Only an interrupt may be now — an
//!   ability whose effect holds an `Effect::Prevent` (`Effect::prevents`) —
//!   and `engine::apply_action` refuses every action but that and a pass
//!   while something is parked. An ability that dealt damage inside the
//!   window would have replaced the parked damage with its own.
//! - **It opened for a card nobody could use.** The gate was "some card in
//!   play prints a prevention"; it is "somebody can use one on *this*" —
//!   requirement met, cost affordable, and the word matches (`could_prevent`)
//!   — so a Runner with no credits is not asked, twice, about every point
//!   of damage for the rest of the game.
//! - **Most trashes never reached it.** Every card in the pool that trashes
//!   a Runner program does it through a selection (`PromptChooseCards` into
//!   a discard pile), which moved the card itself.
//! - **A card that listens for it could never have heard it in time.** The
//!   announcement is dispatched with the thing already parked, and the
//!   dispatcher queues a plan rather than fire it into a parked state — so
//!   "when you would suffer damage" resolved after the damage. See `settle`.
//!
//! **A cost is not prevented.** `Cost::TakeTags`, a card trashed to pay for
//! an ability, the Runner's paid trash of an accessed card and the
//! memory-limit trash are not the text of a card doing something to a
//! player, and stay direct.
//!
//! **One thing is parked at a time.** Something preventable that happens
//! while another is parked — a "would" trigger that deals damage — happens
//! at once: `pending_prevention` is one slot, and no card in the pool could
//! tell the difference.

use crate::cards::CardRegistry;
use crate::dsl::{CardId, Cost, Effect, EndRunPrevention, Preventable, Trigger};
use crate::rules::ability::{self, ResolutionContext};
use crate::rules::damage;
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::payment::Purpose;
use crate::rules::event::GameEvent;
use crate::rules::lingering::Lingering;
use crate::rules::paid_ability;
use crate::rules::state::{GameState, InstallSlot, PendingPrevention, PreventionResume, Side, WindowCheckpoint, WouldHappen};

/// Whether `word` — what a card says it prevents — is about `what`.
pub(crate) fn matches(word: &Preventable, what: &WouldHappen, state: &GameState, registry: &CardRegistry) -> bool {
    match (word, what) {
        (Preventable::Damage { kind, .. }, WouldHappen::Damage { kind: dealt, .. }) => kind.is_none_or(|kind| kind == *dealt),
        (Preventable::Tags(_), WouldHappen::Tags { .. }) => true,
        (Preventable::Trash(filter), WouldHappen::Trash { owner, install }) => {
            let card = match owner {
                Side::Corp => state.find_corp_install(*install).map(|installed| &installed.card),
                Side::Runner => state.find_rig_install(*install).map(|installed| &installed.card),
            };
            card.and_then(|card| registry.get(card)).is_some_and(|definition| crate::dsl::card_matches_filter(definition, filter))
        }
        _ => false,
    }
}

/// How much of it one use prevents.
fn prevents_up_to(word: &Preventable) -> u32 {
    match word {
        Preventable::Damage { up_to, .. } => *up_to,
        Preventable::Tags(amount) => *amount,
        Preventable::Trash(_) => 1,
    }
}

/// Whether either player could use an interrupt on `what` right now: a
/// paid ability on one of their active cards whose effect prevents it,
/// whose requirement is met and whose cost they can pay. The gate on
/// opening the window at all — see the module doc.
pub(crate) fn could_prevent(state: &GameState, registry: &CardRegistry, what: &WouldHappen) -> bool {
    [Side::Runner, Side::Corp].into_iter().any(|side| {
        paid_ability::active_cards_of(state, side).into_iter().any(|(install, card_id)| {
            let Some(card) = registry.get(&card_id) else { return false };
            let ctx = ResolutionContext::for_install(install, &card_id);
            card.abilities.iter().any(|ability| {
                let user = ability.used_by.unwrap_or(side);
                ability.trigger == Trigger::Paid
                    && ability.effect.prevents().is_some_and(|word| matches(&word, what, state, registry))
                    && ability.requirement.as_ref().is_none_or(|req| ability::check_requirement(state, req, user, &ctx, registry).is_ok())
                    && ability.cost.as_ref().is_none_or(|cost| ability::cost_is_affordable(state, registry, user, cost, Purpose::Ability(card), &ctx))
            })
        })
    })
}

/// `what` is about to happen. Parks it, tells the cards that listen for
/// it, asks the players if somebody could prevent some of it, and makes
/// what is left happen — within this resolution when there was nobody to
/// wait for, exactly as it did before there was anything to ask.
///
/// **Damage is always announced; a tag or a trash only when the players
/// will be asked.** "When you would suffer damage" is a moment a card can
/// hear (`Trigger::OnDamageAboutToResolve`, Net Shield), and "the first
/// time each turn" counts every occurrence of it, including the ones
/// before the card was installed (the Turn History Rule) — so it goes
/// through `dispatch_event` whether or not anybody is listening. A tag or
/// a trash about to happen is an occurrence of nothing
/// (`listeners::moments`), and is only worth a line in the record when
/// something comes of it.
///
/// The caller asks `could_prevent` itself first only where the two paths
/// differ in how they make the thing happen (a trash, which the direct
/// path resolves from a `CardTarget`); damage and tags simply call this.
pub(crate) fn would(
    state: &mut GameState,
    registry: &CardRegistry,
    what: WouldHappen,
    ctx: &mut ResolutionContext<'_>,
) -> Result<Vec<GameEvent>, RulesError> {
    // None of it is nothing: Urtica Cipher with no counters on it deals 0
    // net damage, and 0 damage is not damage — nothing for Net Shield to be
    // asked about or for the turn to count as its first, and nothing for
    // "whenever you do damage" to hear. It used to be a
    // `DamageTaken { amount: 0 }`, dispatched like any other.
    if what.amount() == 0 {
        return Ok(Vec::new());
    }
    let heard = matches!(what, WouldHappen::Damage { .. });
    if state.pending_prevention.is_some() || !(heard || could_prevent(state, registry, &what)) {
        let responsible = responsible_for(registry, ctx.acting_card);
        return happen(state, registry, &what, what.amount(), responsible, Some(ctx));
    }
    state.pending_prevention = Some(PendingPrevention {
        what: what.clone(),
        prevented: 0,
        interrupted: None,
        source_card: ctx.acting_card.cloned(),
        source_install: ctx.acting_install,
        resume: PreventionResume::None,
    });
    let about_to = GameEvent::AboutToResolve { what };
    let mut events = Vec::new();
    dispatcher::emit(state, registry, &mut events, about_to)?;
    events.extend(settle_within(state, registry, Some(ctx))?.unwrap_or_default());
    Ok(events)
}

/// A run is about to be ended by a card's text: asks the standing
/// prevention, if one holds. `Some` when it stepped in — the run has not
/// ended, and what decides whether it does is parked — and `None` when the
/// run ends as it would have.
///
/// **A prevention that stands for a duration is a lingering effect
/// (`Lingering::PreventRunEnding`), not an interrupt**: nobody uses it, so
/// there is no window, and it is asked here, first, the way jinteki puts
/// its static preventions ahead of the ones a player chooses. Shred is the
/// one card: "The first time the Corp would end that run, prevent the run
/// from ending unless the Corp reveals and trashes X cards from HQ at
/// random" — *the first time* is this function taking the entry off the
/// list, used or not; *the Corp* is every `Effect::EndTheRun` there is,
/// since no Runner card in the pool prints one. It was
/// `RunState::end_run_prevention`, consumed at the same place, which no
/// view carried.
pub(crate) fn run_ending(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: &mut ResolutionContext<'_>,
) -> Result<Option<Vec<GameEvent>>, RulesError> {
    let Some(position) =
        state.lingering.iter().position(|effect| matches!(effect.what, Lingering::PreventRunEnding(_)) && effect.holds(state))
    else {
        return Ok(None);
    };
    let Lingering::PreventRunEnding(unless) = state.lingering.remove(position).what else { return Ok(None) };
    let Some(server) = state.active_run.as_ref().map(|run| run.server) else { return Ok(None) };
    match unless {
        // X is the root of the server being attacked *now*: a run
        // redirected since the prevention was armed counts the new one.
        // With an empty root there is nothing to pay and the run simply
        // ends; with fewer than X cards in HQ the Corp cannot pay and the
        // run goes on.
        EndRunPrevention::UnlessCorpTrashesRootCountFromHq => {
            let root = state.corp.installed.iter().filter(|c| c.server == server && c.slot == InstallSlot::Root).count() as u32;
            if root == 0 {
                return Ok(None);
            }
            let mut events = vec![GameEvent::RunEndPrevented { server }];
            events.extend(ability::evaluate_effect(
                state,
                &Effect::OfferPaidChoice {
                    side: Side::Corp,
                    cost: Cost::TrashRandomFromHq(root),
                    if_paid: Box::new(Effect::EndTheRun),
                    if_declined: Box::new(Effect::Sequence(Vec::new())),
                    text: None,
                },
                ctx,
                registry,
            )?);
            Ok(Some(events))
        }
    }
}

/// Moves a parked prevention along when nothing stands in front of it:
/// the cards that heard it resolve, then the window opens if somebody
/// could still prevent some of what is left, and otherwise it happens.
/// `None` when there was nothing to do — nothing parked, the window
/// already open, or a decision one of those cards parked still waiting for
/// its answer.
///
/// Called by `would`, and once more at the end of every action
/// (`engine::apply_action`): the answer to that decision arrives as an
/// action of its own, and nothing else would come back for the parked
/// thing behind it.
///
/// **The cards that heard it resolve here, one at a time, not in the
/// dispatch that announced it.** Something is parked by then, and
/// `dispatcher::fire_plan` queues a plan rather than fire it into a parked
/// state — which is why a trigger on damage about to resolve could never
/// have fired in play before this, only in a test that dispatched the
/// event by hand. They resolve in the plan's order (the active player's
/// first, then as installed); a player's choice among their own is not
/// offered, since no card in the pool could make the order matter.
pub(crate) fn settle(state: &mut GameState, registry: &CardRegistry) -> Result<Option<Vec<GameEvent>>, RulesError> {
    settle_within(state, registry, None)
}

fn settle_within(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: Option<&mut ResolutionContext<'_>>,
) -> Result<Option<Vec<GameEvent>>, RulesError> {
    let blocked = |state: &GameState| {
        state.is_over() || state.active_trace.is_some() || state.pending_paid_choice.is_some() || state.pending_decision.is_some()
    };
    if state.pending_prevention.is_none() || asking(state) || blocked(state) {
        return Ok(None);
    }
    let mut events = Vec::new();
    while let Some(position) =
        state.deferred_triggers.iter().position(|due| due.trigger == Trigger::OnDamageAboutToResolve && due.continuation.is_none())
    {
        let due = state.deferred_triggers.remove(position);
        events.extend(dispatcher::fire_deferred(state, registry, &due)?);
        if blocked(state) {
            return Ok(Some(events));
        }
    }
    let Some(pending) = state.pending_prevention.as_ref() else { return Ok(Some(events)) };
    let left = pending.what.clone();
    if pending.prevented < left.amount() && could_prevent(state, registry, &left) {
        let interrupted = state.paid_ability_window.take();
        if let Some(pending) = state.pending_prevention.as_mut() {
            pending.interrupted = interrupted;
        }
        events.push(paid_ability::open_window_for(state, left.affects(), WindowCheckpoint::Prevention));
        return Ok(Some(events));
    }
    events.extend(finish_within(state, registry, ctx)?);
    Ok(Some(events))
}

/// Whether the prevention window is the one open.
pub(crate) fn asking(state: &GameState) -> bool {
    state.paid_ability_window.as_ref().is_some_and(|window| window.checkpoint == WindowCheckpoint::Prevention)
}

/// `Effect::Prevent`: counts `word` against what is parked. Refuses when
/// nothing it is about is parked or none of it is left, which is what
/// keeps an interrupt out of `legal_actions` everywhere but its window.
pub(crate) fn prevent(state: &mut GameState, registry: &CardRegistry, word: &Preventable) -> Result<Vec<GameEvent>, RulesError> {
    let pending = state.pending_prevention.as_ref().ok_or(RulesError::NothingToPrevent)?;
    let left = pending.what.amount().saturating_sub(pending.prevented);
    if left == 0 || !matches(word, &pending.what, state, registry) {
        return Err(RulesError::NothingToPrevent);
    }
    if let Some(pending) = state.pending_prevention.as_mut() {
        pending.prevented += prevents_up_to(word).min(left);
    }
    Ok(Vec::new())
}

/// After an interrupt resolved: with all of it prevented, or nobody able
/// to prevent any more of it, there is nothing left to ask, and the window
/// closes without waiting for two passes.
pub(crate) fn after_interrupt(state: &mut GameState, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    let Some(pending) = state.pending_prevention.as_ref().filter(|_| asking(state)) else { return Ok(Vec::new()) };
    if pending.prevented >= pending.what.amount() || !could_prevent(state, registry, &pending.what) {
        return finish(state, registry);
    }
    Ok(Vec::new())
}

/// The asking is over: says what was prevented, makes the rest happen, and
/// gives the window slot back to whatever was waiting for it.
pub(crate) fn finish(state: &mut GameState, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    finish_within(state, registry, None)
}

fn finish_within(
    state: &mut GameState,
    registry: &CardRegistry,
    ctx: Option<&mut ResolutionContext<'_>>,
) -> Result<Vec<GameEvent>, RulesError> {
    let Some(pending) = state.pending_prevention.take() else { return Ok(Vec::new()) };
    let mut events = Vec::new();
    if asking(state) {
        state.paid_ability_window = None;
        events.push(GameEvent::PaidAbilityWindowClosed);
    }
    let prevented = pending.prevented.min(pending.what.amount());
    if prevented > 0 {
        events.push(GameEvent::Prevented { what: pending.what.clone(), amount: prevented });
    }
    // The waiting window goes back before the rest happens: what happens
    // can end the game (`win::end_game` clears the slot) or the run. A
    // run's window whose run is gone has nothing to resume
    // (`paid_ability::note_window_action` says why that must not linger).
    if let Some(window) = pending.interrupted
        && state.paid_ability_window.is_none()
        && !(window.checkpoint == WindowCheckpoint::Run && state.active_run.is_none())
    {
        state.paid_ability_window = Some(window);
    }
    let left = pending.what.amount() - prevented;
    if left > 0 {
        let responsible = responsible_for(registry, pending.source_card.as_ref());
        events.extend(happen(state, registry, &pending.what, left, responsible, ctx)?);
    }
    if state.paid_ability_window.as_ref().is_some_and(|w| w.checkpoint == WindowCheckpoint::Run) && state.active_run.is_none() {
        state.paid_ability_window = None;
    }
    if pending.resume == PreventionResume::ResumeSubroutines {
        events.extend(paid_ability::resolve_encounter_ice(state, registry)?);
    }
    Ok(events)
}

/// Who is responsible for what a card's text does (CR 10.4.1): that card's
/// side. Read off the card rather than stored on `WouldHappen`, because a
/// parked prevention already remembers its card (`source_card`).
fn responsible_for(registry: &CardRegistry, card: Option<&CardId>) -> Option<Side> {
    card.and_then(|card| registry.get(card)).map(|def| def.side)
}

/// Makes `amount` of `what` happen. `ctx` is the resolution it is part of
/// when there still is one: damage that was parked resumes on a later
/// action, where no `Sequence` is left to read its discards
/// (`ResolutionContext::damage_discarded`).
fn happen(
    state: &mut GameState,
    registry: &CardRegistry,
    what: &WouldHappen,
    amount: u32,
    responsible: Option<Side>,
    ctx: Option<&mut ResolutionContext<'_>>,
) -> Result<Vec<GameEvent>, RulesError> {
    match what {
        WouldHappen::Damage { kind, .. } => {
            let (mut events, discarded) = damage::apply_damage(state, *kind, amount as usize, responsible);
            // Overwrite rather than append: the requirement reading this
            // (`LastDamageTrashedOddCostCard`) asks about the *most
            // recent* damage, so a second `DealDamage` in the same
            // `Sequence` must not be answered from the first's discards.
            if let Some(ctx) = ctx {
                ctx.damage_discarded = discarded;
            }
            events.extend(ability::dispatch_damage_taken(state, registry, &events)?);
            Ok(events)
        }
        WouldHappen::Tags { .. } => {
            // Always the Runner — see `Effect::GiveTags`.
            state.runner.tags = state.runner.tags.saturating_add(amount);
            let mut events = Vec::new();
            // Dispatched, so NBN: Reality Plus hears a tag whichever card
            // gave it.
            dispatcher::emit(state, registry, &mut events, GameEvent::TagsGiven { side: Side::Runner, amount })?;
            Ok(events)
        }
        WouldHappen::Trash { owner, install } => Ok(ability::trash_install(state, registry, *owner, *install)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{AbilityDef, CardDefinition, CardFilter, CardId, CardType, CardZoneRef, Cost, DamageType, Effect};
    use crate::rules::engine::apply_action;
    use crate::rules::state::{Credits, GamePhase, InstallId, InstallSlot, InstalledCard, InstalledRunnerCard, PaidAbilityWindow};
    use crate::rules::{PlayerAction, ServerId};

    fn id(card: &str) -> CardId {
        CardId(card.to_string())
    }

    fn with_paid_ability(card: &str, side: Side, card_type: CardType, cost: Option<Cost>, effect: Effect) -> CardDefinition {
        CardDefinition {
            id: id(card),
            title: card.to_string(),
            side,
            card_type,
            abilities: vec![AbilityDef { text: None, trigger: Trigger::Paid, cost, requirement: None, effect, cost_discount_if: None, used_by: None, access: false }],
            is_playable: true,
            ..Default::default()
        }
    }

    const SOURCE: InstallId = InstallId(1);
    const DECOY: InstallId = InstallId(2);
    const PROGRAM: InstallId = InstallId(3);

    /// The Corp's turn, a rezzed asset whose paid ability does `effect`,
    /// and a Runner with a program and an interrupt in the rig.
    fn table(effect: Effect, interrupt: CardDefinition) -> (GameState, CardRegistry) {
        let mut state = GameState { phase: GamePhase::Action(Side::Corp), ..Default::default() };
        state.corp.resources.clicks = crate::rules::state::Clicks(3);
        state.runner.resources.credits = Credits(5);
        state.runner.grip = vec![id("a"), id("b"), id("c")];
        state.corp.installed = vec![InstalledCard {
            card: id("source"),
            install_id: SOURCE,
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: true,
            ..Default::default()
        }];
        state.runner.rig = vec![
            InstalledRunnerCard { card: interrupt.id.clone(), install_id: DECOY, ..Default::default() },
            InstalledRunnerCard { card: id("program"), install_id: PROGRAM, ..Default::default() },
        ];
        let registry = CardRegistry::from_cards(vec![
            with_paid_ability("source", Side::Corp, CardType::Asset, None, effect),
            interrupt,
            CardDefinition { id: id("program"), side: Side::Runner, card_type: CardType::Program, is_playable: true, ..Default::default() },
        ]);
        (state, registry)
    }

    fn decoy() -> CardDefinition {
        with_paid_ability("decoy", Side::Runner, CardType::Resource, Some(Cost::TrashSelf), Effect::Prevent(Preventable::Tags(1)))
    }

    fn act(state: &GameState, registry: &CardRegistry, action: PlayerAction) -> (GameState, Vec<GameEvent>) {
        apply_action(state, registry, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"))
    }

    fn use_ability(target: InstallId) -> PlayerAction {
        PlayerAction::ActivateAbility { target, ability_index: 0 }
    }

    #[test]
    fn a_tag_is_prevented_by_an_interrupt_and_the_window_closes_with_nothing_left_to_ask() {
        let (state, registry) = table(Effect::GiveTags(1), decoy());
        let (state, events) = act(&state, &registry, use_ability(SOURCE));
        assert_eq!(state.runner.tags, 0, "parked, not given");
        assert!(asking(&state));
        assert_eq!(state.paid_ability_window.as_ref().map(|w| w.active_priority), Some(Side::Runner), "who it happens to is asked first");
        assert!(events.contains(&GameEvent::AboutToResolve { what: WouldHappen::Tags { amount: 1 } }));

        let (state, events) = act(&state, &registry, use_ability(DECOY));
        assert_eq!(state.runner.tags, 0);
        assert!(state.pending_prevention.is_none() && state.paid_ability_window.is_none(), "all of it prevented: nobody is asked again");
        assert!(events.contains(&GameEvent::Prevented { what: WouldHappen::Tags { amount: 1 }, amount: 1 }));
        assert!(!events.iter().any(|event| matches!(event, GameEvent::TagsGiven { .. })));
        assert_eq!(state.runner.heap, vec![id("decoy")], "the interrupt's cost was paid");
    }

    #[test]
    fn what_is_left_happens_once_nobody_can_prevent_more_of_it() {
        let (state, registry) = table(Effect::GiveTags(2), decoy());
        let (state, _) = act(&state, &registry, use_ability(SOURCE));
        let (state, events) = act(&state, &registry, use_ability(DECOY));
        assert_eq!(state.runner.tags, 1, "one tag of two was prevented, and the only interrupt is spent");
        assert!(state.pending_prevention.is_none() && !asking(&state));
        assert!(events.contains(&GameEvent::TagsGiven { side: Side::Runner, amount: 1 }));
    }

    /// The gate is "somebody can use one on this", not "a card that
    /// prevents something is in play".
    #[test]
    fn nobody_is_asked_about_what_nobody_can_prevent() {
        let paid = with_paid_ability(
            "shield",
            Side::Runner,
            CardType::Program,
            Some(Cost::Credits(1)),
            Effect::Prevent(Preventable::Damage { kind: Some(DamageType::Net), up_to: 1 }),
        );
        // The wrong kind of damage.
        let (state, registry) = table(Effect::DealDamage(DamageType::Meat, 1), paid.clone());
        let (state, _) = act(&state, &registry, use_ability(SOURCE));
        assert!(state.pending_prevention.is_none() && state.runner.grip.len() == 2);
        // The right kind and no credit to pay with.
        let (mut state, registry) = table(Effect::DealDamage(DamageType::Net, 1), paid.clone());
        state.runner.resources.credits = Credits(0);
        let (state, _) = act(&state, &registry, use_ability(SOURCE));
        assert!(state.pending_prevention.is_none() && state.runner.grip.len() == 2);
        // A tag, which a net-damage shield says nothing about.
        let (state, registry) = table(Effect::GiveTags(1), paid);
        let (state, _) = act(&state, &registry, use_ability(SOURCE));
        assert!(state.pending_prevention.is_none() && state.runner.tags == 1);
    }

    #[test]
    fn only_an_interrupt_or_a_pass_while_the_players_are_asked() {
        let (mut state, mut registry) = table(Effect::GiveTags(1), decoy());
        registry.insert(with_paid_ability("program", Side::Runner, CardType::Program, None, Effect::GainCredits(Side::Runner, 1)));
        state.runner.resources.clicks = crate::rules::state::Clicks(1);
        let (state, _) = act(&state, &registry, use_ability(SOURCE));
        for refused in [use_ability(PROGRAM), PlayerAction::GainCreditClick { side: Side::Corp }, use_ability(SOURCE)] {
            assert_eq!(apply_action(&state, &registry, refused.clone()).err(), Some(RulesError::ActionBlockedByPrevention), "{refused:?}");
        }
        let (state, _) = act(&state, &registry, PlayerAction::PassPriority { side: Side::Runner });
        let (state, _) = act(&state, &registry, PlayerAction::PassPriority { side: Side::Corp });
        assert_eq!(state.runner.tags, 1);
        assert!(state.pending_prevention.is_none() && !asking(&state));
    }

    /// An interrupt is used when the players are asked, never in a window
    /// of its owner's choosing — so it is no reason to open one after every
    /// action the other player takes.
    #[test]
    fn an_interrupt_is_not_a_usable_paid_ability_outside_its_window() {
        let (state, registry) = table(Effect::GiveTags(1), decoy());
        assert!(!paid_ability::has_usable_paid_ability(&state, &registry, Side::Runner));
        assert_eq!(apply_action(&state, &registry, use_ability(DECOY)).err().map(|_| ()), Some(()), "and cannot be used with nothing parked");
    }

    /// `paid_ability_window` is one slot. A prevention opened during a run's
    /// window used to take it, and the run's window was never seen again.
    #[test]
    fn the_window_it_opens_over_comes_back_when_it_closes() {
        let (mut state, registry) = table(Effect::DealDamage(DamageType::Net, 1), {
            with_paid_ability("shield", Side::Runner, CardType::Program, Some(Cost::Credits(1)), Effect::Prevent(Preventable::Damage { kind: None, up_to: 1 }))
        });
        state.phase = GamePhase::Action(Side::Runner);
        state.active_run = Some(crate::rules::run::RunState { phase: crate::rules::run::RunPhase::Success, ..Default::default() });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 1,
            checkpoint: WindowCheckpoint::Run,
            return_phase: Box::new(state.phase),
        });
        let (state, _) = act(&state, &registry, use_ability(SOURCE));
        assert!(asking(&state));
        let beneath = state.pending_prevention.as_ref().and_then(|pending| pending.interrupted.clone()).expect("the run's window waits beneath");
        assert_eq!((beneath.checkpoint, beneath.active_priority, beneath.consecutive_passes), (WindowCheckpoint::Run, Side::Runner, 0), "and it, not the prevention window, took the toggle for the Corp's action");

        let (state, _) = act(&state, &registry, use_ability(DECOY));
        assert_eq!(state.runner.grip.len(), 3, "prevented");
        let window = state.paid_ability_window.as_ref().expect("the run's window is back");
        assert_eq!((window.checkpoint, window.active_priority), (WindowCheckpoint::Run, Side::Runner));
    }

    /// …and a window the game's own flow opens while the players are asked
    /// waits beneath, rather than taking the slot from the prevention.
    #[test]
    fn a_window_the_flow_opens_while_the_players_are_asked_waits_beneath() {
        let (state, registry) = table(Effect::GiveTags(1), decoy());
        let (mut state, _) = act(&state, &registry, use_ability(SOURCE));
        paid_ability::open_window_for(&mut state, Side::Corp, WindowCheckpoint::PostAction { side: Side::Corp });
        assert!(asking(&state));
        let (state, _) = act(&state, &registry, use_ability(DECOY));
        assert_eq!(state.paid_ability_window.map(|w| w.checkpoint), Some(WindowCheckpoint::PostAction { side: Side::Corp }));
    }

    /// Every card in the pool that trashes a Runner program does it through
    /// a selection, which used to move the card past the window.
    #[test]
    fn a_program_the_corp_selects_to_trash_can_be_saved() {
        let trash = Effect::PromptChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OpponentInstalled,
            filter: CardFilter::CardType(CardType::Program),
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: Some(CardZoneRef::OpponentDiscard),
            then: None,
        };
        let construct = with_paid_ability(
            "construct",
            Side::Runner,
            CardType::Resource,
            Some(Cost::TrashSelf),
            Effect::Prevent(Preventable::Trash(CardFilter::CardTypeOneOf(vec![CardType::Program, CardType::Hardware]))),
        );
        let (state, registry) = table(trash, construct);
        let (state, _) = act(&state, &registry, use_ability(SOURCE));
        let position = crate::rules::legal_actions::legal_actions(&state, &registry)
            .into_iter()
            .find(|action| matches!(action, PlayerAction::ToggleCardSelection { .. }))
            .expect("the program is offered");
        let (state, _) = act(&state, &registry, position);
        let (state, events) = act(&state, &registry, PlayerAction::ConfirmCardSelection);
        assert!(asking(&state), "{events:?}");
        assert!(state.find_rig_install(PROGRAM).is_some(), "still installed while the players are asked");

        let (saved, events) = act(&state, &registry, use_ability(DECOY));
        assert!(saved.find_rig_install(PROGRAM).is_some());
        assert_eq!(saved.runner.heap, vec![id("construct")]);
        assert!(events.contains(&GameEvent::Prevented { what: WouldHappen::Trash { owner: Side::Runner, install: PROGRAM }, amount: 1 }));

        let (lost, _) = act(&state, &registry, PlayerAction::PassPriority { side: Side::Runner });
        let (lost, events) = act(&lost, &registry, PlayerAction::PassPriority { side: Side::Corp });
        assert!(lost.find_rig_install(PROGRAM).is_none());
        assert!(events.contains(&GameEvent::CardTrashed { side: Side::Runner, card: id("program") }));
    }

    /// Snare!'s shape: the damage parks, and the tag behind it in the same
    /// `Sequence` is still given once the asking is over — and is asked
    /// about in its turn.
    #[test]
    fn the_rest_of_a_sequence_follows_the_parked_thing() {
        let (mut state, mut registry) = table(Effect::Sequence(vec![Effect::DealDamage(DamageType::Net, 1), Effect::GiveTags(1)]), decoy());
        registry.insert(with_paid_ability(
            "program",
            Side::Runner,
            CardType::Program,
            Some(Cost::Credits(1)),
            Effect::Prevent(Preventable::Damage { kind: None, up_to: 1 }),
        ));
        state.runner.resources.credits = Credits(1);
        let (state, _) = act(&state, &registry, use_ability(SOURCE));
        assert!(matches!(state.pending_prevention.as_ref().map(|p| &p.what), Some(WouldHappen::Damage { .. })));
        let (state, _) = act(&state, &registry, use_ability(PROGRAM));
        assert_eq!(state.runner.grip.len(), 3, "the damage was prevented");
        assert!(matches!(state.pending_prevention.as_ref().map(|p| &p.what), Some(WouldHappen::Tags { amount: 1 })), "and the tag is next");
        let (state, _) = act(&state, &registry, use_ability(DECOY));
        assert_eq!(state.runner.tags, 0);
        assert!(state.pending_prevention.is_none() && state.deferred_triggers.is_empty());
    }

    /// A subroutine's *choice* gave the tag. "The subroutines are not
    /// finished" was passed on to a decision or a paid choice the chosen
    /// effect parked, and to nothing else, so a tag parked for prevention
    /// lost it: the ice's later subroutines never fired and nothing was
    /// left to end the encounter.
    #[test]
    fn a_tag_parked_out_of_a_subroutines_choice_still_resumes_the_subroutines() {
        use crate::dsl::{IceType, SubroutineDef};
        use crate::rules::run::{EncounteredSubroutine, RunIce, RunPhase, RunState, SubroutineStatus};
        let (mut state, registry) = table(Effect::GiveTags(1), decoy());
        state.phase = GamePhase::Action(Side::Runner);
        let subroutine = |id, effect: Effect, status| EncounteredSubroutine {
            id,
            definition: SubroutineDef { text: String::new(), effect, only_breakable_by: None },
            status,
        };
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                install_id: InstallId::PLACEHOLDER,
                card_id: id("ice"),
                ice_type: IceType::Sentry,
                subroutines: vec![
                    subroutine(0, Effect::GiveTags(1), SubroutineStatus::Resolved),
                    subroutine(1, Effect::EndTheRun, SubroutineStatus::Pending),
                ],
                rezzed: true,
            }],
            ..Default::default()
        });
        crate::rules::test_support::install_the_runs_ice(&mut state);
        state.pending_decision = Some(crate::rules::PendingDecision::ChooseEffect {
            chooser: Side::Runner,
            options: vec![Effect::GiveTags(1)],
            option_texts: Vec::new(),
            source_card: Some(id("ice")),
            prompting_card: None,
            source_install: None,
            resume: crate::rules::state::PendingChoiceResume::ResumeSubroutines,
        });
        let (state, _) = act(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 });
        assert!(asking(&state));
        let (state, _) = act(&state, &registry, PlayerAction::PassPriority { side: Side::Runner });
        let (state, _) = act(&state, &registry, PlayerAction::PassPriority { side: Side::Corp });
        assert_eq!(state.runner.tags, 1);
        assert!(state.active_run.is_none(), "the second subroutine ended the run");
    }

    /// Urtica Cipher with no counters on it: 0 net damage is not damage.
    #[test]
    fn none_of_it_is_not_an_occurrence() {
        let (mut state, registry) = table(Effect::GiveTags(1), decoy());
        let source = id("source");
        for nothing in [Effect::DealDamage(DamageType::Net, 0), Effect::GiveTags(0)] {
            let events = ability::evaluate_effect(&mut state, &nothing, &mut ResolutionContext::for_card(Some(&source)), &registry).unwrap();
            assert!(events.is_empty(), "{nothing:?}: {events:?}");
        }
        assert_eq!(state.this_turn.times(Trigger::OnDamageAboutToResolve), 0, "and the turn has not had its first net damage");
        assert!(state.pending_prevention.is_none());
    }
}
