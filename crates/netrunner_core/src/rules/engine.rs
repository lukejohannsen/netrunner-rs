use crate::cards::CardRegistry;
use crate::dsl::{CardId, Cost, Trigger};
use crate::rules::ability;
use crate::rules::action::{PlayerAction, ServerTarget, TargetZone};
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::{self, RunAction, RunPhase, RunState};
use crate::rules::state::{GamePhase, GameState, InstallSlot, InstalledCard, Side};
use crate::rules::turn;

pub fn apply_action(
    state: &GameState,
    registry: &CardRegistry,
    action: PlayerAction,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    match action {
        PlayerAction::GainCreditClick { side } => gain_credit_click(state, side),
        PlayerAction::DrawCardClick => draw_card_click(state),
        PlayerAction::InstallCard { card_id, zone, slot } => {
            install_card(state, registry, card_id, zone, slot)
        }
        PlayerAction::RezIce { ice_id } => rez_ice(state, ice_id),
        PlayerAction::InitiateRun { server } => initiate_run(state, server),
        PlayerAction::ContinueRun => continue_run(state),
        PlayerAction::JackOut => jack_out(state),
        PlayerAction::CompleteRun => complete_run(state, registry),
        PlayerAction::PlayEvent { card_id } => play_event(state, registry, card_id),
        PlayerAction::InstallHardware { card_id } => install_hardware(state, card_id),
        PlayerAction::InstallProgram { card_id, memory_cost } => {
            install_program(state, card_id, memory_cost)
        }
        PlayerAction::BreakSubroutine { ice_id, subroutine_index } => {
            break_subroutine(state, ice_id, subroutine_index)
        }
        PlayerAction::EndTurn => turn::end_turn(state),
        PlayerAction::DiscardCard { card_id } => turn::discard_card(state, card_id),
        PlayerAction::ActivateAbility { card_id, ability_index } => {
            activate_ability(state, registry, card_id, ability_index)
        }
        PlayerAction::AdvanceCard { card_id } => advance_card(state, registry, card_id),
        PlayerAction::SelectCardToAccess { card_id } => {
            select_card_to_access(state, registry, card_id)
        }
        PlayerAction::StealAgenda { card_id } => steal_agenda(state, registry, card_id),
        PlayerAction::TrashAccessedCard { card_id } => {
            trash_accessed_card(state, registry, card_id)
        }
        PlayerAction::PassAccessedCard { card_id } => {
            pass_accessed_card(state, registry, card_id)
        }
    }
}

fn require_phase(state: &GameState, expected: GamePhase) -> Result<(), RulesError> {
    if state.phase != expected {
        return Err(RulesError::WrongPhase { expected, actual: state.phase });
    }
    Ok(())
}

fn spend_click(state: &mut GameState, side: Side) -> Result<(), RulesError> {
    let resources = state.resources_mut(side);
    let available = resources.clicks.0;
    resources.clicks = resources
        .clicks
        .spend(1)
        .ok_or(RulesError::NotEnoughClicks {
            side,
            available,
            requested: 1,
        })?;
    Ok(())
}

fn gain_credit_click(
    state: &GameState,
    side: Side,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    next.resources_mut(side).credits = next.resources(side).credits.gain(1);

    Ok((
        next,
        vec![
            GameEvent::ClickSpent { side },
            GameEvent::CreditsGained { side, amount: 1 },
        ],
    ))
}

fn draw_card_click(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;

    let mut events = vec![GameEvent::ClickSpent { side }];
    if let Some(card) = next.runner.stack.pop() {
        next.runner.grip.push(card);
        events.push(GameEvent::CardDrawn { side });
    }

    Ok((next, events))
}

fn install_card(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
    zone: TargetZone,
    slot: InstallSlot,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;

    let position = next
        .corp
        .hq
        .iter()
        .position(|c| *c == card_id)
        .ok_or_else(|| RulesError::CardNotInHand {
            side,
            card: card_id.clone(),
        })?;
    next.corp.hq.remove(position);

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(card_def.cost))?);

    next.corp.installed.push(InstalledCard {
        card: card_id.clone(),
        server: zone,
        slot,
        rezzed: false,
        advancement_tokens: 0,
    });
    events.push(GameEvent::CardInstalled {
        side,
        card: card_id,
        server: zone,
    });

    Ok((next, events))
}

fn rez_ice(state: &GameState, ice_id: CardId) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    let rez_window_open =
        matches!(&state.active_run, Some(run) if run.phase == RunPhase::ApproachIce);
    if !rez_window_open {
        require_phase(state, GamePhase::Action(side))?;
    }
    let mut next = state.clone();

    let server = {
        let installed = next
            .corp
            .installed
            .iter_mut()
            .find(|c| c.card == ice_id)
            .ok_or_else(|| RulesError::CardNotInstalled {
                card: ice_id.clone(),
            })?;
        if installed.rezzed {
            return Err(RulesError::AlreadyRezzed { card: ice_id });
        }
        installed.rezzed = true;
        installed.server
    };

    Ok((next, vec![GameEvent::IceRezzed { card: ice_id, server }]))
}

