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

        // LLM suggestion events are advisory-only per CLAUDE.md §LLM
        // scope v1 and SPEC §4.3. They affect the event log but never
        // the projection. Keeping them as explicit `Ok(())` arms (rather
        // than a wildcard) preserves the compiler's exhaustiveness check.
        EventType::LlmSuggestionGenerated
        | EventType::LlmSuggestionAccepted
        | EventType::LlmSuggestionRejected => Ok(()),

        // Handlers land progressively with the increments that introduce
        // their writers. An explicit `Err` here means: if such an event
        // somehow gets built in the wrong increment, the transaction
        // rolls back and the root cause surfaces immediately.
        EventType::ItemEdited => Err("ITEM_EDITED handler lands in I-08".into()),
        EventType::ItemMoved => Err("ITEM_MOVED handler lands in I-06".into()),
        EventType::ItemStateChanged => Err("ITEM_STATE_CHANGED handler lands in I-08".into()),
        EventType::ItemDateSet => Err("ITEM_DATE_SET handler lands in I-09".into()),
        EventType::ItemDeleted => Err("ITEM_DELETED handler lands in I-08".into()),
        EventType::ItemRestored => Err("ITEM_RESTORED handler lands in I-08".into()),
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
