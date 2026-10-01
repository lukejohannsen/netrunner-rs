//! The seat: a `BotAgent` that asks the model, and the planner that
//! answers when the model does not.

use std::collections::{BTreeSet, VecDeque};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use netrunner_bots::BotAgent;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{Deck, PlayerAction, Side};
use netrunner_core::view::ClientView;

use super::profile::{Asks, LlmProfile};
use super::prompt::{self, Chosen, ParseProblem};
use super::protocol::{self, ProviderError, Usage};
use super::secrets::ApiKey;
use super::transport::{Transport, TransportError};

/// How many of its own moves the model is reminded of. Enough for a
/// turn's line of play; few enough that the message stays short.
pub const RECENT_MOVES: usize = 6;

pub struct LlmAgent {
    profile: LlmProfile,
    key: Option<ApiKey>,
    transport: Box<dyn Transport>,
    /// The planner at the chair's rung, which plays every decision the
    /// model is not asked about or fails.
    fallback: Box<dyn BotAgent>,
    /// What the notice calls the fallback: "the operator planner".
    fallback_name: String,
    /// One line per failure, to the board. A send nobody reads is fine.
    notices: Sender<String>,
    recent: VecDeque<(String, String)>,
    own_deck: Deck,
    /// Built on the first decision, once the registry is at hand, and
    /// never again: the same bytes on every request of the game.
    system: Option<String>,
    glossary: BTreeSet<CardId>,
    /// What the game has cost so far, shared with whoever shows it
    /// (`play::MatchHandle`) the way the history is mirrored — read on a
    /// frame, never asked of the match thread.
    spent: Arc<Mutex<Usage>>,
    over_budget: bool,
}

/// Why one decision got no answer from the model.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AskError {
    Provider(ProviderError),
    Transport(TransportError),
    /// The second reply, and what was wrong with it.
    Parse(String, ParseProblem),
}

impl std::fmt::Display for AskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AskError::Provider(error) => write!(f, "{error}"),
            AskError::Transport(error) => write!(f, "{error}"),
            AskError::Parse(text, problem) => write!(f, "{problem} (it said {text:?})"),
        }
    }
}

impl LlmAgent {
    pub fn new(profile: LlmProfile, key: Option<ApiKey>, transport: Box<dyn Transport>, fallback: Box<dyn BotAgent>, fallback_name: String, own_deck: Deck, notices: Sender<String>) -> Self {
        let glossary = prompt::glossary_ids(&own_deck);
        Self {
            profile,
            key,
            transport,
            fallback,
            fallback_name,
            notices,
            recent: VecDeque::new(),
            own_deck,
            system: None,
            glossary,
            spent: Arc::new(Mutex::new(Usage::default())),
            over_budget: false,
        }
    }

    /// The running total, to read beside the board.
    pub fn usage(&self) -> Arc<Mutex<Usage>> {
        Arc::clone(&self.spent)
    }

    pub fn profile(&self) -> &LlmProfile {
        &self.profile
    }

    fn notice(&self, text: String) {
        let _ = self.notices.send(text);
    }

    /// One request, read and parsed; a reply that names no action is
    /// asked about once more with the problem quoted back.
    fn ask(&mut self, system: &str, situation: &str, n: usize) -> Result<Chosen, AskError> {
        let first = self.request(system, situation)?;
        match prompt::parse_reply(&first, n) {
            Ok(chosen) => Ok(chosen),
            Err(problem) => {
                let again = format!("{situation}\n\nYour last reply was {first:?}: {problem}. Answer with the number only.");
                let second = self.request(system, &again)?;
                prompt::parse_reply(&second, n).map_err(|problem| AskError::Parse(second, problem))
            }
        }
    }

    fn request(&mut self, system: &str, user: &str) -> Result<String, AskError> {
        let request = protocol::build(&self.profile, self.key.as_ref(), system, user).map_err(AskError::Provider)?;
        let response = self.transport.complete(&request).map_err(AskError::Transport)?;
        let reply = protocol::read(self.profile.protocol, response.status, &response.body).map_err(AskError::Provider)?;
        let total = {
            let mut spent = self.spent.lock().expect("the usage mirror is never poisoned");
            spent.add(reply.usage);
            spent.total()
        };
        if let Some(budget) = self.profile.token_budget_per_game
            && total >= budget
            && !self.over_budget
        {
            self.over_budget = true;
            self.notice(format!("{}: the budget of {budget} tokens for this game is spent ({total} used) — the built-in {} plays the rest", self.profile.name, self.fallback_name));
        }
        Ok(reply.text)
    }

