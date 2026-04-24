//! Tauri commands for the LAN capture server lifecycle. The server
//! itself lives in `crate::capture::CaptureState`.

use tauri::{AppHandle, State};

use crate::capture::{CaptureState, LanCaptureStatus};
use crate::commands::settings::SettingsState;
use crate::db::SqlitePool;

#[tauri::command]
pub fn toggle_lan_capture(
    app: AppHandle,
    capture: State<'_, CaptureState>,
    pool: State<'_, SqlitePool>,
    settings: State<'_, SettingsState>,
    enabled: bool,
) -> Result<LanCaptureStatus, String> {
    if !enabled {
        capture.stop()?;
        return Ok(LanCaptureStatus {
            enabled: false,
            url: None,
            qr_svg: None,
            port: None,
        });
    }
    let (port, secret) = {
        let s = settings.lock().map_err(|e| format!("settings lock: {e}"))?;
        (s.lan_capture_port, s.lan_capture_shared_secret.clone())
    };
    capture.start(app, (*pool).clone(), port, secret)
}

#[tauri::command]
pub fn get_lan_capture_status(
    capture: State<'_, CaptureState>,
) -> Result<LanCaptureStatus, String> {
    Ok(capture.status())
}
