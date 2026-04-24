use std::path::PathBuf;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};

mod db;

// Domain types are scaffolded ahead of incremental consumers (rank_between
// and state/event types are used from I-03 onward). Silence dead_code while
// the scaffold outpaces usage.
#[allow(dead_code)]
mod domain;

use db::SqlitePool;

fn resolve_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create app data dir {dir:?}: {e}"))?;
    Ok(dir.join("bay.db"))
}

#[tauri::command]
fn bootstrap(_pool: State<SqlitePool>) -> Result<Value, String> {
    // Migrations already ran at setup; we pull the pool in purely to confirm
    // it's present as managed state. Projection is empty until I-03 wires up
    // the create_item path, so we return an empty items vec and the §5.3
    // default Settings verbatim.
    Ok(json!({
        "items": [],
        "settings": {
            "hotkey": "Ctrl+Alt+N",
            "staleness_inbox_days": 3,
            "staleness_a_days": 14,
            "staleness_b_days": 21,
            "staleness_c_days": null,
            "lan_capture_enabled": false,
            "lan_capture_port": 47821,
            "lan_capture_shared_secret": null,
            "llm": {
                "base_url": "http://localhost:11434/v1",
                "model": "llama3.2",
                "has_api_key": false,
                "timeout_ms": 30000
            },
            "analyze_window_days": 30
        }
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let db_path = resolve_db_path(&handle)?;
            let pool = db::open_pool(&db_path)?;
            db::run_migrations(&pool)?;
            app.manage(pool);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![bootstrap])
        .run(tauri::generate_context!())
        .expect("error while running Bay");
}
