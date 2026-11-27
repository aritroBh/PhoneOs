//! An OpenAI-compatible chat-completions client (feature `http`).
//!
//! Plain `http://` to a local server (Ollama, LM Studio, llama.cpp server) is the intended
//! use; the build has no TLS. Requests are deterministic: temperature 0, no streaming.
//!
//! Structured output: a request that carries the action schema asks for
//! `response_format: {type: "json_schema", ...}`. Not every server takes that (some accept
//! only `json_object`, which some of them constrain with a `schema` field, and some none at
//! all), so when the server answers 4xx/5xx and its error names the response format, the
//! request is sent again once with `json_object` (schema attached), then once without a
//! response format. The mode that worked is kept for later requests and reported by
//! [`Model::response_format`] and in the transcript note.

use std::time::{Duration, Instant};

use jarvis_proto::{ModelOutput, ModelRequest, RequestKind};
use serde_json::{json, Value};

use crate::model::{Model, ModelError};

/// Default per-request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
/// Longest slice of an error body kept in messages.
const MAX_ERROR_BODY: usize = 300;

/// How a request asks for structured output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    /// `{"type": "json_schema", "json_schema": {...}}` (OpenAI style).
    JsonSchema,
    /// `{"type": "json_object", "schema": {...}}` (JSON mode; some servers honour the schema).
    JsonObject,
    /// No `response_format`; the prompt alone asks for JSON.
    Unconstrained,
}

impl ResponseFormat {
    /// The name used in reports.
    pub fn name(self) -> &'static str {
        match self {
            ResponseFormat::JsonSchema => "json_schema",
            ResponseFormat::JsonObject => "json_object",
            ResponseFormat::Unconstrained => "none",
        }
    }

    fn fallback(self) -> Option<ResponseFormat> {
        match self {
            ResponseFormat::JsonSchema => Some(ResponseFormat::JsonObject),
            ResponseFormat::JsonObject => Some(ResponseFormat::Unconstrained),
            ResponseFormat::Unconstrained => None,
        }
    }
}

/// Whether an error body blames the response format.
fn rejects_response_format(body: &str) -> bool {
    let b = body.to_lowercase();
    ["response_format", "json_schema", "json_object"]
        .iter()
        .any(|k| b.contains(k))
}

/// Speaks `POST {base}/chat/completions`.
#[derive(Debug)]
pub struct HttpModel {
    base_url: String,
    model: String,
    api_key: Option<String>,
    agent: ureq::Agent,
    note: Option<String>,
    real_ms: Option<u64>,
    format: ResponseFormat,
    schema_requested: bool,
}

impl HttpModel {
    /// A client for `base_url` (for example `http://localhost:11434/v1`) and a model name.
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
            api_key: None,
            agent: ureq::AgentBuilder::new().timeout(DEFAULT_TIMEOUT).build(),
            note: None,
            real_ms: None,
            format: ResponseFormat::JsonSchema,
            schema_requested: false,
        }
    }

    /// The structured-output mode requests currently use.
    pub fn format(&self) -> ResponseFormat {
        self.format
    }

    /// Sets the whole-request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.agent = ureq::AgentBuilder::new().timeout(timeout).build();
        self
    }

    /// Sends `Authorization: Bearer <key>`.
    pub fn with_api_key(mut self, key: &str) -> Self {
        self.api_key = Some(key.to_owned());
        self
    }

    /// The JSON body for a request, with the current [`ResponseFormat`].
    pub fn request_body(&self, req: &ModelRequest) -> Value {
        self.body_with(req, self.format)
    }

    fn body_with(&self, req: &ModelRequest, format: ResponseFormat) -> Value {
        let mut body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": req.system},
                {"role": "user", "content": req.user},
            ],
            "max_tokens": req.max_tokens,
            "temperature": 0,
            "stream": false,
        });
        if let Some(schema) = req
            .json_schema
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
        {
            let rf = match format {
                ResponseFormat::JsonSchema => Some(json!({
                    "type": "json_schema",
                    "json_schema": {"name": "jarvis_action", "strict": true, "schema": schema},
                })),
                ResponseFormat::JsonObject => {
                    Some(json!({"type": "json_object", "schema": schema}))
                }
                ResponseFormat::Unconstrained => None,
            };
            if let (Value::Object(m), Some(rf)) = (&mut body, rf) {
                m.insert("response_format".to_owned(), rf);
            }
        }
        body
    }

    /// The text of the first choice in a chat-completions response.
    pub fn parse_response(body: &str) -> Result<String, ModelError> {
        let v: Value = serde_json::from_str(body)
            .map_err(|e| ModelError::Http(format!("response is not JSON: {e}")))?;
        v.pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                let shown: String = body.chars().take(MAX_ERROR_BODY).collect();
                ModelError::Http(format!(
                    "response has no choices[0].message.content: {shown}"
                ))
            })
    }
}

