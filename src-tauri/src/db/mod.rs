//! SQLite connection pool, linear migration runner, and the
//! transactional "write event" wrapper every event-emitting command
//! routes through.
//!
//! The wrapper is the load-bearing invariant: an event is never appended
//! without its projection update, and a projection update never happens
//! without the event on disk. Both land in a single SQLite transaction
//! or neither does. Direct calls to `events::append_event` or
//! `items::apply_event_to_projection` outside this wrapper are a bug.
//!
//! Schema version is tracked via SQLite's built-in `PRAGMA user_version`.
//! Numbered SQL files embedded at compile time; applied in order, in a
//! transaction per migration. No metadata table, no rollback, no graph.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Transaction;
use serde_json::Value;

use crate::domain::{Event, EventType};

pub mod events;
pub mod items;

pub type SqlitePool = Pool<SqliteConnectionManager>;
#[allow(dead_code)] // consumed from I-10 rebuild_projection onward
pub type SqliteConn = PooledConnection<SqliteConnectionManager>;

/// Migration SQL embedded at compile time so the binary is self-contained.
/// First tuple element is the target `user_version` after the SQL applies.
const MIGRATIONS: &[(i32, &str)] = &[
    (1, include_str!("../../../migrations/001_initial.sql")),
];

pub fn open_pool(db_path: &Path) -> Result<SqlitePool, String> {
    let manager = SqliteConnectionManager::file(db_path).with_init(|c| {
        c.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
    });
    Pool::builder()
        .max_size(8)
        .build(manager)
        .map_err(|e| format!("sqlite pool init failed: {e}"))
}

pub fn run_migrations(pool: &SqlitePool) -> Result<(), String> {
    let mut conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let current: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| format!("read user_version: {e}"))?;

    for (target, sql) in MIGRATIONS {
        if current >= *target {
            continue;
        }
        let tx = conn
            .transaction()
            .map_err(|e| format!("begin migration tx: {e}"))?;
        tx.execute_batch(sql)
            .map_err(|e| format!("apply migration {target}: {e}"))?;
        // PRAGMA does not accept bound parameters. `target` is a compile-time
        // const, not user input, so string interpolation is safe here.
        tx.execute_batch(&format!("PRAGMA user_version = {target};"))
            .map_err(|e| format!("bump user_version to {target}: {e}"))?;
        tx.commit()
            .map_err(|e| format!("commit migration {target}: {e}"))?;
    }
    Ok(())
}

/// What a command builds inside the event-writing transaction. The runner
/// fills in the event `id` and `ts`.
pub struct EventDraft {
    pub event_type: EventType,
    pub item_id: Option<String>,
    pub payload: Value,
}

/// Open a transaction, run `build` to produce the event draft, append it
/// to `events`, apply it to the `items` projection, and commit — all
/// atomically. If any step fails the transaction rolls back; no partial
/// state reaches disk.
///
/// The `build` closure gets access to the transaction so it can consult
/// current projection state (e.g., max rank in a tier, active counts)
/// under the same isolation boundary as the write.
pub fn write_event<F>(pool: &SqlitePool, build: F) -> Result<Event, String>
where
    F: FnOnce(&Transaction<'_>, i64) -> Result<EventDraft, String>,
{
    let mut conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("begin tx: {e}"))?;
    let ts = unix_ms_now();
    let EventDraft {
        event_type,
        item_id,
        payload,
    } = build(&tx, ts)?;
    let id = events::append_event(&tx, ts, event_type, item_id.as_deref(), &payload)?;
    let event = Event {
        id,
        ts,
        event_type,
        item_id,
        payload,
    };
    items::apply_event_to_projection(&tx, &event)?;
    tx.commit().map_err(|e| format!("commit tx: {e}"))?;
    Ok(event)
}

