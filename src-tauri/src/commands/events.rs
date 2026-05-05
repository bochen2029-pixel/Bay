//! Read-side commands: event-log introspection, time-travel replay,
//! projection rebuild. None of these mutate state (they only write to
//! the projection via replay, which is idempotent against the log).
//!
//! Introduced in I-10. I-11 will surface rebuild_projection and an
//! event-log export in Settings; for I-10 these are dev/inspector
//! surfaces only.

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::Serialize;
use tauri::State;

use crate::db::{self, SqlitePool};
use crate::domain::{Event, EventType};

// ── get_events ────────────────────────────────────────────────────

#[tauri::command]
pub fn get_events(
    pool: State<'_, SqlitePool>,
    item_id: Option<String>,
    since_ts: Option<i64>,
    until_ts: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<Event>, String> {
    get_events_inner(&pool, item_id, since_ts, until_ts, limit)
}

pub fn get_events_inner(
    pool: &SqlitePool,
    item_id: Option<String>,
    since_ts: Option<i64>,
    until_ts: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<Event>, String> {
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    // NULL-guarded params keep the prepared statement stable while
    // letting any filter combination light up. SQLite short-circuits
    // on NULL IS NULL so the unfiltered path is cheap.
    let mut stmt = conn
        .prepare(
            "SELECT id, ts, type, item_id, payload FROM events \
             WHERE (?1 IS NULL OR item_id = ?1) \
               AND (?2 IS NULL OR ts >= ?2) \
               AND (?3 IS NULL OR ts <= ?3) \
             ORDER BY id \
             LIMIT COALESCE(?4, -1)",
        )
        .map_err(|e| format!("prepare get_events: {e}"))?;
    let rows = stmt
        .query_map(params![item_id, since_ts, until_ts, limit], parse_event_row)
        .map_err(|e| format!("query_map events: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("event row: {e}"))?);
    }
    Ok(out)
}

// ── get_items_at ──────────────────────────────────────────────────

#[tauri::command]
pub fn get_items_at(
    pool: State<'_, SqlitePool>,
    ts: i64,
) -> Result<Vec<crate::domain::Item>, String> {
    get_items_at_inner(&pool, ts)
}

pub fn get_items_at_inner(
    pool: &SqlitePool,
    ts: i64,
) -> Result<Vec<crate::domain::Item>, String> {
    if ts < 0 {
        return Err("TS_BEFORE_EPOCH".into());
    }

    // Read all events up to ts from the durable DB.
    let events: Vec<Event> = {
        let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, ts, type, item_id, payload FROM events \
                 WHERE ts <= ?1 ORDER BY id",
            )
            .map_err(|e| format!("prepare events for replay: {e}"))?;
        let rows = stmt
            .query_map(params![ts], parse_event_row)
            .map_err(|e| format!("query events for replay: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect events: {e}"))?
    };

    // Spin up an ephemeral in-memory SQLite, migrate, and replay.
    // Fresh pool per call — time-travel is interactive but infrequent;
    // a dedicated mem DB keeps the durable DB untouched.
    let mem_manager = SqliteConnectionManager::memory();
    let mem_pool: SqlitePool = Pool::builder()
        .max_size(1)
        .build(mem_manager)
        .map_err(|e| format!("build mem pool: {e}"))?;
    db::run_migrations(&mem_pool)?;

    // Replay in one transaction so partial failure doesn't leave a
    // half-built projection visible to the read below.
    {
        let mut conn = mem_pool.get().map_err(|e| format!("mem pool get: {e}"))?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("begin mem tx: {e}"))?;
        for event in &events {
            db::items::apply_event_to_projection(&tx, event)?;
        }
        tx.commit().map_err(|e| format!("commit mem tx: {e}"))?;
    }

    let conn = mem_pool
        .get()
        .map_err(|e| format!("mem pool get (read): {e}"))?;
    db::items::list_active_items(&conn)
}

// ── list_archived_items ───────────────────────────────────────────

/// Read all soft-deleted items, most-recently-deleted first. Backs
/// the Archive view in v1.1; restoring an item from this list calls
/// the existing `restore_item` command.
#[tauri::command]
pub fn list_archived_items(
    pool: State<'_, SqlitePool>,
) -> Result<Vec<crate::domain::Item>, String> {
    list_archived_items_inner(&pool)
}

pub fn list_archived_items_inner(
    pool: &SqlitePool,
) -> Result<Vec<crate::domain::Item>, String> {
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    db::items::list_deleted_items(&conn)
}

// ── rebuild_projection ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RebuildResult {
    pub items_affected: i64,
}

