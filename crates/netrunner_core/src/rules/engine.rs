use crate::dsl::CardId;
use crate::rules::action::{PlayerAction, ServerTarget, TargetZone};
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::{self, RunAction, RunPhase, RunState};
use crate::rules::state::{GameState, InstalledCard, Side};
use crate::rules::turn;

pub fn apply_action(
    state: &GameState,
    action: PlayerAction,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    match action {
        PlayerAction::GainCreditClick { side } => gain_credit_click(state, side),
        PlayerAction::DrawCardClick => draw_card_click(state),
        PlayerAction::InstallCard { card_id, zone } => install_card(state, card_id, zone),
        PlayerAction::RezIce { ice_id } => rez_ice(state, ice_id),
        PlayerAction::InitiateRun { server } => initiate_run(state, server),
        PlayerAction::ContinueRun => continue_run(state),
        PlayerAction::JackOut => jack_out(state),
        PlayerAction::CompleteRun => complete_run(state),
        PlayerAction::PlayEvent { card_id } => play_event(state, card_id),
        PlayerAction::InstallHardware { card_id } => install_hardware(state, card_id),
        PlayerAction::InstallProgram { card_id, memory_cost } => {
            install_program(state, card_id, memory_cost)
        }
        PlayerAction::BreakSubroutine { ice_id, subroutine_index } => {
            break_subroutine(state, ice_id, subroutine_index)
        }
        PlayerAction::EndTurn => turn::end_turn(state),
    }
}

