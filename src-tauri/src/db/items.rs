//! `items` projection: single source of truth for the visible board, but
//! fully derivable from the `events` log. Every projection mutation
//! routes through `apply_event_to_projection`.
//!
//! The match below is intentionally exhaustive on `EventType` — when a
//! new event variant is introduced, the compiler forces this match to
//! handle it, which forces the projection to stay in sync with the log.
//! No registry, no dispatch table, no trait. SPEC §4.3 is short enough
//! that the single match is the right primitive.

use rusqlite::{params, OptionalExtension, Row, Transaction};
use serde::Deserialize;

use crate::domain::{Event, EventType, Item, ItemState, Tier};

pub fn apply_event_to_projection(tx: &Transaction<'_>, event: &Event) -> Result<(), String> {
    match event.event_type {
        EventType::ItemCreated => apply_item_created(tx, event),
        EventType::ItemMoved => apply_item_moved(tx, event),
        EventType::ItemEdited => apply_item_edited(tx, event),
        EventType::ItemStateChanged => apply_item_state_changed(tx, event),
        EventType::ItemDateSet => apply_item_date_set(tx, event),
        EventType::ItemDeleted => apply_item_deleted(tx, event),
        EventType::ItemRestored => apply_item_restored(tx, event),

        // LLM suggestion events are advisory-only per CLAUDE.md §LLM
        // scope v1 and SPEC §4.3. They affect the event log but never
        // the projection. Keeping them as explicit `Ok(())` arms (rather
        // than a wildcard) preserves the compiler's exhaustiveness check.
        EventType::LlmSuggestionGenerated
        | EventType::LlmSuggestionAccepted
        | EventType::LlmSuggestionRejected => Ok(()),
    }
}

#[derive(Debug, Deserialize)]
struct ItemCreatedPayload {
    content: String,
    tier: Tier,
    rank: String,
    start_at: Option<i64>,
    due_at: Option<i64>,
}

