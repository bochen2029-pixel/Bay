//! Tauri `#[command]` handlers. Each command is a thin wrapper that
//! validates its arguments, routes writes through `db::write_event`, and
//! returns the affected `Item` (or other result type per SPEC §5.1).

pub mod capture;
pub mod day;
pub mod events;
pub mod items;
pub mod llm;
pub mod settings;