fn require_active_turn(state: &GameState, side: Side) -> Result<(), RulesError> {
    if state.active_turn != side {
        return Err(RulesError::NotYourTurn { side });
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
    require_active_turn(state, side)?;
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
    require_active_turn(state, side)?;
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
    card_id: CardId,
    zone: TargetZone,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    require_active_turn(state, side)?;
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
    next.corp.installed.push(InstalledCard {
        card: card_id.clone(),
        server: zone,
        rezzed: false,
    });

    Ok((
        next,
        vec![
            GameEvent::ClickSpent { side },
            GameEvent::CardInstalled {
                side,
                card: card_id,
                server: zone,
            },
        ],
    ))
}

fn rez_ice(state: &GameState, ice_id: CardId) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    let rez_window_open =
        matches!(&state.active_run, Some(run) if run.phase == RunPhase::ApproachIce);
    if !rez_window_open {
        require_active_turn(state, side)?;
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
    require_active_turn(state, side)?;
    if state.active_run.is_some() {
        return Err(RulesError::RunAlreadyInProgress);
    }

    let mut next = state.clone();
    spend_click(&mut next, side)?;
    next.active_run = Some(RunState {
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
    require_active_turn(state, side)?;
    let active_run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    let (next_run, events) = run::advance_run(active_run, RunAction::Continue)?;

    let mut next = state.clone();
    next.active_run = Some(next_run);

    Ok((next, events))
}

fn jack_out(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_active_turn(state, side)?;
    let active_run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    let (_, events) = run::advance_run(active_run, RunAction::JackOut)?;

    let mut next = state.clone();
    next.active_run = None;

    Ok((next, events))
}

fn complete_run(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_active_turn(state, side)?;
    let active_run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    if active_run.phase != RunPhase::Success {
        return Err(RulesError::RunNotConcluded { phase: active_run.phase });
    }
    let server = active_run.server;

    let mut next = state.clone();
    next.active_run = None;

    let mut events = run::access_server(&next.corp, server);
    events.push(GameEvent::RunCompleted { server });

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

fn play_event(state: &GameState, card_id: CardId) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_active_turn(state, side)?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    take_from_grip(&mut next, side, &card_id)?;

    Ok((
        next,
        vec![
            GameEvent::ClickSpent { side },
            GameEvent::EventPlayed { side, card: card_id },
        ],
    ))
}

fn install_hardware(
    state: &GameState,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_active_turn(state, side)?;
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
    require_active_turn(state, side)?;
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
    require_active_turn(state, side)?;
    let active_run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;

    let pending = active_run
        .ice
        .get(active_run.position)
        .map(|ice| ice.subroutines_pending)
        .unwrap_or(0);
    if subroutine_index as u32 >= pending {
        return Err(RulesError::InvalidSubroutineIndex {
            index: subroutine_index,
            pending,
        });
    }

    let (next_run, events) = run::advance_run(active_run, RunAction::BreakSubroutine)?;

    let mut next = state.clone();
    next.active_run = Some(next_run);

    Ok((next, events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::run::{RunIce, ServerId};
    use crate::rules::state::{AgendaPoints, Clicks, Credits, PlayerResources};

    fn corp_state(clicks: u32, credits: u32) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState {
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
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: crate::rules::state::MemoryUnits(0),
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
            },
            active_turn: Side::Corp,
            active_run: None,
        }
    }

    /// `stack_size`/`grip_size` are filled with distinct placeholder `CardId`s
    /// (identity doesn't matter for the tests using this — only counts do).
    fn runner_state(clicks: u32, stack_size: u32, grip_size: u32) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState {
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
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(clicks),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: crate::rules::state::MemoryUnits(0),
                grip: (0..grip_size).map(|i| CardId(format!("grip_card_{i}"))).collect(),
                stack: (0..stack_size).map(|i| CardId(format!("stack_card_{i}"))).collect(),
                rig: Vec::new(),
            },
            active_turn: Side::Runner,
            active_run: None,
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
        let (next, events) = apply_action(&state, PlayerAction::GainCreditClick { side: Side::Corp })
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
            apply_action(&state, PlayerAction::DrawCardClick).expect("action should succeed");

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
        let result = apply_action(&state, PlayerAction::GainCreditClick { side: Side::Corp });

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
        let result = apply_action(&state, PlayerAction::GainCreditClick { side: Side::Runner });

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Runner }));
    }

    #[test]
    fn runner_draw_card_click_with_empty_stack_does_not_underflow() {
        let state = runner_state(2, 0, 3);
        let (next, events) =
            apply_action(&state, PlayerAction::DrawCardClick).expect("action should succeed");

        assert_eq!(next.runner.stack.len(), 0);
        assert_eq!(next.runner.grip.len(), 3);
        assert_eq!(next.runner.resources.clicks, Clicks(1));
        assert_eq!(events, vec![GameEvent::ClickSpent { side: Side::Runner }]);
    }

    #[test]
    fn corp_install_card_moves_card_from_hq_to_installed_and_spends_click() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, vec![card_id.clone()], Vec::new());
        let (next, events) = apply_action(
            &state,
            PlayerAction::InstallCard {
                card_id: card_id.clone(),
                zone: ServerId::Hq,
            },
        )
        .expect("action should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(2));
        assert!(next.corp.hq.is_empty());
        assert_eq!(
            next.corp.installed,
            vec![InstalledCard {
                card: card_id.clone(),
                server: ServerId::Hq,
                rezzed: false,
            }]
        );
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
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
    fn runner_turn_install_card_returns_not_your_turn() {
        let card_id = CardId("ice_wall".to_string());
        let state = runner_state(3, 5, 3);
        let result = apply_action(
            &state,
            PlayerAction::InstallCard { card_id, zone: ServerId::Hq },
        );

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Corp }));
    }

    #[test]
    fn corp_install_card_with_card_not_in_hq_returns_card_not_in_hand() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), Vec::new());
        let result = apply_action(
            &state,
            PlayerAction::InstallCard {
                card_id: card_id.clone(),
                zone: ServerId::Hq,
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
            PlayerAction::InstallCard { card_id, zone: ServerId::Hq },
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
            card: card_id.clone(),
            server: ServerId::Hq,
            rezzed: false,
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let (next, events) = apply_action(&state, PlayerAction::RezIce { ice_id: card_id.clone() })
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
        let result = apply_action(&state, PlayerAction::RezIce { ice_id: card_id });

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Corp }));
    }

    #[test]
    fn corp_rez_ice_with_card_not_installed_returns_card_not_installed() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), Vec::new());
        let result = apply_action(&state, PlayerAction::RezIce { ice_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::CardNotInstalled { card: card_id }));
    }

    #[test]
    fn corp_rez_ice_already_rezzed_returns_already_rezzed() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            server: ServerId::Hq,
            rezzed: true,
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let result = apply_action(&state, PlayerAction::RezIce { ice_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::AlreadyRezzed { card: card_id }));
    }

    #[test]
    fn corp_can_rez_ice_during_run_approach_ice_even_though_active_turn_is_runner() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            server: ServerId::Hq,
            rezzed: false,
        }];
        let mut state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        state.active_turn = Side::Runner;
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![RunIce { subroutines_pending: 1 }],
            position: 0,
        });

        let (next, events) = apply_action(&state, PlayerAction::RezIce { ice_id: card_id.clone() })
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
        let (next, events) = apply_action(&state, PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert_eq!(
            next.active_run,
            Some(RunState {
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
        let result = apply_action(&state, PlayerAction::InitiateRun { server: ServerId::Hq });

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Runner }));
    }

    #[test]
    fn runner_initiate_run_with_run_already_active_returns_run_already_in_progress() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![RunIce { subroutines_pending: 1 }],
            position: 0,
        });
        let result = apply_action(&state, PlayerAction::InitiateRun { server: ServerId::RnD });

        assert_eq!(result, Err(RulesError::RunAlreadyInProgress));
    }

    #[test]
    fn runner_initiate_run_with_zero_clicks_returns_not_enough_clicks() {
        let state = runner_state(0, 5, 3);
        let result = apply_action(&state, PlayerAction::InitiateRun { server: ServerId::Hq });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughClicks { side: Side::Runner, available: 0, requested: 1 })
        );
    }

    #[test]
    fn runner_jack_out_ends_run_clears_active_run_no_click_cost() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![RunIce { subroutines_pending: 1 }],
            position: 0,
        });
        let (next, events) =
            apply_action(&state, PlayerAction::JackOut).expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(3));
        assert_eq!(next.active_run, None);
        assert_eq!(events, vec![GameEvent::RunJackedOut { server: ServerId::Hq }]);
    }

    #[test]
    fn corp_turn_jack_out_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, PlayerAction::JackOut);

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Runner }));
    }

    #[test]
    fn runner_jack_out_with_no_active_run_returns_no_active_run() {
        let state = runner_state(3, 5, 3);
        let result = apply_action(&state, PlayerAction::JackOut);

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_jack_out_on_concluded_run_propagates_run_already_concluded() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
        });
        let result = apply_action(&state, PlayerAction::JackOut);

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::Success })
        );
    }

    #[test]
    fn runner_can_initiate_run_again_after_jacking_out() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![RunIce { subroutines_pending: 1 }],
            position: 0,
        });

        let (after_jack_out, _) =
            apply_action(&state, PlayerAction::JackOut).expect("jack out should succeed");
        let (after_initiate, _) = apply_action(
            &after_jack_out,
            PlayerAction::InitiateRun { server: ServerId::RnD },
        )
        .expect("initiating a new run should succeed");

        assert_eq!(
            after_initiate.active_run,
            Some(RunState {
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
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
        });
        let (next, events) =
            apply_action(&state, PlayerAction::CompleteRun).expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(3));
        assert_eq!(next.active_run, None);
        assert_eq!(events, vec![GameEvent::RunCompleted { server: ServerId::Hq }]);
    }

    #[test]
    fn runner_complete_run_before_success_returns_run_not_concluded() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![RunIce { subroutines_pending: 1 }],
            position: 0,
        });
        let result = apply_action(&state, PlayerAction::CompleteRun);

        assert_eq!(
            result,
            Err(RulesError::RunNotConcluded { phase: RunPhase::ApproachIce })
        );
    }

    #[test]
    fn corp_turn_complete_run_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, PlayerAction::CompleteRun);

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Runner }));
    }

    #[test]
    fn runner_complete_run_with_no_active_run_returns_no_active_run() {
        let state = runner_state(3, 5, 3);
        let result = apply_action(&state, PlayerAction::CompleteRun);

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_can_initiate_run_again_after_completing_previous_run() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
        });

        let (after_complete, _) =
            apply_action(&state, PlayerAction::CompleteRun).expect("complete run should succeed");
        let (after_initiate, _) = apply_action(
            &after_complete,
            PlayerAction::InitiateRun { server: ServerId::RnD },
        )
        .expect("initiating a new run should succeed");

        assert_eq!(
            after_initiate.active_run,
            Some(RunState {
                server: ServerId::RnD,
                phase: RunPhase::Initiation,
                ice: Vec::new(),
                position: 0,
            })
        );
    }

    #[test]
    fn runner_complete_run_against_hq_surfaces_card_accessed_event_before_run_completed() {
        let mut state = runner_state(3, 5, 3);
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
        });
        let (next, events) =
            apply_action(&state, PlayerAction::CompleteRun).expect("action should succeed");

        assert_eq!(next.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::Hq
                },
                GameEvent::RunCompleted { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn corp_end_turn_via_apply_action_hands_control_to_runner() {
        let state = corp_state(0, 5);
        let (next, events) =
            apply_action(&state, PlayerAction::EndTurn).expect("action should succeed");

        assert_eq!(next.active_turn, Side::Runner);
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
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::Initiation,
            ice: vec![RunIce { subroutines_pending: 0 }],
            position: 0,
        });

        // Initiation -> ApproachIce
        let (state, events) =
            apply_action(&state, PlayerAction::ContinueRun).expect("continue should succeed");
        assert_eq!(state.runner.resources.clicks, Clicks(3));
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::ApproachIce);
        assert_eq!(events, vec![GameEvent::IceApproached { server: ServerId::Hq, position: 0 }]);

        // ApproachIce -> EncounterIce
        let (state, events) =
            apply_action(&state, PlayerAction::ContinueRun).expect("continue should succeed");
        assert_eq!(state.runner.resources.clicks, Clicks(3));
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::EncounterIce);
        assert_eq!(events, vec![GameEvent::IceEncountered { server: ServerId::Hq, position: 0 }]);

        // EncounterIce (0 pending) -> Success
        let (state, events) =
            apply_action(&state, PlayerAction::ContinueRun).expect("continue should succeed");
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
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce { subroutines_pending: 1 }],
            position: 0,
        });

        let result = apply_action(&state, PlayerAction::ContinueRun);

        assert_eq!(result, Err(RulesError::SubroutinesStillPending { pending: 1 }));
    }

    #[test]
    fn corp_turn_continue_run_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, PlayerAction::ContinueRun);

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Runner }));
    }

    #[test]
    fn runner_continue_run_with_no_active_run_returns_no_active_run() {
        let state = runner_state(3, 0, 0);
        let result = apply_action(&state, PlayerAction::ContinueRun);

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_play_event_removes_card_from_grip_and_spends_click() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 5, 0, vec![card_id.clone()]);
        let (next, events) = apply_action(&state, PlayerAction::PlayEvent { card_id: card_id.clone() })
            .expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert!(next.runner.grip.is_empty());
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::EventPlayed { side: Side::Runner, card: card_id },
            ]
        );

        // Original state is untouched.
        assert_eq!(state.runner.grip, vec![CardId("sure_gamble".to_string())]);
    }

    #[test]
    fn corp_turn_play_event_returns_not_your_turn() {
        let card_id = CardId("sure_gamble".to_string());
        let state = corp_state(3, 5);
        let result = apply_action(&state, PlayerAction::PlayEvent { card_id });

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Runner }));
    }

    #[test]
    fn runner_play_event_with_card_not_in_grip_returns_card_not_in_hand() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 5, 0, Vec::new());
        let result = apply_action(&state, PlayerAction::PlayEvent { card_id: card_id.clone() });

        assert_eq!(
            result,
            Err(RulesError::CardNotInHand { side: Side::Runner, card: card_id })
        );
    }

    #[test]
    fn runner_play_event_with_zero_clicks_returns_not_enough_clicks() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(0, 5, 0, vec![card_id.clone()]);
        let result = apply_action(&state, PlayerAction::PlayEvent { card_id });

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
        let result = apply_action(&state, PlayerAction::InstallHardware { card_id });

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Runner }));
    }

    #[test]
    fn runner_install_hardware_with_card_not_in_grip_returns_card_not_in_hand() {
        let card_id = CardId("clone_chip".to_string());
        let state = runner_state_with_grip(3, 5, 0, Vec::new());
        let result = apply_action(&state, PlayerAction::InstallHardware { card_id: card_id.clone() });

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
            PlayerAction::InstallProgram { card_id, memory_cost: 3 },
        );

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Runner }));
    }

    #[test]
    fn runner_install_program_with_card_not_in_grip_returns_card_not_in_hand() {
        let card_id = CardId("gordian_blade".to_string());
        let state = runner_state_with_grip(3, 5, 4, Vec::new());
        let result = apply_action(
            &state,
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
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce { subroutines_pending: 2 }],
            position: 0,
        });
        let (next, events) = apply_action(
            &state,
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        )
        .expect("action should succeed");

        // No click cost: breaking a subroutine isn't a click action.
        assert_eq!(next.runner.resources.clicks, Clicks(3));
        assert_eq!(
            next.active_run,
            Some(RunState {
                server: ServerId::Hq,
                phase: RunPhase::EncounterIce,
                ice: vec![RunIce { subroutines_pending: 1 }],
                position: 0,
            })
        );
        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken { server: ServerId::Hq, position: 0, remaining: 1 }]
        );
    }

    #[test]
    fn corp_turn_break_subroutine_returns_not_your_turn() {
        let ice_id = CardId("ice_wall".to_string());
        let state = corp_state(3, 5);
        let result = apply_action(
            &state,
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        );

        assert_eq!(result, Err(RulesError::NotYourTurn { side: Side::Runner }));
    }

    #[test]
    fn runner_break_subroutine_with_no_active_run_returns_no_active_run() {
        let ice_id = CardId("ice_wall".to_string());
        let state = runner_state(3, 0, 0);
        let result = apply_action(
            &state,
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        );

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_break_subroutine_with_index_out_of_range_returns_invalid_subroutine_index() {
        let ice_id = CardId("ice_wall".to_string());
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce { subroutines_pending: 1 }],
            position: 0,
        });
        let result = apply_action(
            &state,
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 1 },
        );

        assert_eq!(
            result,
            Err(RulesError::InvalidSubroutineIndex { index: 1, pending: 1 })
        );
    }

    #[test]
    fn runner_break_subroutine_outside_encounter_ice_returns_no_subroutines_pending() {
        let ice_id = CardId("ice_wall".to_string());
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![RunIce { subroutines_pending: 1 }],
            position: 0,
        });
        let result = apply_action(
            &state,
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        );

        assert_eq!(result, Err(RulesError::NoSubroutinesPending));
    }
}
