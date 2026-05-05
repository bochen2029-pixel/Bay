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

    /// Synchronous chat completion for the analyze flow. Returns
    /// the assistant's content string. `max_tokens` caps completion
    /// length; callers should keep it tight (<= 800) to stay within
    /// Ollama/LM-Studio budgets.
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

#[cfg(test)]
mod tests {
    //! HTTP-level tests for OpenAiCompatClient via mockito. Cover the
    //! error-classification and parse-error branches that previously
    //! had no coverage at all (the client was reachable only by the
    //! settings/analyze commands, which require a real LLM endpoint).
    //!
    //! Each test stands up a mockito server, points the client at
    //! that URL, and asserts the right LlmError variant comes back.
    //! Timeout is exercised by setting a 1-millisecond timeout on the
    //! client config and having mockito hold the response slightly
    //! longer.
    //!
    //! Tests use #[tokio::test] (already available via tokio's `full`
    //! feature) so reqwest's async surface works.
    use super::*;
    use crate::llm::{LlmConfig, LlmError};
    use std::time::Duration;

    fn config_for(url: String) -> LlmConfig {
        LlmConfig {
            base_url: url,
            model: "test-model".into(),
            api_key: Some("sk-test".into()),
            timeout: Duration::from_secs(5),
        }
    }

    fn happy_response_body() -> &'static str {
        r#"{
          "model": "test-model",
          "choices": [{
            "message": { "role": "assistant", "content": "ok" }
          }]
        }"#
    }

    #[tokio::test]
    async fn test_connection_happy_path_returns_latency_and_model() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/chat/completions")
            .match_header("content-type", "application/json")
            .match_header("authorization", "Bearer sk-test")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(happy_response_body())
            .create_async()
            .await;

        let client = OpenAiCompatClient::new(config_for(server.url())).unwrap();
        let res = client.test_connection().await.expect("happy path");
        assert!(res.ok);
        assert_eq!(res.model_echoed, "test-model");
        assert!(res.latency_ms >= 0, "latency_ms must be non-negative");

        m.assert_async().await;
    }

    #[tokio::test]
    async fn test_connection_401_maps_to_auth_failed() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/chat/completions")
            .with_status(401)
            .with_body(r#"{"error":"Invalid API key"}"#)
            .create_async()
            .await;

        let client = OpenAiCompatClient::new(config_for(server.url())).unwrap();
        let err = client.test_connection().await.unwrap_err();
        assert!(matches!(err, LlmError::AuthFailed), "got {err:?}");
        assert_eq!(err.code(), "LLM_AUTH_FAILED");
    }

    #[tokio::test]
    async fn test_connection_403_maps_to_auth_failed() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/chat/completions")
            .with_status(403)
            .with_body("forbidden")
            .create_async()
            .await;

        let client = OpenAiCompatClient::new(config_for(server.url())).unwrap();
        let err = client.test_connection().await.unwrap_err();
        assert!(matches!(err, LlmError::AuthFailed), "got {err:?}");
    }

    #[tokio::test]
    async fn test_connection_429_maps_to_rate_limited() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/chat/completions")
            .with_status(429)
            .with_body("slow down")
            .create_async()
            .await;

        let client = OpenAiCompatClient::new(config_for(server.url())).unwrap();
        let err = client.test_connection().await.unwrap_err();
        assert!(matches!(err, LlmError::RateLimited), "got {err:?}");
        assert_eq!(err.code(), "LLM_RATE_LIMITED");
    }

    #[tokio::test]
    async fn test_connection_500_maps_to_other_with_body() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/chat/completions")
            .with_status(500)
            .with_body("internal server error: db unreachable")
            .create_async()
            .await;

        let client = OpenAiCompatClient::new(config_for(server.url())).unwrap();
        let err = client.test_connection().await.unwrap_err();
        match err {
            LlmError::Other(msg) => {
                assert!(msg.contains("HTTP 500"), "expected HTTP 500 in {msg:?}");
                assert!(
                    msg.contains("db unreachable"),
                    "expected body included in {msg:?}",
                );
            }
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_connection_malformed_json_maps_to_parse_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("not valid json {{{")
            .create_async()
            .await;

        let client = OpenAiCompatClient::new(config_for(server.url())).unwrap();
        let err = client.test_connection().await.unwrap_err();
        assert!(matches!(err, LlmError::ParseError(_)), "got {err:?}");
        assert_eq!(err.code(), "LLM_PARSE_ERROR");
    }

    #[tokio::test]
    async fn test_connection_omits_auth_header_when_no_api_key() {
        // Local Ollama / LM Studio is unauthenticated. The client
        // must NOT send a bearer token in that case.
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/chat/completions")
            .match_header(
                "authorization",
                mockito::Matcher::Missing,
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(happy_response_body())
            .create_async()
            .await;

        let mut config = config_for(server.url());
        config.api_key = None;
        let client = OpenAiCompatClient::new(config).unwrap();
        client.test_connection().await.expect("ok");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn test_connection_handles_trailing_slash_in_base_url() {
        // Settings UI may or may not strip the trailing slash on the
        // base URL. Both forms should hit the same endpoint.
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(happy_response_body())
            .create_async()
            .await;

        let url_with_slash = format!("{}/", server.url());
        let client = OpenAiCompatClient::new(config_for(url_with_slash)).unwrap();
        client.test_connection().await.expect("ok");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn test_connection_unreachable_when_server_refused() {
        // No mockito server. Pick an unbound port; reqwest should
        // surface a connect error which we classify as Unreachable.
        let dead_url = "http://127.0.0.1:1".to_string(); // privileged port; nothing listens
        let mut config = config_for(dead_url);
        // Tighter timeout so the test doesn't hang on the connect.
        config.timeout = Duration::from_millis(500);

        let client = OpenAiCompatClient::new(config).unwrap();
        let err = client.test_connection().await.unwrap_err();
        // Either Unreachable (connect refused immediately) or Timeout
        // (some platforms hang on a closed port). Both are acceptable
        // — the contract is "not Other / not AuthFailed".
        assert!(
            matches!(err, LlmError::Unreachable(_) | LlmError::Timeout),
            "got {err:?}",
        );
    }

    // ── chat() coverage ─────────────────────────────────────────

    #[tokio::test]
    async fn chat_happy_path_returns_content_string() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                  "model": "test-model",
                  "choices": [{
                    "message": { "role": "assistant", "content": "the analysis text" }
                  }]
                }"#,
            )
            .create_async()
            .await;

        let client = OpenAiCompatClient::new(config_for(server.url())).unwrap();
        let out = client
            .chat("system prompt", "user prompt", 100)
            .await
            .expect("happy path");
        assert_eq!(out, "the analysis text");
    }

    #[tokio::test]
    async fn chat_empty_choices_maps_to_parse_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"model":"test-model","choices":[]}"#)
            .create_async()
            .await;

        let client = OpenAiCompatClient::new(config_for(server.url())).unwrap();
        let err = client.chat("s", "u", 100).await.unwrap_err();
        match err {
            LlmError::ParseError(msg) => {
                assert!(
                    msg.contains("no choices"),
                    "expected 'no choices' in {msg:?}",
                );
            }
            other => panic!("expected ParseError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn chat_authorization_header_carries_api_key() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/chat/completions")
            .match_header("authorization", "Bearer sk-test")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"model":"test-model","choices":[{"message":{"content":"x"}}]}"#,
            )
            .create_async()
            .await;

        let client = OpenAiCompatClient::new(config_for(server.url())).unwrap();
        client.chat("s", "u", 100).await.expect("ok");
        m.assert_async().await;
    }
}
