//! The interactive TUI's state: the live `GameState`, the cached legal-move
//! list, UI selection/focus/scroll state, and the accumulated event log
//! (`GameState` has no built-in history — the app collects it itself from
//! each `apply_action` call).

use crossterm::event::{KeyCode, KeyEvent};

use netrunner_bots::BotAgent;
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{apply_action, legal_actions, GameEvent, GamePhase, GameState, PlayerAction, RulesError, Side};

use crate::bots;
use crate::config::ViewAs;

/// Guard against a stalled/looping game auto-playing forever — same budget
/// as `headless::MAX_TICKS`, which this mirrors for a two-bot game.
const MAX_BOT_STEPS: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Actions,
    Log,
    Inspect,
}

pub struct App {
    pub state: GameState,
    pub registry: CardRegistry,
    pub view_as: ViewAs,
    pub legal: Vec<PlayerAction>,
    pub selected: usize,
    pub event_log: Vec<GameEvent>,
    pub log_scroll: usize,
    pub focus: Focus,
    pub should_quit: bool,
    /// The most recent action rejection, surfaced in the UI rather than
    /// silently dropped — should never actually happen since every offered
    /// action came from `legal_actions` itself, but a defensive display
    /// beats a silent no-op if it ever does.
    pub last_error: Option<RulesError>,
    /// `None` means that side is human-controlled — driven by
    /// `handle_key`/`apply_selected_action` instead of `drive_bots`.
    corp_agent: Option<Box<dyn BotAgent>>,
    runner_agent: Option<Box<dyn BotAgent>>,
}

impl App {
    pub fn new(
        state: GameState,
        registry: CardRegistry,
        view_as: ViewAs,
        corp_agent: Option<Box<dyn BotAgent>>,
        runner_agent: Option<Box<dyn BotAgent>>,
    ) -> Self {
        let legal = legal_actions(&state, &registry);
        let mut app = App {
            state,
            registry,
            view_as,
            legal,
            selected: 0,
            event_log: Vec::new(),
            log_scroll: 0,
            focus: Focus::Actions,
            should_quit: false,
            last_error: None,
            corp_agent,
            runner_agent,
        };
        // The Corp acts first — if it's bot-controlled, play its turn(s)
        // out before the TUI's first draw rather than waiting on a human
        // keypress that would never come.
        app.drive_bots();
        app
    }

    fn refresh_legal_actions(&mut self) {
        self.legal = legal_actions(&self.state, &self.registry);
        if self.selected >= self.legal.len() {
            self.selected = self.legal.len().saturating_sub(1);
        }
    }

    /// Repeatedly lets whichever bot-controlled side currently holds the
    /// decision (`bots::current_actor`) act, until control returns to a
    /// human side, no decision is pending (`StartOfTurn`/`GameOver`), or
    /// `MAX_BOT_STEPS` is hit.
    fn drive_bots(&mut self) {
        for _ in 0..MAX_BOT_STEPS {
            if self.is_game_over() || self.legal.is_empty() {
                break;
            }
            let Some(side) = bots::current_actor(&self.state) else { break };
            let agent = match side {
                Side::Corp => self.corp_agent.as_mut(),
                Side::Runner => self.runner_agent.as_mut(),
            };
            let Some(agent) = agent else { break };

            let action = agent.select_action(&self.state, &self.registry, &self.legal);
            match apply_action(&self.state, &self.registry, action) {
                Ok((next, events)) => {
                    self.state = next;
                    self.event_log.extend(events);
                    self.last_error = None;
                    self.refresh_legal_actions();
                }
                Err(error) => {
                    self.last_error = Some(error);
                    break;
                }
            }
        }
    }

    pub fn is_game_over(&self) -> bool {
        matches!(self.state.phase, GamePhase::GameOver(_))
    }

    pub fn apply_selected_action(&mut self) -> Result<(), RulesError> {
        let Some(action) = self.legal.get(self.selected).cloned() else {
            return Ok(());
        };
        match apply_action(&self.state, &self.registry, action) {
            Ok((next, events)) => {
                self.state = next;
                self.event_log.extend(events);
                self.last_error = None;
                self.refresh_legal_actions();
                self.drive_bots();
                Ok(())
            }
            Err(error) => {
                self.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.is_game_over() {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                self.should_quit = true;
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') if self.focus == Focus::Log => {
                self.log_scroll = self.log_scroll.saturating_add(1);
            }
            KeyCode::Down | KeyCode::Char('j') if self.focus == Focus::Log => {
                self.log_scroll = self.log_scroll.saturating_sub(1);
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let _ = self.apply_selected_action();
            }
            KeyCode::Tab => self.cycle_focus(),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.legal.is_empty() {
            return;
        }
        let len = self.legal.len() as i32;
        let next = (self.selected as i32 + delta).rem_euclid(len);
        self.selected = next as usize;
    }

    fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Actions => Focus::Log,
            Focus::Log => Focus::Inspect,
            Focus::Inspect => Focus::Actions,
        };
    }
}

