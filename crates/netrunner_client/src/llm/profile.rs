//! What the settings file keeps about a model opponent: everything but
//! its key.

use serde::{Deserialize, Serialize};

/// How long a request may take before the planner plays the decision,
/// when the profile says nothing. Two minutes: a reasoning model at low
/// effort answers in seconds, and a local model on a laptop in tens of
/// them; a person watching "thinking…" for longer than this would rather
/// the game went on.
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;
/// A profile's name is spelled into the record as `llm:<name>`, listed
/// on the new-game form and typed on the terminal's flag.
pub const MAX_NAME_LEN: usize = 32;
/// The timeout a profile may ask for, in seconds.
pub const TIMEOUT_RANGE: std::ops::RangeInclusive<u64> = 5..=900;

/// One model opponent, as `settings.toml` keeps it under `[[opponents]]`.
/// The key is in `secrets.toml` under the same `name` (`super::secrets`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmProfile {
    pub name: String,
    pub protocol: Protocol,
    /// The base URL: `https://api.anthropic.com`, `http://localhost:11434`.
    /// The protocol appends its path.
    pub url: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    /// How hard the model thinks, in the provider's own word (`low`,
    /// `medium`, `high`): Anthropic's `output_config.effort`, OpenAI's
    /// `reasoning_effort`, and `think: false` for Ollama at `low`. Sent
    /// only when set, because a model that does not reason refuses it.
    /// Thinking is billed as output, so this is the biggest output-side
    /// lever there is, and the presets set `low`: a card game's move does
    /// not want a treatise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Which decisions the model is asked about.
    #[serde(default, skip_serializing_if = "Asks::is_default")]
    pub asks: Asks,
    /// Whether the model is asked for a word on why, which the log then
    /// shows. Off by default: output tokens cost several times input, and
    /// a number is all the game needs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub explain: bool,
    /// Tokens (input and output together, as the provider counts them)
    /// after which the planner plays the rest of the game, with a notice.
    /// A backstop against a runaway, never a tuning knob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget_per_game: Option<u64>,
}

impl LlmProfile {
    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS))
    }

    /// The base URL without a trailing slash, so a path appended to it
    /// never doubles one.
    pub fn base_url(&self) -> &str {
        self.url.trim_end_matches('/')
    }
}

/// The wire shape a server speaks. Three cover the field: Anthropic's own
/// Messages API; the chat-completions shape OpenAI set and nearly every
/// hosted and local server copies (Gemini, Mistral, Groq, LM Studio,
/// llama.cpp, vLLM, and Ollama's compatible endpoint); and Ollama's own
/// `/api/chat`, which the person asked for by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    Ollama,
}

impl Protocol {
    pub const ALL: [Protocol; 3] = [Protocol::Anthropic, Protocol::OpenAi, Protocol::Ollama];

    pub fn label(self) -> &'static str {
        match self {
            Protocol::Anthropic => "Anthropic",
            Protocol::OpenAi => "OpenAI-compatible",
            Protocol::Ollama => "Ollama",
        }
    }

    /// Whether a request without a key is refused before it is sent.
    /// Only Anthropic's: the OpenAI shape is what a local server speaks,
    /// with no key at all, so a request in it goes without the header and
    /// a hosted provider's 401 is the notice.
    pub fn needs_key(self) -> bool {
        match self {
            Protocol::Anthropic => true,
            Protocol::OpenAi | Protocol::Ollama => false,
        }
    }
}

/// Which decisions the model is asked about. Most of a game's decisions
/// are priority windows — pass, or rez or use something — and `Clicks`
/// hands those and a parked payment split to the planner, cutting the
/// requests a game makes by roughly two thirds. The trade is that whether
/// to rez during a run is then the planner's call, which the screen says
/// beside the choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Asks {
    #[default]
    Everything,
    Clicks,
}

impl Asks {
    pub const ALL: [Asks; 2] = [Asks::Everything, Asks::Clicks];

    pub fn is_default(&self) -> bool {
        *self == Asks::default()
    }

    pub fn label(self) -> &'static str {
        match self {
            Asks::Everything => "Every decision",
            Asks::Clicks => "Clicks and runs only",
        }
    }

    /// The one line the screen puts under the choice.
    pub fn note(self) -> &'static str {
        match self {
            Asks::Everything => "The model answers every priority window too — about three times the requests.",
            Asks::Clicks => "Priority windows and payment splits go to the built-in planner, so whether to rez during a run is its call.",
        }
    }
}

/// What the Add button offers: a provider with its URL and a model filled
/// in, or a blank for any server. Gemini is Google's OpenAI-compatible
/// endpoint with a Google key; Custom is the OpenAI shape with nothing
/// filled, because that is what a local server almost always speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Anthropic,
    OpenAi,
    Gemini,
    Ollama,
    Custom,
}

