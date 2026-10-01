//! The three wire shapes, as pure request builders and reply readers:
//! nothing here sends anything, so every shape is tested on canned text.
//!
//! - **Anthropic**: `POST {url}/v1/messages`, the key in `x-api-key`, the
//!   system prompt as one block marked for caching, no `thinking` field
//!   (adaptive thinking is the current models' default) and the profile's
//!   `effort` as `output_config.effort` when it has one.
//! - **OpenAI-compatible**: `POST {url}/chat/completions`, `Authorization:
//!   Bearer` when there is a key, `temperature: 0`, and the profile's
//!   `effort` as `reasoning_effort` when it has one — only then, because a
//!   model that does not reason refuses the field.
//! - **Ollama**: `POST {url}/api/chat`, `stream: false`, and `think: false`
//!   when the effort is `low`, so a thinking model answers at once.
//!
//! A reply carries what it cost (`Usage`), so the agent can sum a game.

use std::fmt;

use serde_json::{json, Value};

use super::profile::{LlmProfile, Protocol};
use super::secrets::ApiKey;

/// The ceiling on an answer that is a number and a few words. On
/// Anthropic it bounds thinking too, which is why it is not smaller.
pub const MAX_TOKENS: u32 = 4096;

/// One request, ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Value,
}

/// What a reply cost, as the provider counts it. Zero where a provider
/// says nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    /// The part of `input` served from the provider's cache, where it says.
    pub cached_input: u64,
    pub output: u64,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.input + self.output
    }

    pub fn add(&mut self, other: Usage) {
        self.input += other.input;
        self.cached_input += other.cached_input;
        self.output += other.output;
    }
}

/// A reply read: the model's text and what it cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub text: String,
    pub usage: Usage,
}

/// Why no answer came back, in one line for the notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    NoKey,
    Http { status: u16, message: String },
    Refusal,
    Empty,
    Malformed(String),
    Transport(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::NoKey => write!(f, "no API key is set for this opponent"),
            ProviderError::Http { status, message } => write!(f, "HTTP {status}: {message}"),
            ProviderError::Refusal => write!(f, "the model declined to answer"),
            ProviderError::Empty => write!(f, "the model sent an empty answer"),
            ProviderError::Malformed(what) => write!(f, "the reply could not be read: {what}"),
            ProviderError::Transport(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// The request for one decision: `system` once per game, `user` the
/// situation.
pub fn build(profile: &LlmProfile, key: Option<&ApiKey>, system: &str, user: &str) -> Result<Request, ProviderError> {
    let key = key.filter(|key| !key.is_empty());
    if key.is_none() && profile.protocol.needs_key() {
        return Err(ProviderError::NoKey);
    }
    let base = profile.base_url();
    let mut headers: Vec<(&'static str, String)> = vec![("content-type", "application/json".to_string())];
    let request = match profile.protocol {
        Protocol::Anthropic => {
            headers.push(("x-api-key", key.map(ApiKey::expose).unwrap_or_default().to_string()));
            headers.push(("anthropic-version", "2023-06-01".to_string()));
            let mut body = json!({
                "model": profile.model,
                "max_tokens": MAX_TOKENS,
                "system": [{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}],
                "messages": [{"role": "user", "content": user}],
            });
            if let Some(effort) = &profile.effort {
                body["output_config"] = json!({"effort": effort});
            }
            Request { url: format!("{base}/v1/messages"), headers, body }
        }
        Protocol::OpenAi => {
            if let Some(key) = key {
                headers.push(("authorization", format!("Bearer {}", key.expose())));
            }
            let mut body = json!({
                "model": profile.model,
                "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
                "temperature": 0,
            });
            if let Some(effort) = &profile.effort {
                body["reasoning_effort"] = json!(effort);
            }
            Request { url: format!("{base}/chat/completions"), headers, body }
        }
        Protocol::Ollama => {
            if let Some(key) = key {
                headers.push(("authorization", format!("Bearer {}", key.expose())));
            }
            let mut body = json!({
                "model": profile.model,
                "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
                "stream": false,
                "options": {"temperature": 0},
            });
            if profile.effort.as_deref() == Some("low") {
                body["think"] = json!(false);
            }
            Request { url: format!("{base}/api/chat"), headers, body }
        }
    };
    Ok(request)
}

/// The text out of a reply, by the protocol's shape; an error body read
/// for its message.
pub fn read(protocol: Protocol, status: u16, body: &str) -> Result<Reply, ProviderError> {
    let value: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) if !(200..300).contains(&status) => return Err(ProviderError::Http { status, message: snippet(body) }),
        Err(error) => return Err(ProviderError::Malformed(error.to_string())),
    };
    if !(200..300).contains(&status) {
        let message = match protocol {
            Protocol::Ollama => value.get("error").and_then(Value::as_str).map(str::to_string),
            Protocol::Anthropic | Protocol::OpenAi => value.pointer("/error/message").and_then(Value::as_str).map(str::to_string),
        };
        return Err(ProviderError::Http { status, message: message.unwrap_or_else(|| snippet(body)) });
    }
    let (text, usage) = match protocol {
        Protocol::Anthropic => {
            if value.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
                return Err(ProviderError::Refusal);
            }
            let text = value
                .get("content")
                .and_then(Value::as_array)
                .map(|blocks| blocks.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("text")).filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"))
                .ok_or_else(|| ProviderError::Malformed("no content".to_string()))?;
            let usage = Usage {
                input: number(&value, "/usage/input_tokens") + number(&value, "/usage/cache_read_input_tokens") + number(&value, "/usage/cache_creation_input_tokens"),
                cached_input: number(&value, "/usage/cache_read_input_tokens"),
                output: number(&value, "/usage/output_tokens"),
            };
            (text, usage)
        }
        Protocol::OpenAi => {
            let text = value.pointer("/choices/0/message/content").and_then(Value::as_str).map(str::to_string).ok_or_else(|| ProviderError::Malformed("no choices".to_string()))?;
            if value.pointer("/choices/0/finish_reason").and_then(Value::as_str) == Some("content_filter") {
                return Err(ProviderError::Refusal);
            }
            let usage = Usage {
                input: number(&value, "/usage/prompt_tokens"),
                cached_input: number(&value, "/usage/prompt_tokens_details/cached_tokens"),
                output: number(&value, "/usage/completion_tokens"),
            };
            (text, usage)
        }
        Protocol::Ollama => {
            let text = value.pointer("/message/content").and_then(Value::as_str).map(str::to_string).ok_or_else(|| ProviderError::Malformed("no message".to_string()))?;
            let usage = Usage { input: number(&value, "/prompt_eval_count"), cached_input: 0, output: number(&value, "/eval_count") };
            (text, usage)
        }
    };
    if text.trim().is_empty() {
        return Err(ProviderError::Empty);
    }
    Ok(Reply { text, usage })
}