/// Human-readable label for a `PlayerAction`, resolving `CardId`s to
/// registry titles where available.
pub fn describe_action(action: &PlayerAction, registry: &CardRegistry) -> String {
    let title = |card_id: &netrunner_core::dsl::CardId| -> String {
        registry.get(card_id).map(|c| c.title.clone()).unwrap_or_else(|| card_id.0.clone())
    };

    match action {
        PlayerAction::GainCreditClick { side } => format!("Gain 1 credit ({side:?})"),
        PlayerAction::DrawCardClick => "Draw a card".to_string(),
        PlayerAction::InstallCard { card_id, zone, slot } => {
            format!("Install {} into {:?} ({:?})", title(card_id), zone, slot)
        }
        PlayerAction::RezIce { ice_id } => format!("Rez {}", title(ice_id)),
        PlayerAction::InitiateRun { server } => format!("Run {server:?}"),
        PlayerAction::ContinueRun => "Continue run".to_string(),
        PlayerAction::JackOut => "Jack out".to_string(),
        PlayerAction::CompleteRun => "Complete run".to_string(),
        PlayerAction::PlayEvent { card_id } => format!("Play {}", title(card_id)),
        PlayerAction::PlayOperation { card_id } => format!("Play {}", title(card_id)),
        PlayerAction::InstallHardware { card_id } => format!("Install {}", title(card_id)),
        PlayerAction::InstallProgram { card_id, .. } => format!("Install {}", title(card_id)),
        PlayerAction::BreakSubroutine { ice_id, subroutine_index } => {
            format!("Break subroutine {subroutine_index} on {}", title(ice_id))
        }
        PlayerAction::EndTurn => "End turn".to_string(),
        PlayerAction::DiscardCard { card_id } => format!("Discard {}", title(card_id)),
        PlayerAction::KeepHand => "Keep hand".to_string(),
        PlayerAction::TakeMulligan => "Mulligan".to_string(),
        PlayerAction::ActivateAbility { card_id, ability_index } => {
            format!("Activate ability {ability_index} on {}", title(card_id))
        }
        PlayerAction::AdvanceCard { card_id } => format!("Advance {}", title(card_id)),
        PlayerAction::ScoreAgenda { card_id } => format!("Score {}", title(card_id)),
        PlayerAction::RemoveTag => "Remove a tag".to_string(),
        PlayerAction::TrashResource { card_id } => format!("Trash {}", title(card_id)),
        PlayerAction::SelectCardToAccess { card_id } => format!("Access {}", title(card_id)),
        PlayerAction::StealAgenda { card_id } => format!("Steal {}", title(card_id)),
        PlayerAction::TrashAccessedCard { card_id } => format!("Trash {}", title(card_id)),
        PlayerAction::PassAccessedCard { card_id } => format!("Pass on {}", title(card_id)),
        PlayerAction::PayToAvoidAccessTrigger { card_id } => format!("Pay to avoid {}'s trigger", title(card_id)),
        PlayerAction::DeclineAccessTrigger { card_id } => format!("Decline {}'s trigger", title(card_id)),
        PlayerAction::PassPriority { side } => format!("Pass priority ({side:?})"),
        PlayerAction::SubmitCorpTraceBid { amount } => format!("Bid {amount} (Corp trace)"),
        PlayerAction::SubmitRunnerTraceBid { amount } => format!("Bid {amount} (Runner trace)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::RandomAgent;

    use crate::decks;

    fn setup() -> (GameState, CardRegistry) {
        let registry = decks::kate_vs_hb_registry();
        let (corp_deck, runner_deck) = decks::kate_vs_hb_decks();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 1).expect("legal decks set up cleanly");
        (state, registry)
    }

    #[test]
    fn bot_controlled_side_resolves_its_own_mulligan_before_yielding_to_a_human() {
        let (state, registry) = setup();
        let corp_agent: Option<Box<dyn BotAgent>> = Some(Box::new(RandomAgent::new(1)));

        // `App::new` drives the Corp's Mulligan decision automatically
        // (`bots::current_actor` resolves `Mulligan(Corp)` to the Corp),
        // then stops at `Mulligan(Runner)` since `runner_agent` is `None`
        // (human-controlled) — the same handoff a bot-controlled Corp vs. a
        // human Runner would see in the real TUI.
        let app = App::new(state, registry, ViewAs::Omniscient, corp_agent, None);

        assert_eq!(app.state.phase, GamePhase::Mulligan(Side::Runner));
        assert!(!app.legal.is_empty());
        assert!(app.legal.iter().all(|action| matches!(action, PlayerAction::KeepHand | PlayerAction::TakeMulligan)));
    }

    #[test]
    fn two_bot_controlled_sides_play_an_entire_game_with_no_human_input() {
        let (state, registry) = setup();
        let corp_agent: Option<Box<dyn BotAgent>> = Some(Box::new(RandomAgent::new(2)));
        let runner_agent: Option<Box<dyn BotAgent>> = Some(Box::new(RandomAgent::new(3)));

        let app = App::new(state, registry, ViewAs::Omniscient, corp_agent, runner_agent);

        assert!(app.is_game_over(), "expected the game to reach GameOver within {MAX_BOT_STEPS} bot steps");
        assert!(!app.event_log.is_empty());
    }
}