fn initiate_run(
    state: &GameState,
    server: ServerTarget,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    if state.active_run.is_some() {
        return Err(RulesError::RunAlreadyInProgress);
    }

    let mut next = state.clone();
    spend_click(&mut next, side)?;
    next.active_run = Some(RunState { access_state: None,
        server,
        phase: RunPhase::Initiation,
        ice: Vec::new(),
        position: 0,
    });

    Ok((
        next,
        vec![
            GameEvent::ClickSpent { side },
            GameEvent::RunInitiated { server },
        ],
    ))
}

fn continue_run(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    let events = run::advance_run(&mut next, RunAction::Continue)?;

    Ok((next, events))
}

fn jack_out(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    let events = run::advance_run(&mut next, RunAction::JackOut)?;
    next.active_run = None;

    Ok((next, events))
}

fn complete_run(
    state: &GameState,
    registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    let active_run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    if active_run.phase != RunPhase::Success {
        return Err(RulesError::RunNotConcluded { phase: active_run.phase });
    }
    let server = active_run.server;

    let mut next = state.clone();

    let mut events = run::access_server(&mut next, server, registry)?;
    // `access_server` clears `active_run` itself when nothing was accessed
    // (nothing to present a choice about); otherwise it parks the run in
    // `RunPhase::AccessingCard` and `StealAgenda`/`TrashAccessedCard`/
    // `PassAccessedCard` are what eventually finish it off.
    if next.active_run.is_none() {
        events.push(GameEvent::RunCompleted { server });
    }

    Ok((next, events))
}

fn take_from_grip(state: &mut GameState, side: Side, card_id: &CardId) -> Result<(), RulesError> {
    let hand = match side {
        Side::Runner => &mut state.runner.grip,
        Side::Corp => &mut state.corp.hq,
    };
    let position = hand
        .iter()
        .position(|c| c == card_id)
        .ok_or_else(|| RulesError::CardNotInHand {
            side,
            card: card_id.clone(),
        })?;
    hand.remove(position);
    Ok(())
}

fn play_event(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    take_from_grip(&mut next, side, &card_id)?;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(card_def.cost))?);
    events.push(GameEvent::EventPlayed { side, card: card_id });

    Ok((next, events))
}

fn install_hardware(
    state: &GameState,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    take_from_grip(&mut next, side, &card_id)?;
    next.runner.rig.push(card_id.clone());

    Ok((
        next,
        vec![
            GameEvent::ClickSpent { side },
            GameEvent::HardwareInstalled { side, card: card_id },
        ],
    ))
}

fn install_program(
    state: &GameState,
    card_id: CardId,
    memory_cost: u8,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    take_from_grip(&mut next, side, &card_id)?;

    let available = next.runner.memory_units.0;
    let requested = memory_cost as u32;
    next.runner.memory_units = next
        .runner
        .memory_units
        .spend(requested)
        .ok_or(RulesError::InsufficientMemory { available, requested })?;
    next.runner.rig.push(card_id.clone());

    Ok((
        next,
        vec![
            GameEvent::ClickSpent { side },
            GameEvent::ProgramInstalled { side, card: card_id, memory_cost },
        ],
    ))
}

fn break_subroutine(
    state: &GameState,
    // Not cross-checked against `RunState::ice` — see `PlayerAction::BreakSubroutine`'s doc comment.
    _ice_id: CardId,
    subroutine_index: usize,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;

    // `step_subroutine` (via `advance_run`) now does its own bounds/status
    // validation against `RunIce::subroutines`, so there's no need to
    // duplicate a pre-check here — just forward the index.
    let mut next = state.clone();
    let events = run::advance_run(&mut next, RunAction::BreakSubroutine(subroutine_index))?;

    Ok((next, events))
}

/// Pays and resolves the `ability_index`-th `AbilityDef` on `card_id`, per
/// `PlayerAction::ActivateAbility`'s doc comment. Symmetric like
/// `turn::end_turn`/`turn::discard_card` — the acting side is derived from
/// `state.phase` rather than taken as a parameter.
fn activate_ability(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
    ability_index: usize,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = match state.phase {
        GamePhase::Action(side) => side,
        actual => return Err(RulesError::NotInActionPhase { actual }),
    };

    let active = match side {
        Side::Corp => state.corp.installed.iter().any(|c| c.card == card_id && c.rezzed),
        Side::Runner => state.runner.rig.contains(&card_id),
    };
    if !active {
        return Err(RulesError::CardNotActive { side, card: card_id });
    }

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    let ability = card_def
        .abilities
        .get(ability_index)
        .ok_or(RulesError::InvalidAbilityIndex(ability_index))?;
    if ability.trigger != Trigger::Paid {
        return Err(RulesError::AbilityNotManuallyActivatable(ability_index));
    }

    let mut next = state.clone();
    let mut events = Vec::new();
    if let Some(cost) = &ability.cost {
        events.extend(ability::pay_cost(&mut next, side, cost)?);
    }
    events.push(GameEvent::AbilityActivated { side, card_id: card_id.clone(), ability_index });
    events.extend(ability::evaluate_effect(&mut next, &ability.effect)?);

    Ok((next, events))
}

/// Places one advancement token on `card_id`, per
/// `PlayerAction::AdvanceCard`'s doc comment. Corp-only, like `install_card`/
/// `rez_ice`.
fn advance_card(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(1))?);

    let installed = next
        .corp
        .installed
        .iter_mut()
        .find(|c| c.card == card_id)
        .ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    if card_def.advancement_requirement.is_none() {
        return Err(RulesError::CardNotAdvanceable { card: card_id });
    }

    installed.advancement_tokens += 1;
    let advancement_tokens = installed.advancement_tokens;
    events.push(GameEvent::CardAdvanced { card: card_id, advancement_tokens });

    Ok((next, events))
}

