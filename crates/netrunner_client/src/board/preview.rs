//! What a card will ask, said before it is played (ROADMAP Phase 7 §8
//! item 4a, from the second pass over jinteki.net).
//!
//! **The report this answers:** Red Team — "[click]: Run a central server
//! you have not run this turn" — spends its click and only then shows which
//! servers are left. A person who does not remember which they ran finds
//! out by paying. The same holds for every card whose text opens on a
//! choice: the engine pays the cost, moves the card, and parks the
//! decision (`engine::play_event`), and until then the only words on the
//! button are the card's name.
//!
//! **The question is put to the engine, not re-derived.** Which servers
//! Red Team may still run is `exclude_servers_run_this_turn` against the
//! turn's record, narrowed again by whatever forbids a run; which options
//! a `PresentChoice` really offers is the engine's too. Reading that off
//! the card's JSON here would be a second legality check, which a client
//! never makes. So, as `board::breaks` prices a route, the action is
//! applied to `netrunner_bots::determinize`'s sample of the view with the
//! engine's own `apply_action`, and the answers are the legal actions of
//! the viewer's own masked view of what came back — labelled by the same
//! `describe_action` that will label the real buttons, so the preview and
//! the prompt cannot disagree in wording.
//!
//! **Only a choice made from public information is previewed**: a server,
//! or one of a card's own printed options. A selection of *cards* is not —
//! a search of the stack would list the sample's invented stack — and
//! neither is anything reached after the sample drew or accessed, because
//! `GameState::rng_step` moving means the sample's hidden half was read.
//! A preview that might be wrong is worse than none: the button simply
//! says what it said before.
//!
//! **Once per view, never per frame.** A preview costs one `apply_action`
//! for each card that could be played, so a client builds `Asks` when a
//! view arrives and keeps it, the way it keeps `breaks::routes`.

use netrunner_bots::determinize;
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{apply_action, PendingDecision, PlayerAction, Viewer};
use netrunner_core::view::{build_client_view, ClientView};
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::actions::describe_action;

/// What each of a view's legal actions would go on to ask, for those that
/// ask anything.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Asks {
    asked: Vec<(PlayerAction, Vec<String>)>,
}

impl Asks {
    /// Empty for a spectator, who plays nothing.
    pub fn of(view: &ClientView, registry: &CardRegistry) -> Self {
        let Viewer::Player(_) = view.viewer else { return Self::default() };
        let candidates: Vec<&PlayerAction> = view.legal_actions.iter().filter(|action| opens_on_card_text(action)).collect();
        if candidates.is_empty() {
            return Self::default();
        }
        let sample = determinize(view, registry, &mut StdRng::seed_from_u64(0));
        let asked = candidates
            .into_iter()
            .filter_map(|action| {
                let (after, _) = apply_action(&sample, registry, action.clone()).ok()?;
                if after.rng_step != sample.rng_step {
                    return None;
                }
                let next = build_client_view(&after, registry, view.viewer);
                let answers = answers(&next, registry);
                (!answers.is_empty()).then(|| (action.clone(), answers))
            })
            .collect();
        Self { asked }
    }

    /// The answers `action` will offer, in the words their buttons will
    /// carry: `["Run on HQ", "Run on R&D"]`.
    pub fn answers(&self, action: &PlayerAction) -> Option<&[String]> {
        self.asked.iter().find(|(asked, _)| asked == action).map(|(_, answers)| answers.as_slice())
    }

    /// "then asks: Run on HQ / Run on R&D" — what a button adds after the
    /// card's name, and a coaching line after its sentence.
    pub fn line(&self, action: &PlayerAction) -> Option<String> {
        self.answers(action).map(|answers| format!("then asks: {}", answers.join(" / ")))
    }

