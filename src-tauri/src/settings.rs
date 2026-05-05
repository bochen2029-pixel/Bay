//! Settings: durable JSON on disk, mutable in-memory state, hotkey
//! reconfiguration hook. Schema mirrors SPEC §5.3 Settings.
//!
//! The api_key is never persisted here — only `has_api_key` (derived
//! from a keychain lookup). SPEC §5.3 / §10.9 explicitly forbid
//! secrets in the JSON settings file.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const SETTINGS_FILENAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub hotkey: String,
    pub staleness_inbox_days: Option<i64>,
    pub staleness_a_days: Option<i64>,
    pub staleness_b_days: Option<i64>,
    pub staleness_c_days: Option<i64>,
    pub lan_capture_enabled: bool,
    pub lan_capture_port: u16,
    pub lan_capture_shared_secret: Option<String>,
    pub llm: LlmSettings,
    pub analyze_window_days: i64,
    /// When true (default), closing the window hides it to the tray
    /// rather than quitting. Backward-compat: settings.json files
    /// from before this field existed deserialize with the default.
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettings {
    pub base_url: String,
    pub model: String,
    /// Derived at load time from a keychain lookup. Never persisted in
    /// the JSON file — see `write_to_disk` which strips it before
    /// serializing.
    pub has_api_key: bool,
    pub timeout_ms: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Alt+N".into(),
            staleness_inbox_days: Some(3),
            staleness_a_days: Some(14),
            staleness_b_days: Some(21),
            staleness_c_days: None,
            lan_capture_enabled: false,
            lan_capture_port: 47821,
            lan_capture_shared_secret: None,
            llm: LlmSettings {
                base_url: "http://localhost:11434/v1".into(),
                model: "llama3.2".into(),
                has_api_key: false,
                timeout_ms: 30_000,
            },
            analyze_window_days: 30,
            close_to_tray: true,
        }
    }
}

pub fn settings_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(SETTINGS_FILENAME)
}

/// Load settings from disk, falling back to defaults on missing file
/// or parse error. has_api_key is set from the keychain after load.
pub fn load(app_data_dir: &Path) -> Settings {
    let path = settings_path(app_data_dir);
    let mut settings = match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Settings>(&text) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("settings: parse failed ({e}); using defaults");
                Settings::default()
            }
        },
        Err(_) => Settings::default(),
    };
    settings.llm.has_api_key = crate::keychain::has_api_key();
    settings
}

/// Write settings to disk. has_api_key is a derived field and never
/// persisted — we clone + clear it before serializing so a malicious
/// read of the on-disk JSON can't leak which accounts exist.
pub fn write_to_disk(app_data_dir: &Path, settings: &Settings) -> Result<(), String> {
    let mut copy = settings.clone();
    copy.llm.has_api_key = false;
    let text =
        serde_json::to_string_pretty(&copy).map_err(|e| format!("serialize settings: {e}"))?;
    let path = settings_path(app_data_dir);
    fs::write(&path, text).map_err(|e| format!("write settings {path:?}: {e}"))
}
