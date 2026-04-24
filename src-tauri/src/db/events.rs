//! The one place events get written. Called only from
//! `db::write_event` inside a caller-provided transaction — never
//! standalone.

use rusqlite::{params, Transaction};
use serde_json::Value;

use crate::domain::EventType;

/// Append a row to the `events` table. Returns the new `id` (SQLite
/// rowid from AUTOINCREMENT). Must run inside an open transaction that
/// the caller also commits via `write_event`.
pub fn append_event(
    tx: &Transaction<'_>,
    ts: i64,
    event_type: EventType,
    item_id: Option<&str>,
    payload: &Value,
) -> Result<i64, String> {
    let payload_json =
        serde_json::to_string(payload).map_err(|e| format!("serialize payload: {e}"))?;
    tx.execute(
        "INSERT INTO events (ts, type, item_id, payload) VALUES (?1, ?2, ?3, ?4)",
        params![ts, event_type.as_sql(), item_id, payload_json],
    )
    .map_err(|e| format!("insert event: {e}"))?;
    Ok(tx.last_insert_rowid())
}