#[tauri::command]
pub fn rebuild_projection(
    pool: State<'_, SqlitePool>,
) -> Result<RebuildResult, String> {
    rebuild_projection_inner(&pool)
}

pub fn rebuild_projection_inner(pool: &SqlitePool) -> Result<RebuildResult, String> {
    let mut conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("begin rebuild tx: {e}"))?;

    // Wipe the projection; event log is untouched. Rebuild replays
    // every event in id order.
    tx.execute("DELETE FROM items", [])
        .map_err(|e| format!("truncate items: {e}"))?;

    let events: Vec<Event> = {
        let mut stmt = tx
            .prepare("SELECT id, ts, type, item_id, payload FROM events ORDER BY id")
            .map_err(|e| format!("prepare rebuild events: {e}"))?;
        let rows = stmt
            .query_map([], parse_event_row)
            .map_err(|e| format!("query rebuild events: {e}"))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| format!("collect rebuild events: {e}"))?
    };
    for event in &events {
        db::items::apply_event_to_projection(&tx, event)?;
    }

    let items_affected: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM items WHERE deleted = 0",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("count post-rebuild: {e}"))?;

    tx.commit().map_err(|e| format!("commit rebuild tx: {e}"))?;
    Ok(RebuildResult { items_affected })
}

// ── helper ────────────────────────────────────────────────────────