impl Model for HttpModel {
    fn generate(&mut self, req: &ModelRequest) -> Result<ModelOutput, ModelError> {
        let mut notes: Vec<String> = Vec::new();
        if req.kind == RequestKind::Vision || req.image.is_some() {
            notes.push(
                "vision request sent as text only (the simulator has no screenshots)".to_owned(),
            );
        }
        let with_schema = req.json_schema.is_some();
        self.schema_requested |= with_schema;
        let url = format!("{}/chat/completions", self.base_url);
        let started = Instant::now();
        let result = loop {
            let mut call = self
                .agent
                .post(&url)
                .set("Content-Type", "application/json");
            if let Some(k) = &self.api_key {
                call = call.set("Authorization", &format!("Bearer {k}"));
            }
            match call.send_string(&self.body_with(req, self.format).to_string()) {
                Ok(resp) => {
                    break resp.into_string().map_err(|e| {
                        ModelError::Http(format!("reading the response failed: {e}"))
                    });
                }
                Err(ureq::Error::Status(code, resp)) => {
                    let body = resp.into_string().unwrap_or_default();
                    let next = self
                        .format
                        .fallback()
                        .filter(|_| with_schema && code >= 400 && rejects_response_format(&body));
                    if let Some(next) = next {
                        notes.push(format!(
                            "the server rejected response_format {} (HTTP {code}); using {} from now on",
                            self.format.name(),
                            next.name()
                        ));
                        self.format = next;
                        continue;
                    }
                    let shown: String = body.chars().take(MAX_ERROR_BODY).collect();
                    break Err(ModelError::Http(format!("HTTP {code} from {url}: {shown}")));
                }
                Err(e) => break Err(ModelError::Http(format!("request to {url} failed: {e}"))),
            }
        };
        self.real_ms = Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        self.note = (!notes.is_empty()).then(|| notes.join("; "));
        Ok(ModelOutput {
            text: Self::parse_response(&result?)?,
        })
    }

    fn name(&self) -> String {
        format!("http {} @ {}", self.model, self.base_url)
    }

    fn take_note(&mut self) -> Option<String> {
        self.note.take()
    }

    fn take_real_ms(&mut self) -> Option<u64> {
        self.real_ms.take()
    }

