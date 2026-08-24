use crate::cards::CardRegistry;
use crate::dsl::{CardId, Cost};
use crate::rules::ability;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::state::{AccessPhase, AccessState, RunPhase, ServerId};
use crate::rules::state::{GamePhase, GameState, InstallSlot, Side};
use crate::rules::win::check_win_conditions;

/// Root (non-ICE) installs on `server` — ICE is excluded via
/// `InstalledCard::slot`, which the installing action declares explicitly
/// (see `InstallSlot`'s doc comment for why this doesn't need a full
/// `CardRegistry`). A successful run accesses these alongside whatever else
/// that server's arm below yields, since Upgrades can be installed on
/// central servers (Hq/RnD) as well as Remote ones.
fn root_installs_on(state: &GameState, server: ServerId) -> Vec<CardId> {
    state
        .corp
        .installed
        .iter()
        .filter(|installed| installed.server == server && installed.slot == InstallSlot::Root)
        .map(|installed| installed.card.clone())
        .collect()
}

/// Determine which `CardId`s become accessible when a run against `server`
/// concludes successfully. Unchanged logic from before this file's access
/// resolution became interactive — only *which* cards are accessed, not
/// what happens when they are.
fn compute_accessed_cards(state: &mut GameState, server: ServerId) -> Vec<CardId> {
    match server {
        // Real rules access one *randomly* chosen HQ card. `next_u64` is
        // `GameState`'s deterministic pseudo-random source (no external RNG,
        // per AGENTS.md's purity requirement) — the roll is reduced modulo
        // `hq.len()` to pick an index.
        ServerId::Hq => {
            let mut accessed = if state.corp.hq.is_empty() {
                Vec::new()
            } else {
                let roll = state.next_u64();
                let index = (roll as usize) % state.corp.hq.len();
                state.corp.hq.get(index).cloned().into_iter().collect()
            };
            accessed.extend(root_installs_on(state, server));
            accessed
        }
        // Real rules access one card too, but R&D isn't randomized — it's
        // drawn from a fixed deck order. `.last()` mirrors
        // `RunnerState::stack`'s "top of deck is the end of the Vec"
        // convention (see `engine.rs::draw_card_click`'s `stack.pop()`).
        ServerId::RnD => {
            let mut accessed: Vec<CardId> = state.corp.r_and_d.last().cloned().into_iter().collect();
            accessed.extend(root_installs_on(state, server));
            accessed
        }
        // Archives is fully public; a successful run accesses all of it.
        ServerId::Archives => state.corp.archives.clone(),
        ServerId::Remote(_) => root_installs_on(state, server),
    }
}

/// Builds the `AccessPhase::PendingChoice` for `card_id`, from its
/// `CardRegistry` definition (or the "unrecognized card" defaults if it
/// isn't registered — nothing stealable or trashable, so the only legal
/// resolution is `PlayerAction::PassAccessedCard`).
fn compute_pending_choice(card_id: &CardId, runner_credits: u32, registry: &CardRegistry) -> AccessPhase {
    let card_def = registry.get(card_id);
    let is_agenda = card_def.is_some_and(|c| c.agenda_points.is_some());
    let steal_cost = card_def.and_then(|c| c.steal_cost.clone());
    let mandatory_steal = is_agenda && steal_cost.is_none();
    let trash_cost = card_def.and_then(|c| c.trash_cost);
    let can_trash = trash_cost.is_some_and(|cost| runner_credits >= cost);

    AccessPhase::PendingChoice {
        card_id: card_id.clone(),
        can_trash,
        trash_cost,
        mandatory_steal,
        steal_cost,
    }
}

