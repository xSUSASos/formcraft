//! LLM backends behind one trait: Ollama (default, `127.0.0.1:11434`), any OpenAI-compatible
//! local server (llama.cpp `llama-server`, LM Studio), and a Mock for tests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::http::{self, Url};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(c: impl Into<String>) -> Self {
        Message { role: Role::System, content: c.into() }
    }
    pub fn user(c: impl Into<String>) -> Self {
        Message { role: Role::User, content: c.into() }
    }
    pub fn assistant(c: impl Into<String>) -> Self {
        Message { role: Role::Assistant, content: c.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    /// JSON schema the reply must follow (structured output), when the backend supports it.
    pub schema: Option<Value>,
    pub temperature: f64,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Reply {
    pub text: String,
    pub model: String,
    pub prompt_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

pub trait LlmBackend: Send + Sync {
    /// `ollama`, `openai`, `mock`.
    fn kind(&self) -> &'static str;
    fn model(&self) -> String;
    fn chat(&self, req: &ChatRequest) -> Result<Reply, String>;
    /// Server state for the Assistant panel: reachable, models, loaded model and VRAM.
    fn status(&self) -> Result<Value, String> {
        Ok(json!({"backend": self.kind(), "model": self.model()}))
    }
}

/// Backend settings (environment now, the settings dialog in Phase 7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiConfig {
    /// `ollama` or `openai`.
    pub backend: String,
    pub url: String,
    pub model: String,
    pub temperature: f64,
    pub timeout_secs: u64,
    /// Context window asked from Ollama.
    pub num_ctx: u32,
    /// Constrain the reply to the plan's JSON schema (grammar decoding). Off for "thinking"
    /// models (qwen3), which degenerate under the grammar; their reply is parsed after
    /// `</think>` instead.
    pub structured: bool,
    /// Output token limit per reply (thinking models need more).
    pub max_tokens: u32,
}

pub const DEFAULT_MODEL: &str = "qwen2.5-coder:7b";

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig {
            backend: "ollama".into(),
            url: "http://127.0.0.1:11434".into(),
            model: DEFAULT_MODEL.into(),
            temperature: 0.1,
            timeout_secs: 600,
            num_ctx: 8192,
            structured: true,
            max_tokens: 1200,
        }
    }
}

impl AiConfig {
    /// Defaults overridden by `FORMCRAFT_AI_BACKEND`, `FORMCRAFT_AI_URL`, `FORMCRAFT_AI_MODEL`,
    /// `FORMCRAFT_AI_TIMEOUT` (seconds).
    pub fn from_env() -> Self {
        let mut c = AiConfig::default();
        let var = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        if let Some(b) = var("FORMCRAFT_AI_BACKEND") {
            if b == "openai" {
                c.url = "http://127.0.0.1:8080".into();
            }
            c.backend = b;
        }
        if let Some(u) = var("FORMCRAFT_AI_URL") {
            c.url = u;
        }
        if let Some(m) = var("FORMCRAFT_AI_MODEL") {
            c.model = m;
        }
        // thinking models: free-form reasoning, then the JSON plan
        if c.model.starts_with("qwen3") || c.model.contains("deepseek-r1") {
            c.structured = false;
            c.max_tokens = 4096;
        }
        if let Some(v) = var("FORMCRAFT_AI_STRUCTURED") {
            c.structured = !matches!(v.as_str(), "0" | "false" | "no" | "off");
        }
        if let Some(n) =
            var("FORMCRAFT_AI_MAX_TOKENS").and_then(|n| n.parse::<u32>().ok()).filter(|n| (64..=32_768).contains(n))
        {
            c.max_tokens = n;
        }
        if let Some(t) = var("FORMCRAFT_AI_TIMEOUT").and_then(|t| t.parse::<u64>().ok()).filter(|t| *t > 0) {
            c.timeout_secs = t;
        }
        c
    }

    pub fn build(&self) -> Result<Arc<dyn LlmBackend>, String> {
        let url = Url::parse(&self.url)?;
        let timeout = Duration::from_secs(self.timeout_secs.clamp(5, 3600));
        match self.backend.as_str() {
            "ollama" => Ok(Arc::new(Ollama { url, model: self.model.clone(), timeout, num_ctx: self.num_ctx })),
            "openai" => Ok(Arc::new(OpenAi { url, model: self.model.clone(), timeout })),
            other => Err(format!("unknown AI backend `{other}` (ollama, openai)")),
        }
    }
}

pub struct Ollama {
    pub url: Url,
    pub model: String,
    pub timeout: Duration,
    pub num_ctx: u32,
}