    /// `label` with what `action` goes on to ask after it: "Use Red Team —
    /// then asks: Run on HQ / Run on R&D". One wording for every list a
    /// client draws, and `label` untouched for an action that asks nothing.
    pub fn label(&self, action: &PlayerAction, label: String) -> String {
        match self.line(action) {
            Some(line) => format!("{label} — {line}"),
            None => label,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.asked.is_empty()
    }
}

/// The actions whose first effect is a card's text, which is where a
/// choice can be the first thing that happens. An install asks where, but
/// every destination is already its own legal action.
fn opens_on_card_text(action: &PlayerAction) -> bool {
    matches!(action, PlayerAction::PlayEvent { .. } | PlayerAction::PlayOperation { .. } | PlayerAction::ActivateAbility { .. })
}

/// The labelled answers to the decision `next` is parked on, when it is
/// one a preview may show.
fn answers(next: &ClientView, registry: &CardRegistry) -> Vec<String> {
    let wanted: fn(&PlayerAction) -> bool = match &next.pending_decision {
        Some(PendingDecision::ChooseServer { .. }) => |action| matches!(action, PlayerAction::ChooseServerForPendingDecision { .. }),
        Some(PendingDecision::ChooseEffect { .. }) => |action| matches!(action, PlayerAction::ResolvePendingChoice { .. }),
        _ => return Vec::new(),
    };
    next.legal_actions.iter().filter(|action| wanted(action)).map(|action| describe_action(action, registry, Some(next))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::CardId;
    use netrunner_core::rules::{GamePhase, GameState, InstallId, InstalledRunnerCard, ServerId, Side};

    use crate::decks::sample_deck_registry;

    fn runner_turn(registry: &CardRegistry, rig: &[&str], grip: &[&str]) -> GameState {
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks.0 = 4;
        state.runner.resources.credits.0 = 10;
        for (i, card) in rig.iter().enumerate() {
            let definition = registry.get(&CardId(card.to_string())).expect("a rig card in the pool");
            state.runner.rig.push(InstalledRunnerCard { card: definition.id.clone(), install_id: InstallId(100 + i as u32), counters: 12, ..Default::default() });
        }
        state.runner.grip = grip.iter().map(|card| CardId(card.to_string())).collect();
        state
    }

    fn the_ability(view: &ClientView) -> PlayerAction {
        view.legal_actions.iter().find(|action| matches!(action, PlayerAction::ActivateAbility { .. })).cloned().expect("Red Team's ability is legal")
    }

    /// The report: which servers Red Team has left is on its button before
    /// the click is spent.
    #[test]
    fn red_team_says_which_servers_are_left_before_it_is_used() {
        let registry = sample_deck_registry();
        let mut state = runner_turn(&registry, &["red_team"], &[]);
        let view = build_client_view(&state, &registry, Side::Runner);
        let asks = Asks::of(&view, &registry);
        assert_eq!(asks.line(&the_ability(&view)).as_deref(), Some("then asks: Run on HQ / Run on R&D / Run on Archives"));

        state.runner.servers_run_this_turn.push(ServerId::Archives);
        let view = build_client_view(&state, &registry, Side::Runner);
        let asks = Asks::of(&view, &registry);
        assert_eq!(asks.answers(&the_ability(&view)), Some(&["Run on HQ".to_string(), "Run on R&D".to_string()][..]));
    }

    /// A card that asks nothing adds nothing, and looking changes nothing:
    /// the view the preview was read from is the view still held.
    #[test]
    fn a_card_that_asks_nothing_is_left_alone() {
        let registry = sample_deck_registry();
        let state = runner_turn(&registry, &[], &["sure_gamble"]);
        let view = build_client_view(&state, &registry, Side::Runner);
        let play = PlayerAction::PlayEvent { card_id: CardId("sure_gamble".to_string()) };
        assert!(view.legal_actions.contains(&play));
        assert_eq!(Asks::of(&view, &registry).line(&play), None);
    }

    /// An event that opens on a server choice is previewed the same way.
    #[test]
    fn a_run_event_names_its_servers() {
        let registry = sample_deck_registry();
        let state = runner_turn(&registry, &[], &["jailbreak"]);
        let view = build_client_view(&state, &registry, Side::Runner);
        let play = PlayerAction::PlayEvent { card_id: CardId("jailbreak".to_string()) };
        assert_eq!(Asks::of(&view, &registry).line(&play).as_deref(), Some("then asks: Run on HQ / Run on R&D"));
    }
}