/// Determine which cards a successful run against `server` accesses and, if
/// any, park the run in `RunPhase::AccessingCard`. A single accessed card
/// goes straight to an `AccessState` describing its `PendingChoice` —
/// `PlayerAction::StealAgenda`/`TrashAccessedCard`/`PassAccessedCard`
/// (`resolve_steal`/`resolve_trash`/`resolve_pass` below) resolve it. Two or
/// more accessed cards instead park at `AccessPhase::SelectNextCard`, so the
/// Runner picks resolution order via `PlayerAction::SelectCardToAccess`
/// (`resolve_select_card` below) before any `PendingChoice` is presented. If
/// nothing is accessed (empty zone), clears `active_run` immediately instead
/// — there's nothing to present a choice about, so the run is simply over.
///
/// Takes `&mut GameState` because HQ access needs `GameState::next_u64` to
/// pick a pseudo-random index, and every outcome mutates `active_run`.
///
/// Never fails: an empty zone simply yields zero events.
pub fn access_server(state: &mut GameState, server: ServerId, registry: &CardRegistry) -> Vec<GameEvent> {
    let accessed = compute_accessed_cards(state, server);
    if accessed.is_empty() {
        state.active_run = None;
        return Vec::new();
    }

    let runner_credits = state.runner.resources.credits.0;
    let run = state
        .active_run
        .as_mut()
        .expect("engine::complete_run confirmed active_run is Some before calling access_server");
    run.phase = RunPhase::AccessingCard;

    if accessed.len() == 1 {
        let card_id = accessed.into_iter().next().unwrap();
        let phase = compute_pending_choice(&card_id, runner_credits, registry);
        run.access_state =
            Some(AccessState { server, unaccessed_cards: Vec::new(), resolved_cards: Vec::new(), phase });
        vec![GameEvent::CardAccessed { card: card_id, server }]
    } else {
        run.access_state = Some(AccessState {
            server,
            unaccessed_cards: accessed.clone(),
            resolved_cards: Vec::new(),
            phase: AccessPhase::SelectNextCard { selectable_cards: accessed },
        });
        Vec::new()
    }
}

/// The `AccessState` fields `resolve_steal`/`resolve_trash`/`resolve_pass`
/// need, pulled out by value so the borrow of `state.active_run` doesn't
/// outlive the check — each caller goes on to mutate `state` afterward.
struct PendingAccess {
    server: ServerId,
    mandatory_steal: bool,
    steal_cost: Option<Cost>,
    trash_cost: Option<u32>,
}

/// Confirms a run is parked in `RunPhase::AccessingCard` awaiting a choice
/// on exactly `card_id`, and returns that choice's context. Covers every
/// "wrong state to call this" case with a single error
/// (`RulesError::NotInAccessPhase`): no active run, the run isn't
/// `AccessingCard`, or `card_id` doesn't match what's actually pending —
/// mirroring how `RulesError::NotInEncounter` already covers several
/// "not in the right run sub-state" cases at once.
fn require_pending(state: &GameState, card_id: &CardId) -> Result<PendingAccess, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    if run.phase != RunPhase::AccessingCard {
        return Err(RulesError::NotInAccessPhase);
    }
    let access = run.access_state.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    let AccessPhase::PendingChoice { card_id: pending, mandatory_steal, steal_cost, trash_cost, .. } =
        &access.phase
    else {
        return Err(RulesError::NotInAccessPhase);
    };
    if pending != card_id {
        return Err(RulesError::NotInAccessPhase);
    }

    Ok(PendingAccess {
        server: access.server,
        mandatory_steal: *mandatory_steal,
        steal_cost: steal_cost.clone(),
        trash_cost: *trash_cost,
    })
}

/// The `AccessState` fields `resolve_select_card` needs, pulled out by value
/// for the same borrow-scoping reason as `PendingAccess`.
struct PendingSelection {
    server: ServerId,
    selectable_cards: Vec<CardId>,
}

/// Confirms a run is parked in `RunPhase::AccessingCard` awaiting a
/// selection (`AccessPhase::SelectNextCard`), and returns that choice's
/// context. Covers every "wrong state to call this" case with a single
/// error (`RulesError::NotInAccessPhase`), mirroring `require_pending`.
fn require_selectable(state: &GameState) -> Result<PendingSelection, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    if run.phase != RunPhase::AccessingCard {
        return Err(RulesError::NotInAccessPhase);
    }
    let access = run.access_state.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    let AccessPhase::SelectNextCard { selectable_cards } = &access.phase else {
        return Err(RulesError::NotInAccessPhase);
    };

    Ok(PendingSelection { server: access.server, selectable_cards: selectable_cards.clone() })
}

