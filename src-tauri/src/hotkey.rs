//! Global shortcut registration via `tauri-plugin-global-shortcut`.
//!
//! Single-responsibility: register the quick-capture hotkey (default
//! Ctrl+Alt+N per SPEC §10.1) and emit `quick_capture_requested` to
//! the frontend when it fires. Runtime reconfiguration lands in I-11
//! via `reregister`.
//!
//! Failure handling: registration can fail when another app already
//! holds the combination. We surface the error to the setup path so
//! it can toast rather than crashing.

use std::str::FromStr;

use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Tauri event fired when the quick-capture hotkey is pressed.
pub const QUICK_CAPTURE_EVENT: &str = "quick_capture_requested";

pub fn parse_shortcut(s: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(s).map_err(|e| format!("invalid shortcut {s:?}: {e}"))
}

pub fn register<R: Runtime>(app: &AppHandle<R>, shortcut_str: &str) -> Result<(), String> {
    let shortcut = parse_shortcut(shortcut_str)?;
    let emit_handle = app.clone();

    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _sc, event| {
            if event.state() == ShortcutState::Pressed {
                if let Err(e) = emit_handle.emit(QUICK_CAPTURE_EVENT, ()) {
                    eprintln!("emit {QUICK_CAPTURE_EVENT} failed: {e}");
                }
            }
        })
        .map_err(|e| format!("register hotkey {shortcut_str:?}: {e}"))?;
    eprintln!("hotkey: {shortcut_str} registered for quick capture");
    Ok(())
}

pub fn unregister<R: Runtime>(app: &AppHandle<R>, shortcut_str: &str) -> Result<(), String> {
    let shortcut = parse_shortcut(shortcut_str)?;
    app.global_shortcut()
        .unregister(shortcut)
        .map_err(|e| format!("unregister hotkey {shortcut_str:?}: {e}"))?;
    Ok(())
}

/// Unregister the old shortcut and register the new one. If the new
/// registration fails, attempt to restore the old binding rather than
/// leaving the user without a hotkey. Errors surface to the caller.
pub fn reregister<R: Runtime>(
    app: &AppHandle<R>,
    old: &str,
    new: &str,
) -> Result<(), String> {
    if old == new {
        return Ok(());
    }
    let _ = unregister(app, old); // ignore: unregister of stale key is benign
    if let Err(e) = register(app, new) {
        // New one didn't stick; try to put the old one back.
        let _ = register(app, old);
        return Err(e);
    }
    Ok(())
}
