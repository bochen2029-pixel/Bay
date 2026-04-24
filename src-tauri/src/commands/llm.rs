//! Analyze + accept/reject LLM-suggestion commands. All writes go
//! through the event log as LLM_SUGGESTION_GENERATED /
//! LLM_SUGGESTION_ACCEPTED / LLM_SUGGESTION_REJECTED — the LLM never
//! touches the items projection (CLAUDE.md §Design philosophy #2).

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use crate::commands::settings::SettingsState;
use crate::db::{self, EventDraft, SqlitePool};
use crate::domain::EventType;
use crate::llm::compression::{self, AnalyzeContext};
use crate::llm::openai_compat::OpenAiCompatClient;
use crate::llm::parse::{parse_observations, Observation};
use crate::llm::prompt::{format_user_prompt, RETRY_PREFIX, SYSTEM_PROMPT};
use crate::llm::LlmConfig;

const ANALYZE_PROGRESS_EVENT: &str = "analyze_progress";
const MAX_COMPLETION_TOKENS: i64 = 800;

#[derive(Debug, Serialize)]
pub struct AnalyzeResult {
    pub suggestion_event_id: i64,
    pub observations: Vec<Observation>,
    pub scope: AnalyzeScope,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzeScope {
    pub since_ts: i64,
    pub until_ts: i64,
    pub event_count: i64,
    pub window_days: i64,
}

#[tauri::command]
pub async fn analyze(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    settings: State<'_, SettingsState>,
    window_days: Option<i64>,
) -> Result<AnalyzeResult, String> {
    // Snapshot settings + config synchronously; mutex must not cross
    // the await boundary.
    let (config, effective_window_days) = {
        let guard = settings.lock().map_err(|e| format!("settings lock: {e}"))?;
        let config = LlmConfig::from_settings(&guard);
        let w = window_days.unwrap_or(guard.analyze_window_days).max(1);
        (config, w)
    };

    emit_progress(&app, "compressing");

    // Compress: run the SQL aggregates. Uses a blocking task because
    // rusqlite is sync.
    let pool_for_compress = (*pool).clone();
    let settings_snapshot = {
        let guard = settings.lock().map_err(|e| format!("settings lock: {e}"))?;
        guard.clone()
    };
    let now_ms = unix_ms_now();
    let ctx: AnalyzeContext = tokio::task::spawn_blocking(move || {
        compression::compress(
            &pool_for_compress,
            effective_window_days,
            &settings_snapshot,
            now_ms,
        )
    })
    .await
    .map_err(|e| format!("compression join: {e}"))??;

    // Build prompts.
    let user_prompt = format_user_prompt(&ctx);
    emit_progress(&app, "calling_llm");

    let model = config.model.clone();
    let client = OpenAiCompatClient::new(config).map_err(|e| e.into_string())?;

    // First attempt, then one retry with a "return JSON only" prefix
    // on the user prompt if parse fails.
    let known_ids: HashSet<String> = ctx
        .inbox_list
        .iter()
        .chain(ctx.a_list.iter())
        .chain(ctx.b_list.iter())
        .map(|s| s.id.clone())
        .collect();

    let first_response = client
        .chat(SYSTEM_PROMPT, &user_prompt, MAX_COMPLETION_TOKENS)
        .await
        .map_err(|e| e.into_string())?;

    emit_progress(&app, "parsing");

    let observations = match parse_observations(&first_response, &known_ids) {
        Ok(obs) => obs,
        Err(first_err) => {
            // One retry with an explicit "JSON only" prefix.
            emit_progress(&app, "retrying_parse");
            let retry_user = format!("{RETRY_PREFIX}{user_prompt}");
            let retry_response = client
                .chat(SYSTEM_PROMPT, &retry_user, MAX_COMPLETION_TOKENS)
                .await
                .map_err(|e| e.into_string())?;
            match parse_observations(&retry_response, &known_ids) {
                Ok(obs) => obs,
                Err(second_err) => {
                    // Emit the suggestion event with empty observations +
                    // error hint so the audit trail reflects that we tried.
                    let err_payload = json!({
                        "kind": "analyze",
                        "scope": {
                            "since_ts": ctx.since_ts,
                            "until_ts": ctx.until_ts,
                            "event_count": ctx.event_count,
                            "window_days": effective_window_days,
                        },
                        "model": model,
                        "observations": [],
                        "error": format!("first: {first_err}; retry: {second_err}"),
                    });
                    let _ = log_suggestion(&pool, err_payload);
                    return Err(format!("LLM_PARSE_ERROR: {second_err}"));
                }
            }
        }
    };

    let scope = AnalyzeScope {
        since_ts: ctx.since_ts,
        until_ts: ctx.until_ts,
        event_count: ctx.event_count,
        window_days: effective_window_days,
    };
    let payload = json!({
        "kind": "analyze",
        "scope": scope,
        "model": model,
        "observations": observations,
    });

    let suggestion_event_id = log_suggestion(&pool, payload)?;

    Ok(AnalyzeResult {
        suggestion_event_id,
        observations,
        scope,
        model,
    })
}

#[tauri::command]
pub fn accept_suggestion(
    pool: State<'_, SqlitePool>,
    suggestion_event_id: i64,
) -> Result<(), String> {
    log_response(&pool, EventType::LlmSuggestionAccepted, suggestion_event_id, None)
}

#[tauri::command]
pub fn reject_suggestion(
    pool: State<'_, SqlitePool>,
    suggestion_event_id: i64,
    reason: Option<String>,
) -> Result<(), String> {
    log_response(
        &pool,
        EventType::LlmSuggestionRejected,
        suggestion_event_id,
        reason,
    )
}

// ── helpers ──────────────────────────────────────────────────────

fn log_suggestion(pool: &SqlitePool, payload: serde_json::Value) -> Result<i64, String> {
    let event = db::write_event(pool, |_tx, _ts| {
        Ok(EventDraft {
            event_type: EventType::LlmSuggestionGenerated,
            item_id: None,
            payload,
        })
    })?;
    Ok(event.id)
}

fn log_response(
    pool: &SqlitePool,
    event_type: EventType,
    suggestion_event_id: i64,
    reason: Option<String>,
) -> Result<(), String> {
    // Verify the referenced suggestion event exists.
    {
        let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE id = ?1 AND type = 'LLM_SUGGESTION_GENERATED'",
                [suggestion_event_id],
                |r| r.get(0),
            )
            .map_err(|e| format!("lookup suggestion: {e}"))?;
        if found == 0 {
            return Err("EVENT_NOT_FOUND".into());
        }
    }

    let payload = match event_type {
        EventType::LlmSuggestionAccepted => json!({
            "suggestion_event_id": suggestion_event_id,
            // v1 is advisory-only: no item mutations flow from accept,
            // so resulting_event_ids is always empty. SPEC §10.5.
            "resulting_event_ids": Vec::<i64>::new(),
        }),
        EventType::LlmSuggestionRejected => json!({
            "suggestion_event_id": suggestion_event_id,
            "reason": reason,
        }),
        _ => unreachable!("log_response called with wrong event_type"),
    };

    let _ = db::write_event(pool, |_tx, _ts| {
        Ok(EventDraft {
            event_type,
            item_id: None,
            payload,
        })
    })?;
    Ok(())
}

fn emit_progress(app: &AppHandle, stage: &str) {
    let _ = app.emit(
        ANALYZE_PROGRESS_EVENT,
        json!({ "stage": stage }),
    );
}

fn unix_ms_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
