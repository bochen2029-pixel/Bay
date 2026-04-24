use serde_json::{json, Value};

#[tauri::command]
fn bootstrap() -> Value {
    json!({
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
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![bootstrap])
        .run(tauri::generate_context!())
        .expect("error while running Bay");
}
