//! The AI opponents screen, as state: the profiles the settings hold,
//! the keys beside them, which one is open, and what each control does.
//!
//! Pure: the screen hands every press in as an [`Intent`] and reads the
//! [`Outcome`] for what to save and whether to send a probe, so the
//! whole form is tested under plain `cargo test` (AGENTS.md §5).

use netrunner_client::llm::profile::{model_problem, name_problem, url_problem};
use netrunner_client::llm::{profile_problem, ApiKey, Asks, LlmProfile, Preset, Protocol, Secrets};

/// The text fields a profile has. The key is one of them on the screen
/// but never in the profile: it is the secrets file's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Url,
    Model,
    Key,
}

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Name => "Name",
            Field::Url => "Server URL",
            Field::Model => "Model",
            Field::Key => "API key",
        }
    }

    pub fn max_len(self) -> usize {
        match self {
            Field::Name => netrunner_client::llm::profile::MAX_NAME_LEN,
            Field::Url => 300,
            Field::Model => 120,
            Field::Key => 400,
        }
    }
}

/// What a control on the screen asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// Open the profile at that place in the list.
    Select(usize),
    /// A new profile from a preset, named after it and opened.
    Add(Preset),
    /// Remove the open profile and its key.
    Remove,
    /// Refill the open profile's URL, model and effort from a preset,
    /// keeping its name.
    Preset(Preset),
    Protocol(Protocol),
    Asks(Asks),
    ToggleExplain,
    StepTimeout(i32),
    StepBudget(i32),
    /// Open a text field for one of the profile's values.
    Edit(Field),
    /// The field committed its text.
    Committed(Field, String),
    Cancelled,
    ClearKey,
    /// Send the probe.
    Test,
    /// The probe's answer, in a line.
    Tested(Result<String, String>),
}

/// What the screen does after an intent: redraw, save either file, and
/// send a probe for the profile the test asked about.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub redraw: bool,
    pub save_settings: bool,
    pub save_secrets: bool,
    pub probe: Option<(LlmProfile, Option<ApiKey>)>,
}

impl Outcome {
    const NOTHING: Outcome = Outcome { redraw: false, save_settings: false, save_secrets: false, probe: None };
    const REDRAW: Outcome = Outcome { redraw: true, ..Outcome::NOTHING };
    const SETTINGS: Outcome = Outcome { redraw: true, save_settings: true, ..Outcome::NOTHING };
    const SECRETS: Outcome = Outcome { redraw: true, save_secrets: true, ..Outcome::NOTHING };
    const BOTH: Outcome = Outcome { redraw: true, save_settings: true, save_secrets: true, probe: None };
}

/// The timeouts the `< >` step through, in seconds; `None` is the
/// default (`llm::profile::DEFAULT_TIMEOUT_SECS`).
const TIMEOUTS: [Option<u64>; 6] = [Some(30), Some(60), None, Some(300), Some(600), Some(900)];
/// The per-game token budgets the `< >` step through; `None` is no cap.
const BUDGETS: [Option<u64>; 6] = [None, Some(50_000), Some(100_000), Some(200_000), Some(500_000), Some(1_000_000)];

#[derive(Debug, Clone, PartialEq)]
pub struct Opponents {
    pub profiles: Vec<LlmProfile>,
    pub secrets: Secrets,
    pub selected: Option<usize>,
    pub editing: Option<Field>,
    /// Why the last edit was refused, for the line under the form.
    pub notice: Option<String>,
    pub testing: bool,
    pub last_test: Option<Result<String, String>>,
}

impl Opponents {
    pub fn new(profiles: Vec<LlmProfile>, secrets: Secrets) -> Self {
        let selected = (!profiles.is_empty()).then_some(0);
        Self { profiles, secrets, selected, editing: None, notice: None, testing: false, last_test: None }
    }

    pub fn selected(&self) -> Option<&LlmProfile> {
        self.selected.and_then(|i| self.profiles.get(i))
    }

    fn selected_mut(&mut self) -> Option<&mut LlmProfile> {
        self.selected.and_then(|i| self.profiles.get_mut(i))
    }

    pub fn has_key(&self, name: &str) -> bool {
        self.secrets.get(name).is_some()
    }

    /// The list's rows: each profile's name and its model.
    pub fn rows(&self) -> Vec<String> {
        self.profiles.iter().map(|profile| format!("{} · {}", profile.name, profile.model)).collect()
    }

    /// A field's value as the form shows it; the key as "set" or "not set".
    pub fn value(&self, field: Field) -> String {
        let Some(profile) = self.selected() else { return String::new() };
        match field {
            Field::Name => profile.name.clone(),
            Field::Url => profile.url.clone(),
            Field::Model => profile.model.clone(),
            Field::Key => if self.has_key(&profile.name) { "set".to_string() } else { "not set".to_string() },
        }
    }