/// Resolves `PlayerAction::SelectCardToAccess`, per its doc comment.
/// Runner-only, like every other access-resolution action.
fn select_card_to_access(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    let mut next = state.clone();
    let events = run::resolve_select_card(&mut next, &card_id, registry)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::StealAgenda`, per its doc comment. Runner-only,
/// like every other access-resolution action.
fn steal_agenda(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    let mut next = state.clone();
    let events = run::resolve_steal(&mut next, &card_id, registry)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::TrashAccessedCard`, per its doc comment.
fn trash_accessed_card(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    let mut next = state.clone();
    let events = run::resolve_trash(&mut next, &card_id, registry)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::PassAccessedCard`, per its doc comment.
fn pass_accessed_card(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    let mut next = state.clone();
    let events = run::resolve_pass(&mut next, &card_id, registry)?;
    Ok((next, events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{AbilityDef, Card, CardType, Cost, Effect, SubroutineDef};
    use crate::rules::run::{EncounteredSubroutine, RunIce, ServerId, SubroutineStatus};
    use crate::rules::state::{AgendaPoints, Clicks, Credits, PlayerResources};

    /// An empty registry, for every test that doesn't exercise
    /// `PlayerAction::ActivateAbility` and so doesn't need real card
    /// definitions.
    fn registry() -> CardRegistry {
        CardRegistry::new()
    }

    /// Builds a `RunIce` with `subroutine_count` placeholder `Pending`
    /// subroutines — identity/effect content doesn't matter for tests using
    /// this, only status transitions and counts do.
    fn test_ice(card_id: &str, strength: i32, subroutine_count: usize) -> RunIce {
        RunIce {
            card_id: CardId(card_id.to_string()),
            current_strength: strength,
            subroutines: (0..subroutine_count)
                .map(|id| EncounteredSubroutine {
                    id,
                    definition: SubroutineDef {
                        text: format!("Subroutine {id}"),
                        effect: Effect::EndTheRun,
                    },
                    status: SubroutineStatus::Pending,
                })
                .collect(),
        }
    }

    fn corp_state(clicks: u32, credits: u32) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState {
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(credits),
                    clicks: Clicks(clicks),
                    agenda_points: AgendaPoints(0),
                },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: crate::rules::state::RunnerState {
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: crate::rules::state::MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
                heap: Vec::new(),
            },
            phase: GamePhase::Action(Side::Corp),
            active_run: None,
            seed: 0,
            rng_step: 0,
        }
    }

    /// `stack_size`/`grip_size` are filled with distinct placeholder `CardId`s
    /// (identity doesn't matter for the tests using this — only counts do).
    fn runner_state(clicks: u32, stack_size: u32, grip_size: u32) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState {
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: crate::rules::state::RunnerState {
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(clicks),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: crate::rules::state::MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: (0..grip_size).map(|i| CardId(format!("grip_card_{i}"))).collect(),
                stack: (0..stack_size).map(|i| CardId(format!("stack_card_{i}"))).collect(),
                rig: Vec::new(),
                heap: Vec::new(),
            },
            phase: GamePhase::Action(Side::Runner),
            active_run: None,
            seed: 0,
            rng_step: 0,
        }
    }

    fn corp_state_with_hq_and_installed(
        clicks: u32,
        credits: u32,
        hq: Vec<CardId>,
        installed: Vec<InstalledCard>,
    ) -> GameState {
        let mut state = corp_state(clicks, credits);
        state.corp.hq = hq;
        state.corp.installed = installed;
        state
    }

    fn runner_state_with_grip(
        clicks: u32,
        credits: u32,
        memory_units: u32,
        grip: Vec<CardId>,
    ) -> GameState {
        let mut state = runner_state(clicks, 0, 0);
        state.runner.resources.credits = Credits(credits);
        state.runner.memory_units = crate::rules::state::MemoryUnits(memory_units);
        state.runner.grip = grip;
        state
    }

    #[test]
    fn corp_gain_credit_click_spends_click_and_gains_credit() {
        let state = corp_state(3, 5);
        let (next, events) = apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Corp })
            .expect("action should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(2));
        assert_eq!(next.corp.resources.credits, Credits(6));
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::CreditsGained {
                    side: Side::Corp,
                    amount: 1
                },
            ]
        );

        // Original state is untouched.
        assert_eq!(state.corp.resources.clicks, Clicks(3));
        assert_eq!(state.corp.resources.credits, Credits(5));
    }

    #[test]
    fn runner_draw_card_click_spends_click_and_draws_card() {
        let state = runner_state(4, 10, 5);
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::DrawCardClick).expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(3));
        assert_eq!(next.runner.stack.len(), 9);
        assert_eq!(next.runner.grip.len(), 6);
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::CardDrawn { side: Side::Runner },
            ]
        );
    }

    #[test]
    fn spending_click_with_zero_clicks_returns_error() {
        let state = corp_state(0, 5);
        let result = apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Corp });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughClicks {
                side: Side::Corp,
                available: 0,
                requested: 1,
            })
        );
    }

    #[test]
    fn acting_out_of_turn_returns_error() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Runner });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_draw_card_click_with_empty_stack_does_not_underflow() {
        let state = runner_state(2, 0, 3);
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::DrawCardClick).expect("action should succeed");

        assert_eq!(next.runner.stack.len(), 0);
        assert_eq!(next.runner.grip.len(), 3);
        assert_eq!(next.runner.resources.clicks, Clicks(1));
        assert_eq!(events, vec![GameEvent::ClickSpent { side: Side::Runner }]);
    }

    #[test]
    fn corp_install_card_moves_card_from_hq_to_installed_and_spends_click_and_credits() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, vec![card_id.clone()], Vec::new());
        let mut registry = CardRegistry::new();
        registry.insert(test_card("ice_wall", Side::Corp, CardType::Ice(crate::dsl::IceType::Barrier), 1, None));

        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallCard {
                card_id: card_id.clone(),
                zone: ServerId::Hq,
                slot: InstallSlot::Ice,
            },
        )
        .expect("action should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(2));
        assert_eq!(next.corp.resources.credits, Credits(4));
        assert!(next.corp.hq.is_empty());
        assert_eq!(
            next.corp.installed,
            vec![InstalledCard {
                advancement_tokens: 0,
                card: card_id.clone(),
                server: ServerId::Hq,
                slot: InstallSlot::Ice,
                rezzed: false,
            }]
        );
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::CreditsSpent { side: Side::Corp, amount: 1 },
                GameEvent::CardInstalled {
                    side: Side::Corp,
                    card: card_id,
                    server: ServerId::Hq,
                },
            ]
        );

        // Original state is untouched.
        assert_eq!(state.corp.hq, vec![CardId("ice_wall".to_string())]);
        assert!(state.corp.installed.is_empty());
    }

    #[test]
    fn corp_install_card_not_in_registry_returns_card_not_found_in_registry() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, vec![card_id.clone()], Vec::new());

        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallCard { card_id: card_id.clone(), zone: ServerId::Hq, slot: InstallSlot::Ice },
        );

        assert_eq!(result, Err(RulesError::CardNotFoundInRegistry(card_id)));
    }

    #[test]
    fn corp_install_card_with_insufficient_credits_for_registry_cost_returns_not_enough_credits() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 0, vec![card_id.clone()], Vec::new());
        let mut registry = CardRegistry::new();
        registry.insert(test_card("ice_wall", Side::Corp, CardType::Ice(crate::dsl::IceType::Barrier), 1, None));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::InstallCard { card_id, zone: ServerId::Hq, slot: InstallSlot::Ice },
        );

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Corp, available: 0, requested: 1 })
        );
    }

    #[test]
    fn runner_turn_install_card_returns_not_your_turn() {
        let card_id = CardId("ice_wall".to_string());
        let state = runner_state(3, 5, 3);
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallCard { card_id, zone: ServerId::Hq, slot: InstallSlot::Ice },
        );

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::Action(Side::Runner),
            })
        );
    }

    #[test]
    fn corp_install_card_with_card_not_in_hq_returns_card_not_in_hand() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), Vec::new());
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallCard {
                card_id: card_id.clone(),
                zone: ServerId::Hq,
                slot: InstallSlot::Ice,
            },
        );

        assert_eq!(
            result,
            Err(RulesError::CardNotInHand { side: Side::Corp, card: card_id })
        );
    }

    #[test]
    fn corp_install_card_with_zero_clicks_returns_not_enough_clicks() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(0, 5, vec![card_id.clone()], Vec::new());
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallCard { card_id, zone: ServerId::Hq, slot: InstallSlot::Ice },
        );

        assert_eq!(
            result,
            Err(RulesError::NotEnoughClicks { side: Side::Corp, available: 0, requested: 1 })
        );
    }

    #[test]
    fn corp_rez_ice_flips_installed_card_and_costs_nothing() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: card_id.clone(),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: false,
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let (next, events) = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id.clone() })
            .expect("action should succeed");

        assert!(next.corp.installed[0].rezzed);
        assert_eq!(next.corp.resources.clicks, Clicks(3));
        assert_eq!(next.corp.resources.credits, Credits(5));
        assert_eq!(
            events,
            vec![GameEvent::IceRezzed { card: card_id, server: ServerId::Hq }]
        );
    }

    #[test]
    fn runner_turn_rez_ice_returns_not_your_turn() {
        let card_id = CardId("ice_wall".to_string());
        let state = runner_state(3, 5, 3);
        let result = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::Action(Side::Runner),
            })
        );
    }

    #[test]
    fn corp_rez_ice_with_card_not_installed_returns_card_not_installed() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), Vec::new());
        let result = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::CardNotInstalled { card: card_id }));
    }

    #[test]
    fn corp_rez_ice_already_rezzed_returns_already_rezzed() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: card_id.clone(),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: true,
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let result = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::AlreadyRezzed { card: card_id }));
    }

    #[test]
    fn corp_can_rez_ice_during_run_approach_ice_even_though_phase_is_runner_action() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: card_id.clone(),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: false,
        }];
        let mut state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1)],
            position: 0,
        });

        let (next, events) = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id.clone() })
            .expect("Corp should be able to rez ICE during the Runner's run");

        assert!(next.corp.installed[0].rezzed);
        assert_eq!(next.corp.resources.clicks, Clicks(3));
        assert_eq!(next.corp.resources.credits, Credits(5));
        assert_eq!(
            events,
            vec![GameEvent::IceRezzed { card: card_id, server: ServerId::Hq }]
        );
    }

    #[test]
    fn runner_initiate_run_starts_run_and_spends_click() {
        let state = runner_state(3, 5, 3);
        let (next, events) = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert_eq!(
            next.active_run,
            Some(RunState { access_state: None,
                server: ServerId::Hq,
                phase: RunPhase::Initiation,
                ice: Vec::new(),
                position: 0,
            })
        );
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::RunInitiated { server: ServerId::Hq },
            ]
        );

        // Original state is untouched.
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn corp_turn_initiate_run_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_initiate_run_with_run_already_active_returns_run_already_in_progress() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1)],
            position: 0,
        });
        let result = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::RnD });

        assert_eq!(result, Err(RulesError::RunAlreadyInProgress));
    }

    #[test]
    fn runner_initiate_run_with_zero_clicks_returns_not_enough_clicks() {
        let state = runner_state(0, 5, 3);
        let result = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughClicks { side: Side::Runner, available: 0, requested: 1 })
        );
    }

    #[test]
    fn runner_jack_out_ends_run_clears_active_run_no_click_cost() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1)],
            position: 0,
        });
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::JackOut).expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(3));
        assert_eq!(next.active_run, None);
        assert_eq!(events, vec![GameEvent::RunJackedOut { server: ServerId::Hq }]);
    }

    #[test]
    fn corp_turn_jack_out_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::JackOut);

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_jack_out_with_no_active_run_returns_no_active_run() {
        let state = runner_state(3, 5, 3);
        let result = apply_action(&state, &registry(), PlayerAction::JackOut);

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_jack_out_on_concluded_run_propagates_run_already_concluded() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
        });
        let result = apply_action(&state, &registry(), PlayerAction::JackOut);

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::Success })
        );
    }

    #[test]
    fn runner_can_initiate_run_again_after_jacking_out() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1)],
            position: 0,
        });

        let (after_jack_out, _) =
            apply_action(&state, &registry(), PlayerAction::JackOut).expect("jack out should succeed");
        let (after_initiate, _) = apply_action(
            &after_jack_out,
            &registry(),
            PlayerAction::InitiateRun { server: ServerId::RnD },
        )
        .expect("initiating a new run should succeed");

        assert_eq!(
            after_initiate.active_run,
            Some(RunState { access_state: None,
                server: ServerId::RnD,
                phase: RunPhase::Initiation,
                ice: Vec::new(),
                position: 0,
            })
        );
    }

    #[test]
    fn runner_complete_run_clears_active_run_after_success() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
        });
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::CompleteRun).expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(3));
        assert_eq!(next.active_run, None);
        assert_eq!(events, vec![GameEvent::RunCompleted { server: ServerId::Hq }]);
    }

    #[test]
    fn runner_complete_run_before_success_returns_run_not_concluded() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1)],
            position: 0,
        });
        let result = apply_action(&state, &registry(), PlayerAction::CompleteRun);

        assert_eq!(
            result,
            Err(RulesError::RunNotConcluded { phase: RunPhase::ApproachIce })
        );
    }

    #[test]
    fn corp_turn_complete_run_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::CompleteRun);

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_complete_run_with_no_active_run_returns_no_active_run() {
        let state = runner_state(3, 5, 3);
        let result = apply_action(&state, &registry(), PlayerAction::CompleteRun);

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_can_initiate_run_again_after_completing_previous_run() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
        });

        let (after_complete, _) =
            apply_action(&state, &registry(), PlayerAction::CompleteRun).expect("complete run should succeed");
        let (after_initiate, _) = apply_action(
            &after_complete,
            &registry(),
            PlayerAction::InitiateRun { server: ServerId::RnD },
        )
        .expect("initiating a new run should succeed");

        assert_eq!(
            after_initiate.active_run,
            Some(RunState { access_state: None,
                server: ServerId::RnD,
                phase: RunPhase::Initiation,
                ice: Vec::new(),
                position: 0,
            })
        );
    }

    #[test]
    fn runner_complete_run_against_hq_parks_the_run_awaiting_an_access_choice() {
        let mut state = runner_state(3, 5, 3);
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        state.active_run = Some(RunState {
            access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
        });
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::CompleteRun).expect("action should succeed");

        // Not an Agenda and not in the (empty) registry, so nothing is
        // stealable/trashable — but the run still waits for
        // `PassAccessedCard` rather than completing on its own.
        assert_eq!(
            events,
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::Hq
            }]
        );
        assert_eq!(
            next.active_run,
            Some(RunState {
                access_state: Some(run::AccessState {
                    server: ServerId::Hq,
                    unaccessed_cards: Vec::new(),
                    resolved_cards: Vec::new(),
                    phase: run::AccessPhase::PendingChoice {
                        card_id: CardId("hedge_fund".to_string()),
                        can_trash: false,
                        trash_cost: None,
                        mandatory_steal: false,
                        steal_cost: None,
                    },
                }),
                server: ServerId::Hq,
                phase: RunPhase::AccessingCard,
                ice: Vec::new(),
                position: 0,
            })
        );
    }

    #[test]
    fn runner_complete_run_against_empty_hq_completes_immediately() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
        });
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::CompleteRun).expect("action should succeed");

        assert_eq!(next.active_run, None);
        assert_eq!(events, vec![GameEvent::RunCompleted { server: ServerId::Hq }]);
    }

    #[test]
    fn corp_end_turn_via_apply_action_hands_control_to_runner() {
        let state = corp_state(0, 5);
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::EndTurn).expect("action should succeed");

        assert_eq!(next.phase, GamePhase::Action(Side::Runner));
        assert_eq!(next.runner.resources.clicks, Clicks(4));
        assert_eq!(
            events,
            vec![
                GameEvent::TurnEnded { side: Side::Corp },
                GameEvent::TurnStarted { side: Side::Runner, clicks: 4 },
            ]
        );
    }

    #[test]
    fn runner_continue_run_steps_through_phases_with_no_click_cost() {
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Initiation,
            ice: vec![test_ice("ice_wall", 0, 0)],
            position: 0,
        });

        // Initiation -> ApproachIce
        let (state, events) =
            apply_action(&state, &registry(), PlayerAction::ContinueRun).expect("continue should succeed");
        assert_eq!(state.runner.resources.clicks, Clicks(3));
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::ApproachIce);
        assert_eq!(events, vec![GameEvent::IceApproached { server: ServerId::Hq, position: 0 }]);

        // ApproachIce -> EncounterIce
        let (state, events) =
            apply_action(&state, &registry(), PlayerAction::ContinueRun).expect("continue should succeed");
        assert_eq!(state.runner.resources.clicks, Clicks(3));
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::EncounterIce);
        assert_eq!(
            events,
            vec![GameEvent::IceEncountered {
                card_id: CardId("ice_wall".to_string()),
                strength: 0,
                subroutine_count: 0,
            }]
        );

        // EncounterIce (0 pending) -> Success
        let (state, events) =
            apply_action(&state, &registry(), PlayerAction::ContinueRun).expect("continue should succeed");
        assert_eq!(state.runner.resources.clicks, Clicks(3));
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::Success);
        assert_eq!(
            events,
            vec![
                GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
                GameEvent::RunSucceeded { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn runner_continue_run_with_subroutines_pending_propagates_error() {
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 1)],
            position: 0,
        });

        let result = apply_action(&state, &registry(), PlayerAction::ContinueRun);

        assert_eq!(result, Err(RulesError::SubroutinesStillPending { pending: 1 }));
    }

    #[test]
    fn corp_turn_continue_run_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::ContinueRun);

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_continue_run_with_no_active_run_returns_no_active_run() {
        let state = runner_state(3, 0, 0);
        let result = apply_action(&state, &registry(), PlayerAction::ContinueRun);

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_play_event_removes_card_from_grip_and_spends_click_and_credits() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 5, 0, vec![card_id.clone()]);
        let mut registry = CardRegistry::new();
        registry.insert(test_card("sure_gamble", Side::Runner, CardType::Event, 5, None));

        let (next, events) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: card_id.clone() })
            .expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert_eq!(next.runner.resources.credits, Credits(0));
        assert!(next.runner.grip.is_empty());
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::CreditsSpent { side: Side::Runner, amount: 5 },
                GameEvent::EventPlayed { side: Side::Runner, card: card_id },
            ]
        );

        // Original state is untouched.
        assert_eq!(state.runner.grip, vec![CardId("sure_gamble".to_string())]);
    }

    #[test]
    fn runner_play_event_not_in_registry_returns_card_not_found_in_registry() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 5, 0, vec![card_id.clone()]);

        let result = apply_action(&state, &registry(), PlayerAction::PlayEvent { card_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::CardNotFoundInRegistry(card_id)));
    }

    #[test]
    fn runner_play_event_with_insufficient_credits_for_registry_cost_returns_not_enough_credits() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 0, 0, vec![card_id.clone()]);
        let mut registry = CardRegistry::new();
        registry.insert(test_card("sure_gamble", Side::Runner, CardType::Event, 5, None));

        let result = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Runner, available: 0, requested: 5 })
        );
    }

    #[test]
    fn corp_turn_play_event_returns_not_your_turn() {
        let card_id = CardId("sure_gamble".to_string());
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::PlayEvent { card_id });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_play_event_with_card_not_in_grip_returns_card_not_in_hand() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 5, 0, Vec::new());
        let result = apply_action(&state, &registry(), PlayerAction::PlayEvent { card_id: card_id.clone() });

        assert_eq!(
            result,
            Err(RulesError::CardNotInHand { side: Side::Runner, card: card_id })
        );
    }

    #[test]
    fn runner_play_event_with_zero_clicks_returns_not_enough_clicks() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(0, 5, 0, vec![card_id.clone()]);
        let result = apply_action(&state, &registry(), PlayerAction::PlayEvent { card_id });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughClicks { side: Side::Runner, available: 0, requested: 1 })
        );
    }

    #[test]
    fn runner_install_hardware_moves_card_from_grip_to_rig_and_spends_click() {
        let card_id = CardId("clone_chip".to_string());
        let state = runner_state_with_grip(3, 5, 0, vec![card_id.clone()]);
        let (next, events) = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallHardware { card_id: card_id.clone() },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert!(next.runner.grip.is_empty());
        assert_eq!(next.runner.rig, vec![card_id.clone()]);
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::HardwareInstalled { side: Side::Runner, card: card_id },
            ]
        );
    }

    #[test]
    fn corp_turn_install_hardware_returns_not_your_turn() {
        let card_id = CardId("clone_chip".to_string());
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::InstallHardware { card_id });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_install_hardware_with_card_not_in_grip_returns_card_not_in_hand() {
        let card_id = CardId("clone_chip".to_string());
        let state = runner_state_with_grip(3, 5, 0, Vec::new());
        let result = apply_action(&state, &registry(), PlayerAction::InstallHardware { card_id: card_id.clone() });

        assert_eq!(
            result,
            Err(RulesError::CardNotInHand { side: Side::Runner, card: card_id })
        );
    }

    #[test]
    fn runner_install_program_moves_card_and_reserves_memory() {
        let card_id = CardId("gordian_blade".to_string());
        let state = runner_state_with_grip(3, 5, 4, vec![card_id.clone()]);
        let (next, events) = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 3 },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert!(next.runner.grip.is_empty());
        assert_eq!(next.runner.rig, vec![card_id.clone()]);
        assert_eq!(next.runner.memory_units, crate::rules::state::MemoryUnits(1));
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::ProgramInstalled {
                    side: Side::Runner,
                    card: card_id,
                    memory_cost: 3,
                },
            ]
        );
    }

    #[test]
    fn corp_turn_install_program_returns_not_your_turn() {
        let card_id = CardId("gordian_blade".to_string());
        let state = corp_state(3, 5);
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallProgram { card_id, memory_cost: 3 },
        );

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_install_program_with_card_not_in_grip_returns_card_not_in_hand() {
        let card_id = CardId("gordian_blade".to_string());
        let state = runner_state_with_grip(3, 5, 4, Vec::new());
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 3 },
        );

        assert_eq!(
            result,
            Err(RulesError::CardNotInHand { side: Side::Runner, card: card_id })
        );
    }

    #[test]
    fn runner_install_program_with_insufficient_memory_returns_insufficient_memory() {
        let card_id = CardId("gordian_blade".to_string());
        let state = runner_state_with_grip(3, 5, 2, vec![card_id.clone()]);
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 3 },
        );

        assert_eq!(
            result,
            Err(RulesError::InsufficientMemory { available: 2, requested: 3 })
        );

        // Original state is untouched: the card is still in the grip.
        assert_eq!(state.runner.grip, vec![card_id]);
    }

    #[test]
    fn runner_break_subroutine_decrements_pending_on_current_ice() {
        let ice_id = CardId("ice_wall".to_string());
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 2)],
            position: 0,
        });
        let (next, events) = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        )
        .expect("action should succeed");

        // No click cost: breaking a subroutine isn't a click action.
        assert_eq!(next.runner.resources.clicks, Clicks(3));
        let ice = &next.active_run.as_ref().unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(ice.subroutines[1].status, SubroutineStatus::Pending);
        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0 }]
        );
    }

    #[test]
    fn corp_turn_break_subroutine_returns_not_your_turn() {
        let ice_id = CardId("ice_wall".to_string());
        let state = corp_state(3, 5);
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        );

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_break_subroutine_with_no_active_run_returns_no_active_run() {
        let ice_id = CardId("ice_wall".to_string());
        let state = runner_state(3, 0, 0);
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        );

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_break_subroutine_with_index_out_of_range_returns_invalid_subroutine_index() {
        let ice_id = CardId("ice_wall".to_string());
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 1)],
            position: 0,
        });
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 1 },
        );

        assert_eq!(result, Err(RulesError::InvalidSubroutineIndex(1)));
    }

    #[test]
    fn runner_break_subroutine_outside_encounter_ice_returns_not_in_encounter() {
        let ice_id = CardId("ice_wall".to_string());
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1)],
            position: 0,
        });
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        );

        assert_eq!(result, Err(RulesError::NotInEncounter));
    }

    #[test]
    fn actions_issued_during_game_over_fail_with_wrong_phase() {
        let mut state = corp_state(3, 5);
        state.phase = GamePhase::GameOver(Side::Runner);

        let result = apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Corp });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::GameOver(Side::Runner),
            })
        );
    }

    #[test]
    fn end_turn_issued_during_game_over_fails_with_not_in_action_phase() {
        let mut state = corp_state(3, 5);
        state.phase = GamePhase::GameOver(Side::Runner);

        let result = apply_action(&state, &registry(), PlayerAction::EndTurn);

        assert_eq!(
            result,
            Err(RulesError::NotInActionPhase { actual: GamePhase::GameOver(Side::Runner) })
        );
    }

    /// A minimal `Card` with the given install/play `cost` and
    /// `advancement_requirement`, no abilities — used by the
    /// `InstallCard`/`PlayEvent`/`AdvanceCard` cost/advancement tests, which
    /// only care about those two fields.
    fn test_card(
        card_id: &str,
        side: Side,
        card_type: CardType,
        cost: u32,
        advancement_requirement: Option<u32>,
    ) -> Card {
        Card {
            id: CardId(card_id.to_string()),
            title: card_id.to_string(),
            side,
            card_type,
            cost,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement,
            agenda_points: None,
            min_deck_size: None,
        }
    }

    /// A minimal `Card` whose only `abilities` entry is the given
    /// `trigger`/`cost`/`effect` — everything about the card besides its id,
    /// side, and that one ability is irrelevant to `activate_ability`'s
    /// logic, so it's held to placeholder values.
    fn test_card_with_ability(
        card_id: &str,
        side: Side,
        trigger: Trigger,
        cost: Option<Cost>,
        effect: Effect,
    ) -> Card {
        Card {
            id: CardId(card_id.to_string()),
            title: card_id.to_string(),
            side,
            card_type: CardType::Program,
            cost: 0,
            triggers: Vec::new(),
            abilities: vec![AbilityDef { trigger, cost, effect }],
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: None,
            agenda_points: None,
            min_deck_size: None,
        }
    }

    #[test]
    fn runner_activate_ability_pumps_icebreaker_and_deducts_credits() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![card_id.clone()];
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0)],
            position: 0,
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "gordian_blade",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::ModifyStrength(1),
        ));

        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.resources.credits, Credits(4));
        assert_eq!(next.active_run.unwrap().ice[0].current_strength, 1);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 1 },
                GameEvent::AbilityActivated { side: Side::Runner, card_id, ability_index: 0 },
                GameEvent::IceStrengthModified {
                    card_id: CardId("ice_wall".to_string()),
                    new_strength: 1,
                    delta: 1,
                },
            ]
        );
    }

    #[test]
    fn corp_activate_ability_on_unrezzed_asset_returns_card_not_active() {
        let card_id = CardId("pad_campaign".to_string());
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: card_id.clone(),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: false,
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "pad_campaign",
            Side::Corp,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::GainCredits(Side::Corp, 1),
        ));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        );

        assert_eq!(result, Err(RulesError::CardNotActive { side: Side::Corp, card: card_id }));
    }

    #[test]
    fn runner_activate_ability_with_insufficient_credits_propagates_pay_cost_error() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(0);
        state.runner.rig = vec![card_id.clone()];
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0)],
            position: 0,
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "gordian_blade",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::ModifyStrength(1),
        ));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id, ability_index: 0 },
        );

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Runner, available: 0, requested: 1 })
        );
    }

    #[test]
    fn runner_activate_ability_with_invalid_index_returns_invalid_ability_index() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.rig = vec![card_id.clone()];

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "gordian_blade",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::ModifyStrength(1),
        ));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id, ability_index: 1 },
        );

        assert_eq!(result, Err(RulesError::InvalidAbilityIndex(1)));
    }

    #[test]
    fn runner_activate_ability_on_non_paid_trigger_returns_not_manually_activatable() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.rig = vec![card_id.clone()];

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "gordian_blade",
            Side::Runner,
            Trigger::OnEncounter,
            Some(Cost::Credits(1)),
            Effect::ModifyStrength(1),
        ));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id, ability_index: 0 },
        );

        assert_eq!(result, Err(RulesError::AbilityNotManuallyActivatable(0)));
    }

    #[test]
    fn corp_advance_card_adds_advancement_token_and_charges_click_and_credit() {
        let card_id = CardId("priority_requisition".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: false,
            advancement_tokens: 1,
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let mut registry = CardRegistry::new();
        registry.insert(test_card("priority_requisition", Side::Corp, CardType::Agenda, 0, Some(5)));

        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::AdvanceCard { card_id: card_id.clone() },
        )
        .expect("action should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(2));
        assert_eq!(next.corp.resources.credits, Credits(4));
        assert_eq!(next.corp.installed[0].advancement_tokens, 2);
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::CreditsSpent { side: Side::Corp, amount: 1 },
                GameEvent::CardAdvanced { card: card_id, advancement_tokens: 2 },
            ]
        );
    }

    #[test]
    fn corp_advance_card_on_non_agenda_returns_card_not_advanceable() {
        let card_id = CardId("pad_campaign".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: false,
            advancement_tokens: 0,
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let mut registry = CardRegistry::new();
        registry.insert(test_card("pad_campaign", Side::Corp, CardType::Asset, 2, None));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::AdvanceCard { card_id: card_id.clone() },
        );

        assert_eq!(result, Err(RulesError::CardNotAdvanceable { card: card_id }));
    }

    #[test]
    fn corp_advance_card_on_uninstalled_card_returns_card_not_installed() {
        let card_id = CardId("priority_requisition".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), Vec::new());
        let mut registry = CardRegistry::new();
        registry.insert(test_card("priority_requisition", Side::Corp, CardType::Agenda, 0, Some(5)));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::AdvanceCard { card_id: card_id.clone() },
        );

        assert_eq!(result, Err(RulesError::CardNotInstalled { card: card_id }));
    }

    #[test]
    fn runner_turn_advance_card_returns_not_your_turn() {
        let card_id = CardId("priority_requisition".to_string());
        let state = runner_state(3, 0, 0);

        let result = apply_action(&state, &registry(), PlayerAction::AdvanceCard { card_id });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::Action(Side::Runner),
            })
        );
    }

    #[test]
    fn corp_advance_card_with_insufficient_credits_returns_not_enough_credits() {
        let card_id = CardId("priority_requisition".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: false,
            advancement_tokens: 0,
        }];
        let state = corp_state_with_hq_and_installed(3, 0, Vec::new(), installed);
        let mut registry = CardRegistry::new();
        registry.insert(test_card("priority_requisition", Side::Corp, CardType::Agenda, 0, Some(5)));

        let result = apply_action(&state, &registry, PlayerAction::AdvanceCard { card_id });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Corp, available: 0, requested: 1 })
        );
    }
}
