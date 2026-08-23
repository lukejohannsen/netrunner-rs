use crate::dsl::CardId;
use crate::rules::action::{PlayerAction, ServerTarget, TargetZone};
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::{self, RunAction, RunPhase, RunState};
use crate::rules::state::{GameState, InstalledCard, Side};

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
        PlayerAction::JackOut => jack_out(state),
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
    if next.runner.stack_size > 0 {
        next.runner.stack_size -= 1;
        next.runner.grip_size += 1;
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
    require_active_turn(state, side)?;
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

fn jack_out(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_active_turn(state, side)?;
    let active_run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    let (_, events) = run::advance_run(active_run, RunAction::JackOut)?;

    let mut next = state.clone();
    next.active_run = None;

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
                installed: Vec::new(),
            },
            runner: crate::rules::state::RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: crate::rules::state::MemoryUnits(0),
                grip_size: 0,
                stack_size: 0,
            },
            active_turn: Side::Corp,
            active_run: None,
        }
    }

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
                installed: Vec::new(),
            },
            runner: crate::rules::state::RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(clicks),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: crate::rules::state::MemoryUnits(0),
                grip_size,
                stack_size,
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
        assert_eq!(next.runner.stack_size, 9);
        assert_eq!(next.runner.grip_size, 6);
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

        assert_eq!(next.runner.stack_size, 0);
        assert_eq!(next.runner.grip_size, 3);
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
}