impl LlmBackend for Ollama {
    fn kind(&self) -> &'static str {
        "ollama"
    }
    fn model(&self) -> String {
        self.model.clone()
    }
    fn chat(&self, req: &ChatRequest) -> Result<Reply, String> {
        let mut body = json!({
            "model": self.model,
            "messages": req.messages,
            "stream": false,
            "options": {"temperature": req.temperature, "num_predict": req.max_tokens, "num_ctx": self.num_ctx},
        });
        body["format"] = req.schema.clone().unwrap_or(json!("json"));
        let v = http::request(&self.url, "POST", "/api/chat", Some(&body), self.timeout)?;
        let text = v["message"]["content"].as_str().ok_or("Ollama reply without message.content")?.to_string();
        Ok(Reply {
            text,
            model: v["model"].as_str().unwrap_or(&self.model).to_string(),
            prompt_tokens: v["prompt_eval_count"].as_u64(),
            output_tokens: v["eval_count"].as_u64(),
        })
    }
    fn status(&self) -> Result<Value, String> {
        let short = Duration::from_secs(5);
        let tags = http::request(&self.url, "GET", "/api/tags", None, short)?;
        let models: Vec<Value> = tags["models"].as_array().into_iter().flatten().map(|m| m["name"].clone()).collect();
        let ps = http::request(&self.url, "GET", "/api/ps", None, short).unwrap_or(Value::Null);
        let loaded: Vec<Value> = ps["models"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| json!({"name": m["name"], "size": m["size"], "vram": m["size_vram"]}))
            .collect();
        Ok(json!({
            "backend": "ollama", "url": format!("http://{}:{}", self.url.host, self.url.port),
            "model": self.model, "available": models.iter().any(|m| m.as_str() == Some(self.model.as_str())),
            "models": models, "loaded": loaded,
        }))
    }
}

pub struct OpenAi {
    pub url: Url,
    pub model: String,
    pub timeout: Duration,
}

impl OpenAi {
    fn endpoint(&self, tail: &str) -> String {
        if self.url.path.ends_with("/v1") { tail.to_string() } else { format!("/v1{tail}") }
    }
}

impl LlmBackend for OpenAi {
    fn kind(&self) -> &'static str {
        "openai"
    }
    fn model(&self) -> String {
        self.model.clone()
    }
    fn chat(&self, req: &ChatRequest) -> Result<Reply, String> {
        let mut body = json!({
            "model": self.model, "messages": req.messages,
            "temperature": req.temperature, "max_tokens": req.max_tokens, "stream": false,
        });
        body["response_format"] = match &req.schema {
            Some(s) => json!({"type": "json_schema", "json_schema": {"name": "plan", "schema": s}}),
            None => json!({"type": "json_object"}),
        };
        let v = http::request(&self.url, "POST", &self.endpoint("/chat/completions"), Some(&body), self.timeout)?;
        let text = v["choices"][0]["message"]["content"].as_str().ok_or("reply without choices[0].message")?;
        Ok(Reply {
            text: text.to_string(),
            model: v["model"].as_str().unwrap_or(&self.model).to_string(),
            prompt_tokens: v["usage"]["prompt_tokens"].as_u64(),
            output_tokens: v["usage"]["completion_tokens"].as_u64(),
        })
    }
    fn status(&self) -> Result<Value, String> {
        let v = http::request(&self.url, "GET", &self.endpoint("/models"), None, Duration::from_secs(5))?;
        let models: Vec<Value> = v["data"].as_array().into_iter().flatten().map(|m| m["id"].clone()).collect();
        Ok(json!({"backend": "openai", "model": self.model, "models": models}))
    }
}

type Responder = Box<dyn Fn(&ChatRequest) -> Result<String, String> + Send + Sync>;

/// Test backend: canned replies in order, or a function of the request. Records requests.
pub struct Mock {
    replies: Mutex<VecDeque<String>>,
    responder: Option<Responder>,
    pub requests: Mutex<Vec<ChatRequest>>,
}

impl Mock {
    pub fn replies<S: Into<String>>(r: impl IntoIterator<Item = S>) -> Self {
        Mock {
            replies: Mutex::new(r.into_iter().map(Into::into).collect()),
            responder: None,
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn with(f: impl Fn(&ChatRequest) -> Result<String, String> + Send + Sync + 'static) -> Self {
        Mock { replies: Mutex::new(VecDeque::new()), responder: Some(Box::new(f)), requests: Mutex::new(Vec::new()) }
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().map(|r| r.len()).unwrap_or(0)
    }
}

impl LlmBackend for Mock {
    fn kind(&self) -> &'static str {
        "mock"
    }
    fn model(&self) -> String {
        "mock".into()
    }
    fn chat(&self, req: &ChatRequest) -> Result<Reply, String> {
        if let Ok(mut r) = self.requests.lock() {
            r.push(req.clone());
        }
        let text = match &self.responder {
            Some(f) => f(req)?,
            None => self
                .replies
                .lock()
                .map_err(|_| "mock poisoned".to_string())?
                .pop_front()
                .ok_or("mock: no more replies")?,
        };
        Ok(Reply { text, model: "mock".into(), prompt_tokens: None, output_tokens: None })
    }
}
