use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{PendingDecision, PlayerAction};
use netrunner_core::view::ClientView;

/// Whether `action` would undo progress inside a parked card selection: a
/// `ToggleCardSelection` on a position the prompt's `selected` already
/// holds is a **deselect**.
///
/// Deselecting is a legitimate move for a human and a loop for a bot. Two
/// toggles undo each other for free — no click, no card moves, nothing the
/// evaluator scores changes — so a search that is indifferent between the
/// toggles and `ConfirmCardSelection` splits its visits near-uniformly and
/// argmax lands on a toggle by a hair, every time. That is what a 10,000-
/// action "stall" was in every volume run: one Corp toggling 3–5 HQ cards
/// inside a `min == max` prompt (Plutus's rez cost, Anoetic Void, Sprint),
/// 98.7% of the game's actions, Confirm taken once in 9,888 steps, turns
/// never advancing so deck-out never fired (ROADMAP Phase 2 §5).
///
/// Without deselects a bot's selection can only grow. The engine already
/// refuses selecting past `max` (`pending_choice::toggle_card_selection`),
/// and at `max` Confirm is the one non-regressive move left — so **every
/// prompt resolves in at most `max + 1` of the bot's actions**, and no
/// cycle guard is needed to escape a cycle that cannot form. The rules are
/// untouched: the engine still accepts a deselect from a client that wants
/// one. This is a choice about what a bot *chooses from*, which is the
/// bots' half of the boundary AGENTS.md draws.
pub fn is_regressive(action: &PlayerAction, pending: Option<&PendingDecision>) -> bool {
    match (action, pending) {
        (PlayerAction::ToggleCardSelection { position }, Some(PendingDecision::ChooseCards { selected, .. })) => {
            selected.contains(position)
        }
        _ => false,
    }
}

/// `actions` with every regressive move removed — the list a bot should
/// choose from. Never empty when `actions` is not: inside a prompt, below
/// `max` there is always an unselected eligible card or a legal Confirm
/// (`eligible ≥ min` is guaranteed at park time), and at `max` Confirm is
/// legal. Outside a prompt nothing is regressive. The fallback to the
/// original list exists so a future rule that breaks that argument
/// degrades to today's behaviour rather than to a panic; the
/// `debug_assert!` is there so it does not do so silently.
pub fn progressive(actions: &[PlayerAction], pending: Option<&PendingDecision>) -> Vec<PlayerAction> {
    let kept: Vec<PlayerAction> = actions.iter().filter(|action| !is_regressive(action, pending)).cloned().collect();
    if kept.is_empty() && !actions.is_empty() {
        debug_assert!(false, "every legal action was a deselect — a prompt with no way forward: {pending:?}");
        return actions.to_vec();
    }
    kept
}

/// Unified interface for an automated player. Given the current `view` —
/// everything this agent's side is entitled to know, plus `view.
/// legal_actions` (only the actions it may actually submit) — choose
/// exactly one to play.
///
/// `view.legal_actions` is always non-empty — callers only invoke this once
/// it's non-empty (an empty result means the game is over or no decision is
/// pending for this side right now). `: Send` since a `MatchSession` owns
/// agents across `.await` points.
pub trait BotAgent: Send {
    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction;

    /// Shown every view this seat is asked to act on, before
    /// `select_action` — the one door a bot's memory has
    /// (`knowledge::Knowledge::observe`): a card accessed in HQ or looked
    /// at on top of R&D is in this view and in no later one. Called by
    /// the driver (`netrunner_session::Session::step`,
    /// `agent_adapter::BotAgentIndexAdapter`) rather than by each
    /// agent's `select_action`, so a wrapper that never asks its inner
    /// agent (`HandicapAgent` on a blunder) still lets it see. No-op by
    /// default: the random and scripted agents remember nothing.
    fn observe(&mut self, _view: &ClientView) {}
}

/// Lets a boxed trait object stand in for `impl BotAgent` — the same
/// "forward to the inner value" pattern as `policy::PolicyEvaluator`'s own
/// `impl PolicyEvaluator for Box<dyn PolicyEvaluator>`. Needed by any caller
/// that only has a `Box<dyn BotAgent>` (e.g. `netrunner_cli::bots::
/// make_agent`'s return type) but wants to wrap it in something generic
/// over a concrete, sized `A: BotAgent` — such as `agent_adapter::
/// BotAgentIndexAdapter<A>`.
impl BotAgent for Box<dyn BotAgent> {
    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction {
        (**self).select_action(view, registry)
    }

    fn observe(&mut self, view: &ClientView) {
        (**self).observe(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardFilter, CardZoneRef};
    use netrunner_core::rules::{PendingChoiceResume, Side};

    fn prompt(selected: Vec<usize>) -> PendingDecision {
        PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OwnHq,
            filter: CardFilter::Any,
            min: 2,
            max: 2,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected,
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        }
    }

    fn toggle(position: usize) -> PlayerAction {
        PlayerAction::ToggleCardSelection { position }
    }

    #[test]
    fn a_toggle_on_a_selected_position_is_regressive_and_nothing_else_is() {
        let pending = prompt(vec![1]);
        assert!(is_regressive(&toggle(1), Some(&pending)), "toggling a selected card off");
        assert!(!is_regressive(&toggle(0), Some(&pending)), "toggling an unselected card on");
        assert!(!is_regressive(&PlayerAction::ConfirmCardSelection, Some(&pending)));
        assert!(!is_regressive(&toggle(1), None), "no prompt, no selection to regress");
        assert!(!is_regressive(&PlayerAction::EndTurn, Some(&pending)));
    }

    #[test]
    fn progressive_keeps_every_way_forward_and_is_never_empty_inside_a_prompt() {
        // Below max with cards left: the selects survive, the deselect goes.
        let below = prompt(vec![0]);
        let legal = vec![toggle(0), toggle(1), toggle(2)];
        assert_eq!(progressive(&legal, Some(&below)), vec![toggle(1), toggle(2)]);

        // At max: only Confirm is offered by the engine besides deselects,
        // and Confirm is what survives.
        let at_max = prompt(vec![0, 1]);
        let legal = vec![toggle(0), toggle(1), PlayerAction::ConfirmCardSelection];
        assert_eq!(progressive(&legal, Some(&at_max)), vec![PlayerAction::ConfirmCardSelection]);

        // Outside a prompt nothing is filtered.
        let legal = vec![PlayerAction::EndTurn, toggle(3)];
        assert_eq!(progressive(&legal, None), legal);
    }
}