    /// The text a field opens with: the value, or nothing for the key,
    /// which is never shown back.
    pub fn editable(&self, field: Field) -> String {
        match field {
            Field::Key => String::new(),
            other => self.value(other),
        }
    }

    pub fn timeout_label(&self) -> String {
        match self.selected().and_then(|profile| profile.timeout_secs) {
            Some(secs) => format!("{secs} s"),
            None => format!("{} s (default)", netrunner_client::llm::profile::DEFAULT_TIMEOUT_SECS),
        }
    }

    pub fn budget_label(&self) -> String {
        match self.selected().and_then(|profile| profile.token_budget_per_game) {
            Some(n) => format!("{}K tokens", n / 1000),
            None => "none".to_string(),
        }
    }

    /// A name no profile has: the preset's, numbered when taken.
    fn fresh_name(&self, preset: Preset) -> String {
        let base = preset.label().to_lowercase();
        let taken = |name: &str| self.profiles.iter().any(|profile| profile.name == name);
        if !taken(&base) {
            return base;
        }
        (2..).map(|n| format!("{base}-{n}")).find(|name| !taken(name)).expect("a free name")
    }

    pub fn apply(&mut self, intent: Intent) -> Outcome {
        self.notice = None;
        match intent {
            Intent::Select(index) => {
                if index < self.profiles.len() && self.selected != Some(index) {
                    self.selected = Some(index);
                    self.editing = None;
                    self.last_test = None;
                    return Outcome::REDRAW;
                }
                Outcome::NOTHING
            }
            Intent::Add(preset) => {
                let name = self.fresh_name(preset);
                self.profiles.push(preset.profile(&name));
                self.selected = Some(self.profiles.len() - 1);
                self.editing = None;
                self.last_test = None;
                Outcome::SETTINGS
            }
            Intent::Remove => {
                let Some(index) = self.selected else { return Outcome::NOTHING };
                let removed = self.profiles.remove(index);
                let had_key = self.secrets.set(&removed.name, None);
                self.selected = if self.profiles.is_empty() { None } else { Some(index.min(self.profiles.len() - 1)) };
                self.editing = None;
                self.last_test = None;
                Outcome { save_secrets: had_key, ..Outcome::SETTINGS }
            }
            Intent::Preset(preset) => {
                let Some(profile) = self.selected_mut() else { return Outcome::NOTHING };
                let filled = preset.profile(&profile.name);
                profile.protocol = filled.protocol;
                profile.url = filled.url;
                profile.model = filled.model;
                profile.effort = filled.effort;
                Outcome::SETTINGS
            }
            Intent::Protocol(protocol) => {
                let Some(profile) = self.selected_mut() else { return Outcome::NOTHING };
                if profile.protocol == protocol {
                    return Outcome::NOTHING;
                }
                profile.protocol = protocol;
                Outcome::SETTINGS
            }
            Intent::Asks(asks) => {
                let Some(profile) = self.selected_mut() else { return Outcome::NOTHING };
                if profile.asks == asks {
                    return Outcome::NOTHING;
                }
                profile.asks = asks;
                Outcome::SETTINGS
            }
            Intent::ToggleExplain => {
                let Some(profile) = self.selected_mut() else { return Outcome::NOTHING };
                profile.explain = !profile.explain;
                Outcome::SETTINGS
            }
            Intent::StepTimeout(delta) => {
                let Some(profile) = self.selected_mut() else { return Outcome::NOTHING };
                let index = TIMEOUTS.iter().position(|t| *t == profile.timeout_secs).unwrap_or(2) as i32;
                profile.timeout_secs = TIMEOUTS[(index + delta).rem_euclid(TIMEOUTS.len() as i32) as usize];
                Outcome::SETTINGS
            }
            Intent::StepBudget(delta) => {
                let Some(profile) = self.selected_mut() else { return Outcome::NOTHING };
                let index = BUDGETS.iter().position(|b| *b == profile.token_budget_per_game).unwrap_or(0) as i32;
                profile.token_budget_per_game = BUDGETS[(index + delta).rem_euclid(BUDGETS.len() as i32) as usize];
                Outcome::SETTINGS
            }
            Intent::Edit(field) => {
                if self.selected.is_none() {
                    return Outcome::NOTHING;
                }
                self.editing = Some(field);
                Outcome::REDRAW
            }
            Intent::Cancelled => {
                self.editing = None;
                Outcome::REDRAW
            }
            Intent::Committed(field, text) => {
                self.editing = None;
                let Some(index) = self.selected else { return Outcome::REDRAW };
                let text = text.trim().to_string();
                match field {
                    Field::Key => {
                        let name = self.profiles[index].name.clone();
                        if text.is_empty() {
                            return Outcome::REDRAW;
                        }
                        self.secrets.set(&name, Some(ApiKey::new(text)));
                        Outcome::SECRETS
                    }
                    Field::Name | Field::Url | Field::Model => {
                        // The edited field alone is judged: a Custom profile
                        // starts blank and is filled in one field at a time.
                        let problem = match field {
                            Field::Name => name_problem(&text),
                            Field::Url => url_problem(&text),
                            _ => model_problem(&text),
                        };
                        if let Some(problem) = problem {
                            self.notice = Some(format!("Not changed: {problem}"));
                            return Outcome::REDRAW;
                        }
                        let mut edited = self.profiles[index].clone();
                        match field {
                            Field::Name => edited.name = text,
                            Field::Url => edited.url = text,
                            _ => edited.model = text,
                        }
                        if field == Field::Name && self.profiles.iter().enumerate().any(|(i, p)| i != index && p.name == edited.name) {
                            self.notice = Some(format!("There is already an opponent named {:?}", edited.name));
                            return Outcome::REDRAW;
                        }
                        let renamed = field == Field::Name && edited.name != self.profiles[index].name;
                        if renamed {
                            self.secrets.rename(&self.profiles[index].name, &edited.name);
                        }
                        self.profiles[index] = edited;
                        if renamed { Outcome::BOTH } else { Outcome::SETTINGS }
                    }
                }
            }
            Intent::ClearKey => {
                let Some(profile) = self.selected() else { return Outcome::NOTHING };
                let name = profile.name.clone();
                if self.secrets.set(&name, None) { Outcome::SECRETS } else { Outcome::NOTHING }
            }
            Intent::Test => {
                let Some(profile) = self.selected().cloned() else { return Outcome::NOTHING };
                if self.testing {
                    return Outcome::NOTHING;
                }
                if let Some(problem) = profile_problem(&profile) {
                    self.notice = Some(format!("Cannot test: {problem}"));
                    return Outcome::REDRAW;
                }
                self.testing = true;
                self.last_test = None;
                let key = self.secrets.get(&profile.name).cloned();
                Outcome { probe: Some((profile, key)), ..Outcome::REDRAW }
            }
            Intent::Tested(answer) => {
                self.testing = false;
                self.last_test = Some(answer);
                Outcome::REDRAW
            }
        }
    }
}

