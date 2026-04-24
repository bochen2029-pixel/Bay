//! LLM client integration. SPEC §8. One OpenAI-compatible
//! implementation (works against Ollama, LM Studio, OpenAI,
//! Anthropic-via-proxy). I-13 wires `test_connection`; I-14 adds
//! the analyze prompt + output parser.
//!
//! All HTTP work is async via reqwest. The client reads the LLM
//! settings from Tauri state on demand — cloning the fields it needs
//! so it doesn't hold the Mutex guard across the await boundary.

pub mod openai_compat;

use std::time::Duration;

use serde::Serialize;

use crate::keychain;
use crate::settings::Settings;

#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub ok: bool,
    pub latency_ms: i64,
    pub model_echoed: String,
}

/// User-facing error classification. SPEC §8.5 maps each variant to
/// a specific toast message; the frontend discriminates on `.code()`.
#[derive(Debug)]
pub enum LlmError {
    Unreachable(String),
    AuthFailed,
    Timeout,
    ParseError(String),
    RateLimited,
    Other(String),
}

impl LlmError {
    pub fn code(&self) -> &'static str {
        match self {
            LlmError::Unreachable(_) => "LLM_UNREACHABLE",
            LlmError::AuthFailed => "LLM_AUTH_FAILED",
            LlmError::Timeout => "LLM_TIMEOUT",
            LlmError::ParseError(_) => "LLM_PARSE_ERROR",
            LlmError::RateLimited => "LLM_RATE_LIMITED",
            LlmError::Other(_) => "LLM_OTHER",
        }
    }

    pub fn into_string(self) -> String {
        match self {
            LlmError::Unreachable(s) => format!("LLM_UNREACHABLE: {s}"),
            LlmError::AuthFailed => "LLM_AUTH_FAILED".into(),
            LlmError::Timeout => "LLM_TIMEOUT".into(),
            LlmError::ParseError(s) => format!("LLM_PARSE_ERROR: {s}"),
            LlmError::RateLimited => "LLM_RATE_LIMITED".into(),
            LlmError::Other(s) => format!("LLM_OTHER: {s}"),
        }
    }
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code())
    }
}

impl std::error::Error for LlmError {}

/// Snapshot of LLM config taken at command entry so the async client
/// never touches the settings mutex across await points.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
}

impl LlmConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            base_url: settings.llm.base_url.clone(),
            model: settings.llm.model.clone(),
            api_key: keychain::get_api_key(),
            timeout: Duration::from_millis(settings.llm.timeout_ms.max(1_000) as u64),
        }
    }
}
