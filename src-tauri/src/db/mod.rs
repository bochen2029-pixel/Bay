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

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{OptionalExtension, Transaction};
use serde_json::Value;

use crate::domain::{Actor, Event, EventType};

pub mod events;
pub mod items;

pub type SqlitePool = Pool<SqliteConnectionManager>;

/// Migration SQL embedded at compile time so the binary is self-contained.
/// First tuple element is the target `user_version` after the SQL applies.
const MIGRATIONS: &[(i32, &str)] = &[
    (1, include_str!("../../../migrations/001_initial.sql")),
    (2, include_str!("../../../migrations/002_invariants.sql")),
    (3, include_str!("../../../migrations/003_event_envelope.sql")),
    (4, include_str!("../../../migrations/004_recurrence.sql")),
    (5, include_str!("../../../migrations/005_execution_core.sql")),
];

/// The `PRAGMA user_version` a fully-migrated database reports.
/// Derived from MIGRATIONS so version-pinning tests never lag a new
/// migration.
pub const SCHEMA_VERSION: i32 = MIGRATIONS[MIGRATIONS.len() - 1].0;

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

    // Post-migration idempotent defaults: the durable device identity
    // (envelope v2, ADR-008). INSERT OR IGNORE = generated exactly once
    // per database, stable forever after; readable inside every write
    // transaction with zero API churn.
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('device_id', ?1)",
        rusqlite::params![uuid::Uuid::now_v7().to_string()],
    )
    .map_err(|e| format!("ensure device_id: {e}"))?;

    Ok(())
}

/// Provenance context for one write transaction (envelope v2, ADR-007).
/// Default = a human-initiated write with no finer origin recorded.
/// `actor: System` is reserved for deterministic execution of
/// human-configured timers (VISION law 6); the LLM is never an actor.
#[derive(Clone, Debug, Default)]
pub struct WriteCtx {
    pub actor: Actor,
    pub origin: Option<String>,
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
    write_event_ctx(pool, WriteCtx::default(), build)
}