/// Resolves `PlayerAction::SelectCardToAccess`. See its doc comment for the
/// error conditions.
pub fn resolve_select_card(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_selectable(state)?;
    if !pending.selectable_cards.contains(card_id) {
        return Err(RulesError::InvalidAccessSelection { card: card_id.clone() });
    }

    let runner_credits = state.runner.resources.credits.0;
    let run = state.active_run.as_mut().expect("resolve_select_card called mid-access");
    let access = run.access_state.as_mut().expect("resolve_select_card called mid-access");
    if let Some(pos) = access.unaccessed_cards.iter().position(|c| c == card_id) {
        access.unaccessed_cards.remove(pos);
    }
    access.phase = compute_pending_choice(card_id, runner_credits, registry);

    Ok(vec![GameEvent::CardAccessed { card: card_id.clone(), server: pending.server }])
}

/// Shared tail of `resolve_steal`/`resolve_trash`/`resolve_pass`: if a steal
/// just won the game, finalize immediately without presenting further
/// accessed cards; otherwise record `resolved_card` as resolved and either
/// auto-present the last remaining card's `PendingChoice`, offer a choice
/// among 2+ remaining cards, or finalize if none remain.
fn advance_or_finish(
    state: &mut GameState,
    registry: &CardRegistry,
    server: ServerId,
    resolved_card: CardId,
) -> Vec<GameEvent> {
    if let GamePhase::GameOver(winner) = state.phase {
        state.active_run = None;
        return vec![GameEvent::GameOver { winner }, GameEvent::RunCompleted { server }];
    }

    let runner_credits = state.runner.resources.credits.0;
    let run = state.active_run.as_mut().expect("advance_or_finish called mid-access");
    let access = run.access_state.as_mut().expect("advance_or_finish called mid-access");
    access.resolved_cards.push(resolved_card);

    match access.unaccessed_cards.len() {
        0 => {
            state.active_run = None;
            vec![GameEvent::RunCompleted { server }]
        }
        1 => {
            let next_card = access.unaccessed_cards.remove(0);
            access.phase = compute_pending_choice(&next_card, runner_credits, registry);
            vec![GameEvent::CardAccessed { card: next_card, server }]
        }
        _ => {
            access.phase = AccessPhase::SelectNextCard { selectable_cards: access.unaccessed_cards.clone() };
            Vec::new()
        }
    }
}

/// Resolves `PlayerAction::StealAgenda`. See its doc comment for the error
/// conditions.
pub fn resolve_steal(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_pending(state, card_id)?;
    if !pending.mandatory_steal && pending.steal_cost.is_none() {
        return Err(RulesError::NotInAccessPhase);
    }

    let mut events = Vec::new();
    if let Some(cost) = &pending.steal_cost {
        if let Cost::Credits(requested) = cost {
            let available = state.runner.resources.credits.0;
            if available < *requested {
                return Err(RulesError::CannotAffordStealCost {
                    card: card_id.clone(),
                    available,
                    requested: *requested,
                });
            }
        }
        events.extend(ability::pay_cost(state, Side::Runner, cost)?);
    }

    state.runner.scored_agendas.push(card_id.clone());
    let agenda_points = registry.get(card_id).and_then(|c| c.agenda_points).unwrap_or(0);
    state.runner.resources.agenda_points = state.runner.resources.agenda_points.gain(agenda_points);
    events.push(GameEvent::AgendaStolen { card: card_id.clone(), agenda_points });

    check_win_conditions(state, registry);
    events.extend(advance_or_finish(state, registry, pending.server, card_id.clone()));
    Ok(events)
}