fn number(value: &Value, pointer: &str) -> u64 {
    value.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

/// The first line of a body that is not the JSON a provider promised,
/// trimmed to something a notice can hold.
fn snippet(body: &str) -> String {
    let line = body.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
    let mut out: String = line.chars().take(120).collect();
    if out.is_empty() {
        out = "no body".to_string();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::profile::Preset;

    fn key() -> ApiKey {
        ApiKey::new("not-a-real-key")
    }

    #[test]
    fn an_anthropic_request_carries_the_key_the_version_and_a_cached_system_block() {
        let profile = Preset::Anthropic.profile("claude");
        let request = build(&profile, Some(&key()), "SYS", "USER").unwrap();
        assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
        assert!(request.headers.contains(&("x-api-key", "not-a-real-key".to_string())));
        assert!(request.headers.contains(&("anthropic-version", "2023-06-01".to_string())));
        assert_eq!(request.body["model"], "claude-opus-5-5");
        assert_eq!(request.body["max_tokens"], MAX_TOKENS);
        assert_eq!(request.body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(request.body["system"][0]["text"], "SYS");
        assert_eq!(request.body["messages"][0]["content"], "USER");
        assert_eq!(request.body["output_config"]["effort"], "low");
        assert!(request.body.get("thinking").is_none(), "adaptive thinking is the default; nothing is sent");
        let plain = LlmProfile { effort: None, ..profile };
        assert!(build(&plain, Some(&key()), "S", "U").unwrap().body.get("output_config").is_none());
    }

    #[test]
    fn an_openai_request_bears_the_key_and_sends_effort_only_when_set() {
        let profile = Preset::Gemini.profile("gem");
        let request = build(&profile, Some(&key()), "SYS", "USER").unwrap();
        assert_eq!(request.url, "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions");
        assert!(request.headers.contains(&("authorization", "Bearer not-a-real-key".to_string())));
        assert_eq!(request.body["messages"][0]["role"], "system");
        assert_eq!(request.body["messages"][1]["content"], "USER");
        assert_eq!(request.body["temperature"], 0);
        assert!(request.body.get("reasoning_effort").is_none());
        let reasoning = LlmProfile { effort: Some("low".to_string()), ..profile };
        assert_eq!(build(&reasoning, Some(&key()), "S", "U").unwrap().body["reasoning_effort"], "low");
        // A local server in the OpenAI shape takes no key and is sent none.
        let local = LlmProfile { url: "http://localhost:1234/v1/".to_string(), ..Preset::Custom.profile("lm") };
        let request = build(&local, None, "S", "U").unwrap();
        assert_eq!(request.url, "http://localhost:1234/v1/chat/completions");
        assert!(!request.headers.iter().any(|(name, _)| *name == "authorization"));
    }

    #[test]
    fn an_ollama_request_needs_no_key_and_turns_thinking_off_at_low_effort() {
        let profile = Preset::Ollama.profile("local");
        let request = build(&profile, None, "SYS", "USER").unwrap();
        assert_eq!(request.url, "http://localhost:11434/api/chat");
        assert!(!request.headers.iter().any(|(name, _)| *name == "x-api-key" || *name == "authorization"));
        assert_eq!(request.body["stream"], false);
        assert_eq!(request.body["options"]["temperature"], 0);
        assert!(request.body.get("think").is_none());
        let quick = LlmProfile { effort: Some("low".to_string()), ..profile };
        assert_eq!(build(&quick, None, "S", "U").unwrap().body["think"], false);
    }

    #[test]
    fn a_missing_key_is_refused_before_any_request() {
        assert_eq!(build(&Preset::Anthropic.profile("a"), None, "S", "U").unwrap_err(), ProviderError::NoKey);
        assert_eq!(build(&Preset::Anthropic.profile("a"), Some(&ApiKey::new("  ")), "S", "U").unwrap_err(), ProviderError::NoKey);
        // The OpenAI shape is a local server's too, so it goes without a key
        // and a hosted provider's 401 says what is missing.
        let keyless = build(&Preset::OpenAi.profile("o"), None, "S", "U").unwrap();
        assert!(!keyless.headers.iter().any(|(name, _)| *name == "authorization"));
        assert!(build(&Preset::Ollama.profile("l"), None, "S", "U").is_ok());
    }

    #[test]
    fn each_reply_shape_is_read_with_its_usage() {
        let anthropic = r#"{"content":[{"type":"thinking","thinking":""},{"type":"text","text":"{\"action\": 2}"}],"stop_reason":"end_turn","usage":{"input_tokens":100,"cache_read_input_tokens":3000,"output_tokens":12}}"#;
        let reply = read(Protocol::Anthropic, 200, anthropic).unwrap();
        assert_eq!(reply.text, "{\"action\": 2}");
        assert_eq!(reply.usage, Usage { input: 3100, cached_input: 3000, output: 12 });
        let openai = r#"{"choices":[{"message":{"role":"assistant","content":"3"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2000,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":1500}}}"#;
        let reply = read(Protocol::OpenAi, 200, openai).unwrap();
        assert_eq!(reply.text, "3");
        assert_eq!(reply.usage, Usage { input: 2000, cached_input: 1500, output: 5 });
        let ollama = r#"{"message":{"role":"assistant","content":"1"},"done":true,"prompt_eval_count":900,"eval_count":4}"#;
        let reply = read(Protocol::Ollama, 200, ollama).unwrap();
        assert_eq!(reply.text, "1");
        assert_eq!(reply.usage, Usage { input: 900, cached_input: 0, output: 4 });
        let mut total = Usage::default();
        total.add(reply.usage);
        total.add(reply.usage);
        assert_eq!(total.total(), 1808);
    }

    #[test]
    fn a_refusal_an_empty_answer_and_an_error_body_are_named() {
        assert_eq!(read(Protocol::Anthropic, 200, r#"{"content":[],"stop_reason":"refusal"}"#).unwrap_err(), ProviderError::Refusal);
        assert_eq!(read(Protocol::Anthropic, 200, r#"{"content":[],"stop_reason":"max_tokens"}"#).unwrap_err(), ProviderError::Empty);
        assert_eq!(read(Protocol::OpenAi, 200, r#"{"choices":[{"message":{"content":"  "}}]}"#).unwrap_err(), ProviderError::Empty);
        assert_eq!(
            read(Protocol::Anthropic, 401, r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#).unwrap_err(),
            ProviderError::Http { status: 401, message: "invalid x-api-key".to_string() }
        );
        assert_eq!(
            read(Protocol::OpenAi, 429, r#"{"error":{"message":"Rate limit reached","type":"rate_limit_error"}}"#).unwrap_err(),
            ProviderError::Http { status: 429, message: "Rate limit reached".to_string() }
        );
        assert_eq!(read(Protocol::Ollama, 404, r#"{"error":"model 'x' not found"}"#).unwrap_err(), ProviderError::Http { status: 404, message: "model 'x' not found".to_string() });
        assert_eq!(read(Protocol::Ollama, 502, "<html>Bad gateway</html>").unwrap_err(), ProviderError::Http { status: 502, message: "<html>Bad gateway</html>".to_string() });
        assert!(matches!(read(Protocol::OpenAi, 200, "not json").unwrap_err(), ProviderError::Malformed(_)));
        assert_eq!(ProviderError::Http { status: 401, message: "bad key".to_string() }.to_string(), "HTTP 401: bad key");
    }
}
