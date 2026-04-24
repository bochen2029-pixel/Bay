//! Global shortcut registration via `tauri-plugin-global-shortcut`.
//!
//! Single-responsibility: register the quick-capture hotkey (default
//! Ctrl+Alt+N per SPEC §10.1) and emit `quick_capture_requested` to
//! the frontend when it fires. Reconfiguration UI lands in I-11.
//!
//! Failure handling: registration can fail when another app already
//! holds the combination. We log the failure and return the error so
//! the setup path can surface a toast to the user rather than
//! crashing.

use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Tauri event fired when the quick-capture hotkey is pressed.
pub const QUICK_CAPTURE_EVENT: &str = "quick_capture_requested";

/// The shipping default. Ctrl+Alt+N per SPEC §10.1 because
/// Ctrl+Shift+Space collides with Windows IME switching and some
/// screen readers. `Shortcut::new` is non-const, so we build the
/// value lazily rather than in a `const`.
fn default_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL.union(Modifiers::ALT)), Code::KeyN)
}

pub fn register_default<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let shortcut = default_shortcut();
    let emit_handle = app.clone();

    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _sc, event| {
            if event.state() == ShortcutState::Pressed {
                if let Err(e) = emit_handle.emit(QUICK_CAPTURE_EVENT, ()) {
                    // Emitting should never fail in a healthy app; log
                    // loudly but don't propagate (the handler signature
                    // doesn't allow it anyway).
                    eprintln!("emit {QUICK_CAPTURE_EVENT} failed: {e}");
                }
            }
        })
        .map_err(|e| format!("register quick-capture hotkey: {e}"))?;

    eprintln!("hotkey: Ctrl+Alt+N registered for quick capture");
    Ok(())
}