/// Removes `card_id` from wherever it currently lives (HQ, R&D, or a
/// Root-slot Corp install) and pushes it onto Archives — unless it was
/// already being accessed *from* Archives, in which case it's already
/// there and this is a no-op.
fn move_to_archives(state: &mut GameState, card_id: &CardId, server: ServerId) {
    if server == ServerId::Archives {
        return;
    }
    if let Some(pos) = state.corp.hq.iter().position(|c| c == card_id) {
        state.corp.hq.remove(pos);
    } else if let Some(pos) = state.corp.r_and_d.iter().position(|c| c == card_id) {
        state.corp.r_and_d.remove(pos);
    } else if let Some(pos) = state.corp.installed.iter().position(|c| &c.card == card_id) {
        state.corp.installed.remove(pos);
    }
    state.corp.archives.push(card_id.clone());
}

/// Resolves `PlayerAction::TrashAccessedCard`. See its doc comment for the
/// error conditions.
pub fn resolve_trash(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_pending(state, card_id)?;
    let cost = pending.trash_cost.ok_or(RulesError::NotInAccessPhase)?;

    let available = state.runner.resources.credits.0;
    if available < cost {
        return Err(RulesError::CannotAffordTrashCost { card: card_id.clone(), available, requested: cost });
    }

    let mut events = ability::pay_cost(state, Side::Runner, &Cost::Credits(cost))?;
    move_to_archives(state, card_id, pending.server);
    events.push(GameEvent::CardTrashedFromAccess { card: card_id.clone(), cost_paid: cost });

    events.extend(advance_or_finish(state, registry, pending.server, card_id.clone()));
    Ok(events)
}

