//! Sending a request: the one place in the module that does I/O, behind
//! a trait so every test answers from a script.
//!
//! `BotAgent::select_action` is synchronous and `Session::step` calls it
//! inline, so the request blocks the thread that pumps the session. On
//! the desktop that is the match's own thread (`play::MatchHandle`), a
//! plain thread where `Handle::block_on` is right; the terminal pumps on
//! its `#[tokio::main]` thread, where `block_on` would panic and
//! `block_in_place` is the way. `HttpTransport::complete` asks
//! `Handle::try_current` which one it is on.

use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

use super::protocol::Request;

/// What came back: the status and the body, for `protocol::read` to
/// interpret. The transport knows no provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

/// Why nothing came back at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Timeout,
    Connect(String),
    Other(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Timeout => write!(f, "the request timed out"),
            TransportError::Connect(what) => write!(f, "could not connect: {what}"),
            TransportError::Other(what) => write!(f, "{what}"),
        }
    }
}

pub trait Transport: Send {
    fn complete(&mut self, request: &Request) -> Result<Response, TransportError>;
}

/// The real one: reqwest on a tokio runtime the caller hands over.
pub struct HttpTransport {
    handle: tokio::runtime::Handle,
    client: reqwest::Client,
}

impl HttpTransport {
    /// `timeout` bounds the whole request; the profile's
    /// (`LlmProfile::timeout`).
    pub fn new(handle: tokio::runtime::Handle, timeout: Duration) -> Self {
        Self { handle, client: client(timeout) }
    }
}

impl Transport for HttpTransport {
    fn complete(&mut self, request: &Request) -> Result<Response, TransportError> {
        let send = send(&self.client, request);
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| self.handle.block_on(send))
        } else {
            self.handle.block_on(send)
        }
    }
}

fn client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("netrunner-rs/", env!("CARGO_PKG_VERSION"), " (+https://github.com/lukejohannsen/netrunner-rs)"))
        .timeout(timeout)
        .build()
        .expect("a client with a user agent and a timeout builds")
}

async fn send(client: &reqwest::Client, request: &Request) -> Result<Response, TransportError> {
    let mut builder = client.post(&request.url);
    for (name, value) in &request.headers {
        builder = builder.header(*name, value);
    }
    let outcome = async {
        let response = builder.json(&request.body).send().await?;
        let status = response.status().as_u16();
        let body = response.text().await?;
        Ok::<Response, reqwest::Error>(Response { status, body })
    }
    .await;
    outcome.map_err(|error| {
        if error.is_timeout() {
            TransportError::Timeout
        } else if error.is_connect() {
            TransportError::Connect(error.to_string())
        } else {
            TransportError::Other(error.to_string())
        }
    })
}

/// One request on whatever runtime awaits it — the desktop's Test
/// button, on `core::TokioRuntime`. A client of its own per call, which
/// a button pressed now and then can afford.
pub async fn send_once(request: &Request, timeout: Duration) -> Result<Response, TransportError> {
    send(&client(timeout), request).await
}

/// Answers from a script, in order, and keeps every request it was
/// handed so a test can read what the model was asked.
#[derive(Debug, Default, Clone)]
pub struct ScriptedTransport {
    pub replies: VecDeque<Result<Response, TransportError>>,
    pub seen: Vec<Request>,
    /// Said once the script runs out, every time.
    pub exhausted: Option<Result<Response, TransportError>>,
}

impl ScriptedTransport {
    /// Each text as an OpenAI-shaped 200, then the connection error.
    pub fn answering(texts: &[&str]) -> Self {
        Self { replies: texts.iter().map(|text| Ok(openai_reply(text))).collect(), seen: Vec::new(), exhausted: None }
    }

    /// The same text for good.
    pub fn repeating(text: &str) -> Self {
        Self { replies: VecDeque::new(), seen: Vec::new(), exhausted: Some(Ok(openai_reply(text))) }
    }

    pub fn failing(error: TransportError) -> Self {
        Self { replies: VecDeque::new(), seen: Vec::new(), exhausted: Some(Err(error)) }
    }
}

