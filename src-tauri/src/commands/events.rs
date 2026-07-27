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
use crate::domain::{Actor, Event, EventType};

/// The eleven `events` columns every reader selects (envelope v2,
/// migration 003). Order matches `parse_event_row`.
const EVENT_COLS: &str =
    "id, ts, type, item_id, payload, txn_id, actor, origin, device_id, schema_ver, prev_hash";

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
        .prepare(&format!(
            "SELECT {EVENT_COLS} FROM events \
             WHERE (?1 IS NULL OR item_id = ?1) \
               AND (?2 IS NULL OR ts >= ?2) \
               AND (?3 IS NULL OR ts <= ?3) \
             ORDER BY id \
             LIMIT COALESCE(?4, -1)"
        ))
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
            .prepare(&format!(
                "SELECT {EVENT_COLS} FROM events WHERE ts <= ?1 ORDER BY id"
            ))
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

// ── search_events (I-18) ──────────────────────────────────────────
//
// Full-text search across the event log. Surfaces the event log as a
// first-class product surface (per CLAUDE.md "Event log is the product"
// — undo, time-travel, and analysis are all queries against the log;
// search is the fourth).
//
// v1: pure-Rust filter. The query is a case-insensitive substring match
// against the event's payload JSON (which includes content, reasons,
// before/after values — the human-meaningful text). Optional filters:
// event_type (e.g. "ITEM_MOVED"), item_id (restrict to one item's
// history), since_ts/until_ts (date range), limit.
//
// FTS5 virtual table is a heavier migration (would need a trigger to
// keep the FTS index in sync with events). Deferred per ADR; the
// pure-Rust filter is O(n) over the event log, which is fine for
// single-user local-first usage (event logs in the thousands, not
// millions). Revisit FTS5 if search becomes a perf concern.

#[derive(Debug, Clone, Serialize)]
pub struct SearchEventsParams {
    /// Case-insensitive substring match against the payload JSON.
    /// None or empty = no text filter (return all matching the other
    /// filters).
    pub query: Option<String>,
    /// Filter by event type (e.g. "ITEM_MOVED"). None = all types.
    pub event_type: Option<String>,
    /// Restrict to one item's history. None = all items.
    pub item_id: Option<String>,
    pub since_ts: Option<i64>,
    pub until_ts: Option<i64>,
    pub limit: Option<i64>,
}