/// Resolves `PlayerAction::PassAccessedCard`. See its doc comment for the
/// error conditions.
pub fn resolve_pass(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_pending(state, card_id)?;
    if pending.mandatory_steal {
        return Err(RulesError::MandatoryStealViolation { card: card_id.clone() });
    }

    let mut events = vec![GameEvent::AccessPassed { card: card_id.clone() }];
    events.extend(advance_or_finish(state, registry, pending.server, card_id.clone()));
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{Card, CardType};
    use crate::rules::run::state::RunState;
    use crate::rules::state::{
        AgendaPoints, Clicks, Credits, InstalledCard, MemoryUnits, PlayerResources, RunnerState,
        Side,
    };
    use std::collections::HashSet;

    /// An empty registry, for every test that doesn't exercise agenda
    /// scoring and so doesn't need real card definitions.
    fn registry() -> CardRegistry {
        CardRegistry::new()
    }

    /// A minimal Agenda `Card` worth `points` — everything besides id and
    /// `agenda_points` is irrelevant to these tests.
    fn agenda_card(id: &str, points: u32) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Agenda,
            cost: 0,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: Some(points),
            agenda_points: Some(points),
            min_deck_size: None,
        }
    }

    /// A NAPD-Contract-style Agenda: worth `points`, but costs `steal_cost`
    /// credits to steal instead of being a mandatory free steal.
    fn costed_agenda_card(id: &str, points: u32, steal_cost: u32) -> Card {
        Card { steal_cost: Some(Cost::Credits(steal_cost)), ..agenda_card(id, points) }
    }

    /// A minimal non-Agenda Asset `Card` with the given `trash_cost` —
    /// everything besides id and `trash_cost` is irrelevant to these tests.
    fn trashable_card(id: &str, trash_cost: u32) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Asset,
            cost: 0,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: Some(trash_cost),
            steal_cost: None,
            advancement_requirement: None,
            agenda_points: None,
            min_deck_size: None,
        }
    }

    /// A run against `server` already in `RunPhase::Success`, ready for
    /// `access_server` to park in `AccessingCard`.
    fn run_in_success(server: ServerId) -> RunState {
        RunState { server, phase: RunPhase::Success, ice: Vec::new(), position: 0, access_state: None }
    }

    fn game_state(
        hq: Vec<CardId>,
        r_and_d: Vec<CardId>,
        archives: Vec<CardId>,
        installed: Vec<InstalledCard>,
        seed: u64,
    ) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState {
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                hq,
                r_and_d,
                archives,
                installed,
            },
            runner: RunnerState {
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
                heap: Vec::new(),
            },
            phase: crate::rules::state::GamePhase::Action(Side::Corp),
            active_run: None,
            seed,
            rng_step: 0,
        }
    }

    #[test]
    fn accessing_hq_with_one_card_yields_that_card() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        assert_eq!(
            access_server(&mut state, ServerId::Hq, &registry()),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::Hq,
            }]
        );
        // The RNG step still advances even with only one possible index.
        assert_eq!(state.rng_step, 1);
        assert_eq!(state.active_run.unwrap().phase, RunPhase::AccessingCard);
    }

    #[test]
    fn accessing_hq_is_deterministic_for_a_given_seed() {
        let hq = vec![
            CardId("card_0".to_string()),
            CardId("card_1".to_string()),
            CardId("card_2".to_string()),
            CardId("card_3".to_string()),
            CardId("card_4".to_string()),
        ];
        let mut state_a = game_state(hq.clone(), Vec::new(), Vec::new(), Vec::new(), 42);
        state_a.active_run = Some(run_in_success(ServerId::Hq));
        let mut state_b = game_state(hq, Vec::new(), Vec::new(), Vec::new(), 42);
        state_b.active_run = Some(run_in_success(ServerId::Hq));

        let events_a = access_server(&mut state_a, ServerId::Hq, &registry());
        let events_b = access_server(&mut state_b, ServerId::Hq, &registry());

        assert_eq!(events_a, events_b);
        assert_eq!(events_a.len(), 1);
    }

    #[test]
    fn accessing_hq_yields_varied_indices_across_different_seeds() {
        let hq = vec![
            CardId("card_0".to_string()),
            CardId("card_1".to_string()),
            CardId("card_2".to_string()),
            CardId("card_3".to_string()),
            CardId("card_4".to_string()),
        ];

        let accessed_cards: HashSet<CardId> = (0..20u64)
            .map(|seed| {
                let mut state = game_state(hq.clone(), Vec::new(), Vec::new(), Vec::new(), seed);
                state.active_run = Some(run_in_success(ServerId::Hq));
                match access_server(&mut state, ServerId::Hq, &registry()).into_iter().next() {
                    Some(GameEvent::CardAccessed { card, .. }) => card,
                    other => panic!("expected a CardAccessed event, got {other:?}"),
                }
            })
            .collect();

        assert!(
            accessed_cards.len() > 1,
            "expected varied indices across seeds, got only {accessed_cards:?}"
        );
    }

    #[test]
    fn accessing_rnd_yields_the_last_card() {
        let mut state = game_state(
            Vec::new(),
            vec![CardId("enigma".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::RnD));
        assert_eq!(
            access_server(&mut state, ServerId::RnD, &registry()),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::RnD,
            }]
        );
    }

    #[test]
    fn accessing_hq_yields_hq_card_and_root_installed_upgrades() {
        let installed = vec![
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("ice_wall".to_string()),
                server: ServerId::Hq,
                slot: InstallSlot::Ice,
                rezzed: true,
            },
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("ash_2_0".to_string()),
                server: ServerId::Hq,
                slot: InstallSlot::Root,
                rezzed: false,
            },
        ];
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            installed,
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        // Two cards are accessed (the HQ card and the Root-installed
        // Upgrade), so nothing is presented until the Runner picks which to
        // resolve first (see
        // `multi_card_sequence_advances_through_each_card_in_order`).
        assert_eq!(access_server(&mut state, ServerId::Hq, &registry()), Vec::new());
        let access_state = state.active_run.unwrap().access_state.unwrap();
        assert_eq!(
            access_state.unaccessed_cards,
            vec![CardId("hedge_fund".to_string()), CardId("ash_2_0".to_string())]
        );
        assert_eq!(
            access_state.phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![
                    CardId("hedge_fund".to_string()),
                    CardId("ash_2_0".to_string())
                ]
            }
        );
    }

    #[test]
    fn accessing_rnd_yields_rnd_card_and_root_installed_upgrades() {
        let installed = vec![
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("wraparound".to_string()),
                server: ServerId::RnD,
                slot: InstallSlot::Ice,
                rezzed: true,
            },
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("crisium_grid".to_string()),
                server: ServerId::RnD,
                slot: InstallSlot::Root,
                rezzed: false,
            },
        ];
        let mut state = game_state(
            Vec::new(),
            vec![CardId("enigma".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            installed,
            0,
        );
        state.active_run = Some(run_in_success(ServerId::RnD));
        assert_eq!(access_server(&mut state, ServerId::RnD, &registry()), Vec::new());
        assert_eq!(
            state.active_run.unwrap().access_state.unwrap().unaccessed_cards,
            vec![CardId("hedge_fund".to_string()), CardId("crisium_grid".to_string())]
        );
    }

    #[test]
    fn accessing_archives_yields_every_card_in_it() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        assert_eq!(access_server(&mut state, ServerId::Archives, &registry()), Vec::new());
        assert_eq!(
            state.active_run.unwrap().access_state.unwrap().unaccessed_cards,
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())]
        );
    }

    #[test]
    fn accessing_remote_skips_installed_ice_and_yields_only_root_installs() {
        let installed = vec![
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("ice_wall".to_string()),
                server: ServerId::Remote(0),
                slot: InstallSlot::Ice,
                rezzed: true,
            },
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("pad_campaign".to_string()),
                server: ServerId::Remote(0),
                slot: InstallSlot::Root,
                rezzed: false,
            },
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("enigma".to_string()),
                server: ServerId::Remote(1),
                slot: InstallSlot::Ice,
                rezzed: true,
            },
        ];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        assert_eq!(
            access_server(&mut state, ServerId::Remote(0), &registry()),
            vec![GameEvent::CardAccessed {
                card: CardId("pad_campaign".to_string()),
                server: ServerId::Remote(0)
            }]
        );
    }

    #[test]
    fn accessing_remote_with_only_ice_yields_no_events() {
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: CardId("ice_wall".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Ice,
            rezzed: true,
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        assert_eq!(access_server(&mut state, ServerId::Remote(0), &registry()), Vec::new());
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn accessing_an_empty_zone_yields_no_events() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        assert_eq!(access_server(&mut state, ServerId::Hq, &registry()), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::RnD, &registry()), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::Archives, &registry()), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::Remote(0), &registry()), Vec::new());
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn free_agenda_access_is_a_mandatory_steal() {
        let registry = CardRegistry::from_cards(vec![agenda_card("priority_requisition", 3)]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("priority_requisition".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("priority_requisition".to_string());
        assert_eq!(
            resolve_pass(&mut state, &card_id, &registry),
            Err(RulesError::MandatoryStealViolation { card: card_id.clone() })
        );

        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");
        assert_eq!(state.runner.scored_agendas, vec![card_id.clone()]);
        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(3));
        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 3 },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn stealing_an_agenda_that_reaches_seven_points_ends_the_game_with_a_runner_win() {
        let registry = CardRegistry::from_cards(vec![
            agenda_card("priority_requisition", 3),
            agenda_card("already_scored", 4),
        ]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("priority_requisition".to_string())],
            Vec::new(),
            0,
        );
        // Simulate having already stolen 4 points' worth of Agendas earlier
        // in the game.
        state.runner.scored_agendas = vec![CardId("already_scored".to_string())];
        state.runner.resources.agenda_points = AgendaPoints(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("priority_requisition".to_string());
        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");

        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(7));
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 3 },
                GameEvent::GameOver { winner: Side::Runner },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn winning_mid_sequence_never_presents_the_next_accessed_card() {
        let registry = CardRegistry::from_cards(vec![
            agenda_card("priority_requisition", 3),
            agenda_card("hostile_takeover", 1),
            agenda_card("already_scored", 4),
        ]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![
                CardId("priority_requisition".to_string()),
                CardId("hostile_takeover".to_string()),
            ],
            Vec::new(),
            0,
        );
        state.runner.scored_agendas = vec![CardId("already_scored".to_string())];
        state.runner.resources.agenda_points = AgendaPoints(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("priority_requisition".to_string());
        resolve_select_card(&mut state, &card_id, &registry).expect("selecting should succeed");
        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");

        // Capped at the winning threshold, not 8 — the second agenda
        // (worth 1 more point) was never reached, and never presented.
        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(7));
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 3 },
                GameEvent::GameOver { winner: Side::Runner },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn costed_agenda_can_be_stolen_by_paying_its_steal_cost() {
        let registry = CardRegistry::from_cards(vec![costed_agenda_card("napd_contract", 2, 4)]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("napd_contract".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("napd_contract".to_string());
        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");

        assert_eq!(state.runner.resources.credits, Credits(0));
        assert_eq!(state.runner.scored_agendas, vec![card_id.clone()]);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 4 },
                GameEvent::AgendaStolen { card: card_id, agenda_points: 2 },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn costed_agenda_can_be_declined_when_unaffordable() {
        let registry = CardRegistry::from_cards(vec![costed_agenda_card("napd_contract", 2, 4)]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("napd_contract".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(2);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("napd_contract".to_string());
        assert_eq!(
            resolve_steal(&mut state, &card_id, &registry),
            Err(RulesError::CannotAffordStealCost { card: card_id.clone(), available: 2, requested: 4 })
        );
        // Declining is legal — this Agenda isn't a mandatory steal.
        let events = resolve_pass(&mut state, &card_id, &registry).expect("passing should succeed");

        assert!(state.runner.scored_agendas.is_empty());
        assert_eq!(state.runner.resources.credits, Credits(2));
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: card_id },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn trashing_an_installed_asset_pays_its_trash_cost_and_moves_it_to_archives() {
        let registry = CardRegistry::from_cards(vec![trashable_card("pad_campaign", 2)]);
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: true,
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.runner.resources.credits = Credits(3);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        access_server(&mut state, ServerId::Remote(0), &registry);

        let card_id = CardId("pad_campaign".to_string());
        let events = resolve_trash(&mut state, &card_id, &registry).expect("trash should succeed");

        assert_eq!(state.runner.resources.credits, Credits(1));
        assert!(state.corp.installed.is_empty());
        assert_eq!(state.corp.archives, vec![card_id.clone()]);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 2 },
                GameEvent::CardTrashedFromAccess { card: card_id, cost_paid: 2 },
                GameEvent::RunCompleted { server: ServerId::Remote(0) },
            ]
        );
    }

    #[test]
    fn trashing_with_insufficient_credits_errors() {
        let registry = CardRegistry::from_cards(vec![trashable_card("pad_campaign", 2)]);
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: true,
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.runner.resources.credits = Credits(1);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        access_server(&mut state, ServerId::Remote(0), &registry);

        let card_id = CardId("pad_campaign".to_string());
        assert_eq!(
            resolve_trash(&mut state, &card_id, &registry),
            Err(RulesError::CannotAffordTrashCost { card: card_id, available: 1, requested: 2 })
        );
        assert_eq!(state.corp.installed.len(), 1);
    }

    #[test]
    fn passing_a_non_agenda_non_trashable_card_completes_the_run() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry());

        let card_id = CardId("hedge_fund".to_string());
        let events = resolve_pass(&mut state, &card_id, &registry()).expect("passing should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: card_id },
                GameEvent::RunCompleted { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn multi_card_sequence_advances_through_each_card_in_order() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        let first = access_server(&mut state, ServerId::Archives, &registry());
        assert_eq!(first, Vec::new());
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![
                    CardId("hedge_fund".to_string()),
                    CardId("ice_wall".to_string())
                ]
            }
        );

        // Pick the second card first — order is the Runner's choice, not
        // the fixed access-determination order.
        let selected = resolve_select_card(&mut state, &CardId("ice_wall".to_string()), &registry())
            .expect("selecting the second card should succeed");
        assert_eq!(
            selected,
            vec![GameEvent::CardAccessed {
                card: CardId("ice_wall".to_string()),
                server: ServerId::Archives
            }]
        );

        let second = resolve_pass(&mut state, &CardId("ice_wall".to_string()), &registry())
            .expect("passing the selected card should succeed");
        // Only one card remains, so it auto-presents rather than offering
        // another `SelectNextCard` choice.
        assert_eq!(
            second,
            vec![
                GameEvent::AccessPassed { card: CardId("ice_wall".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::Archives
                },
            ]
        );
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::AccessingCard);

        let last = resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry())
            .expect("passing the last card should succeed");
        assert_eq!(
            last,
            vec![
                GameEvent::AccessPassed { card: CardId("hedge_fund".to_string()) },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn multi_card_selection_lets_runner_pick_the_second_card_first() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry());

        let events = resolve_select_card(&mut state, &CardId("ice_wall".to_string()), &registry())
            .expect("selecting the second card should succeed");
        assert_eq!(
            events,
            vec![GameEvent::CardAccessed {
                card: CardId("ice_wall".to_string()),
                server: ServerId::Archives
            }]
        );
        let access_state = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(access_state.unaccessed_cards, vec![CardId("hedge_fund".to_string())]);
        assert_eq!(
            access_state.phase,
            AccessPhase::PendingChoice {
                card_id: CardId("ice_wall".to_string()),
                can_trash: false,
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            }
        );

        let resolved = resolve_pass(&mut state, &CardId("ice_wall".to_string()), &registry())
            .expect("passing the selected card should succeed");
        assert_eq!(
            resolved,
            vec![
                GameEvent::AccessPassed { card: CardId("ice_wall".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::Archives
                },
            ]
        );
    }

    #[test]
    fn three_card_access_supports_out_of_order_resolution() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![
                CardId("card_1".to_string()),
                CardId("card_2".to_string()),
                CardId("card_3".to_string()),
            ],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry());

        resolve_select_card(&mut state, &CardId("card_3".to_string()), &registry())
            .expect("selecting card_3 should succeed");
        resolve_pass(&mut state, &CardId("card_3".to_string()), &registry())
            .expect("passing card_3 should succeed");

        // Two cards remain, so the Runner is offered another choice rather
        // than auto-advancing.
        let access_state = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(
            access_state.phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![CardId("card_1".to_string()), CardId("card_2".to_string())]
            }
        );
        assert_eq!(access_state.resolved_cards, vec![CardId("card_3".to_string())]);

        resolve_select_card(&mut state, &CardId("card_1".to_string()), &registry())
            .expect("selecting card_1 should succeed");
        let resolved = resolve_pass(&mut state, &CardId("card_1".to_string()), &registry())
            .expect("passing card_1 should succeed");

        // Only card_2 remains, so it auto-presents.
        assert_eq!(
            resolved,
            vec![
                GameEvent::AccessPassed { card: CardId("card_1".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("card_2".to_string()),
                    server: ServerId::Archives
                },
            ]
        );
    }

    #[test]
    fn selecting_the_final_remaining_card_bypasses_select_next_card() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry());
        resolve_select_card(&mut state, &CardId("hedge_fund".to_string()), &registry())
            .expect("selecting should succeed");

        let events = resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry())
            .expect("passing should succeed");

        // With exactly one card left in `unaccessed_cards`, it goes
        // straight to `PendingChoice` instead of another `SelectNextCard`.
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: CardId("hedge_fund".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("ice_wall".to_string()),
                    server: ServerId::Archives
                },
            ]
        );
        assert!(matches!(
            state.active_run.unwrap().access_state.unwrap().phase,
            AccessPhase::PendingChoice { .. }
        ));
    }

    #[test]
    fn selecting_a_card_not_in_selectable_cards_errors() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry());

        let wrong_id = CardId("wrong_card".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &wrong_id, &registry()),
            Err(RulesError::InvalidAccessSelection { card: wrong_id })
        );
    }

    #[test]
    fn selecting_while_already_at_pending_choice_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry());

        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &card_id, &registry()),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn selecting_with_no_active_run_errors() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &card_id, &registry()),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn resolving_with_a_card_id_that_does_not_match_the_pending_card_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry());

        let wrong_id = CardId("wrong_card".to_string());
        assert_eq!(resolve_pass(&mut state, &wrong_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn resolving_with_no_active_run_errors() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(resolve_pass(&mut state, &card_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn stealing_a_non_agenda_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry());

        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(resolve_steal(&mut state, &card_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn trashing_a_non_trashable_card_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry());

        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(resolve_trash(&mut state, &card_id, &registry()), Err(RulesError::NotInAccessPhase));
    }
}