    fn remember(&mut self, label: &str, why: &str) {
        self.recent.push_back((label.to_string(), why.to_string()));
        while self.recent.len() > RECENT_MOVES {
            self.recent.pop_front();
        }
    }
}

impl BotAgent for LlmAgent {
    fn observe(&mut self, view: &ClientView) {
        // The planner keeps seeing, so what it knows when it is asked to
        // play a decision is what it would have known playing the game.
        self.fallback.observe(view);
    }

    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction {
        let menu = prompt::menu(view, registry);
        if menu.len() == 1 {
            return menu[0].action.clone();
        }
        if menu.is_empty() || self.over_budget || (self.profile.asks == Asks::Clicks && prompt::is_routine(view)) {
            return self.fallback.select_action(view, registry);
        }
        let side = view.viewer.side().unwrap_or(Side::Corp);
        if self.system.is_none() {
            self.system = Some(prompt::system_prompt(side, &self.own_deck, registry, self.profile.explain));
        }
        let system = self.system.clone().expect("just built");
        let recent: Vec<(String, String)> = self.recent.iter().cloned().collect();
        let situation = prompt::situation(view, registry, &menu, &recent, &self.glossary);
        match self.ask(&system, &situation, menu.len()) {
            Ok(chosen) => {
                let entry = &menu[chosen.index];
                self.remember(&entry.label, &chosen.why);
                entry.action.clone()
            }
            Err(error) => {
                self.notice(format!("{}: {error} — the built-in {} played this decision", self.profile.name, self.fallback_name));
                let action = self.fallback.select_action(view, registry);
                let label = menu.iter().find(|entry| entry.action == action).map_or_else(|| "a move".to_string(), |entry| entry.label.clone());
                self.remember(&label, "(the built-in planner chose this)");
                action
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::fixture::view_where;
    use crate::llm::profile::Preset;
    use crate::llm::transport::{openai_reply, Response, ScriptedTransport};
    use netrunner_bots::RandomAgent;
    use netrunner_core::rules::GamePhase;
    use std::sync::mpsc::{channel, Receiver};

    /// A scripted transport the test keeps a hand on after it is boxed.
    #[derive(Clone)]
    struct Shared(Arc<Mutex<ScriptedTransport>>);

    impl Transport for Shared {
        fn complete(&mut self, request: &protocol::Request) -> Result<Response, TransportError> {
            self.0.lock().unwrap().complete(request)
        }
    }

    fn make(profile: LlmProfile, scripted: ScriptedTransport, deck: Deck) -> (LlmAgent, Shared, Receiver<String>) {
        let shared = Shared(Arc::new(Mutex::new(scripted)));
        let (tx, rx) = channel();
        let fallback = Box::new(RandomAgent::new(9));
        let agent = LlmAgent::new(profile, Some(ApiKey::new("not-a-real-key")), Box::new(shared.clone()), fallback, "novice planner".to_string(), deck, tx);
        (agent, shared, rx)
    }

    /// A click decision with at least two options.
    fn a_click(seed: u64, side: Side) -> (CardRegistry, ClientView, Deck) {
        view_where(seed, side, |view| matches!(view.phase, GamePhase::Action(s) if s == side) && view.paid_ability_window.is_none() && view.pending_decision.is_none() && view.legal_actions.len() > 2)
            .expect("a click")
    }

    #[test]
    fn the_chosen_number_is_the_menus_action_and_the_move_is_remembered() {
        let (registry, view, deck) = a_click(1, Side::Corp);
        let (mut agent, shared, rx) = make(Preset::OpenAi.profile("chat"), ScriptedTransport::answering(&["{\"action\": 2, \"why\": \"money\"}"]), deck);
        let menu = prompt::menu(&view, &registry);
        let action = agent.select_action(&view, &registry);
        assert_eq!(action, menu[1].action);
        assert!(rx.try_recv().is_err(), "no notice on success");
        assert_eq!(agent.recent.len(), 1);
        assert_eq!(agent.recent[0], (menu[1].label.clone(), "money".to_string()));
        let seen = &shared.0.lock().unwrap().seen;
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].body["messages"][0]["role"], "system");
        assert!(seen[0].body["messages"][1]["content"].as_str().unwrap().contains("Legal actions:"));
    }

    #[test]
    fn a_bad_reply_is_asked_again_with_the_problem_quoted_and_two_fall_back_with_a_notice() {
        let (registry, view, deck) = a_click(1, Side::Corp);
        let (mut agent, shared, rx) = make(Preset::OpenAi.profile("chat"), ScriptedTransport::answering(&["pass", "1"]), deck.clone());
        let menu = prompt::menu(&view, &registry);
        assert_eq!(agent.select_action(&view, &registry), menu[0].action);
        let seen = shared.0.lock().unwrap().seen.clone();
        assert_eq!(seen.len(), 2);
        let again = seen[1].body["messages"][1]["content"].as_str().unwrap();
        assert!(again.contains("Your last reply was \"pass\": the reply named no action number"), "{again}");
        assert!(rx.try_recv().is_err());
        let (mut agent, shared, rx) = make(Preset::OpenAi.profile("chat"), ScriptedTransport::answering(&["pass", "99"]), deck);
        let action = agent.select_action(&view, &registry);
        assert!(view.legal_actions.contains(&action), "the planner's move is legal");
        assert_eq!(shared.0.lock().unwrap().seen.len(), 2, "one retry, never a loop");
        let notice = rx.try_recv().unwrap();
        assert!(notice.starts_with("chat: 99 is not one of the numbers 1 to"), "{notice}");
        assert!(notice.ends_with("— the built-in novice planner played this decision"), "{notice}");
    }

    #[test]
    fn a_transport_error_a_refusal_and_a_missing_key_each_fall_back_and_say_so() {
        let (registry, view, deck) = a_click(2, Side::Runner);
        let (mut agent, _, rx) = make(Preset::OpenAi.profile("chat"), ScriptedTransport::failing(TransportError::Timeout), deck.clone());
        assert!(view.legal_actions.contains(&agent.select_action(&view, &registry)));
        assert!(rx.try_recv().unwrap().contains("the request timed out"));
        let refusal = Response { status: 200, body: r#"{"content":[],"stop_reason":"refusal"}"#.to_string() };
        let scripted = ScriptedTransport { replies: vec![Ok(refusal), Ok(openai_reply("1"))].into(), seen: Vec::new(), exhausted: None };
        let (mut agent, shared, rx) = make(Preset::Anthropic.profile("claude"), scripted, deck.clone());
        assert!(view.legal_actions.contains(&agent.select_action(&view, &registry)));
        assert!(rx.try_recv().unwrap().contains("declined"));
        // The next decision asks again — and the second scripted reply is
        // an OpenAI body, which the Anthropic reader cannot read, which is
        // itself a named failure rather than a panic.
        assert!(view.legal_actions.contains(&agent.select_action(&view, &registry)));
        assert_eq!(shared.0.lock().unwrap().seen.len(), 2);
        assert!(rx.try_recv().unwrap().contains("could not be read"));
        let (tx, rx) = channel();
        let mut keyless = LlmAgent::new(Preset::Anthropic.profile("claude"), None, Box::new(ScriptedTransport::repeating("1")), Box::new(RandomAgent::new(1)), "novice planner".to_string(), deck, tx);
        assert!(view.legal_actions.contains(&keyless.select_action(&view, &registry)));
        assert!(rx.try_recv().unwrap().contains("no API key"));
    }

    #[test]
    fn a_lone_action_is_taken_unasked_and_under_clicks_a_window_goes_to_the_planner() {
        let (registry, view, deck) = view_where(1, Side::Corp, |view| view.legal_actions.len() == 1).expect("a lone action");
        let (mut agent, shared, _) = make(Preset::OpenAi.profile("chat"), ScriptedTransport::repeating("1"), deck.clone());
        assert_eq!(agent.select_action(&view, &registry), view.legal_actions[0]);
        assert!(shared.0.lock().unwrap().seen.is_empty());
        let (registry, view, deck) = view_where(1, Side::Corp, |view| view.paid_ability_window.is_some() && view.active_run.is_none() && view.pending_decision.is_none() && view.legal_actions.len() > 1).expect("a window with options");
        let profile = LlmProfile { asks: Asks::Clicks, ..Preset::OpenAi.profile("chat") };
        let (mut agent, shared, _) = make(profile, ScriptedTransport::repeating("1"), deck.clone());
        assert!(view.legal_actions.contains(&agent.select_action(&view, &registry)));
        assert!(shared.0.lock().unwrap().seen.is_empty(), "a window is the planner's");
        let (registry, view, _) = a_click(1, Side::Corp);
        assert!(view.legal_actions.contains(&agent.select_action(&view, &registry)));
        assert_eq!(shared.0.lock().unwrap().seen.len(), 1, "a click is the model's");
    }

    #[test]
    fn the_system_prompt_is_the_same_bytes_on_every_request_and_usage_sums_to_a_budget() {
        let (registry, view, deck) = a_click(1, Side::Corp);
        let profile = LlmProfile { token_budget_per_game: Some(2500), ..Preset::OpenAi.profile("chat") };
        let (mut agent, shared, rx) = make(profile, ScriptedTransport::repeating("1"), deck);
        agent.select_action(&view, &registry);
        agent.select_action(&view, &registry);
        let seen = shared.0.lock().unwrap().seen.clone();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].body["messages"][0]["content"], seen[1].body["messages"][0]["content"]);
        assert_eq!(*agent.usage().lock().unwrap(), Usage { input: 2000, cached_input: 1600, output: 20 });
        assert!(rx.try_recv().is_err(), "2020 of 2500");
        agent.select_action(&view, &registry);
        let notice = rx.try_recv().unwrap();
        assert!(notice.contains("budget of 2500 tokens") && notice.contains("3030 used"), "{notice}");
        agent.select_action(&view, &registry);
        assert_eq!(shared.0.lock().unwrap().seen.len(), 3, "no request once the budget is spent");
        assert!(rx.try_recv().is_err(), "said once");
        for i in 0..10 {
            agent.remember(&format!("move {i}"), "");
        }
        assert_eq!(agent.recent.len(), RECENT_MOVES);
    }