fn parse_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let type_str: String = row.get(2)?;
    let payload_str: String = row.get(4)?;
    let event_type = EventType::from_sql(&type_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            format!("invalid event type {type_str:?}").into(),
        )
    })?;
    let payload: serde_json::Value = serde_json::from_str(&payload_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    Ok(Event {
        id: row.get(0)?,
        ts: row.get(1)?,
        event_type,
        item_id: row.get(3)?,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::{
        create_item_inner, delete_item_inner, edit_item_inner, move_item_inner,
        restore_item_inner, set_item_state_inner,
    };
    use crate::domain::{ItemState, Tier};
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    fn fresh_pool() -> SqlitePool {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        db::run_migrations(&pool).unwrap();
        pool
    }

    #[test]
    fn get_events_unfiltered_returns_all_in_order() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "a".into(), None, None).unwrap();
        edit_item_inner(&pool, a.id.clone(), "a2".into()).unwrap();
        let events = get_events_inner(&pool, None, None, None, None).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, EventType::ItemCreated);
        assert_eq!(events[1].event_type, EventType::ItemEdited);
        assert!(events[0].id < events[1].id);
    }

    #[test]
    fn get_events_item_id_filter() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "a".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::B, "b".into(), None, None).unwrap();
        edit_item_inner(&pool, a.id.clone(), "a2".into()).unwrap();
        let only_a = get_events_inner(&pool, Some(a.id.clone()), None, None, None).unwrap();
        assert_eq!(only_a.len(), 2);
        assert!(only_a.iter().all(|e| e.item_id.as_deref() == Some(a.id.as_str())));
        let only_b = get_events_inner(&pool, Some(b.id.clone()), None, None, None).unwrap();
        assert_eq!(only_b.len(), 1);
    }

    #[test]
    fn get_events_ts_and_limit_filters() {
        let pool = fresh_pool();
        let _a = create_item_inner(&pool, Tier::Inbox, "a".into(), None, None).unwrap();
        let _b = create_item_inner(&pool, Tier::Inbox, "b".into(), None, None).unwrap();
        let _c = create_item_inner(&pool, Tier::Inbox, "c".into(), None, None).unwrap();
        let limited = get_events_inner(&pool, None, None, None, Some(2)).unwrap();
        assert_eq!(limited.len(), 2);

        // since_ts far in the future → empty.
        let future = get_events_inner(&pool, None, Some(i64::MAX), None, None).unwrap();
        assert!(future.is_empty());
    }

    #[test]
    fn get_items_at_now_matches_live_projection() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "alpha".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::B, "beta".into(), None, None).unwrap();
        set_item_state_inner(
            &pool,
            b.id.clone(),
            ItemState::Blocked,
            Some("pending".into()),
        )
        .unwrap();

        let at_now = get_items_at_inner(&pool, i64::MAX).unwrap();
        assert_eq!(at_now.len(), 2);
        let a_out = at_now.iter().find(|i| i.id == a.id).unwrap();
        let b_out = at_now.iter().find(|i| i.id == b.id).unwrap();
        assert_eq!(a_out.state, ItemState::Active);
        assert_eq!(b_out.state, ItemState::Blocked);
    }

    #[test]
    fn get_items_at_epoch_is_empty() {
        let pool = fresh_pool();
        let _a = create_item_inner(&pool, Tier::A, "alpha".into(), None, None).unwrap();
        let items = get_items_at_inner(&pool, 0).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn get_items_at_rejects_negative_ts() {
        let pool = fresh_pool();
        let err = get_items_at_inner(&pool, -1).unwrap_err();
        assert_eq!(err, "TS_BEFORE_EPOCH");
    }

    #[test]
    fn get_items_at_replays_moves_and_states() {
        // Create → move → block. get_items_at with no upper bound must
        // show the post-block state.
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "x".into(), None, None).unwrap();
        move_item_inner(
            &pool,
            item.id.clone(),
            Tier::A,
            Some("z".into()),
            None,
        )
        .unwrap();
        set_item_state_inner(
            &pool,
            item.id.clone(),
            ItemState::Blocked,
            Some("stuck".into()),
        )
        .unwrap();

        let items = get_items_at_inner(&pool, i64::MAX).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].tier, Tier::A);
        assert_eq!(items[0].state, ItemState::Blocked);
        assert_eq!(items[0].blocked_reason.as_deref(), Some("stuck"));
    }

    #[test]
    fn rebuild_projection_is_idempotent() {
        let pool = fresh_pool();
        create_item_inner(&pool, Tier::A, "alpha".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::B, "beta".into(), None, None).unwrap();

        let before = snapshot(&pool);

        let r1 = rebuild_projection_inner(&pool).unwrap();
        let after1 = snapshot(&pool);
        assert_eq!(r1.items_affected, 2);
        assert_eq!(before, after1);

        let r2 = rebuild_projection_inner(&pool).unwrap();
        let after2 = snapshot(&pool);
        assert_eq!(r2.items_affected, 2);
        assert_eq!(after1, after2);
    }

    #[test]
    fn list_archived_items_returns_only_deleted_sorted_by_recency() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::Inbox, "first".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::A, "second".into(), None, None).unwrap();
        let _c = create_item_inner(&pool, Tier::B, "still alive".into(), None, None).unwrap();

        // Delete in order a then b — b should rank first in the
        // archive listing (sorted by updated_at DESC).
        delete_item_inner(&pool, &a.id).unwrap();
        // Bump ts so the sort order is unambiguous on fast machines
        // where consecutive write_event() calls can land in the same
        // millisecond.
        std::thread::sleep(std::time::Duration::from_millis(2));
        delete_item_inner(&pool, &b.id).unwrap();

        let archived = list_archived_items_inner(&pool).unwrap();
        assert_eq!(archived.len(), 2, "alive item must be excluded");
        assert_eq!(archived[0].id, b.id, "most-recently-deleted first");
        assert_eq!(archived[1].id, a.id);
        assert!(archived.iter().all(|i| i.deleted));
    }

    #[test]
    fn list_archived_items_empty_when_nothing_deleted() {
        let pool = fresh_pool();
        create_item_inner(&pool, Tier::Inbox, "alive".into(), None, None).unwrap();
        let archived = list_archived_items_inner(&pool).unwrap();
        assert!(archived.is_empty());
    }

    #[test]
    fn list_archived_items_excludes_restored() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::Inbox, "x".into(), None, None).unwrap();
        delete_item_inner(&pool, &a.id).unwrap();
        assert_eq!(list_archived_items_inner(&pool).unwrap().len(), 1);
        restore_item_inner(&pool, &a.id).unwrap();
        assert!(list_archived_items_inner(&pool).unwrap().is_empty());
    }

    #[test]
    fn rebuild_projection_reproduces_deletes_and_restores() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "x".into(), None, None).unwrap();
        edit_item_inner(&pool, item.id.clone(), "x2".into()).unwrap();
        delete_item_inner(&pool, &item.id).unwrap();
        restore_item_inner(&pool, &item.id).unwrap();
        let before = snapshot(&pool);

        rebuild_projection_inner(&pool).unwrap();
        let after = snapshot(&pool);
        assert_eq!(before, after);
    }

    fn snapshot(pool: &SqlitePool) -> Vec<(String, String, String, i64)> {
        let conn = pool.get().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, content, tier, deleted FROM items ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    }
}