    fn response_format(&self) -> Option<String> {
        self.schema_requested.then(|| self.format.name().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn body_has_system_and_user_messages_and_schema_when_present() {
        let m = HttpModel::new("http://localhost:11434/v1/", "qwen");
        let req = ModelRequest {
            system: "S".into(),
            user: "U".into(),
            max_tokens: 160,
            json_schema: Some("{\"type\":\"object\"}".into()),
            ..ModelRequest::default()
        };
        let b = m.request_body(&req);
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(b["messages"][1]["content"], "U");
        assert_eq!(b["temperature"], 0);
        assert_eq!(b["max_tokens"], 160);
        assert_eq!(
            b["response_format"]["json_schema"]["schema"]["type"],
            "object"
        );
        let plain = m.request_body(&ModelRequest::default());
        assert!(plain.get("response_format").is_none());
    }

    #[test]
    fn parses_content_and_reports_bad_bodies() {
        let ok =
            r#"{"choices":[{"message":{"role":"assistant","content":"{\"action\":\"done\"}"}}]}"#;
        assert_eq!(
            HttpModel::parse_response(ok).unwrap(),
            "{\"action\":\"done\"}"
        );
        assert!(HttpModel::parse_response("{}").is_err());
        assert!(HttpModel::parse_response("nope").is_err());
    }

    /// Serves one canned HTTP response on a local port and returns the request text.
    fn serve_once(status: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let reply = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let h = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            // Read headers plus Content-Length bytes: one `read` can return before the
            // body arrives, and replying then resets the connection under the client.
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let n = s.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let content_length = text[..head_end]
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse::<usize>().ok())?
                        })
                        .unwrap_or(0);
                    if buf.len() >= head_end + 4 + content_length {
                        break;
                    }
                }
            }
            s.write_all(reply.as_bytes()).unwrap();
            String::from_utf8_lossy(&buf).into_owned()
        });
        (url, h)
    }

    #[test]
    fn round_trip_against_a_local_stub_server() {
        let (url, h) = serve_once(
            "200 OK",
            r#"{"choices":[{"message":{"content":"{\"action\":\"done\",\"summary\":\"ok\"}"}}]}"#,
        );
        let mut m = HttpModel::new(&url, "stub");
        let out = m
            .generate(&ModelRequest {
                user: "hello".into(),
                max_tokens: 8,
                ..ModelRequest::default()
            })
            .unwrap();
        assert!(out.text.contains("done"));
        let sent = h.join().unwrap();
        assert!(sent.starts_with("POST /v1/chat/completions"), "{sent}");
        assert!(sent.contains("\"temperature\":0"));
        assert!(m.take_real_ms().is_some());
    }

    /// Serves canned responses, one connection each, and returns the request texts.
    fn serve_seq(
        replies: Vec<(&'static str, String)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let h = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    let n = s.read(&mut chunk).unwrap();
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf);
                    if let Some(head_end) = text.find("\r\n\r\n") {
                        let len = text[..head_end]
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        if buf.len() >= head_end + 4 + len {
                            break;
                        }
                    }
                }
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                s.write_all(reply.as_bytes()).unwrap();
                seen.push(String::from_utf8_lossy(&buf).into_owned());
            }
            seen
        });
        (url, h)
    }

    /// A server that only knows `text`/`json_object` (the evaluation's llama.cpp Python
    /// server answered `json_schema` with HTTP 500 naming `response_format`): the client
    /// steps down once per mode, keeps what worked and says so.
    #[test]
    fn a_rejected_json_schema_falls_back_to_json_object_then_none() {
        let reject = r#"{"error":{"message":"1 validation error: ('body', 'response_format', 'type'): Input should be 'text' or 'json_object'"}}"#.to_owned();
        let ok = r#"{"choices":[{"message":{"content":"{\"action\":\"done\"}"}}]}"#.to_owned();
        let req = ModelRequest {
            user: "u".into(),
            json_schema: Some(jarvis_agent::action_json_schema()),
            ..ModelRequest::default()
        };
        let (url, h) = serve_seq(vec![
            ("500 Internal Server Error", reject.clone()),
            ("200 OK", ok.clone()),
            ("200 OK", ok.clone()),
        ]);
        let mut m = HttpModel::new(&url, "stub");
        assert_eq!(m.response_format(), None);
        assert!(m.generate(&req).unwrap().text.contains("done"));
        assert_eq!(m.format(), ResponseFormat::JsonObject);
        assert_eq!(m.response_format().as_deref(), Some("json_object"));
        let note = m.take_note().unwrap();
        assert!(
            note.contains("rejected response_format json_schema"),
            "{note}"
        );
        // The next request goes straight to json_object.
        assert!(m.generate(&req).is_ok());
        assert_eq!(m.take_note(), None);
        let sent = h.join().unwrap();
        assert_eq!(sent.len(), 3);
        assert!(sent[0].contains("\"type\":\"json_schema\""), "{}", sent[0]);
        assert!(sent[1].contains("\"type\":\"json_object\""), "{}", sent[1]);
        assert!(sent[2].contains("\"type\":\"json_object\""));

        // Rejected twice: no response_format at all. An unrelated error is not retried.
        let (url, h) = serve_seq(vec![
            ("400 Bad Request", reject.clone()),
            (
                "400 Bad Request",
                "{\"error\":\"json_object unsupported\"}".to_owned(),
            ),
            ("200 OK", ok),
            (
                "500 Internal Server Error",
                "{\"error\":\"out of memory\"}".to_owned(),
            ),
        ]);
        let mut m = HttpModel::new(&url, "stub");
        assert!(m.generate(&req).is_ok());
        assert_eq!(m.response_format().as_deref(), Some("none"));
        let e = m.generate(&req).unwrap_err();
        assert!(e.to_string().contains("out of memory"), "{e}");
        let sent = h.join().unwrap();
        assert!(!sent[2].contains("response_format"), "{}", sent[2]);
        assert!(!sent[3].contains("response_format"));
    }

    #[test]
    fn http_errors_and_dead_servers_are_model_errors() {
        let (url, h) = serve_once("500 Internal Server Error", "{\"error\":\"boom\"}");
        let mut m = HttpModel::new(&url, "stub");
        let e = m.generate(&ModelRequest::default()).unwrap_err();
        h.join().unwrap();
        assert!(e.to_string().contains("HTTP 500"), "{e}");
        let mut dead = HttpModel::new("http://127.0.0.1:1/v1", "stub")
            .with_timeout(Duration::from_millis(500));
        assert!(matches!(
            dead.generate(&ModelRequest::default()),
            Err(ModelError::Http(_))
        ));
    }
}