pub fn unix_ms_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_pool() -> SqlitePool {
        let manager = SqliteConnectionManager::memory();
        // max_size=1 ensures all queries hit the same in-memory DB; otherwise
        // different pool conns see empty DBs.
        Pool::builder().max_size(1).build(manager).unwrap()
    }

    #[test]
    fn migrations_bring_fresh_db_to_target_version() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        let v: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);
    }

    #[test]
    fn migrations_are_idempotent() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        let v: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);
    }

    #[test]
    fn schema_has_expected_tables_and_indexes() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type IN ('table','index') AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            names,
            vec![
                "events".to_string(),
                "idx_events_item".to_string(),
                "idx_events_ts".to_string(),
                "idx_items_tier_rank".to_string(),
                "items".to_string(),
            ]
        );
    }

    #[test]
    fn write_event_rolls_back_on_apply_failure() {
        use serde_json::json;

        let pool = mem_pool();
        run_migrations(&pool).unwrap();

        // Build a payload that apply_event_to_projection will reject
        // (ITEM_EDITED has no I-03 handler yet and returns Err), then
        // confirm neither the event nor any item row persisted.
        let result = write_event(&pool, |_tx, _ts| {
            Ok(EventDraft {
                event_type: EventType::ItemEdited,
                item_id: Some("test".into()),
                payload: json!({"content_before":"x","content_after":"y"}),
            })
        });
        assert!(result.is_err(), "unimplemented handler must return Err");

        let conn = pool.get().unwrap();
        let event_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(event_count, 0, "failed write must roll back event row");
        let item_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(item_count, 0, "failed write must roll back item row");
    }

    #[test]
    fn write_event_item_created_lands_in_both_tables() {
        use serde_json::json;

        let pool = mem_pool();
        run_migrations(&pool).unwrap();

        let event = write_event(&pool, |_tx, _ts| {
            Ok(EventDraft {
                event_type: EventType::ItemCreated,
                item_id: Some("itm-1".into()),
                payload: json!({
                    "content": "hello",
                    "tier": "inbox",
                    "rank": "m",
                    "start_at": null,
                    "due_at": null,
                }),
            })
        })
        .expect("happy-path write_event");

        assert_eq!(event.event_type, EventType::ItemCreated);
        assert_eq!(event.item_id.as_deref(), Some("itm-1"));
        assert!(event.id >= 1, "event id should come from AUTOINCREMENT");

        let conn = pool.get().unwrap();
        let (ev_type, ev_item, ev_payload): (String, Option<String>, String) = conn
            .query_row(
                "SELECT type, item_id, payload FROM events WHERE id = ?1",
                [event.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(ev_type, "ITEM_CREATED");
        assert_eq!(ev_item.as_deref(), Some("itm-1"));
        assert!(ev_payload.contains("\"content\":\"hello\""));

        let (content, tier, rank, state, deleted): (String, String, String, String, i64) = conn
            .query_row(
                "SELECT content, tier, rank, state, deleted FROM items WHERE id = 'itm-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(content, "hello");
        assert_eq!(tier, "inbox");
        assert_eq!(rank, "m");
        assert_eq!(state, "active");
        assert_eq!(deleted, 0);
    }

    #[test]
    fn projection_rebuilds_from_events() {
        // The load-bearing invariant: the items table is a pure projection
        // of the events log. Dropping items and replaying every event
        // reconstructs the same item rows byte-for-byte.
        use serde_json::json;

        let pool = mem_pool();
        run_migrations(&pool).unwrap();

        // Write three ITEM_CREATED events.
        for (i, rank) in ["m", "p", "s"].iter().enumerate() {
            write_event(&pool, |_tx, _ts| {
                Ok(EventDraft {
                    event_type: EventType::ItemCreated,
                    item_id: Some(format!("itm-{i}")),
                    payload: json!({
                        "content": format!("item {i}"),
                        "tier": "inbox",
                        "rank": rank,
                        "start_at": null,
                        "due_at": null,
                    }),
                })
            })
            .unwrap();
        }

        let snapshot_before = snapshot_items(&pool);
        assert_eq!(snapshot_before.len(), 3);

        // Wipe the projection.
        {
            let conn = pool.get().unwrap();
            conn.execute("DELETE FROM items", []).unwrap();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 0);
        }

        // Replay every event through the same projection handler. No new
        // events are written during replay — we manually apply the ones
        // already on disk.
        {
            let mut conn = pool.get().unwrap();
            let tx = conn.transaction().unwrap();
            let rows: Vec<(i64, i64, String, Option<String>, String)> = tx
                .prepare("SELECT id, ts, type, item_id, payload FROM events ORDER BY id")
                .unwrap()
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            for (id, ts, type_str, item_id, payload_str) in rows {
                let event_type = EventType::from_sql(&type_str).expect("valid event type");
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_str).expect("valid JSON payload");
                let event = Event {
                    id,
                    ts,
                    event_type,
                    item_id,
                    payload,
                };
                items::apply_event_to_projection(&tx, &event).unwrap();
            }
            tx.commit().unwrap();
        }

        let snapshot_after = snapshot_items(&pool);
        assert_eq!(
            snapshot_before, snapshot_after,
            "replayed projection must equal pre-wipe projection"
        );
    }

    fn snapshot_items(pool: &SqlitePool) -> Vec<(String, String, String, String, String)> {
        let conn = pool.get().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, content, tier, rank, state FROM items ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    }
}