/// `write_event` with explicit provenance (envelope v2).
pub fn write_event_ctx<F>(pool: &SqlitePool, ctx: WriteCtx, build: F) -> Result<Event, String>
where
    F: FnOnce(&Transaction<'_>, i64) -> Result<EventDraft, String>,
{
    let events = write_events_ctx(pool, ctx, |tx, ts| build(tx, ts).map(|d| vec![d]))?;
    events
        .into_iter()
        .next()
        .ok_or_else(|| "write_event: builder produced no events".to_string())
}

/// Compound variant: the builder returns a vector of drafts to be
/// appended and applied in order, all inside a single transaction.
/// Every draft shares the same timestamp (one logical moment). If any
/// append or apply fails mid-sequence, the whole transaction rolls
/// back — the load-bearing correctness property for I-07's swap_move.
///
/// `swap_move` is the first caller; future cascading reorgs (v2+)
/// will build on this same primitive. Opening a raw transaction
/// outside of here is a bug: the wrapper is the only place that
/// guarantees append+apply stays coupled on every event.
pub fn write_events<F>(pool: &SqlitePool, build: F) -> Result<Vec<Event>, String>
where
    F: FnOnce(&Transaction<'_>, i64) -> Result<Vec<EventDraft>, String>,
{
    write_events_ctx(pool, WriteCtx::default(), build)
}

/// `write_events` with explicit provenance. This is the real body: one
/// transaction, one shared `ts`, one shared `txn_id` (THE transaction
/// boundary undo groups by), the device identity from `meta`, and the
/// hash chain threaded draft-to-draft — all under the same isolation
/// as the append + apply.
pub fn write_events_ctx<F>(pool: &SqlitePool, ctx: WriteCtx, build: F) -> Result<Vec<Event>, String>
where
    F: FnOnce(&Transaction<'_>, i64) -> Result<Vec<EventDraft>, String>,
{
    let mut conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("begin tx: {e}"))?;
    let ts = unix_ms_now();
    let drafts = build(&tx, ts)?;

    let txn_id = uuid::Uuid::now_v7().to_string();
    let device_id: Option<String> = tx
        .query_row("SELECT value FROM meta WHERE key = 'device_id'", [], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| format!("read device_id: {e}"))?;
    // Chain tail read inside this tx: the previous row cannot change
    // under us (SQLite write transactions are exclusive; the 002
    // triggers forbid UPDATE/DELETE besides).
    let mut prev_hash =
        events::last_event_hash(&tx)?.unwrap_or_else(|| events::GENESIS_HASH.to_string());

    let mut out = Vec::with_capacity(drafts.len());
    for EventDraft {
        event_type,
        item_id,
        payload,
    } in drafts
    {
        // Serialize once; the same bytes are stored AND hashed.
        let payload_json =
            serde_json::to_string(&payload).map_err(|e| format!("serialize payload: {e}"))?;
        let stamp = events::EnvelopeStamp {
            txn_id: &txn_id,
            actor: ctx.actor,
            origin: ctx.origin.as_deref(),
            device_id: device_id.as_deref(),
            schema_ver: events::ENVELOPE_SCHEMA_VER,
            prev_hash: &prev_hash,
        };
        let id = events::append_event(&tx, ts, event_type, item_id.as_deref(), &payload_json, &stamp)?;
        let event = Event {
            id,
            ts,
            event_type,
            item_id,
            payload,
            txn_id: Some(txn_id.clone()),
            actor: Some(ctx.actor),
            origin: ctx.origin.clone(),
            device_id: device_id.clone(),
            schema_ver: Some(events::ENVELOPE_SCHEMA_VER),
            prev_hash: Some(prev_hash.clone()),
        };
        items::apply_event_to_projection(&tx, &event)?;
        prev_hash = events::event_row_hash(
            id,
            ts,
            event_type.as_sql(),
            event.item_id.as_deref(),
            &payload_json,
            Some(&txn_id),
            Some(ctx.actor.as_sql()),
            ctx.origin.as_deref(),
            device_id.as_deref(),
            Some(events::ENVELOPE_SCHEMA_VER),
            Some(&prev_hash),
        );
        out.push(event);
    }
    tx.commit().map_err(|e| format!("commit tx: {e}"))?;
    Ok(out)
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
        // 001 schema; 002 DB-enforced invariants; 003 event envelope
        // v2 + meta; 004 items.recurrence (I-21); 005 execution core
        // (first_step + today_on). SCHEMA_VERSION derives from
        // MIGRATIONS so this test tracks new migrations automatically.
        assert_eq!(v, SCHEMA_VERSION);
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
        assert_eq!(v, SCHEMA_VERSION);
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
                "idx_events_txn".to_string(),
                "idx_items_tier_rank".to_string(),
                "idx_items_today".to_string(),
                "items".to_string(),
                "meta".to_string(),
            ]
        );
    }

    // ── Append-only trigger enforcement (migration 002) ────────────
    //
    // These tests prove the `events` append-only trigger actually
    // blocks UPDATE and DELETE at the storage layer. The trigger is
    // the mechanical guarantee behind CLAUDE.md's "events is append-
    // only" doctrine — without these tests, a future code path (or a
    // hand-rolled write) could silently violate the invariant and the
    // only signal would be a runtime ABORT. With these tests, the
    // trigger's presence and behavior are pinned.

    #[test]
    fn events_append_only_trigger_blocks_update() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        // Seed one event via the legitimate INSERT path.
        let _ = write_event(&pool, |_tx, _ts| {
            Ok(EventDraft {
                event_type: EventType::ItemCreated,
                item_id: Some("itm-1".into()),
                payload: serde_json::json!({
                    "content": "hello", "tier": "inbox", "rank": "m",
                    "start_at": null, "due_at": null,
                }),
            })
        })
        .unwrap();

        // Direct UPDATE on events must ABORT with the doctrine message.
        let conn = pool.get().unwrap();
        let err = conn
            .execute("UPDATE events SET type = 'ITEM_EDITED' WHERE id = 1", [])
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("append-only"),
            "UPDATE on events must be blocked by the trigger; got: {msg}"
        );

        // The event row must be unchanged.
        let type_str: String = conn
            .query_row("SELECT type FROM events WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(type_str, "ITEM_CREATED", "blocked UPDATE must not mutate the row");
    }

    #[test]
    fn events_append_only_trigger_blocks_delete() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        let _ = write_event(&pool, |_tx, _ts| {
            Ok(EventDraft {
                event_type: EventType::ItemCreated,
                item_id: Some("itm-1".into()),
                payload: serde_json::json!({
                    "content": "hello", "tier": "inbox", "rank": "m",
                    "start_at": null, "due_at": null,
                }),
            })
        })
        .unwrap();

        let conn = pool.get().unwrap();
        let err = conn
            .execute("DELETE FROM events WHERE id = 1", [])
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("append-only"),
            "DELETE on events must be blocked by the trigger; got: {msg}"
        );

        // The event row must still exist.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "blocked DELETE must not remove the row");
    }

    #[test]
    fn events_append_only_trigger_allows_insert() {
        // The trigger must NOT block the legitimate INSERT path — only
        // UPDATE and DELETE. This confirms the trigger doesn't over-fire
        // and break the only legal write path (db::write_events).
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        for i in 0..5 {
            let _ = write_event(&pool, |_tx, _ts| {
                Ok(EventDraft {
                    event_type: EventType::ItemCreated,
                    item_id: Some(format!("itm-{i}")),
                    payload: serde_json::json!({
                        "content": format!("item {i}"), "tier": "inbox", "rank": "m",
                        "start_at": null, "due_at": null,
                    }),
                })
            })
            .unwrap();
        }
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 5, "INSERT path must work normally; trigger only blocks UPDATE/DELETE");
    }

    #[test]
    fn items_check_constraints_reject_invalid_rows() {
        // The CHECK constraints added in migration 002 must reject
        // rows that violate the invariants, even if a buggy handler
        // tried to write them. We can't easily drive these through
        // the Rust handlers (they validate upstream), so test the
        // constraints directly via raw SQL.
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();

        // deleted must be 0 or 1.
        let err = conn
            .execute(
                "INSERT INTO items (id, content, tier, rank, state, created_at, updated_at, deleted) \
                 VALUES ('x', 'c', 'inbox', 'm', 'active', 0, 0, 2)",
                [],
            )
            .unwrap_err();
        assert!(format!("{err}").contains("CHECK") || format!("{err}").contains("constraint"));

        // content must be 1..=4096 chars.
        let err = conn
            .execute(
                "INSERT INTO items (id, content, tier, rank, state, created_at, updated_at) \
                 VALUES ('x', '', 'inbox', 'm', 'active', 0, 0)",
                [],
            )
            .unwrap_err();
        assert!(format!("{err}").contains("CHECK") || format!("{err}").contains("constraint"));

        // rank must be non-empty.
        let err = conn
            .execute(
                "INSERT INTO items (id, content, tier, rank, state, created_at, updated_at) \
                 VALUES ('x', 'c', 'inbox', '', 'active', 0, 0)",
                [],
            )
            .unwrap_err();
        assert!(format!("{err}").contains("CHECK") || format!("{err}").contains("constraint"));

        // state='blocked' requires blocked_reason NOT NULL.
        let err = conn
            .execute(
                "INSERT INTO items (id, content, tier, rank, state, blocked_reason, created_at, updated_at) \
                 VALUES ('x', 'c', 'inbox', 'm', 'blocked', NULL, 0, 0)",
                [],
            )
            .unwrap_err();
        assert!(format!("{err}").contains("CHECK") || format!("{err}").contains("constraint"));

        // A valid row must insert cleanly.
        conn.execute(
            "INSERT INTO items (id, content, tier, rank, state, blocked_reason, created_at, updated_at) \
             VALUES ('ok', 'c', 'inbox', 'm', 'active', NULL, 0, 0)",
            [],
        )
        .expect("valid row must insert");
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
    fn write_events_rolls_back_when_any_apply_fails() {
        // The load-bearing correctness property behind swap_move: a
        // partial compound write must leave zero state on disk.
        // Drive the failure by staging a second event whose apply
        // handler rejects — here, an ITEM_MOVED targeting a
        // non-existent item_id (handler checks that exactly 1 row
        // was updated and errors otherwise).
        use serde_json::json;

        let pool = mem_pool();
        run_migrations(&pool).unwrap();

        // Seed a real item via a clean write_event so the projection has
        // something to "roll back to" after the failed compound.
        let created = write_event(&pool, |_tx, _ts| {
            Ok(EventDraft {
                event_type: EventType::ItemCreated,
                item_id: Some("seed-id".into()),
                payload: json!({
                    "content": "seed",
                    "tier": "inbox",
                    "rank": "m",
                    "start_at": null,
                    "due_at": null,
                }),
            })
        })
        .unwrap();
        assert_eq!(created.event_type, EventType::ItemCreated);

        // Two-draft compound: first draft is a valid ITEM_MOVED for our
        // seed; second draft targets a non-existent id and the
        // apply_item_moved handler errors because UPDATE matched zero
        // rows. Both drafts must roll back.
        let result = write_events(&pool, |_tx, _ts| {
            Ok(vec![
                EventDraft {
                    event_type: EventType::ItemMoved,
                    item_id: Some("seed-id".into()),
                    payload: json!({
                        "tier_before": "inbox",
                        "rank_before": "m",
                        "tier_after": "A",
                        "rank_after": "z",
                        "reason": null,
                    }),
                },
                EventDraft {
                    event_type: EventType::ItemMoved,
                    item_id: Some("does-not-exist".into()),
                    payload: json!({
                        "tier_before": "inbox",
                        "rank_before": "q",
                        "tier_after": "C",
                        "rank_after": "q",
                        "reason": null,
                    }),
                },
            ])
        });
        assert!(result.is_err(), "second draft must fail — missing target row");

        let conn = pool.get().unwrap();
        let event_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            event_count, 1,
            "only the pre-existing ITEM_CREATED must remain; the compound must roll back"
        );
        let tier: String = conn
            .query_row(
                "SELECT tier FROM items WHERE id = 'seed-id'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tier, "inbox", "projection must be untouched by the failed compound");
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
                    txn_id: None,
                    actor: None,
                    origin: None,
                    device_id: None,
                    schema_ver: None,
                    prev_hash: None,
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

    // ── Property test (non-LLM oracle for write_events rollback) ────
    //
    // The existing `write_events_rolls_back_when_any_apply_fails` test
    // pins one scenario (failing event at position 1 of 2). This
    // property test generalizes: for ANY position of the failing
    // event in a multi-event batch (first, middle, last), the entire
    // batch rolls back — zero events appended, zero projection
    // changes. This is the atomicity guarantee swap_move depends on.

    use proptest::prelude::*;

    #[test]
    fn prop_write_events_rolls_back_for_any_failing_position() {
        use serde_json::json;
        proptest!(|(batch_size in 2u32..5, fail_pos in 0u32..5)| {
            // fail_pos must be within the batch; if not, skip (proptest
            // will regenerate).
            prop_assume!(fail_pos < batch_size);

            let pool = mem_pool();
            run_migrations(&pool).unwrap();

            // Seed one real item so the projection has a "before" state
            // to verify the rollback preserves.
            let _seed = write_event(&pool, |_tx, _ts| {
                Ok(EventDraft {
                    event_type: EventType::ItemCreated,
                    item_id: Some("seed-id".into()),
                    payload: json!({
                        "content": "seed",
                        "tier": "inbox",
                        "rank": "m",
                        "start_at": null,
                        "due_at": null,
                    }),
                })
            })
            .unwrap();

            let events_before: i64 = pool.get().unwrap()
                .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
                .unwrap();
            let items_before = snapshot_items(&pool);

            // Build a batch of `batch_size` ITEM_MOVED drafts, where the
            // draft at `fail_pos` targets a non-existent id (apply_item_moved
            // errors because UPDATE matched zero rows). All others target
            // the seed item (which would succeed in isolation).
            let result = write_events(&pool, |_tx, _ts| {
                let mut drafts = Vec::new();
                for i in 0..batch_size {
                    let (item_id, rank_after) = if i == fail_pos {
                        ("does-not-exist".to_string(), "q".to_string())
                    } else {
                        ("seed-id".to_string(), format!("z{i}"))
                    };
                    drafts.push(EventDraft {
                        event_type: EventType::ItemMoved,
                        item_id: Some(item_id),
                        payload: json!({
                            "tier_before": "inbox",
                            "rank_before": "m",
                            "tier_after": "A",
                            "rank_after": rank_after,
                            "reason": null,
                        }),
                    });
                }
                Ok(drafts)
            });

            // The batch must error (because fail_pos targets a missing id).
            prop_assert!(result.is_err(),
                "write_events must error when any draft's apply fails");

            // Atomicity: zero new events, projection unchanged.
            let events_after: i64 = pool.get().unwrap()
                .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
                .unwrap();
            prop_assert_eq!(events_after, events_before,
                "failed write_events must roll back ALL event appends (atomicity)");
            let items_after = snapshot_items(&pool);
            prop_assert_eq!(items_after, items_before,
                "failed write_events must leave projection untouched (atomicity)");
        });
    }

    // ── Envelope v2 (migration 003) ─────────────────────────────────

    fn create_draft(i: usize) -> EventDraft {
        EventDraft {
            event_type: EventType::ItemCreated,
            item_id: Some(format!("env-itm-{i}")),
            payload: serde_json::json!({
                "content": format!("item {i}"), "tier": "inbox", "rank": format!("m{i}"),
                "start_at": null, "due_at": null,
            }),
        }
    }

    #[test]
    fn envelope_stamped_on_every_write() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        let ev = write_event(&pool, |_tx, _ts| Ok(create_draft(0))).unwrap();

        // The returned Event carries the envelope.
        assert!(ev.txn_id.is_some());
        assert_eq!(ev.actor, Some(Actor::Human));
        assert_eq!(ev.origin, None);
        assert_eq!(ev.schema_ver, Some(events::ENVELOPE_SCHEMA_VER));
        assert_eq!(ev.prev_hash.as_deref(), Some(events::GENESIS_HASH));

        // And the stored row matches, device_id = meta.device_id.
        let conn = pool.get().unwrap();
        let (txn_id, actor, origin, device_id, schema_ver, prev_hash): (
            Option<String>, Option<String>, Option<String>,
            Option<String>, Option<i64>, Option<String>,
        ) = conn
            .query_row(
                "SELECT txn_id, actor, origin, device_id, schema_ver, prev_hash \
                 FROM events WHERE id = ?1",
                [ev.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap();
        assert_eq!(txn_id, ev.txn_id);
        assert_eq!(actor.as_deref(), Some("human"));
        assert_eq!(origin, None);
        let meta_device: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'device_id'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(device_id.as_deref(), Some(meta_device.as_str()));
        assert_eq!(schema_ver, Some(events::ENVELOPE_SCHEMA_VER));
        assert_eq!(prev_hash.as_deref(), Some(events::GENESIS_HASH));
    }

    #[test]
    fn batch_shares_txn_id_and_chain_threads_between_writes() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        let first = write_event(&pool, |_tx, _ts| Ok(create_draft(0))).unwrap();
        let batch = write_events(&pool, |_tx, _ts| Ok(vec![create_draft(1), create_draft(2)])).unwrap();

        assert_eq!(batch.len(), 2);
        // One txn_id per write_events call, shared inside, distinct across.
        assert_eq!(batch[0].txn_id, batch[1].txn_id);
        assert_ne!(batch[0].txn_id, first.txn_id);

        // Chain: batch[0] chains from first's row hash; batch[1] from batch[0]'s.
        // verify_event_chain recomputes independently and must agree.
        let conn = pool.get().unwrap();
        let report = events::verify_event_chain(&conn).unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.enveloped, 3);
        assert_ne!(batch[0].prev_hash.as_deref(), Some(events::GENESIS_HASH));
        assert_ne!(batch[0].prev_hash, batch[1].prev_hash);
    }

    #[test]
    fn chain_detects_tampering_via_raw_insert() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        let _ = write_event(&pool, |_tx, _ts| Ok(create_draft(0))).unwrap();

        // The append-only triggers block UPDATE/DELETE, but a raw INSERT
        // that bypasses db::write_events is still possible at the SQL
        // layer. The chain catches it: a bogus prev_hash breaks verify.
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO events (ts, type, item_id, payload, txn_id, actor, origin, device_id, schema_ver, prev_hash) \
             VALUES (0, 'ITEM_DELETED', 'env-itm-0', '{\"soft\":true}', 'rogue-txn', 'human', NULL, NULL, 1, 'deadbeef')",
            [],
        )
        .unwrap();
        let err = events::verify_event_chain(&conn).unwrap_err();
        assert!(
            err.contains("CHAIN_BROKEN"),
            "bogus prev_hash must break the chain, got: {err}"
        );
    }

    #[test]
    fn legacy_pre_envelope_rows_upgrade_and_chain_extends_over_them() {
        // Simulate a v2 database (pre-envelope): apply 001+002 only,
        // insert a legacy 4-column event row, then run the full
        // migration set and keep writing. The chain must tolerate the
        // legacy head and extend from the hash of the last legacy row.
        let pool = mem_pool();
        {
            let conn = pool.get().unwrap();
            conn.execute_batch(MIGRATIONS[0].1).unwrap();
            conn.execute_batch(MIGRATIONS[1].1).unwrap();
            conn.execute_batch("PRAGMA user_version = 2;").unwrap();
            conn.execute(
                "INSERT INTO events (ts, type, item_id, payload) VALUES \
                 (1, 'ITEM_CREATED', 'legacy-1', '{\"content\":\"old\",\"tier\":\"inbox\",\"rank\":\"m\",\"start_at\":null,\"due_at\":null}')",
                [],
            )
            .unwrap();
        }
        run_migrations(&pool).unwrap();
        {
            let conn = pool.get().unwrap();
            let v: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
            assert_eq!(v, SCHEMA_VERSION);
            let report = events::verify_event_chain(&conn).unwrap();
            assert_eq!((report.total, report.enveloped), (1, 0));
        }

        let ev = write_event(&pool, |_tx, _ts| Ok(create_draft(9))).unwrap();
        // Chains from the legacy row's recomputed hash, NOT genesis.
        assert!(ev.prev_hash.is_some());
        assert_ne!(ev.prev_hash.as_deref(), Some(events::GENESIS_HASH));

        let conn = pool.get().unwrap();
        let report = events::verify_event_chain(&conn).unwrap();
        assert_eq!((report.total, report.enveloped), (2, 1));
    }

    #[test]
    fn device_id_is_stable_across_migration_runs() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        let first: String = pool
            .get()
            .unwrap()
            .query_row("SELECT value FROM meta WHERE key = 'device_id'", [], |r| r.get(0))
            .unwrap();
        run_migrations(&pool).unwrap();
        let second: String = pool
            .get()
            .unwrap()
            .query_row("SELECT value FROM meta WHERE key = 'device_id'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(first, second, "INSERT OR IGNORE must never regenerate the id");
    }

    // Property: the hash chain verifies for ANY sequence of writes —
    // single and batched, interleaved. The non-LLM oracle for the
    // envelope's integrity claim.
    #[test]
    fn prop_chain_verifies_for_any_write_sequence() {
        proptest!(|(batch_sizes in proptest::collection::vec(1usize..4, 1..6))| {
            let pool = mem_pool();
            run_migrations(&pool).unwrap();
            let mut n = 0usize;
            for size in &batch_sizes {
                let drafts: Vec<EventDraft> = (0..*size).map(|_| { n += 1; create_draft(n) }).collect();
                write_events(&pool, |_tx, _ts| Ok(drafts)).unwrap();
            }
            let conn = pool.get().unwrap();
            let report = events::verify_event_chain(&conn).unwrap();
            prop_assert_eq!(report.total as usize, batch_sizes.iter().sum::<usize>());
            prop_assert_eq!(report.enveloped, report.total);
        });
    }
}