impl Preset {
    pub const ALL: [Preset; 5] = [Preset::Anthropic, Preset::OpenAi, Preset::Gemini, Preset::Ollama, Preset::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Anthropic => "Anthropic",
            Preset::OpenAi => "OpenAI",
            Preset::Gemini => "Gemini",
            Preset::Ollama => "Ollama",
            Preset::Custom => "Custom",
        }
    }

    pub fn protocol(self) -> Protocol {
        match self {
            Preset::Anthropic => Protocol::Anthropic,
            Preset::OpenAi | Preset::Gemini | Preset::Custom => Protocol::OpenAi,
            Preset::Ollama => Protocol::Ollama,
        }
    }

    /// Whether the screen asks for a key. Ollama and a local server take
    /// none; a hosted provider always does.
    pub fn needs_key(self) -> bool {
        match self {
            Preset::Anthropic | Preset::OpenAi | Preset::Gemini => true,
            Preset::Ollama | Preset::Custom => false,
        }
    }

    /// The profile the preset fills in under `name`. The model ids are
    /// the providers' current ones at the time of writing (1 October
    /// 2026); the person types another in the model field.
    pub fn profile(self, name: &str) -> LlmProfile {
        let (url, model, effort) = match self {
            Preset::Anthropic => ("https://api.anthropic.com", "claude-opus-5-5", Some("low")),
            Preset::OpenAi => ("https://api.openai.com/v1", "gpt-5", Some("low")),
            Preset::Gemini => ("https://generativelanguage.googleapis.com/v1beta/openai", "gemini-2.5-flash", None),
            Preset::Ollama => ("http://localhost:11434", "llama3.1", None),
            Preset::Custom => ("", "", None),
        };
        LlmProfile {
            name: name.to_string(),
            protocol: self.protocol(),
            url: url.to_string(),
            model: model.to_string(),
            timeout_secs: None,
            effort: effort.map(str::to_string),
            asks: Asks::default(),
            explain: false,
            token_budget_per_game: None,
        }
    }

    /// The cheap tier for a card game, named beside the model field so
    /// the person knows the lever is theirs.
    pub fn cheap_hint(self) -> Option<&'static str> {
        match self {
            Preset::Anthropic => Some("For a card game, claude-haiku-4-5 is the cheap choice."),
            Preset::OpenAi => Some("A \"mini\" model is the cheap choice."),
            Preset::Gemini => Some("A \"flash\" model is the cheap choice."),
            Preset::Ollama | Preset::Custom => None,
        }
    }
}

/// Why a profile cannot be played or tested, in a line, or `None`. The
/// shape of `relay_setting` on the desktop: a reason, never a silent
/// drop. One question per field, so a form can refuse the field being
/// edited without holding a blank Custom profile's other fields against
/// it — a profile is filled in one field at a time.
pub fn profile_problem(profile: &LlmProfile) -> Option<String> {
    name_problem(&profile.name).or_else(|| url_problem(&profile.url)).or_else(|| model_problem(&profile.model)).or_else(|| {
        profile.timeout_secs.filter(|secs| !TIMEOUT_RANGE.contains(secs)).map(|_| format!("The timeout must be between {} and {} seconds", TIMEOUT_RANGE.start(), TIMEOUT_RANGE.end()))
    })
}

pub fn name_problem(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return Some("The opponent needs a name".to_string());
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Some(format!("The name is longer than {MAX_NAME_LEN} characters"));
    }
    if name.contains(':') {
        return Some("The name cannot contain ':'".to_string());
    }
    None
}

pub fn url_problem(url: &str) -> Option<String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Some("The URL must start with http:// or https://".to_string());
    }
    if url.len() <= "https://".len() {
        return Some("The URL names no server".to_string());
    }
    None
}

pub fn model_problem(model: &str) -> Option<String> {
    if model.trim().is_empty() { Some("The model needs a name".to_string()) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_fill_url_and_model_and_say_whether_a_key_is_needed() {
        for preset in Preset::ALL {
            let profile = preset.profile("p");
            assert_eq!(profile.protocol, preset.protocol());
            if preset != Preset::Custom {
                assert_eq!(profile_problem(&profile), None, "{preset:?}: {profile:?}");
            }
        }
        assert_eq!(Preset::Anthropic.profile("a").model, "claude-opus-5-5");
        assert!(Preset::Gemini.profile("g").url.contains("/openai"), "Gemini speaks the OpenAI shape");
        assert!(!Preset::Ollama.needs_key() && !Preset::Custom.needs_key());
        assert!(Preset::Anthropic.needs_key() && Preset::Gemini.needs_key());
        assert_eq!(Preset::Anthropic.profile("a").effort.as_deref(), Some("low"));
    }

    #[test]
    fn a_profile_problem_names_the_field() {
        let good = Preset::Ollama.profile("local");
        assert_eq!(profile_problem(&good), None);
        let problem = |edit: fn(&mut LlmProfile)| {
            let mut p = good.clone();
            edit(&mut p);
            profile_problem(&p).expect("a problem")
        };
        assert!(problem(|p| p.name = "  ".into()).contains("name"));
        assert!(problem(|p| p.name = "a:b".into()).contains("':'"));
        assert!(problem(|p| p.name = "x".repeat(33)).contains("longer"));
        assert!(problem(|p| p.url = "ftp://x".into()).contains("http"));
        assert!(problem(|p| p.url = "https://".into()).contains("server"));
        assert!(problem(|p| p.model = String::new()).contains("model"));
        assert!(problem(|p| p.timeout_secs = Some(2)).contains("timeout"));
        assert_eq!(profile_problem(&Preset::Custom.profile("c")).as_deref(), Some("The URL must start with http:// or https://"));
    }

    #[test]
    fn a_profile_round_trips_as_toml_and_the_defaults_are_left_out() {
        let profile = Preset::OpenAi.profile("chat");
        let text = toml::to_string(&profile).unwrap();
        assert!(text.contains("protocol = \"openai\""), "{text}");
        assert!(!text.contains("asks") && !text.contains("explain") && !text.contains("budget"), "{text}");
        assert_eq!(toml::from_str::<LlmProfile>(&text).unwrap(), profile);
        let full = LlmProfile { asks: Asks::Clicks, explain: true, token_budget_per_game: Some(200_000), timeout_secs: Some(30), ..profile };
        let text = toml::to_string(&full).unwrap();
        assert!(text.contains("asks = \"clicks\""), "{text}");
        assert_eq!(toml::from_str::<LlmProfile>(&text).unwrap(), full);
        assert_eq!(full.base_url(), "https://api.openai.com/v1");
        assert_eq!(LlmProfile { url: "http://x/".into(), ..full.clone() }.base_url(), "http://x");
    }
}
