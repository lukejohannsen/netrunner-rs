//! The interactive TUI's state: the live `GameState`, the cached legal-move
//! list, UI selection/focus/scroll state, and the accumulated event log
//! (`GameState` has no built-in history — the app collects it itself from
//! each `apply_action` call).

use crossterm::event::{KeyCode, KeyEvent};

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{apply_action, legal_actions, GameEvent, GamePhase, GameState, PlayerAction, RulesError};

use crate::config::ViewAs;

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
}

impl App {
    pub fn new(state: GameState, registry: CardRegistry, view_as: ViewAs) -> Self {
        let legal = legal_actions(&state, &registry);
        App {
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
        }
    }

    fn refresh_legal_actions(&mut self) {
        self.legal = legal_actions(&self.state, &self.registry);
        if self.selected >= self.legal.len() {
            self.selected = self.legal.len().saturating_sub(1);
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
