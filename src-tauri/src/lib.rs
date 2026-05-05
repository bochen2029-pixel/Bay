use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

mod capture;
mod commands;
mod db;
mod hotkey;
mod keychain;
mod llm;
mod settings;

// `pub mod` so binaries under `src/bin/` (e.g. rank_fixture_gen) can
// reach `bay_lib::domain::rank_between` to produce cross-language
// fixtures. The exposed surface is benign — Tier/ItemState/Event types,
// capacity constants, the rank helper — none of it sensitive.
pub mod domain;

use capture::CaptureState;
use commands::settings::{DataDir, SettingsState};
use db::SqlitePool;

/// Event emitted when the backend would like the frontend to surface a
/// user-visible warning (e.g. hotkey registration failed). Frontend
/// renders these as transient toasts.
const WARNING_EVENT: &str = "backend_warning";

fn resolve_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create app data dir {dir:?}: {e}"))?;
    Ok(dir)
}

#[tauri::command]
fn bootstrap(
    pool: State<'_, SqlitePool>,
    settings: State<'_, SettingsState>,
) -> Result<Value, String> {
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let items = db::items::list_active_items(&conn)?;
    let s = settings.lock().map_err(|e| format!("settings lock: {e}"))?;
    Ok(json!({
        "items": items,
        "settings": *s,
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = resolve_data_dir(&handle)?;

            // DB pool + migrations.
            let pool = db::open_pool(&data_dir.join("bay.db"))?;
            db::run_migrations(&pool)?;
            app.manage(pool);

            // Settings (loads from JSON; checks keychain for has_api_key).
            let loaded = settings::load(&data_dir);
            let hotkey_at_start = loaded.hotkey.clone();
            app.manage(Mutex::new(loaded));
            app.manage(DataDir(data_dir));
            app.manage(CaptureState::new());

            // Hotkey registration: failure becomes a toast, not a crash.
            if let Err(e) = hotkey::register(&handle, &hotkey_at_start) {
                eprintln!("hotkey registration failed: {e}");
                let _ = handle.emit(
                    WARNING_EVENT,
                    json!({
                        "kind": "hotkey_registration_failed",
                        "message": e,
                    }),
                );
            }

            // Tray icon + menu: close-to-tray behavior per SPEC §10.10.
            // The window's close_requested handler below redirects to
            // .hide() when settings.close_to_tray is true (the default).
            // Users can flip the setting off to get a regular OS-level
            // close. Quit is always reachable from the tray menu.
            build_tray(&handle)?;
            if let Some(win) = app.get_webview_window("main") {
                let win_clone = win.clone();
                win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        let close_to_tray = win_clone
                            .app_handle()
                            .state::<commands::settings::SettingsState>()
                            .lock()
                            .map(|s| s.close_to_tray)
                            .unwrap_or(true);
                        if close_to_tray {
                            api.prevent_close();
                            let _ = win_clone.hide();
                        }
                        // else: let the close proceed; the window
                        // closes and the app exits.
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            commands::items::create_item,
            commands::items::move_item,
            commands::items::swap_move,
            commands::items::edit_item,
            commands::items::set_item_state,
            commands::items::set_item_date,
            commands::items::delete_item,
            commands::items::restore_item,
            commands::events::get_events,
            commands::events::get_items_at,
            commands::events::rebuild_projection,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::set_llm_api_key,
            commands::settings::export_events,
            commands::capture::toggle_lan_capture,
            commands::capture::get_lan_capture_status,
            commands::settings::test_llm_connection,
            commands::llm::analyze,
            commands::llm::accept_suggestion,
            commands::llm::reject_suggestion,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bay");
}

fn build_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let open = MenuItemBuilder::with_id("tray_open", "Open Bay").build(app)?;
    let quit = MenuItemBuilder::with_id("tray_quit", "Quit Bay").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&open, &quit]).build()?;

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Bay")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray_open" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "tray_quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}
