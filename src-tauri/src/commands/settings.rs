//! Settings + LLM-config + export_events commands. All state here
//! lives behind `Mutex<Settings>` in Tauri state; hotkey changes
//! drive through `hotkey::reregister`.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use crate::db::SqlitePool;
use crate::hotkey;
use crate::keychain;
use crate::llm::{openai_compat::OpenAiCompatClient, LlmConfig, TestResult};
use crate::settings::{self, Settings};

pub type SettingsState = Mutex<Settings>;

// Frontend sends "partial" settings as a flat object where any field
// may be absent. We deserialize into a struct whose every field is
// Option<T> so missing keys survive JSON round-trip.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SettingsPatch {
    pub hotkey: Option<String>,
    pub staleness_inbox_days: Option<Option<i64>>,
    pub staleness_a_days: Option<Option<i64>>,
    pub staleness_b_days: Option<Option<i64>>,
    pub staleness_c_days: Option<Option<i64>>,
    pub lan_capture_enabled: Option<bool>,
    pub lan_capture_port: Option<u16>,
    pub lan_capture_shared_secret: Option<Option<String>>,
    pub llm: Option<LlmPatch>,
    pub analyze_window_days: Option<i64>,
    pub close_to_tray: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LlmPatch {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub timeout_ms: Option<i64>,
}

#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> Result<Settings, String> {
    let s = state.lock().map_err(|e| format!("lock: {e}"))?;
    Ok(s.clone())
}

#[tauri::command]
pub fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SettingsState>,
    data_dir: State<'_, DataDir>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    let mut s = state.lock().map_err(|e| format!("lock: {e}"))?;

    let old_hotkey = s.hotkey.clone();

    // Validate + apply the patch. Any validation failure leaves `s`
    // unchanged.
    if let Some(new_hotkey) = &patch.hotkey {
        // Parse as a probe — re-parsing happens inside
        // hotkey::reregister too, but we want to surface the error
        // before mutating in-memory state.
        hotkey::parse_shortcut(new_hotkey)?;
    }
    if let Some(t) = patch.llm.as_ref().and_then(|l| l.timeout_ms) {
        if t < 1_000 || t > 600_000 {
            return Err("INVALID_SETTING: llm.timeout_ms must be in [1000, 600000]".into());
        }
    }
    if let Some(w) = patch.analyze_window_days {
        if w <= 0 || w > 3650 {
            return Err("INVALID_SETTING: analyze_window_days out of range".into());
        }
    }
    for (label, v) in [
        ("staleness_inbox_days", patch.staleness_inbox_days),
        ("staleness_a_days", patch.staleness_a_days),
        ("staleness_b_days", patch.staleness_b_days),
        ("staleness_c_days", patch.staleness_c_days),
    ] {
        if let Some(Some(days)) = v {
            if days <= 0 || days > 3650 {
                return Err(format!(
                    "INVALID_SETTING: {label} must be null or positive int"
                ));
            }
        }
    }

    // Apply patch.
    if let Some(v) = patch.hotkey {
        s.hotkey = v;
    }
    if let Some(v) = patch.staleness_inbox_days {
        s.staleness_inbox_days = v;
    }
    if let Some(v) = patch.staleness_a_days {
        s.staleness_a_days = v;
    }
    if let Some(v) = patch.staleness_b_days {
        s.staleness_b_days = v;
    }
    if let Some(v) = patch.staleness_c_days {
        s.staleness_c_days = v;
    }
    if let Some(v) = patch.lan_capture_enabled {
        s.lan_capture_enabled = v;
    }
    if let Some(v) = patch.lan_capture_port {
        s.lan_capture_port = v;
    }
    if let Some(v) = patch.lan_capture_shared_secret {
        s.lan_capture_shared_secret = v;
    }
    if let Some(llm) = patch.llm {
        if let Some(v) = llm.base_url {
            s.llm.base_url = v;
        }
        if let Some(v) = llm.model {
            s.llm.model = v;
        }
        if let Some(v) = llm.timeout_ms {
            s.llm.timeout_ms = v;
        }
    }
    if let Some(v) = patch.analyze_window_days {
        s.analyze_window_days = v;
    }
    if let Some(v) = patch.close_to_tray {
        s.close_to_tray = v;
    }

    // Persist.
    settings::write_to_disk(&data_dir.0, &s)?;

    // Re-register hotkey if changed.
    if s.hotkey != old_hotkey {
        if let Err(e) = hotkey::reregister(&app, &old_hotkey, &s.hotkey) {
            // Roll back in-memory change so the UI reflects reality.
            s.hotkey = old_hotkey;
            settings::write_to_disk(&data_dir.0, &s)?;
            return Err(format!("hotkey reregister failed: {e}"));
        }
    }

    Ok(s.clone())
}

// ── LLM API key (keychain-backed) ─────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SetLlmKeyArgs {
    /// Empty string deletes the existing key. Any other value stores
    /// it in the OS keychain.
    pub api_key: String,
}

#[tauri::command]
pub fn set_llm_api_key(
    state: State<'_, SettingsState>,
    args: SetLlmKeyArgs,
) -> Result<Settings, String> {
    keychain::set_api_key(args.api_key.trim())?;
    let mut s = state.lock().map_err(|e| format!("lock: {e}"))?;
    s.llm.has_api_key = keychain::has_api_key();
    Ok(s.clone())
}

// ── export_events ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub events_written: i64,
    pub path: String,
}

#[tauri::command]
pub fn export_events(
    pool: State<'_, SqlitePool>,
    path: String,
) -> Result<ExportResult, String> {
    use std::io::Write;

    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT id, ts, type, item_id, payload FROM events ORDER BY id",
        )
        .map_err(|e| format!("prepare export: {e}"))?;

    let mut file = std::fs::File::create(&path)
        .map_err(|e| format!("IO_ERROR: create {path}: {e}"))?;
    let mut count = 0i64;

    let rows = stmt
        .query_map([], |r| {
            let id: i64 = r.get(0)?;
            let ts: i64 = r.get(1)?;
            let ty: String = r.get(2)?;
            let item_id: Option<String> = r.get(3)?;
            let payload_str: String = r.get(4)?;
            let payload: serde_json::Value =
                serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
            Ok(serde_json::json!({
                "id": id,
                "ts": ts,
                "type": ty,
                "item_id": item_id,
                "payload": payload,
            }))
        })
        .map_err(|e| format!("query export: {e}"))?;

    for row in rows {
        let value = row.map_err(|e| format!("row export: {e}"))?;
        let line = serde_json::to_string(&value)
            .map_err(|e| format!("serialize row: {e}"))?;
        writeln!(file, "{line}").map_err(|e| format!("IO_ERROR: write: {e}"))?;
        count += 1;
    }
    file.flush().map_err(|e| format!("IO_ERROR: flush: {e}"))?;

    Ok(ExportResult {
        events_written: count,
        path,
    })
}

/// App data directory held in state so commands that write to disk
/// can locate settings.json without re-resolving the handle every call.
pub struct DataDir(pub PathBuf);

// ── test_llm_connection ───────────────────────────────────────────

#[tauri::command]
pub async fn test_llm_connection(
    settings: State<'_, SettingsState>,
) -> Result<TestResult, String> {
    // Snapshot the config synchronously so the mutex guard is dropped
    // before the async HTTP work starts.
    let config = {
        let guard = settings.lock().map_err(|e| format!("lock: {e}"))?;
        LlmConfig::from_settings(&guard)
    };
    let client =
        OpenAiCompatClient::new(config).map_err(|e| e.into_string())?;
    client
        .test_connection()
        .await
        .map_err(|e| e.into_string())
}