impl Outcome {
    pub fn is_nothing(&self) -> bool {
        *self == Outcome::NOTHING
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Opponents {
        Opponents::new(Vec::new(), Secrets::default())
    }

    #[test]
    fn adding_from_a_preset_opens_a_named_profile_and_saves_the_settings() {
        let mut form = fresh();
        assert_eq!(form.selected(), None);
        assert_eq!(form.apply(Intent::Add(Preset::Ollama)), Outcome::SETTINGS);
        assert_eq!(form.selected().unwrap().name, "ollama");
        assert_eq!(form.apply(Intent::Add(Preset::Ollama)), Outcome::SETTINGS);
        assert_eq!(form.selected().unwrap().name, "ollama-2", "a taken name is numbered");
        assert_eq!(form.rows(), ["ollama · llama3.1", "ollama-2 · llama3.1"]);
        assert_eq!(form.value(Field::Key), "not set");
        assert_eq!(form.apply(Intent::Select(0)), Outcome::REDRAW);
        assert_eq!(form.apply(Intent::Select(0)), Outcome::NOTHING, "already open");
        assert_eq!(form.apply(Intent::Select(9)), Outcome::NOTHING);
    }

    #[test]
    fn a_key_goes_to_the_secrets_and_follows_a_rename_and_a_removal() {
        let mut form = fresh();
        form.apply(Intent::Add(Preset::Anthropic));
        assert_eq!(form.apply(Intent::Edit(Field::Key)), Outcome::REDRAW);
        assert_eq!(form.editing, Some(Field::Key));
        assert_eq!(form.editable(Field::Key), "", "a key is never shown back");
        assert_eq!(form.apply(Intent::Committed(Field::Key, "  not-a-real-key ".to_string())), Outcome::SECRETS);
        assert_eq!(form.editing, None);
        assert_eq!(form.value(Field::Key), "set");
        assert!(form.profiles.iter().all(|p| !format!("{p:?}").contains("not-a-real-key")), "the profile never holds it");
        assert_eq!(form.apply(Intent::Committed(Field::Name, "claude".to_string())), Outcome::BOTH, "the key follows the name");
        assert!(form.has_key("claude") && !form.has_key("anthropic"));
        assert_eq!(form.apply(Intent::Committed(Field::Key, String::new())), Outcome::REDRAW, "an empty commit changes nothing");
        assert_eq!(form.value(Field::Key), "set");
        assert_eq!(form.apply(Intent::ClearKey), Outcome::SECRETS);
        assert_eq!(form.value(Field::Key), "not set");
        assert_eq!(form.apply(Intent::ClearKey), Outcome::NOTHING);
        form.apply(Intent::Committed(Field::Key, "another-fake-key".to_string()));
        assert_eq!(form.apply(Intent::Remove), Outcome::BOTH);
        assert_eq!(form.selected(), None);
        assert!(!form.has_key("claude"));
    }

    #[test]
    fn a_bad_edit_is_refused_with_its_reason_and_the_profile_kept() {
        let mut form = fresh();
        form.apply(Intent::Add(Preset::OpenAi));
        form.apply(Intent::Add(Preset::Gemini));
        assert_eq!(form.apply(Intent::Committed(Field::Url, "ftp://x".to_string())), Outcome::REDRAW);
        assert!(form.notice.as_deref().unwrap().contains("http"));
        assert_eq!(form.selected().unwrap().url, Preset::Gemini.profile("g").url);
        assert_eq!(form.apply(Intent::Committed(Field::Name, "openai".to_string())), Outcome::REDRAW);
        assert!(form.notice.as_deref().unwrap().contains("already"));
        assert_eq!(form.apply(Intent::Committed(Field::Model, "gemini-x".to_string())), Outcome::SETTINGS);
        assert_eq!(form.notice, None);
        assert_eq!(form.value(Field::Model), "gemini-x");
    }

    #[test]
    fn the_dials_step_and_wrap_and_a_preset_refills_without_renaming() {
        let mut form = fresh();
        form.apply(Intent::Add(Preset::Custom));
        form.apply(Intent::Committed(Field::Name, "mine".to_string()));
        assert_eq!(form.selected().unwrap().name, "mine", "Custom's blank URL is not a problem for a name");
        assert_eq!(form.timeout_label(), "120 s (default)");
        assert_eq!(form.apply(Intent::StepTimeout(1)), Outcome::SETTINGS);
        assert_eq!(form.timeout_label(), "300 s");
        form.apply(Intent::StepTimeout(-2));
        assert_eq!(form.timeout_label(), "60 s");
        assert_eq!(form.budget_label(), "none");
        form.apply(Intent::StepBudget(-1));
        assert_eq!(form.budget_label(), "1000K tokens", "wraps");
        form.apply(Intent::StepBudget(1));
        assert_eq!(form.budget_label(), "none");
        assert_eq!(form.apply(Intent::Asks(Asks::Clicks)), Outcome::SETTINGS);
        assert_eq!(form.apply(Intent::Asks(Asks::Clicks)), Outcome::NOTHING);
        assert_eq!(form.apply(Intent::ToggleExplain), Outcome::SETTINGS);
        assert!(form.selected().unwrap().explain);
        assert_eq!(form.apply(Intent::Preset(Preset::Anthropic)), Outcome::SETTINGS);
        let profile = form.selected().unwrap();
        assert_eq!((profile.name.as_str(), profile.protocol, profile.model.as_str()), ("mine", Protocol::Anthropic, "claude-opus-5-5"));
        assert_eq!(profile.asks, Asks::Clicks, "the dials are the person's, not the preset's");
        assert_eq!(form.apply(Intent::Protocol(Protocol::Ollama)), Outcome::SETTINGS);
        assert_eq!(form.apply(Intent::Protocol(Protocol::Ollama)), Outcome::NOTHING);
    }

    #[test]
    fn a_test_sends_one_probe_at_a_time_and_shows_its_answer() {
        let mut form = fresh();
        assert_eq!(form.apply(Intent::Test), Outcome::NOTHING, "nothing to test");
        form.apply(Intent::Add(Preset::Custom));
        assert_eq!(form.apply(Intent::Test), Outcome::REDRAW);
        assert!(form.notice.as_deref().unwrap().starts_with("Cannot test:"), "a blank URL cannot be probed");
        form.apply(Intent::Committed(Field::Url, "http://localhost:1234/v1".to_string()));
        form.apply(Intent::Committed(Field::Model, "local".to_string()));
        let outcome = form.apply(Intent::Test);
        assert!(form.testing);
        let (profile, key) = outcome.probe.expect("a probe");
        assert_eq!((profile.name.as_str(), key), ("custom", None));
        assert_eq!(form.apply(Intent::Test), Outcome::NOTHING, "one at a time");
        assert_eq!(form.apply(Intent::Tested(Ok("The model answered: OK".to_string()))), Outcome::REDRAW);
        assert!(!form.testing);
        assert_eq!(form.last_test, Some(Ok("The model answered: OK".to_string())));
        form.apply(Intent::Select(0));
        assert_eq!(form.last_test, Some(Ok("The model answered: OK".to_string())), "reselecting the same profile keeps it");
    }
}