#[tauri::command]
pub fn search_events(
    pool: State<'_, SqlitePool>,
    query: Option<String>,
    event_type: Option<String>,
    item_id: Option<String>,
    since_ts: Option<i64>,
    until_ts: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<Event>, String> {
    search_events_inner(
        &pool,
        SearchEventsParams {
            query,
            event_type,
            item_id,
            since_ts,
            until_ts,
            limit,
        },
    )
}

pub fn search_events_inner(
    pool: &SqlitePool,
    params: SearchEventsParams,
) -> Result<Vec<Event>, String> {
    // Fetch events filtered by item_id/ts (the indexed columns) via the
    // existing get_events_inner, then filter by event_type + query text
    // in Rust. This keeps the SQL simple (uses the existing prepared
    // statement) and the text filter is O(n) over the result set.
    let mut events = get_events_inner(
        pool,
        params.item_id.clone(),
        params.since_ts,
        params.until_ts,
        // Fetch more than the limit so the post-filter set can reach
        // the limit. Cap at 10000 to bound work.
        Some(10000),
    )?;

    // Filter by event_type.
    if let Some(et) = &params.event_type {
        events.retain(|e| e.event_type.as_sql() == et);
    }

    // Filter by query (case-insensitive substring on payload JSON).
    if let Some(q) = &params.query {
        let q_lower = q.to_lowercase();
        if !q_lower.is_empty() {
            events.retain(|e| {
                let payload_str = e.payload.to_string();
                payload_str.to_lowercase().contains(&q_lower)
            });
        }
    }

    // Apply the user's limit (after filtering).
    if let Some(limit) = params.limit {
        if limit >= 0 {
            events.truncate(limit as usize);
        }
    }

    Ok(events)
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
            .prepare(&format!("SELECT {EVENT_COLS} FROM events ORDER BY id"))
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

// ── undo_last_action (I-17) ───────────────────────────────────────
//
// The event-sourcing payoff v1 underdelivered: a full undo over the
// event log. Undo = append a compensating event (or events, for a
// multi-event action like swap) in a single write_events tx. The undo
// is itself an event — it lands in the append-only log, so it's
// auditable and reversible (redo = undo the undo, or re-apply).
//
// "Action" = one write_events transaction. Since migration 003 every
// event carries a txn_id, so the action boundary is EXACT: the most
// recent human-actor non-LLM event names the transaction, and every
// event sharing its txn_id is undone together (swap = 2 ITEM_MOVED;
// batch = N same-type; an accepted re-org = N item events + the
// acceptance audit row; I-21's recurrence completion = a mixed-type
// trio). Legacy rows (txn_id NULL, pre-003) fall back to the (ts,type)
// heuristic I-17 shipped with (QUESTIONS Q01 — closed by this change).
//
// System-actor transactions (deterministic executions of a human-set
// timer, VISION law 6 — e.g. the Today day-roll) are never targeted by
// Ctrl+Z: undo looks past them to the most recent HUMAN transaction.
//
// Compensation logic per event type:
//   ITEM_CREATED     -> ITEM_DELETED (soft-delete)
//   ITEM_EDITED      -> ITEM_EDITED (content_after -> content_before)
//   ITEM_MOVED       -> ITEM_MOVED (tier/rank after -> before)
//   ITEM_STATE_CHANGED -> ITEM_STATE_CHANGED (state_after -> before;
//                       blocked_reason restored if before was blocked)
//   ITEM_DATE_SET    -> ITEM_DATE_SET (value_after -> before)
//   ITEM_DELETED     -> ITEM_RESTORED
//   ITEM_RESTORED    -> ITEM_DELETED (re-soft-delete)
//   LLM_SUGGESTION_* -> NOT undoable (advisory only; skip and look
//                       further back). The LLM firewall holds: these
//                       never touched the projection, so there's
//                       nothing to undo.
//
// Cap enforcement: undo only ever reverses the MOST RECENT action, and
// every action's cap effect is reversible into the slot it just touched
// (a delete frees a slot, so undoing it refills exactly that slot; a
// move/activate that was admitted under the cap reverses cleanly). Since
// nothing has happened since the action being undone, the compensating
// events never exceed a cap. (The explicit restore_item command — archive
// / undo-toast — is different: arbitrary time may have passed and the
// tier may have refilled, so THAT path is cap-gated in restore_item_inner.)

#[derive(Debug, Clone, Serialize)]
pub struct UndoResult {
    /// "Undone ITEM_EDITED on item X" — human-readable.
    pub description: String,
    /// The event types that were compensated (one per undone event).
    pub undone_event_types: Vec<String>,
    /// The item ids affected by the compensating events.
    pub affected_item_ids: Vec<String>,
}

#[tauri::command]
pub fn undo_last_action(
    app: tauri::AppHandle,
    pool: State<'_, SqlitePool>,
) -> Result<UndoResult, String> {
    let result = undo_last_action_inner(&pool)?;
    // Emit the appropriate frontend event per affected item so the
    // store's idempotent onItemCreated/onItemUpdated/onItemDeleted
    // handlers refresh the UI. We read the post-undo projection state
    // to decide which event to emit: if the item is now deleted, emit
    // item_deleted; if it was just restored (re-created from deleted),
    // emit item_created; otherwise item_updated.
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    use tauri::Emitter;
    for id in &result.affected_item_ids {
        // Read the item's current projection state (including deleted).
        // Column order: deleted, blocked_reason, content, tier, rank,
        // state, start_at, due_at, created_at, updated_at.
        let row: Option<(i64, Option<String>, String, String, String, String, Option<i64>, Option<i64>, Option<String>, i64, i64)> = conn
            .query_row(
                "SELECT deleted, blocked_reason, content, tier, rank, state, start_at, due_at, recurrence, created_at, updated_at \
                 FROM items WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?)),
            )
            .ok();
        if let Some((deleted, blocked_reason, content, tier, rank, state, start_at, due_at, recurrence, created_at, updated_at)) = row {
            let item = crate::domain::Item {
                id: id.clone(),
                content,
                tier: crate::domain::Tier::from_sql(tier.as_str()).unwrap_or(crate::domain::Tier::Inbox),
                rank,
                state: crate::domain::ItemState::from_sql(state.as_str()).unwrap_or(crate::domain::ItemState::Active),
                blocked_reason,
                start_at,
                due_at,
                recurrence,
                created_at,
                updated_at,
                deleted: deleted != 0,
            };
            // If the item is now deleted, emit item_deleted so the
            // store removes it. Otherwise emit item_updated; the store's
            // onItemUpdated re-inserts into the correct tier (handles
            // restore-into-tier too, since the item moves from deleted
            // to alive with a tier).
            if deleted != 0 {
                let _ = app.emit("item_deleted", serde_json::json!({ "id": id }));
            } else {
                let _ = app.emit("item_updated", &item);
            }
        }
    }
    Ok(result)
}

pub fn undo_last_action_inner(pool: &SqlitePool) -> Result<UndoResult, String> {
    use crate::db::EventDraft;
    use crate::domain::EventType;
    use serde_json::json;

    // Find the most recent HUMAN action and undo it atomically.
    // - LLM_SUGGESTION_* rows are advisory-only (they never touched the
    //   projection); a pure-LLM transaction (analyze / reject) contains
    //   no non-LLM row, so the type filter skips the whole txn.
    // - actor = 'system' rows are deterministic timer executions
    //   (VISION law 6); Ctrl+Z looks past them to the last human txn.
    //   Legacy rows (actor NULL, pre-003) are human by definition.
    let last = {
        let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {EVENT_COLS} FROM events \
                 WHERE type NOT IN ('LLM_SUGGESTION_GENERATED','LLM_SUGGESTION_ACCEPTED','LLM_SUGGESTION_REJECTED') \
                   AND (actor IS NULL OR actor = 'human') \
                 ORDER BY id DESC LIMIT 1"
            ))
            .map_err(|e| format!("prepare last event: {e}"))?;
        let mut rows = stmt
            .query_map([], parse_event_row)
            .map_err(|e| format!("query last event: {e}"))?;
        match rows.next().transpose().map_err(|e| format!("last event row: {e}"))? {
            Some(e) => e,
            None => return Err("NOTHING_TO_UNDO".into()),
        }
    };

    // Gather every event of this action, newest first so the
    // compensating events unwind in LIFO order. Exact txn boundary when
    // the envelope is present (migration 003); the (ts,type) heuristic
    // survives only for legacy pre-envelope rows, pinned to
    // txn_id IS NULL so a legacy row can never co-group with an
    // enveloped one.
    let action_events: Vec<Event> = {
        let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
        match &last.txn_id {
            Some(txn) => {
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT {EVENT_COLS} FROM events WHERE txn_id = ?1 ORDER BY id DESC"
                    ))
                    .map_err(|e| format!("prepare action events: {e}"))?;
                let rows = stmt
                    .query_map(rusqlite::params![txn], parse_event_row)
                    .map_err(|e| format!("query action events: {e}"))?;
                rows.collect::<rusqlite::Result<Vec<Event>>>()
                    .map_err(|e| format!("action event row: {e}"))?
            }
            None => {
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT {EVENT_COLS} FROM events \
                         WHERE ts = ?1 AND type = ?2 AND txn_id IS NULL ORDER BY id DESC"
                    ))
                    .map_err(|e| format!("prepare action events: {e}"))?;
                let rows = stmt
                    .query_map(
                        rusqlite::params![last.ts, last.event_type.as_sql()],
                        parse_event_row,
                    )
                    .map_err(|e| format!("query action events: {e}"))?;
                rows.collect::<rusqlite::Result<Vec<Event>>>()
                    .map_err(|e| format!("action event row: {e}"))?
            }
        }
    };

    // Build compensating drafts in LIFO order (the query above is
    // already newest-first), e.g. a swap's two ITEM_MOVED events unwind
    // correctly. For single-event actions there is just one draft.
    let mut drafts: Vec<EventDraft> = Vec::new();
    let mut undone_types: Vec<String> = Vec::new();
    let mut affected_ids: Vec<String> = Vec::new();
    let mut descriptions: Vec<String> = Vec::new();

    for event in &action_events {
        let item_id = match &event.item_id {
            Some(id) => id.clone(),
            None => continue, // non-item event; skip
        };
        let draft = match event.event_type {
            EventType::ItemCreated => {
                descriptions.push(format!("Undone ITEM_CREATED on item {item_id}"));
                EventDraft {
                    event_type: EventType::ItemDeleted,
                    item_id: Some(item_id.clone()),
                    payload: json!({ "soft": true }),
                }
            }
            EventType::ItemEdited => {
                let content_before = event.payload["content_before"]
                    .as_str()
                    .ok_or_else(|| format!("ITEM_EDITED payload missing content_before: {}", event.id))?;
                let content_after = event.payload["content_after"]
                    .as_str()
                    .ok_or_else(|| format!("ITEM_EDITED payload missing content_after: {}", event.id))?;
                descriptions.push(format!("Undone ITEM_EDITED on item {item_id}"));
                EventDraft {
                    event_type: EventType::ItemEdited,
                    item_id: Some(item_id.clone()),
                    payload: json!({
                        "content_before": content_after,
                        "content_after": content_before,
                    }),
                }
            }
            EventType::ItemMoved => {
                let tier_before = event.payload["tier_before"]
                    .as_str()
                    .ok_or_else(|| format!("ITEM_MOVED payload missing tier_before: {}", event.id))?;
                let rank_before = event.payload["rank_before"]
                    .as_str()
                    .ok_or_else(|| format!("ITEM_MOVED payload missing rank_before: {}", event.id))?;
                let tier_after = event.payload["tier_after"]
                    .as_str()
                    .ok_or_else(|| format!("ITEM_MOVED payload missing tier_after: {}", event.id))?;
                let rank_after = event.payload["rank_after"]
                    .as_str()
                    .ok_or_else(|| format!("ITEM_MOVED payload missing rank_after: {}", event.id))?;
                descriptions.push(format!("Undone ITEM_MOVED on item {item_id}"));
                EventDraft {
                    event_type: EventType::ItemMoved,
                    item_id: Some(item_id.clone()),
                    payload: json!({
                        "tier_before": tier_after,
                        "rank_before": rank_after,
                        "tier_after": tier_before,
                        "rank_after": rank_before,
                        "reason": "undo",
                    }),
                }
            }
            EventType::ItemStateChanged => {
                let state_before = event.payload["state_before"]
                    .as_str()
                    .ok_or_else(|| format!("ITEM_STATE_CHANGED payload missing state_before: {}", event.id))?;
                let state_after = event.payload["state_after"]
                    .as_str()
                    .ok_or_else(|| format!("ITEM_STATE_CHANGED payload missing state_after: {}", event.id))?;
                let blocked_reason = if state_before == "blocked" {
                    event.payload["blocked_reason"].as_str().map(|s| s.to_string())
                } else {
                    None
                };
                descriptions.push(format!("Undone ITEM_STATE_CHANGED on item {item_id}"));
                EventDraft {
                    event_type: EventType::ItemStateChanged,
                    item_id: Some(item_id.clone()),
                    payload: json!({
                        "state_before": state_after,
                        "state_after": state_before,
                        "blocked_reason": blocked_reason,
                    }),
                }
            }
            EventType::ItemDateSet => {
                let field = event.payload["field"]
                    .as_str()
                    .ok_or_else(|| format!("ITEM_DATE_SET payload missing field: {}", event.id))?;
                let value_before = &event.payload["value_before"];
                let value_after = &event.payload["value_after"];
                descriptions.push(format!("Undone ITEM_DATE_SET on item {item_id}"));
                EventDraft {
                    event_type: EventType::ItemDateSet,
                    item_id: Some(item_id.clone()),
                    payload: json!({
                        "field": field,
                        "value_before": value_after,
                        "value_after": value_before,
                    }),
                }
            }
            EventType::ItemDeleted => {
                descriptions.push(format!("Undone ITEM_DELETED on item {item_id}"));
                EventDraft {
                    event_type: EventType::ItemRestored,
                    item_id: Some(item_id.clone()),
                    payload: json!({}),
                }
            }
            EventType::ItemRestored => {
                descriptions.push(format!("Undone ITEM_RESTORED on item {item_id}"));
                EventDraft {
                    event_type: EventType::ItemDeleted,
                    item_id: Some(item_id.clone()),
                    payload: json!({ "soft": true }),
                }
            }
            EventType::ItemRecurrenceSet => {
                let before = event.payload.get("before").cloned().unwrap_or(serde_json::Value::Null);
                let after = event.payload.get("after").cloned().unwrap_or(serde_json::Value::Null);
                descriptions.push(format!("Undone ITEM_RECURRENCE_SET on item {item_id}"));
                EventDraft {
                    event_type: EventType::ItemRecurrenceSet,
                    item_id: Some(item_id.clone()),
                    payload: json!({ "before": after, "after": before }),
                }
            }
            // Audit/link event (I-21): the parent's STATE_CHANGED and
            // the child's CREATED in this same txn carry the actual
            // compensations; the link itself never touched the
            // projection, so there is nothing to compensate.
            EventType::ItemRecurred => continue,
            // LLM events are advisory-only; nothing to undo. This
            // branch is unreachable because the query filtered them
            // out, but the match must be exhaustive.
            EventType::LlmSuggestionGenerated
            | EventType::LlmSuggestionAccepted
            | EventType::LlmSuggestionRejected => continue,
        };
        undone_types.push(event.event_type.as_sql().to_string());
        affected_ids.push(item_id);
        drafts.push(draft);
    }

    if drafts.is_empty() {
        return Err("NOTHING_TO_UNDO".into());
    }

    // drafts is in LIFO order (the action-events query is newest-first).
    // Append all compensating drafts in one atomic tx — the undo of a
    // batch is itself a single atomic (and undoable) action. Its origin
    // names the transaction it reversed (envelope provenance).
    let undo_origin = match &last.txn_id {
        Some(txn) => format!("undo:{txn}"),
        None => format!("undo:event:{}", last.id),
    };
    let _ = db::write_events_ctx(
        pool,
        db::WriteCtx {
            origin: Some(undo_origin),
            ..Default::default()
        },
        |_tx, _ts| Ok(drafts),
    )?;

    Ok(UndoResult {
        description: descriptions.join("; "),
        undone_event_types: undone_types,
        affected_item_ids: affected_ids,
    })
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
    let actor = row
        .get::<_, Option<String>>(6)?
        .map(|s| {
            Actor::from_sql(&s).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    format!("invalid actor {s:?}").into(),
                )
            })
        })
        .transpose()?;
    Ok(Event {
        id: row.get(0)?,
        ts: row.get(1)?,
        event_type,
        item_id: row.get(3)?,
        payload,
        txn_id: row.get(5)?,
        actor,
        origin: row.get(7)?,
        device_id: row.get(8)?,
        schema_ver: row.get(9)?,
        prev_hash: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::{
        batch_delete_inner, batch_set_state_inner, create_item_inner, delete_item_inner,
        edit_item_inner, move_item_inner, restore_item_inner, set_item_state_inner,
        swap_move_inner,
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

    // ── Property tests (non-LLM oracle for projection determinism) ──
    //
    // THE single most important property in the system: for ANY valid
    // event sequence, rebuild_projection (wipe + replay all events)
    // reproduces the items table exactly. If this ever breaks, the
    // event-sourcing invariant is violated — items is no longer a pure
    // projection of events. This is the Externality Principle's
    // mechanical check on apply_event_to_projection + rebuild_projection.
    //
    // The existing tests above pin specific scenarios (creates, moves,
    // states, deletes, restores). This property test generalizes: any
    // random interleaving of valid ops must preserve determinism.

    use proptest::prelude::*;

    /// One operation in a random event sequence. Each variant maps to
    /// a `*_inner` call. The strategy keeps op sequences bounded so
    /// the test stays fast (<60s per property test, per charter).
    #[derive(Debug, Clone)]
    enum Op {
        Create { tier: Tier, content: String },
        Edit { item_idx: usize, content: String },
        Move { item_idx: usize, to_tier: Tier },
        SetState { item_idx: usize, state: ItemState },
        Delete { item_idx: usize },
        Restore { item_idx: usize },
    }

    fn tier_strategy() -> impl Strategy<Value = Tier> {
        prop_oneof![
            Just(Tier::Inbox),
            Just(Tier::A),
            Just(Tier::B),
            Just(Tier::C),
        ]
    }

    fn state_strategy() -> impl Strategy<Value = ItemState> {
        prop_oneof![
            Just(ItemState::Active),
            Just(ItemState::Done),
            // Blocked requires a reason; we always pass a valid one.
            Just(ItemState::Blocked),
        ]
    }

    fn content_strategy() -> impl Strategy<Value = String> {
        "[a-z0-9 ]{1,20}"
    }

    fn op_strategy(num_items: usize) -> impl Strategy<Value = Op> {
        let item_idx = if num_items > 0 { 0..num_items } else { 0..1 };
        prop_oneof![
            (tier_strategy(), content_strategy()).prop_map(|(tier, content)| Op::Create { tier, content }),
            (item_idx.clone(), content_strategy()).prop_map(move |(idx, content)| Op::Edit { item_idx: idx, content }),
            (item_idx.clone(), tier_strategy()).prop_map(move |(idx, to_tier)| Op::Move { item_idx: idx, to_tier }),
            (item_idx.clone(), state_strategy()).prop_map(move |(idx, state)| Op::SetState { item_idx: idx, state }),
            item_idx.clone().prop_map(move |idx| Op::Delete { item_idx: idx }),
            item_idx.prop_map(move |idx| Op::Restore { item_idx: idx }),
        ]
    }

    /// Apply a sequence of ops to a fresh pool, returning the live item ids
    /// (so the caller can correlate). Ops that would error (edit/delete/restore
    /// on a missing index, move into a full tier, etc.) are silently skipped —
    /// the property is about determinism for ops that DID land, not about
    /// covering every op.
    fn apply_ops(pool: &SqlitePool, ops: &[Op]) -> Vec<String> {
        let mut live_ids: Vec<String> = Vec::new();
        let mut deleted_ids: Vec<String> = Vec::new();
        for op in ops {
            match op {
                Op::Create { tier, content } => {
                    if let Ok(item) = create_item_inner(pool, *tier, content.clone(), None, None) {
                        live_ids.push(item.id);
                    }
                }
                Op::Edit { item_idx, content } => {
                    if let Some(id) = live_ids.get(*item_idx) {
                        let _ = edit_item_inner(pool, id.clone(), content.clone());
                    }
                }
                Op::Move { item_idx, to_tier } => {
                    if let Some(id) = live_ids.get(*item_idx) {
                        // move_item_inner needs a to_rank; pass None to get end-of-tier.
                        let _ = move_item_inner(pool, id.clone(), *to_tier, None, None);
                    }
                }
                Op::SetState { item_idx, state } => {
                    if let Some(id) = live_ids.get(*item_idx) {
                        let reason = if *state == ItemState::Blocked {
                            Some("test block reason".to_string())
                        } else {
                            None
                        };
                        let _ = set_item_state_inner(pool, id.clone(), *state, reason);
                    }
                }
                Op::Delete { item_idx } => {
                    if let Some(id) = live_ids.get(*item_idx).cloned() {
                        if delete_item_inner(pool, &id).is_ok() {
                            live_ids.retain(|x| x != &id);
                            deleted_ids.push(id);
                        }
                    }
                }
                Op::Restore { item_idx } => {
                    if let Some(id) = deleted_ids.get(*item_idx).cloned() {
                        if restore_item_inner(pool, &id).is_ok() {
                            live_ids.push(id.clone());
                            deleted_ids.retain(|x| x != &id);
                        }
                    }
                }
            }
        }
        live_ids
    }

    /// Property 1 (the load-bearing one): for any valid event sequence,
    /// rebuild_projection reproduces the items table exactly.
    #[test]
    fn prop_rebuild_reproduces_items_for_any_event_sequence() {
        proptest!(|(ops in prop::collection::vec(op_strategy(10), 1..15))| {
            let pool = fresh_pool();
            apply_ops(&pool, &ops);

            let before = snapshot(&pool);
            rebuild_projection_inner(&pool).expect("rebuild must succeed for any valid event sequence");
            let after = snapshot(&pool);
            prop_assert_eq!(before, after,
                "rebuild_projection must reproduce items exactly for any event sequence");
        });
    }

    /// Property 2: get_items_at(now) matches the live projection for any
    /// event sequence. (Time-travel to now == live state.)
    #[test]
    fn prop_get_items_at_now_matches_live() {
        proptest!(|(ops in prop::collection::vec(op_strategy(10), 1..15))| {
            let pool = fresh_pool();
            apply_ops(&pool, &ops);

            let live = snapshot(&pool);
            let now_ts = db::unix_ms_now();
            let at_now = get_items_at_inner(&pool, now_ts).expect("get_items_at(now) must succeed");
            // get_items_at returns Item[]; snapshot returns (id,content,tier,deleted).
            // Compare the id set + content + tier + deleted flag.
            let at_now_snapshot: Vec<(String, String, String, i64)> = at_now.iter()
                .map(|i| (i.id.clone(), i.content.clone(), i.tier.as_sql().to_string(), 0i64))
                .collect();
            // live includes deleted=1 rows; at_now only returns non-deleted.
            // Filter live to non-deleted for comparison.
            let live_non_deleted: Vec<_> = live.into_iter().filter(|(_, _, _, d)| *d == 0).collect();
            // Both sorted by id for stable comparison.
            let mut a = live_non_deleted.clone();
            let mut b = at_now_snapshot.clone();
            a.sort_by(|x, y| x.0.cmp(&y.0));
            b.sort_by(|x, y| x.0.cmp(&y.0));
            prop_assert_eq!(a, b,
                "get_items_at(now) must match live non-deleted projection");
        });
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

    // ── undo_last_action tests (I-17) ──────────────────────────────

    #[test]
    fn undo_nothing_returns_nothing_to_undo() {
        let pool = fresh_pool();
        let err = undo_last_action_inner(&pool).unwrap_err();
        assert_eq!(err, "NOTHING_TO_UNDO");
    }

    #[test]
    fn undo_create_soft_deletes_the_item() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "created".into(), None, None).unwrap();
        assert!(!item.deleted);

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types, vec!["ITEM_CREATED"]);
        assert_eq!(result.affected_item_ids, vec![item.id.clone()]);

        // The item is now soft-deleted in the projection.
        let conn = pool.get().unwrap();
        let deleted: i64 = conn
            .query_row(
                "SELECT deleted FROM items WHERE id = ?1",
                [&item.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deleted, 1, "undo of create must soft-delete the item");
    }

    #[test]
    fn undo_edit_restores_previous_content() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "original".into(), None, None).unwrap();
        edit_item_inner(&pool, item.id.clone(), "edited".into()).unwrap();

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types, vec!["ITEM_EDITED"]);

        // Content restored to "original".
        let conn = pool.get().unwrap();
        let content: String = conn
            .query_row(
                "SELECT content FROM items WHERE id = ?1",
                [&item.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(content, "original", "undo of edit must restore content_before");
    }

    #[test]
    fn undo_move_restores_previous_tier_and_rank() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "mover".into(), None, None).unwrap();
        let original_rank = item.rank.clone();
        move_item_inner(&pool, item.id.clone(), Tier::A, None, Some("to A".into())).unwrap();

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types, vec!["ITEM_MOVED"]);

        // Item back in Inbox with original rank.
        let conn = pool.get().unwrap();
        let (tier, rank): (String, String) = conn
            .query_row(
                "SELECT tier, rank FROM items WHERE id = ?1",
                [&item.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(tier, "inbox", "undo of move must restore tier_before");
        assert_eq!(rank, original_rank, "undo of move must restore rank_before");
    }

    #[test]
    fn undo_state_change_restores_previous_state() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "stated".into(), None, None).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Done, None).unwrap();

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types, vec!["ITEM_STATE_CHANGED"]);

        let conn = pool.get().unwrap();
        let state: String = conn
            .query_row(
                "SELECT state FROM items WHERE id = ?1",
                [&item.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "active", "undo of state change must restore state_before");
    }

    #[test]
    fn undo_delete_restores_the_item() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "doomed".into(), None, None).unwrap();
        delete_item_inner(&pool, &item.id).unwrap();

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types, vec!["ITEM_DELETED"]);

        let conn = pool.get().unwrap();
        let deleted: i64 = conn
            .query_row(
                "SELECT deleted FROM items WHERE id = ?1",
                [&item.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deleted, 0, "undo of delete must restore the item");
    }

    #[test]
    fn undo_skips_llm_events_and_undoes_the_real_action_before() {
        // LLM_SUGGESTION_GENERATED is advisory-only; undo must skip it
        // and undo the real user action before it (the create).
        use crate::db::{write_event, EventDraft};
        use crate::domain::EventType;
        use serde_json::json;

        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "real".into(), None, None).unwrap();

        // Append an LLM event after the create (simulating an analyze run).
        let _ = write_event(&pool, |_tx, _ts| {
            Ok(EventDraft {
                event_type: EventType::LlmSuggestionGenerated,
                item_id: None,
                payload: json!({
                    "kind": "analyze",
                    "scope": { "since_ts": 0, "until_ts": 0, "event_count": 1 },
                    "model": "test",
                    "observations": []
                }),
            })
        })
        .unwrap();

        // Undo must skip the LLM event and undo the create.
        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(
            result.undone_event_types,
            vec!["ITEM_CREATED"],
            "undo must skip LLM events and undo the real action before"
        );

        let conn = pool.get().unwrap();
        let deleted: i64 = conn
            .query_row(
                "SELECT deleted FROM items WHERE id = ?1",
                [&item.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deleted, 1, "the create must be undone (item soft-deleted)");
    }

    #[test]
    fn undo_is_itself_an_event_in_the_log() {
        // The undo appends a compensating event; it's auditable.
        let pool = fresh_pool();
        create_item_inner(&pool, Tier::Inbox, "auditable".into(), None, None).unwrap();
        let events_before = get_events_inner(&pool, None, None, None, None).unwrap().len();
        assert_eq!(events_before, 1);

        let _ = undo_last_action_inner(&pool).unwrap();
        let events_after = get_events_inner(&pool, None, None, None, None).unwrap().len();
        assert_eq!(events_after, 2, "undo must append a compensating event to the log");
    }

    #[test]
    fn undo_batch_delete_restores_every_item() {
        // A batch_delete writes N ITEM_DELETED events in one tx (shared
        // ts). Undo must group them by (ts, type) and restore ALL of
        // them, not just the most recent — the batch-undo property.
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::Inbox, "a".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::Inbox, "b".into(), None, None).unwrap();
        let c = create_item_inner(&pool, Tier::Inbox, "c".into(), None, None).unwrap();
        batch_delete_inner(&pool, vec![a.id.clone(), b.id.clone(), c.id.clone()]).unwrap();

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types.len(), 3, "all three deletes must be undone");
        assert!(result.undone_event_types.iter().all(|t| t == "ITEM_DELETED"));

        let conn = pool.get().unwrap();
        let live: i64 = conn
            .query_row("SELECT COUNT(*) FROM items WHERE deleted = 0", [], |r| r.get(0))
            .unwrap();
        assert_eq!(live, 3, "undo of a batch delete must restore every item");
    }

    #[test]
    fn undo_batch_set_state_reverts_every_item() {
        // batch_set_state writes N ITEM_STATE_CHANGED in one tx. Undo
        // reverts all of them.
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "a".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::A, "b".into(), None, None).unwrap();
        batch_set_state_inner(&pool, vec![a.id.clone(), b.id.clone()], ItemState::Done, None).unwrap();

        // Both are done now.
        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types.len(), 2);

        let conn = pool.get().unwrap();
        let active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE tier='A' AND state='active' AND deleted=0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active, 2, "undo of a batch mark-done must restore both to active");
    }

    #[test]
    fn undo_swap_reverses_both_moves() {
        // swap_move writes two ITEM_MOVED in one tx. Undo (grouping by
        // ts+type) must reverse both — the entering item leaves A and
        // the demoted item returns to A.
        let pool = fresh_pool();
        // Fill A to cap (5 active).
        let mut a_items = Vec::new();
        for i in 0..5 {
            a_items.push(create_item_inner(&pool, Tier::A, format!("a-{i}"), None, None).unwrap());
        }
        let inbox = create_item_inner(&pool, Tier::Inbox, "incoming".into(), None, None).unwrap();
        let demoted = a_items.last().unwrap().clone();
        let entering_rank = crate::domain::rank_between(None, Some(a_items[0].rank.as_str()));

        swap_move_inner(
            &pool,
            demoted.id.clone(),
            Tier::B,
            inbox.id.clone(),
            Tier::A,
            entering_rank,
            None,
        )
        .unwrap();

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types.len(), 2, "both swap moves must be undone");

        // Post-undo: demoted back in A, incoming back in Inbox.
        let conn = pool.get().unwrap();
        let demoted_tier: String = conn
            .query_row("SELECT tier FROM items WHERE id = ?1", [&demoted.id], |r| r.get(0))
            .unwrap();
        let inbox_tier: String = conn
            .query_row("SELECT tier FROM items WHERE id = ?1", [&inbox.id], |r| r.get(0))
            .unwrap();
        assert_eq!(demoted_tier, "A", "the demoted item must return to A");
        assert_eq!(inbox_tier, "inbox", "the entering item must return to Inbox");
    }

    #[test]
    fn undo_after_unblock_does_not_violate_check_constraint() {
        // Regression (P2e verifier BLOCKING-1): before the fix, undoing an
        // unblock (blocked→active) tried to set state='blocked' with a
        // null blocked_reason, violating the migration-002 CHECK and
        // aborting the undo with a raw SQL error. After the fix the unblock
        // event carries the outgoing reason, so undo succeeds and the
        // restored blocked row has its reason.
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "task".into(), None, None).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Blocked, Some("reason".into()))
            .unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Active, None).unwrap();

        // Must not error (previously aborted on the CHECK constraint). In a
        // same-millisecond test sequence the (ts,type) grouping may revert
        // both the block and the unblock together; the load-bearing
        // assertion is simply that the undo does not crash.
        let result = undo_last_action_inner(&pool);
        assert!(
            result.is_ok(),
            "undo after unblock must not crash on the blocked-reason CHECK: {result:?}",
        );
    }

    // ── search_events tests (I-18) ─────────────────────────────────

    #[test]
    fn search_events_finds_by_content_substring() {
        let pool = fresh_pool();
        create_item_inner(&pool, Tier::Inbox, "buy groceries".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::A, "review quarterly report".into(), None, None).unwrap();

        let results = search_events_inner(
            &pool,
            SearchEventsParams {
                query: Some("grocer".into()),
                event_type: None,
                item_id: None,
                since_ts: None,
                until_ts: None,
                limit: None,
            },
        )
        .unwrap();
        assert_eq!(results.len(), 1, "should find the 'buy groceries' ITEM_CREATED");
        assert_eq!(results[0].event_type, EventType::ItemCreated);
    }

    #[test]
    fn search_events_filters_by_event_type() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "searchable".into(), None, None).unwrap();
        edit_item_inner(&pool, item.id.clone(), "edited".into()).unwrap();

        let created_only = search_events_inner(
            &pool,
            SearchEventsParams {
                query: None,
                event_type: Some("ITEM_CREATED".into()),
                item_id: None,
                since_ts: None,
                until_ts: None,
                limit: None,
            },
        )
        .unwrap();
        assert_eq!(created_only.len(), 1);
        assert_eq!(created_only[0].event_type, EventType::ItemCreated);

        let edited_only = search_events_inner(
            &pool,
            SearchEventsParams {
                query: None,
                event_type: Some("ITEM_EDITED".into()),
                item_id: None,
                since_ts: None,
                until_ts: None,
                limit: None,
            },
        )
        .unwrap();
        assert_eq!(edited_only.len(), 1);
        assert_eq!(edited_only[0].event_type, EventType::ItemEdited);
    }

    #[test]
    fn search_events_filters_by_item_id() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::Inbox, "item a".into(), None, None).unwrap();
        // `b` is a decoy whose events must be excluded by the item_id filter.
        let _b = create_item_inner(&pool, Tier::Inbox, "item b".into(), None, None).unwrap();
        edit_item_inner(&pool, a.id.clone(), "a v2".into()).unwrap();

        let only_a = search_events_inner(
            &pool,
            SearchEventsParams {
                query: None,
                event_type: None,
                item_id: Some(a.id.clone()),
                since_ts: None,
                until_ts: None,
                limit: None,
            },
        )
        .unwrap();
        assert_eq!(only_a.len(), 2, "item a has create + edit = 2 events");
        assert!(only_a.iter().all(|e| e.item_id.as_deref() == Some(a.id.as_str())));
    }

    #[test]
    fn search_events_is_case_insensitive() {
        let pool = fresh_pool();
        create_item_inner(&pool, Tier::Inbox, "Buy Groceries".into(), None, None).unwrap();

        let results = search_events_inner(
            &pool,
            SearchEventsParams {
                query: Some("buy groceries".into()),
                event_type: None,
                item_id: None,
                since_ts: None,
                until_ts: None,
                limit: None,
            },
        )
        .unwrap();
        assert_eq!(results.len(), 1, "case-insensitive match should find 'Buy Groceries'");
    }

    #[test]
    fn search_events_empty_query_returns_all_matching_filters() {
        let pool = fresh_pool();
        create_item_inner(&pool, Tier::Inbox, "one".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::A, "two".into(), None, None).unwrap();

        let all = search_events_inner(
            &pool,
            SearchEventsParams {
                query: None,
                event_type: None,
                item_id: None,
                since_ts: None,
                until_ts: None,
                limit: None,
            },
        )
        .unwrap();
        assert_eq!(all.len(), 2, "empty query + no filters = all events");
    }

    #[test]
    fn search_events_respects_limit() {
        let pool = fresh_pool();
        for i in 0..10 {
            create_item_inner(&pool, Tier::Inbox, format!("item {i}"), None, None).unwrap();
        }

        let limited = search_events_inner(
            &pool,
            SearchEventsParams {
                query: Some("item".into()),
                event_type: None,
                item_id: None,
                since_ts: None,
                until_ts: None,
                limit: Some(3),
            },
        )
        .unwrap();
        assert_eq!(limited.len(), 3, "limit should cap the result set");
    }

    // ── undo by txn_id (migration 003 payoff — QUESTIONS Q01 closed) ─

    #[test]
    fn undo_mixed_type_txn_reverses_all_events_as_one_action() {
        // The exact shape the (ts,type) heuristic could NOT unwind
        // (Q01): one atomic transaction containing events of DIFFERENT
        // types. txn_id grouping must reverse the whole transaction in
        // a single Ctrl+Z. (This is the precondition for I-21's
        // recurrence-completion trio.)
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "mover".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::B, "finisher".into(), None, None).unwrap();

        let _ = db::write_events(&pool, |tx, _ts| {
            let cur_a = db::items::read_item_by_id_tx(tx, &a.id)?.unwrap();
            let cur_b = db::items::read_item_by_id_tx(tx, &b.id)?.unwrap();
            Ok(vec![
                crate::db::EventDraft {
                    event_type: EventType::ItemMoved,
                    item_id: Some(a.id.clone()),
                    payload: serde_json::json!({
                        "tier_before": cur_a.tier, "rank_before": cur_a.rank,
                        "tier_after": "C", "rank_after": "zz", "reason": null,
                    }),
                },
                crate::db::EventDraft {
                    event_type: EventType::ItemStateChanged,
                    item_id: Some(b.id.clone()),
                    payload: serde_json::json!({
                        "state_before": cur_b.state, "state_after": "done",
                        "blocked_reason": null,
                    }),
                },
            ])
        })
        .unwrap();

        // Sanity: the mixed txn applied.
        {
            let conn = pool.get().unwrap();
            let tier_a: String = conn
                .query_row("SELECT tier FROM items WHERE id = ?1", [&a.id], |r| r.get(0))
                .unwrap();
            let state_b: String = conn
                .query_row("SELECT state FROM items WHERE id = ?1", [&b.id], |r| r.get(0))
                .unwrap();
            assert_eq!((tier_a.as_str(), state_b.as_str()), ("C", "done"));
        }

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(
            result.undone_event_types.len(),
            2,
            "one undo must reverse BOTH events of the mixed-type txn"
        );

        let conn = pool.get().unwrap();
        let tier_a: String = conn
            .query_row("SELECT tier FROM items WHERE id = ?1", [&a.id], |r| r.get(0))
            .unwrap();
        let state_b: String = conn
            .query_row("SELECT state FROM items WHERE id = ?1", [&b.id], |r| r.get(0))
            .unwrap();
        assert_eq!(tier_a, "A", "move must be reversed");
        assert_eq!(state_b, "active", "state change must be reversed");
    }

    #[test]
    fn undo_skips_system_actor_txns() {
        // A system-actor transaction (deterministic timer execution,
        // e.g. the Today day-roll) lands AFTER the last human action.
        // Ctrl+Z must look past it: the system write stays in force,
        // the human action (the create) is what gets reversed.
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "human-made".into(), None, None).unwrap();

        let _ = db::write_events_ctx(
            &pool,
            db::WriteCtx {
                actor: crate::domain::Actor::System,
                origin: Some("test_system_timer".into()),
            },
            |_tx, _ts| {
                Ok(vec![crate::db::EventDraft {
                    event_type: EventType::ItemStateChanged,
                    item_id: Some(a.id.clone()),
                    payload: serde_json::json!({
                        "state_before": "active", "state_after": "done",
                        "blocked_reason": null,
                    }),
                }])
            },
        )
        .unwrap();

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(
            result.undone_event_types,
            vec!["ITEM_CREATED".to_string()],
            "undo must target the last HUMAN txn (the create), not the system one"
        );
        let conn = pool.get().unwrap();
        let deleted: i64 = conn
            .query_row("SELECT deleted FROM items WHERE id = ?1", [&a.id], |r| r.get(0))
            .unwrap();
        assert_eq!(deleted, 1, "create was compensated by soft-delete");
    }

    #[test]
    fn undo_of_accepted_reorg_reverses_item_events_and_skips_audit_row() {
        // An accept_suggestion-shaped txn: item events + the
        // LLM_SUGGESTION_ACCEPTED audit row in ONE transaction. Undo by
        // txn_id must compensate the item events and skip the audit row
        // (advisory events never touched the projection).
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "reorg-me".into(), None, None).unwrap();

        let _ = db::write_events_ctx(
            &pool,
            db::WriteCtx {
                origin: Some("llm_accept:999".into()),
                ..Default::default()
            },
            |tx, _ts| {
                let cur = db::items::read_item_by_id_tx(tx, &a.id)?.unwrap();
                Ok(vec![
                    crate::db::EventDraft {
                        event_type: EventType::ItemMoved,
                        item_id: Some(a.id.clone()),
                        payload: serde_json::json!({
                            "tier_before": cur.tier, "rank_before": cur.rank,
                            "tier_after": "C", "rank_after": "zz", "reason": null,
                        }),
                    },
                    crate::db::EventDraft {
                        event_type: EventType::LlmSuggestionAccepted,
                        item_id: None,
                        payload: serde_json::json!({
                            "suggestion_event_id": 999, "resulting_event_ids": [],
                        }),
                    },
                ])
            },
        )
        .unwrap();

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(
            result.undone_event_types,
            vec!["ITEM_MOVED".to_string()],
            "the audit row is skipped; only the item event is compensated"
        );
        let conn = pool.get().unwrap();
        let tier: String = conn
            .query_row("SELECT tier FROM items WHERE id = ?1", [&a.id], |r| r.get(0))
            .unwrap();
        assert_eq!(tier, "A");
    }

    #[test]
    fn undo_recurrence_completion_unwinds_the_whole_trio() {
        // THE txn_id payoff (FUTURE_WORK I-21): completing a recurring
        // item writes STATE_CHANGED + CREATED(child) + RECURRED in one
        // transaction. One Ctrl+Z must revert the parent to active AND
        // soft-delete the spawned child, skipping the audit link.
        use crate::commands::items::set_item_recurrence_inner;
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "weekly sync".into(), None, None).unwrap();
        set_item_recurrence_inner(&pool, item.id.clone(), Some("FREQ=WEEKLY".into())).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Done, None).unwrap();

        let conn = pool.get().unwrap();
        let child_id: String = conn
            .query_row(
                "SELECT json_extract(payload, '$.child_id') FROM events WHERE type = 'ITEM_RECURRED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        drop(conn);

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(
            result.undone_event_types.len(),
            2,
            "STATE_CHANGED + CREATED compensated; RECURRED audit link skipped"
        );

        let conn = pool.get().unwrap();
        let parent_state: String = conn
            .query_row("SELECT state FROM items WHERE id = ?1", [&item.id], |r| r.get(0))
            .unwrap();
        assert_eq!(parent_state, "active", "parent back to active");
        let child_deleted: i64 = conn
            .query_row("SELECT deleted FROM items WHERE id = ?1", [&child_id], |r| r.get(0))
            .unwrap();
        assert_eq!(child_deleted, 1, "spawned child soft-deleted — no orphan");
        // Parent keeps its recurrence (the rule was never part of the trio).
        let rule: Option<String> = conn
            .query_row("SELECT recurrence FROM items WHERE id = ?1", [&item.id], |r| r.get(0))
            .unwrap();
        assert_eq!(rule.as_deref(), Some("FREQ=WEEKLY"));
    }

    #[test]
    fn undo_recurrence_set_restores_prior_rule() {
        use crate::commands::items::set_item_recurrence_inner;
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::B, "r".into(), None, None).unwrap();
        set_item_recurrence_inner(&pool, item.id.clone(), Some("FREQ=DAILY".into())).unwrap();
        set_item_recurrence_inner(&pool, item.id.clone(), Some("FREQ=MONTHLY;INTERVAL=2".into()))
            .unwrap();

        let result = undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types, vec!["ITEM_RECURRENCE_SET".to_string()]);
        let conn = pool.get().unwrap();
        let rule: Option<String> = conn
            .query_row("SELECT recurrence FROM items WHERE id = ?1", [&item.id], |r| r.get(0))
            .unwrap();
        assert_eq!(rule.as_deref(), Some("FREQ=DAILY"), "undo restores the prior rule");
    }
}
