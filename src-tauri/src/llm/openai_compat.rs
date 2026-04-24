//! OpenAI-compatible chat client. Targets /v1/chat/completions.
//! Works against Ollama's OpenAI shim, LM Studio, OpenAI itself,
//! Anthropic via a compatible proxy, etc.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{LlmConfig, LlmError, TestResult};

pub struct OpenAiCompatClient {
    client: reqwest::Client,
    config: LlmConfig,
}

impl OpenAiCompatClient {
    pub fn new(config: LlmConfig) -> Result<Self, LlmError> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| LlmError::Other(format!("build http client: {e}")))?;
        Ok(Self { client, config })
    }

    pub async fn test_connection(&self) -> Result<TestResult, LlmError> {
        let start = Instant::now();
        let body = json!({
            "model": self.config.model,
            "messages": [{ "role": "user", "content": "hi" }],
            "max_tokens": 5,
            "stream": false,
        });
        let url = format!("{}/chat/completions", trim_trailing_slash(&self.config.base_url));

        let mut req = self.client.post(&url).json(&body);
        if let Some(key) = &self.config.api_key {
            req = req.bearer_auth(key);
        }

        let res = req.send().await.map_err(classify_send_error)?;

        let status = res.status();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(LlmError::AuthFailed);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimited);
        }
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(LlmError::Other(format!(
                "HTTP {} — {}",
                status.as_u16(),
                body
            )));
        }

        let parsed: ChatCompletionResponse = res
            .json()
            .await
            .map_err(|e| LlmError::ParseError(format!("decode response: {e}")))?;
        let latency_ms = start.elapsed().as_millis() as i64;

        Ok(TestResult {
            ok: true,
            latency_ms,
            model_echoed: parsed.model.unwrap_or_else(|| self.config.model.clone()),
        })
    }

    /// Synchronous chat completion for I-14's analyze flow. Returns
    /// the assistant's content string. `max_tokens` caps completion
    /// length; callers should keep it tight (<= 800) to stay within
    /// Ollama/LM-Studio budgets.
    #[allow(dead_code)] // consumed by analyze command in I-14
    pub async fn chat(
        &self,
        system: &str,
        user: &str,
        max_tokens: i64,
    ) -> Result<String, LlmError> {
        let body = json!({
            "model": self.config.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
            "max_tokens": max_tokens,
            "stream": false,
        });
        let url = format!("{}/chat/completions", trim_trailing_slash(&self.config.base_url));
        let mut req = self.client.post(&url).json(&body);
        if let Some(key) = &self.config.api_key {
            req = req.bearer_auth(key);
        }
        let res = req.send().await.map_err(classify_send_error)?;

        let status = res.status();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(LlmError::AuthFailed);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimited);
        }
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(LlmError::Other(format!(
                "HTTP {} — {}",
                status.as_u16(),
                body
            )));
        }

        let parsed: ChatCompletionResponse = res
            .json()
            .await
            .map_err(|e| LlmError::ParseError(format!("decode response: {e}")))?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| LlmError::ParseError("no choices in response".into()))?;
        Ok(content)
    }
}

fn classify_send_error(e: reqwest::Error) -> LlmError {
    if e.is_timeout() {
        LlmError::Timeout
    } else if e.is_connect() {
        LlmError::Unreachable(format!("{e}"))
    } else if e.is_status() {
        LlmError::Other(format!("{e}"))
    } else {
        LlmError::Other(format!("{e}"))
    }
}

fn trim_trailing_slash(s: &str) -> &str {
    s.trim_end_matches('/')
}

// ── response shape ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    #[serde(default)]
    content: Option<String>,
}