    /// The Testing Rule's sweep in small: a model that always says "1"
    /// — the first entry, whatever it is — through whole games in both
    /// chairs never hands the session an illegal action, because what is
    /// played is always the menu's.
    #[test]
    fn a_model_that_always_answers_one_never_hands_the_session_an_illegal_action() {
        use netrunner_core::rules::{DeckOrder, GameState, MatchRules};
        use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};
        for seed in 1..=4u64 {
            for side in [Side::Corp, Side::Runner] {
                let (corp, runner) = sweep_decks_for_seed(seed);
                let registry = crate::decks::sample_deck_registry();
                let own = if side == Side::Corp { corp.to_deck() } else { runner.to_deck() };
                let (state, _) = GameState::setup_with(&corp.to_deck(), &runner.to_deck(), &registry, seed, MatchRules::default(), DeckOrder::Shuffled).unwrap();
                let (tx, rx) = channel();
                let model = LlmAgent::new(Preset::Custom.profile("local"), None, Box::new(ScriptedTransport::repeating("1")), Box::new(RandomAgent::new(seed)), "novice planner".to_string(), own, tx);
                let other = Box::new(RandomAgent::new(seed + 100));
                let (corp_seat, runner_seat) = match side {
                    Side::Corp => (Seat::Agent(Box::new(model)), Seat::Agent(other)),
                    Side::Runner => (Seat::Agent(other), Seat::Agent(Box::new(model))),
                };
                let mut session = Session::new(state, registry, corp_seat, runner_seat);
                match session.run() {
                    SessionStep::Ended { .. } | SessionStep::Stalled(_) => {}
                    other => panic!("seed {seed} {side:?}: {other:?}"),
                }
                assert!(rx.try_recv().is_err(), "seed {seed} {side:?}: a legal menu never fails");
            }
        }
    }
}
