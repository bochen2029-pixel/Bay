use std::path::PathBuf;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

mod commands;
mod db;
mod hotkey;

// Domain types are scaffolded ahead of incremental consumers (rank_between
// and state/event types are used from I-03 onward). Silence dead_code while
// the scaffold outpaces usage.
#[allow(dead_code)]
mod domain;

use db::SqlitePool;

/// Event emitted when the backend would like the frontend to surface a
/// user-visible warning (e.g. hotkey registration failed). Frontend
/// renders these as transient toasts.
const WARNING_EVENT: &str = "backend_warning";

fn resolve_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create app data dir {dir:?}: {e}"))?;
    Ok(dir.join("bay.db"))
}

#[tauri::command]
fn bootstrap(pool: State<'_, SqlitePool>) -> Result<Value, String> {
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let items = db::items::list_active_items(&conn)?;
    Ok(json!({
        "items": items,
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
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let db_path = resolve_db_path(&handle)?;
            let pool = db::open_pool(&db_path)?;
            db::run_migrations(&pool)?;
            app.manage(pool);

            // Hotkey registration: log-and-toast on failure rather than
            // crashing — another app may already hold Ctrl+Alt+N.
            // Reconfiguration UI lands in I-11.
            if let Err(e) = hotkey::register_default(&handle) {
                eprintln!("hotkey registration failed: {e}");
                let _ = handle.emit(
                    WARNING_EVENT,
                    json!({
                        "kind": "hotkey_registration_failed",
                        "message": e,
                    }),
                );
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            commands::items::create_item,
            commands::items::move_item,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bay");
}