/// A chat-completions body that says `text`, with a token count a test
/// can sum.
pub fn openai_reply(text: &str) -> Response {
    let body = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 10, "prompt_tokens_details": {"cached_tokens": 800}},
    });
    Response { status: 200, body: body.to_string() }
}

impl Transport for ScriptedTransport {
    fn complete(&mut self, request: &Request) -> Result<Response, TransportError> {
        self.seen.push(request.clone());
        match self.replies.pop_front() {
            Some(reply) => reply,
            None => self.exhausted.clone().unwrap_or_else(|| Err(TransportError::Connect("the script ran out".to_string()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn request() -> Request {
        Request { url: String::new(), headers: vec![("x-test", "1".to_string())], body: serde_json::json!({"model": "m"}) }
    }

    #[test]
    fn a_scripted_transport_answers_in_order_then_says_it_is_exhausted() {
        let mut scripted = ScriptedTransport::answering(&["1", "2"]);
        assert_eq!(scripted.complete(&request()).unwrap(), openai_reply("1"));
        assert_eq!(scripted.complete(&request()).unwrap(), openai_reply("2"));
        assert!(matches!(scripted.complete(&request()), Err(TransportError::Connect(_))));
        assert_eq!(scripted.seen.len(), 3);
        let mut forever = ScriptedTransport::repeating("3");
        for _ in 0..3 {
            assert_eq!(forever.complete(&request()).unwrap(), openai_reply("3"));
        }
        assert_eq!(ScriptedTransport::failing(TransportError::Timeout).complete(&request()), Err(TransportError::Timeout));
    }

    /// One canned HTTP/1.1 answer per connection, on a loopback port.
    fn serve_once(status_line: &'static str, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1/messages", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = vec![0u8; 65536];
            let mut read = 0;
            let request = loop {
                let n = stream.read(&mut buffer[read..]).unwrap();
                read += n;
                let text = String::from_utf8_lossy(&buffer[..read]).into_owned();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end].lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
                    if read >= end + 4 + length {
                        break text;
                    }
                }
                if n == 0 {
                    break text;
                }
            };
            write!(stream, "{status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).unwrap();
            stream.flush().unwrap();
            request
        });
        (url, thread)
    }

    #[test]
    fn the_http_transport_works_from_a_plain_thread_and_from_a_runtime_thread() {
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(2).build().unwrap();
        // A plain thread, as the desktop's match thread is.
        let (url, server) = serve_once("HTTP/1.1 200 OK", r#"{"ok":true}"#);
        let handle = runtime.handle().clone();
        let answer = std::thread::spawn(move || {
            let mut transport = HttpTransport::new(handle, Duration::from_secs(10));
            transport.complete(&Request { url, ..request() })
        })
        .join()
        .unwrap()
        .unwrap();
        assert_eq!(answer, Response { status: 200, body: r#"{"ok":true}"#.to_string() });
        let seen = server.join().unwrap();
        assert!(seen.starts_with("POST /v1/messages HTTP/1.1"), "{seen}");
        assert!(seen.to_ascii_lowercase().contains("x-test: 1"), "{seen}");
        assert!(seen.ends_with(r#"{"model":"m"}"#), "{seen}");
        // A runtime thread, as the terminal's main thread is.
        let (url, server) = serve_once("HTTP/1.1 503 Service Unavailable", "down");
        let handle = runtime.handle().clone();
        let answer = runtime.block_on(async move {
            tokio::task::block_in_place(|| {
                let mut transport = HttpTransport::new(handle, Duration::from_secs(10));
                transport.complete(&Request { url, ..request() })
            })
        });
        assert_eq!(answer.unwrap(), Response { status: 503, body: "down".to_string() });
        server.join().unwrap();
        // Nobody listening is a connection error, not a hang.
        let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", dead.local_addr().unwrap());
        drop(dead);
        let mut transport = HttpTransport::new(runtime.handle().clone(), Duration::from_secs(10));
        assert!(matches!(transport.complete(&Request { url, ..request() }), Err(TransportError::Connect(_))));
    }
}
