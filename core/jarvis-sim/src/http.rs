//! An OpenAI-compatible chat-completions client (feature `http`).
//!
//! Plain `http://` to a local server (Ollama, LM Studio, llama.cpp server) is the intended
//! use; the build has no TLS. Requests are deterministic: temperature 0, no streaming.

use std::time::{Duration, Instant};

use jarvis_proto::{ModelOutput, ModelRequest, RequestKind};
use serde_json::{json, Value};

use crate::model::{Model, ModelError};

/// Default per-request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
/// Longest slice of an error body kept in messages.
const MAX_ERROR_BODY: usize = 300;

/// Speaks `POST {base}/chat/completions`.
#[derive(Debug)]
pub struct HttpModel {
    base_url: String,
    model: String,
    api_key: Option<String>,
    agent: ureq::Agent,
    note: Option<String>,
    real_ms: Option<u64>,
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
        }
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

    /// The JSON body for a request.
    pub fn request_body(&self, req: &ModelRequest) -> Value {
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
            if let Value::Object(m) = &mut body {
                m.insert(
                    "response_format".to_owned(),
                    json!({
                        "type": "json_schema",
                        "json_schema": {"name": "jarvis_action", "strict": true, "schema": schema},
                    }),
                );
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
        self.note = (req.kind == RequestKind::Vision || req.image.is_some()).then(|| {
            "vision request sent as text only (the simulator has no screenshots)".to_owned()
        });
        let url = format!("{}/chat/completions", self.base_url);
        let mut call = self
            .agent
            .post(&url)
            .set("Content-Type", "application/json");
        if let Some(k) = &self.api_key {
            call = call.set("Authorization", &format!("Bearer {k}"));
        }
        let started = Instant::now();
        let reply = call.send_string(&self.request_body(req).to_string());
        self.real_ms = Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        match reply {
            Ok(resp) => {
                let body = resp
                    .into_string()
                    .map_err(|e| ModelError::Http(format!("reading the response failed: {e}")))?;
                Ok(ModelOutput {
                    text: Self::parse_response(&body)?,
                })
            }
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                let shown: String = body.chars().take(MAX_ERROR_BODY).collect();
                Err(ModelError::Http(format!("HTTP {code} from {url}: {shown}")))
            }
            Err(e) => Err(ModelError::Http(format!("request to {url} failed: {e}"))),
        }
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
            let mut buf = vec![0u8; 65536];
            let n = s.read(&mut buf).unwrap();
            s.write_all(reply.as_bytes()).unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
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