fn apply_item_created(tx: &Transaction<'_>, event: &Event) -> Result<(), String> {
    let id = event
        .item_id
        .as_deref()
        .ok_or_else(|| "ITEM_CREATED event missing item_id".to_string())?;
    let p: ItemCreatedPayload = serde_json::from_value(event.payload.clone())
        .map_err(|e| format!("decode ITEM_CREATED payload: {e}"))?;

    tx.execute(
        "INSERT INTO items (id, content, tier, rank, state, blocked_reason, \
                            start_at, due_at, created_at, updated_at, deleted) \
         VALUES (?1, ?2, ?3, ?4, 'active', NULL, ?5, ?6, ?7, ?7, 0)",
        params![
            id,
            p.content,
            p.tier.as_sql(),
            p.rank,
            p.start_at,
            p.due_at,
            event.ts,
        ],
    )
    .map_err(|e| format!("insert item row: {e}"))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ItemMovedPayload {
    // `tier_before` + `rank_before` are captured for audit / replay;
    // they don't affect the post-move projection row but they anchor
    // the event to a specific preconditional state on the log.
    #[allow(dead_code)]
    tier_before: Tier,
    #[allow(dead_code)]
    rank_before: String,
    tier_after: Tier,
    rank_after: String,
    #[allow(dead_code)]
    reason: Option<String>,
}

fn apply_item_moved(tx: &Transaction<'_>, event: &Event) -> Result<(), String> {
    let id = event
        .item_id
        .as_deref()
        .ok_or_else(|| "ITEM_MOVED event missing item_id".to_string())?;
    let p: ItemMovedPayload = serde_json::from_value(event.payload.clone())
        .map_err(|e| format!("decode ITEM_MOVED payload: {e}"))?;

    let updated = tx
        .execute(
            "UPDATE items SET tier = ?1, rank = ?2, updated_at = ?3 \
             WHERE id = ?4 AND deleted = 0",
            params![p.tier_after.as_sql(), p.rank_after, event.ts, id],
        )
        .map_err(|e| format!("update item row (move): {e}"))?;
    if updated != 1 {
        return Err(format!(
            "ITEM_MOVED target item {id} not found (updated {updated} rows)"
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ItemEditedPayload {
    #[allow(dead_code)]
    content_before: String,
    content_after: String,
}

fn apply_item_edited(tx: &Transaction<'_>, event: &Event) -> Result<(), String> {
    let id = event
        .item_id
        .as_deref()
        .ok_or_else(|| "ITEM_EDITED event missing item_id".to_string())?;
    let p: ItemEditedPayload = serde_json::from_value(event.payload.clone())
        .map_err(|e| format!("decode ITEM_EDITED payload: {e}"))?;
    let updated = tx
        .execute(
            "UPDATE items SET content = ?1, updated_at = ?2 \
             WHERE id = ?3 AND deleted = 0",
            params![p.content_after, event.ts, id],
        )
        .map_err(|e| format!("update item row (edit): {e}"))?;
    if updated != 1 {
        return Err(format!(
            "ITEM_EDITED target item {id} not found (updated {updated} rows)"
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ItemStateChangedPayload {
    #[allow(dead_code)]
    state_before: ItemState,
    state_after: ItemState,
    /// Only populated when `state_after == Blocked`. SPEC §4.3.
    blocked_reason: Option<String>,
}

fn apply_item_state_changed(tx: &Transaction<'_>, event: &Event) -> Result<(), String> {
    let id = event
        .item_id
        .as_deref()
        .ok_or_else(|| "ITEM_STATE_CHANGED event missing item_id".to_string())?;
    let p: ItemStateChangedPayload = serde_json::from_value(event.payload.clone())
        .map_err(|e| format!("decode ITEM_STATE_CHANGED payload: {e}"))?;

    // blocked_reason is meaningful only while state is Blocked; clear
    // it on any other transition so the field doesn't leak across
    // state changes (active→done shouldn't carry a prior blocked reason).
    let reason_cell: Option<String> = if p.state_after == ItemState::Blocked {
        p.blocked_reason
    } else {
        None
    };

    let updated = tx
        .execute(
            "UPDATE items SET state = ?1, blocked_reason = ?2, updated_at = ?3 \
             WHERE id = ?4 AND deleted = 0",
            params![p.state_after.as_sql(), reason_cell, event.ts, id],
        )
        .map_err(|e| format!("update item row (state): {e}"))?;
    if updated != 1 {
        return Err(format!(
            "ITEM_STATE_CHANGED target item {id} not found (updated {updated} rows)"
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ItemDateSetPayload {
    field: String, // "start" | "due"
    #[allow(dead_code)]
    value_before: Option<i64>,
    value_after: Option<i64>,
}

fn apply_item_date_set(tx: &Transaction<'_>, event: &Event) -> Result<(), String> {
    let id = event
        .item_id
        .as_deref()
        .ok_or_else(|| "ITEM_DATE_SET event missing item_id".to_string())?;
    let p: ItemDateSetPayload = serde_json::from_value(event.payload.clone())
        .map_err(|e| format!("decode ITEM_DATE_SET payload: {e}"))?;

    // Only two valid column names; matched explicitly to avoid any
    // mistaken-looking string interpolation into the SQL.
    let updated = match p.field.as_str() {
        "start" => tx
            .execute(
                "UPDATE items SET start_at = ?1, updated_at = ?2 \
                 WHERE id = ?3 AND deleted = 0",
                params![p.value_after, event.ts, id],
            )
            .map_err(|e| format!("update item start_at: {e}"))?,
        "due" => tx
            .execute(
                "UPDATE items SET due_at = ?1, updated_at = ?2 \
                 WHERE id = ?3 AND deleted = 0",
                params![p.value_after, event.ts, id],
            )
            .map_err(|e| format!("update item due_at: {e}"))?,
        other => return Err(format!("ITEM_DATE_SET invalid field {other:?}")),
    };
    if updated != 1 {
        return Err(format!(
            "ITEM_DATE_SET target item {id} not found (updated {updated} rows)"
        ));
    }
    Ok(())
}

fn apply_item_deleted(tx: &Transaction<'_>, event: &Event) -> Result<(), String> {
    let id = event
        .item_id
        .as_deref()
        .ok_or_else(|| "ITEM_DELETED event missing item_id".to_string())?;
    // Soft delete: flip the `deleted` flag. Do NOT constrain on
    // `deleted = 0` — replaying a DELETE event after a RESTORE/re-DELETE
    // chain needs to re-mark the row regardless of current state.
    let updated = tx
        .execute(
            "UPDATE items SET deleted = 1, updated_at = ?1 WHERE id = ?2",
            params![event.ts, id],
        )
        .map_err(|e| format!("update item row (delete): {e}"))?;
    if updated != 1 {
        return Err(format!(
            "ITEM_DELETED target item {id} not found (updated {updated} rows)"
        ));
    }
    Ok(())
}

fn apply_item_restored(tx: &Transaction<'_>, event: &Event) -> Result<(), String> {
    let id = event
        .item_id
        .as_deref()
        .ok_or_else(|| "ITEM_RESTORED event missing item_id".to_string())?;
    let updated = tx
        .execute(
            "UPDATE items SET deleted = 0, updated_at = ?1 WHERE id = ?2",
            params![event.ts, id],
        )
        .map_err(|e| format!("update item row (restore): {e}"))?;
    if updated != 1 {
        return Err(format!(
            "ITEM_RESTORED target item {id} not found (updated {updated} rows)"
        ));
    }
    Ok(())
}

/// Read all non-deleted items, sorted by tier then rank, for the
/// bootstrap payload. Takes a plain `Connection` because the caller is
/// outside any in-flight transaction; a transactional variant lands in
/// I-10 when `rebuild_projection` needs it.
pub fn list_active_items(conn: &rusqlite::Connection) -> Result<Vec<Item>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, content, tier, rank, state, blocked_reason, \
                    start_at, due_at, created_at, updated_at, deleted \
             FROM items WHERE deleted = 0 ORDER BY tier, rank",
        )
        .map_err(|e| format!("prepare list_items: {e}"))?;
    let rows = stmt
        .query_map([], row_to_item)
        .map_err(|e| format!("query_map items: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("row_to_item: {e}"))?);
    }
    Ok(out)
}

/// Read a single non-deleted item by id inside a transaction. Returns
/// `Ok(None)` when the id is unknown or already soft-deleted.
pub fn read_item_by_id_tx(tx: &Transaction<'_>, id: &str) -> Result<Option<Item>, String> {
    tx.query_row(
        "SELECT id, content, tier, rank, state, blocked_reason, \
                start_at, due_at, created_at, updated_at, deleted \
         FROM items WHERE id = ?1 AND deleted = 0",
        params![id],
        row_to_item,
    )
    .optional()
    .map_err(|e| format!("read item by id: {e}"))
}

/// Read any item by id, including soft-deleted ones. Used by
/// `restore_item` to verify the item exists-and-is-deleted before
/// emitting ITEM_RESTORED.
pub fn read_item_by_id_any_tx(
    tx: &Transaction<'_>,
    id: &str,
) -> Result<Option<Item>, String> {
    tx.query_row(
        "SELECT id, content, tier, rank, state, blocked_reason, \
                start_at, due_at, created_at, updated_at, deleted \
         FROM items WHERE id = ?1",
        params![id],
        row_to_item,
    )
    .optional()
    .map_err(|e| format!("read any item by id: {e}"))
}

fn row_to_item(row: &Row<'_>) -> rusqlite::Result<Item> {
    let tier_str: String = row.get("tier")?;
    let state_str: String = row.get("state")?;
    let deleted_int: i64 = row.get("deleted")?;
    Ok(Item {
        id: row.get("id")?,
        content: row.get("content")?,
        tier: Tier::from_sql(&tier_str).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid tier {tier_str:?}").into(),
            )
        })?,
        rank: row.get("rank")?,
        state: ItemState::from_sql(&state_str).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid state {state_str:?}").into(),
            )
        })?,
        blocked_reason: row.get("blocked_reason")?,
        start_at: row.get("start_at")?,
        due_at: row.get("due_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        deleted: deleted_int != 0,
    })
}

/// Read the lexicographically-smallest non-deleted rank in a tier. Used
/// by `create_item` to place new items at top-of-tier so fresh
/// captures land visible (Inbox is triage; surfaces what just arrived).
pub fn min_rank_in_tier(tx: &Transaction<'_>, tier: Tier) -> Result<Option<String>, String> {
    tx.query_row(
        "SELECT rank FROM items WHERE tier = ?1 AND deleted = 0 ORDER BY rank ASC LIMIT 1",
        params![tier.as_sql()],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| format!("query min rank: {e}"))
}

/// Read the lexicographically-greatest non-deleted rank in a tier.
/// Kept for future uses (end-of-tier insertion via drag-to-end); not
/// currently called, but parallel to min_rank_in_tier and cheap to
/// carry.
#[allow(dead_code)]
pub fn max_rank_in_tier(tx: &Transaction<'_>, tier: Tier) -> Result<Option<String>, String> {
    tx.query_row(
        "SELECT rank FROM items WHERE tier = ?1 AND deleted = 0 ORDER BY rank DESC LIMIT 1",
        params![tier.as_sql()],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| format!("query max rank: {e}"))
}

/// Count items with `state = 'active'` in a tier. Caps apply to active
/// items only (CLAUDE.md §Design philosophy #1); blocked and done
/// items never count.
pub fn count_active_in_tier(tx: &Transaction<'_>, tier: Tier) -> Result<i64, String> {
    tx.query_row(
        "SELECT COUNT(*) FROM items WHERE tier = ?1 AND state = 'active' AND deleted = 0",
        params![tier.as_sql()],
        |r| r.get(0),
    )
    .map_err(|e| format!("count active in tier: {e}"))
}
