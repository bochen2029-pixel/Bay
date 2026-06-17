//! Item-mutation commands. Each opens a transaction via
//! `db::write_event`, never touching the events or items tables
//! outside that wrapper.

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::db::{self, EventDraft, SqlitePool};
use crate::domain::{rank_between, A_CAP, B_CAP, EventType, Item, ItemState, Tier};

/// 1 ≤ content length ≤ MAX_CONTENT_LEN characters (SPEC §4.3
/// ITEM_CREATED payload shape). Counted as Unicode scalar values to
/// match what the user sees, not bytes.
const MAX_CONTENT_LEN: usize = 4096;

/// Tauri event name broadcast to the frontend after any item creation
/// path (command or LAN capture). SPEC §5.2.
const ITEM_CREATED_EVENT: &str = "item_created";

/// Tauri event name broadcast after any projection mutation on an
/// existing item. SPEC §5.2.
const ITEM_UPDATED_EVENT: &str = "item_updated";

/// Tauri event name broadcast when an item is soft-deleted. SPEC §5.2.
/// Payload shape: { id: String } (Item itself is gone from projection).
const ITEM_DELETED_EVENT: &str = "item_deleted";

#[derive(Debug, Clone, Serialize)]
struct DeletedIdPayload {
    id: String,
}

#[tauri::command]
pub fn create_item(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    tier: Tier,
    content: String,
    start_at: Option<i64>,
    due_at: Option<i64>,
) -> Result<Item, String> {
    let item = create_item_inner(&pool, tier, content, start_at, due_at)?;
    // Fire the frontend event AFTER the transaction commits so subscribers
    // never see an item that isn't yet in the projection. Store handlers
    // on the frontend are idempotent — the invoke() promise resolution
    // and this event may both land; whichever arrives first wins.
    app.emit(ITEM_CREATED_EVENT, &item)
        .map_err(|e| format!("emit {ITEM_CREATED_EVENT}: {e}"))?;
    Ok(item)
}

/// Pure function behind the `create_item` Tauri command. Extracted so
/// unit tests can drive the full write path without constructing a
/// Tauri `State<T>`.
///
/// Cap enforcement: backend is authoritative. Frontend pre-checks
/// (disabled +Add at cap) are UX, not enforcement. CAP_EXCEEDED is
/// returned for A/B when `count_active >= cap`. Inbox and C are
/// unbounded. SPEC §5.1, CLAUDE.md §Interaction rules.
///
/// Placement: new items land at TOP of tier (lex-smallest rank).
/// Inbox is triage — fresh captures must be visible on arrival.
/// Drag reorder in I-06 computes ranks from explicit neighbors and
/// is unaffected.
pub fn create_item_inner(
    pool: &SqlitePool,
    tier: Tier,
    content: String,
    start_at: Option<i64>,
    due_at: Option<i64>,
) -> Result<Item, String> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("CONTENT_EMPTY".into());
    }
    if content.chars().count() > MAX_CONTENT_LEN {
        return Err("CONTENT_TOO_LONG".into());
    }

    // UUIDv7: time-ordered per RFC 9562. The monotonic property is
    // load-bearing later (inspector history + LLM compression); do not
    // substitute UUIDv4. SPEC §6.1.
    let item_id = Uuid::now_v7().to_string();

    let event = db::write_event(pool, |tx, _ts| {
        // Cap check FIRST, inside the tx: same isolation as the insert.
        match tier {
            Tier::A => {
                if db::items::count_active_in_tier(tx, Tier::A)? >= A_CAP as i64 {
                    return Err("CAP_EXCEEDED".into());
                }
            }
            Tier::B => {
                if db::items::count_active_in_tier(tx, Tier::B)? >= B_CAP as i64 {
                    return Err("CAP_EXCEEDED".into());
                }
            }
            Tier::Inbox | Tier::C => {}
        }

        // Top-of-tier placement: new ranks land strictly less than the
        // current smallest. Empty tier falls through to (None, None).
        let min_rank = db::items::min_rank_in_tier(tx, tier)?;
        let rank = rank_between(None, min_rank.as_deref());

        let payload = json!({
            "content": content,
            "tier": tier,
            "rank": rank,
            "start_at": start_at,
            "due_at": due_at,
        });

        Ok(EventDraft {
            event_type: EventType::ItemCreated,
            item_id: Some(item_id.clone()),
            payload,
        })
    })?;

    // Read back the newly-projected row. A fresh pool handle — the write
    // transaction has committed by now, so this sees the new row.
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let items = db::items::list_active_items(&conn)?;
    items
        .into_iter()
        .find(|i| Some(&i.id) == event.item_id.as_ref())
        .ok_or_else(|| "created item not found in projection".to_string())
}

#[tauri::command]
pub fn move_item(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    id: String,
    to_tier: Tier,
    to_rank: Option<String>,
    reason: Option<String>,
) -> Result<Item, String> {
    let item = move_item_inner(&pool, id, to_tier, to_rank, reason)?;
    app.emit(ITEM_UPDATED_EVENT, &item)
        .map_err(|e| format!("emit {ITEM_UPDATED_EVENT}: {e}"))?;
    Ok(item)
}

/// Pure inner function behind `move_item`. All decisions (cap check,
/// no-op detection, rank-fallback) are made inside the write
/// transaction so they share isolation with the append + apply.
///
/// Errors:
///   - `ITEM_NOT_FOUND` — the id doesn't match an active (non-deleted) row
///   - `CAP_EXCEEDED`  — cross-tier move into A/B at cap, for an active item
///   - `NO_OP`         — target tier and rank match current state; caller
///                       should have pre-checked, but the backend defends
pub fn move_item_inner(
    pool: &SqlitePool,
    id: String,
    to_tier: Tier,
    to_rank: Option<String>,
    reason: Option<String>,
) -> Result<Item, String> {
    let event = db::write_event(pool, |tx, _ts| {
        let current = db::items::read_item_by_id_tx(tx, &id)?
            .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;

        // Cap check: only cross-tier moves of active items can violate
        // target-tier capacity. Intra-tier reorder and blocked/done
        // items are exempt. Caller in I-06 rejects cross-tier at the
        // frontend; this is belt-and-suspenders for I-07 swap flow.
        let cross_tier = current.tier != to_tier;
        if cross_tier && current.state == ItemState::Active {
            match to_tier {
                Tier::A => {
                    if db::items::count_active_in_tier(tx, Tier::A)? >= A_CAP as i64 {
                        return Err("CAP_EXCEEDED".into());
                    }
                }
                Tier::B => {
                    if db::items::count_active_in_tier(tx, Tier::B)? >= B_CAP as i64 {
                        return Err("CAP_EXCEEDED".into());
                    }
                }
                Tier::Inbox | Tier::C => {}
            }
        }

        // Rank fallback: when `to_rank` is omitted, place at end of
        // target tier. The I-06 frontend always passes an explicit
        // rank; this is the documented fallback path per SPEC §5.1.
        let final_rank = match to_rank {
            Some(r) => r,
            None => {
                let max = db::items::max_rank_in_tier(tx, to_tier)?;
                rank_between(max.as_deref(), None)
            }
        };

        // No-op rejection per SPEC §9 I-06 verify: same tier + same
        // rank means the drop produced no visual change. Roll back
        // without emitting an event — event-log hygiene demands we
        // never record moves that were literally no moves.
        if current.tier == to_tier && current.rank == final_rank {
            return Err("NO_OP".into());
        }

        let payload = json!({
            "tier_before": current.tier,
            "rank_before": current.rank,
            "tier_after": to_tier,
            "rank_after": final_rank,
            "reason": reason,
        });

        Ok(EventDraft {
            event_type: EventType::ItemMoved,
            item_id: Some(id.clone()),
            payload,
        })
    })?;

    // Read back the updated row from a fresh connection.
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let items = db::items::list_active_items(&conn)?;
    items
        .into_iter()
        .find(|i| Some(&i.id) == event.item_id.as_ref())
        .ok_or_else(|| "moved item not found in projection".to_string())
}

// ── edit_item ─────────────────────────────────────────────────────

#[tauri::command]
pub fn edit_item(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    id: String,
    content: String,
) -> Result<Item, String> {
    let item = edit_item_inner(&pool, id, content)?;
    app.emit(ITEM_UPDATED_EVENT, &item)
        .map_err(|e| format!("emit {ITEM_UPDATED_EVENT}: {e}"))?;
    Ok(item)
}

pub fn edit_item_inner(
    pool: &SqlitePool,
    id: String,
    content: String,
) -> Result<Item, String> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("CONTENT_EMPTY".into());
    }
    if content.chars().count() > MAX_CONTENT_LEN {
        return Err("CONTENT_TOO_LONG".into());
    }

    let _event = db::write_event(pool, |tx, _ts| {
        let current = db::items::read_item_by_id_tx(tx, &id)?
            .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;
        if current.content == content {
            return Err("NO_OP".into());
        }
        let payload = json!({
            "content_before": current.content,
            "content_after": content,
        });
        Ok(EventDraft {
            event_type: EventType::ItemEdited,
            item_id: Some(id.clone()),
            payload,
        })
    })?;

    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let items = db::items::list_active_items(&conn)?;
    items
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| "edited item not found in projection".to_string())
}

// ── set_item_state ────────────────────────────────────────────────

#[tauri::command]
pub fn set_item_state(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    id: String,
    state: ItemState,
    blocked_reason: Option<String>,
) -> Result<Item, String> {
    let item = set_item_state_inner(&pool, id, state, blocked_reason)?;
    app.emit(ITEM_UPDATED_EVENT, &item)
        .map_err(|e| format!("emit {ITEM_UPDATED_EVENT}: {e}"))?;
    Ok(item)
}

pub fn set_item_state_inner(
    pool: &SqlitePool,
    id: String,
    target_state: ItemState,
    blocked_reason: Option<String>,
) -> Result<Item, String> {
    // Trim + validate blocked_reason up front — a blocked transition
    // with a whitespace-only reason is indistinguishable from an empty
    // one.
    let blocked_reason = blocked_reason.map(|s| s.trim().to_string());
    if target_state == ItemState::Blocked {
        let reason = blocked_reason.as_deref().unwrap_or("");
        if reason.is_empty() {
            return Err("REASON_REQUIRED".into());
        }
    }

    let _event = db::write_event(pool, |tx, _ts| {
        let current = db::items::read_item_by_id_tx(tx, &id)?
            .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;

        if current.state == target_state {
            return Err("NO_OP".into());
        }

        // Cap enforcement: transitions TO active in A/B must respect
        // cap. Transitions FROM active free a slot (no cap concern).
        // Blocked↔Done within the same tier don't change active count.
        if target_state == ItemState::Active && current.state != ItemState::Active {
            match current.tier {
                Tier::A => {
                    if db::items::count_active_in_tier(tx, Tier::A)? >= A_CAP as i64 {
                        return Err("CAP_EXCEEDED".into());
                    }
                }
                Tier::B => {
                    if db::items::count_active_in_tier(tx, Tier::B)? >= B_CAP as i64 {
                        return Err("CAP_EXCEEDED".into());
                    }
                }
                Tier::Inbox | Tier::C => {}
            }
        }

        let payload = json!({
            "state_before": current.state,
            "state_after": target_state,
            "blocked_reason": if target_state == ItemState::Blocked {
                blocked_reason.clone()
            } else {
                None
            },
        });
        Ok(EventDraft {
            event_type: EventType::ItemStateChanged,
            item_id: Some(id.clone()),
            payload,
        })
    })?;

    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let items = db::items::list_active_items(&conn)?;
    items
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| "state-changed item not found in projection".to_string())
}

// ── set_item_date ─────────────────────────────────────────────────

#[tauri::command]
pub fn set_item_date(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    id: String,
    field: String,
    value: Option<i64>,
) -> Result<Item, String> {
    let item = set_item_date_inner(&pool, id, field, value)?;
    app.emit(ITEM_UPDATED_EVENT, &item)
        .map_err(|e| format!("emit {ITEM_UPDATED_EVENT}: {e}"))?;
    Ok(item)
}

pub fn set_item_date_inner(
    pool: &SqlitePool,
    id: String,
    field: String,
    value: Option<i64>,
) -> Result<Item, String> {
    if field != "start" && field != "due" {
        return Err(format!("BAD_ARGS: invalid date field {field:?}"));
    }

    let _ = db::write_event(pool, |tx, _ts| {
        let current = db::items::read_item_by_id_tx(tx, &id)?
            .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;
        let value_before = match field.as_str() {
            "start" => current.start_at,
            "due" => current.due_at,
            _ => unreachable!(),
        };
        if value_before == value {
            return Err("NO_OP".into());
        }
        let payload = json!({
            "field": field,
            "value_before": value_before,
            "value_after": value,
        });
        Ok(EventDraft {
            event_type: EventType::ItemDateSet,
            item_id: Some(id.clone()),
            payload,
        })
    })?;

    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let items = db::items::list_active_items(&conn)?;
    items
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| "dated item not found in projection".to_string())
}

// ── delete_item ───────────────────────────────────────────────────

#[tauri::command]
pub fn delete_item(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    id: String,
) -> Result<(), String> {
    delete_item_inner(&pool, &id)?;
    app.emit(ITEM_DELETED_EVENT, DeletedIdPayload { id })
        .map_err(|e| format!("emit {ITEM_DELETED_EVENT}: {e}"))?;
    Ok(())
}

pub fn delete_item_inner(pool: &SqlitePool, id: &str) -> Result<(), String> {
    let _ = db::write_event(pool, |tx, _ts| {
        let _current = db::items::read_item_by_id_tx(tx, id)?
            .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;
        Ok(EventDraft {
            event_type: EventType::ItemDeleted,
            item_id: Some(id.to_string()),
            payload: json!({ "soft": true }),
        })
    })?;
    Ok(())
}

// ── restore_item ──────────────────────────────────────────────────

#[tauri::command]
pub fn restore_item(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    id: String,
) -> Result<Item, String> {
    let item = restore_item_inner(&pool, &id)?;
    // Emit item_created so the frontend's idempotent
    // onItemCreated adds it back to the store — same shape as a fresh
    // creation from the frontend's point of view. SPEC §4.3
    // ITEM_RESTORED lives on the backend; the Tauri event channel
    // reuses item_created for the "now-alive-again" wire signal.
    app.emit(ITEM_CREATED_EVENT, &item)
        .map_err(|e| format!("emit {ITEM_CREATED_EVENT}: {e}"))?;
    Ok(item)
}

pub fn restore_item_inner(pool: &SqlitePool, id: &str) -> Result<Item, String> {
    let _ = db::write_event(pool, |tx, _ts| {
        let current = db::items::read_item_by_id_any_tx(tx, id)?
            .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;
        if !current.deleted {
            return Err("NOT_DELETED".into());
        }
        Ok(EventDraft {
            event_type: EventType::ItemRestored,
            item_id: Some(id.to_string()),
            payload: json!({}),
        })
    })?;

    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let items = db::items::list_active_items(&conn)?;
    items
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| "restored item not found in projection".to_string())
}

/// Result of a successful swap — both items in their post-swap state.
#[derive(Debug, Serialize)]
pub struct SwapResult {
    pub leaving: Item,
    pub entering: Item,
}

#[tauri::command]
pub fn swap_move(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    leaving_id: String,
    leaving_dest: Tier,
    entering_id: String,
    entering_tier: Tier,
    entering_rank: String,
    reason: Option<String>,
) -> Result<SwapResult, String> {
    let result = swap_move_inner(
        &pool,
        leaving_id,
        leaving_dest,
        entering_id,
        entering_tier,
        entering_rank,
        reason,
    )?;
    // Both items changed — emit `item_updated` for each so subscribers
    // that only listen to the event stream stay consistent even if the
    // invoke caller doesn't forward SwapResult. The store handler is
    // idempotent on updated_at, so the invoke-resolve path + these two
    // events never double-process.
    app.emit(ITEM_UPDATED_EVENT, &result.leaving)
        .map_err(|e| format!("emit leaving: {e}"))?;
    app.emit(ITEM_UPDATED_EVENT, &result.entering)
        .map_err(|e| format!("emit entering: {e}"))?;
    Ok(result)
}

/// Pure inner function behind `swap_move`. Emits two `ITEM_MOVED`
/// events in a single SQLite transaction via `db::write_events`; if
/// either append or apply fails, both roll back.
///
/// I-07 covers the drag-into-full-tier path: `entering_id` is always
/// an existing item (the one being dragged in). The SPEC §5.1 shape
/// also allows creating a new item via swap, but that's out of scope
/// for v1 (the frontend "+ Add" button is disabled at cap rather than
/// opening the swap modal).
///
/// Errors:
///   - `ITEM_NOT_FOUND: {leaving|entering}` — the id doesn't match
///   - `BAD_ARGS: ...` — degenerate arg combinations
///   - `NO_SWAP_NEEDED` — target tier is not actually at cap
///   - `CAP_EXCEEDED: leaving_dest` — leaving target is also full
pub fn swap_move_inner(
    pool: &SqlitePool,
    leaving_id: String,
    leaving_dest: Tier,
    entering_id: String,
    entering_tier: Tier,
    entering_rank: String,
    reason: Option<String>,
) -> Result<SwapResult, String> {
    if leaving_id == entering_id {
        return Err("BAD_ARGS: leaving and entering must differ".into());
    }
    if !matches!(entering_tier, Tier::A | Tier::B) {
        return Err("BAD_ARGS: swap entering_tier must be A or B".into());
    }

    let events = db::write_events(pool, |tx, _ts| {
        let leaving = db::items::read_item_by_id_tx(tx, &leaving_id)?
            .ok_or_else(|| "ITEM_NOT_FOUND: leaving".to_string())?;
        let entering = db::items::read_item_by_id_tx(tx, &entering_id)?
            .ok_or_else(|| "ITEM_NOT_FOUND: entering".to_string())?;

        // `leaving` must currently sit in the tier being swapped into
        // (that's the whole point of swap: pick one of the existing
        // cap-filling items to demote).
        if leaving.tier != entering_tier {
            return Err("BAD_ARGS: leaving not in entering_tier".into());
        }

        // Defensive: swap_move is only meaningful when the target tier
        // is actually at cap. Frontend needsSwap() already checks, but
        // belt-and-suspenders keeps the event log clean of bogus swaps.
        let cap = match entering_tier {
            Tier::A => A_CAP as i64,
            Tier::B => B_CAP as i64,
            _ => unreachable!(),
        };
        if db::items::count_active_in_tier(tx, entering_tier)? < cap {
            return Err("NO_SWAP_NEEDED".into());
        }

        // leaving_dest cap check for active items. v1 doesn't cascade;
        // if the user picked B-as-destination but B is also full, the
        // swap fails cleanly rather than triggering a second swap modal.
        if leaving.state == ItemState::Active {
            match leaving_dest {
                Tier::A => {
                    if db::items::count_active_in_tier(tx, Tier::A)? >= A_CAP as i64 {
                        return Err("CAP_EXCEEDED: leaving_dest".into());
                    }
                }
                Tier::B => {
                    if db::items::count_active_in_tier(tx, Tier::B)? >= B_CAP as i64 {
                        return Err("CAP_EXCEEDED: leaving_dest".into());
                    }
                }
                Tier::Inbox | Tier::C => {}
            }
        }

        // Demoted item lands at the end of leaving_dest. The user chose
        // the tier in the modal; choosing the position within that tier
        // is intra-tier drag work (I-06), orthogonal to the swap.
        let leaving_new_rank =
            rank_between(db::items::max_rank_in_tier(tx, leaving_dest)?.as_deref(), None);

        let leaving_payload = json!({
            "tier_before": leaving.tier,
            "rank_before": leaving.rank,
            "tier_after": leaving_dest,
            "rank_after": leaving_new_rank,
            "reason": reason,
        });
        let entering_payload = json!({
            "tier_before": entering.tier,
            "rank_before": entering.rank,
            "tier_after": entering_tier,
            "rank_after": entering_rank,
            "reason": reason,
        });

        Ok(vec![
            EventDraft {
                event_type: EventType::ItemMoved,
                item_id: Some(leaving_id.clone()),
                payload: leaving_payload,
            },
            EventDraft {
                event_type: EventType::ItemMoved,
                item_id: Some(entering_id.clone()),
                payload: entering_payload,
            },
        ])
    })?;
    debug_assert_eq!(events.len(), 2, "swap must emit exactly two events");

    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let all = db::items::list_active_items(&conn)?;
    let leaving_out = all
        .iter()
        .find(|i| i.id == leaving_id)
        .cloned()
        .ok_or_else(|| "leaving item missing post-swap".to_string())?;
    let entering_out = all
        .into_iter()
        .find(|i| i.id == entering_id)
        .ok_or_else(|| "entering item missing post-swap".to_string())?;
    Ok(SwapResult {
        leaving: leaving_out,
        entering: entering_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ItemState;
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    fn fresh_pool() -> SqlitePool {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        db::run_migrations(&pool).unwrap();
        pool
    }

    #[test]
    fn create_item_returns_populated_item() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "first item".into(), None, None)
            .expect("create_item");
        assert_eq!(item.content, "first item");
        assert_eq!(item.tier, Tier::Inbox);
        assert_eq!(item.state, ItemState::Active);
        assert!(!item.deleted);
        assert!(!item.rank.is_empty());
    }

    #[test]
    fn create_item_writes_one_event_and_one_projection_row() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "a-tier item".into(), None, None).unwrap();

        let conn = pool.get().unwrap();
        let event_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        let item_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(event_count, 1);
        assert_eq!(item_count, 1);

        // Event shape per SPEC §4.3 ITEM_CREATED
        let (ev_type, ev_item, payload_str): (String, Option<String>, String) = conn
            .query_row(
                "SELECT type, item_id, payload FROM events",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(ev_type, "ITEM_CREATED");
        assert_eq!(ev_item.as_deref(), Some(item.id.as_str()));
        let payload: serde_json::Value = serde_json::from_str(&payload_str).unwrap();
        assert_eq!(payload["content"], "a-tier item");
        assert_eq!(payload["tier"], "A");
        assert_eq!(payload["rank"], item.rank);
    }

    #[test]
    fn create_item_rejects_empty_content() {
        let pool = fresh_pool();
        let err = create_item_inner(&pool, Tier::Inbox, "   ".into(), None, None).unwrap_err();
        assert_eq!(err, "CONTENT_EMPTY");
        let conn = pool.get().unwrap();
        let ev: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ev, 0, "rejected content must not emit an event");
    }

    #[test]
    fn create_item_rejects_oversize_content() {
        let pool = fresh_pool();
        let oversize = "x".repeat(MAX_CONTENT_LEN + 1);
        let err = create_item_inner(&pool, Tier::Inbox, oversize, None, None).unwrap_err();
        assert_eq!(err, "CONTENT_TOO_LONG");
    }

    #[test]
    fn repeated_creates_in_same_tier_place_at_top() {
        // Top-of-tier placement: each new item's rank is lex-less than
        // every previous rank in that tier. I-05 pointer: new captures
        // must be visible on arrival.
        let pool = fresh_pool();
        let mut prev_rank: Option<String> = None;
        for i in 0..5 {
            let item = create_item_inner(
                &pool,
                Tier::Inbox,
                format!("item {i}"),
                None,
                None,
            )
            .unwrap();
            if let Some(prev) = &prev_rank {
                assert!(
                    item.rank.as_str() < prev.as_str(),
                    "rank must shrink (top placement): prev={prev:?} new={:?}",
                    item.rank
                );
            }
            prev_rank = Some(item.rank);
        }
    }

    #[test]
    fn creates_land_in_correct_tier() {
        let pool = fresh_pool();
        create_item_inner(&pool, Tier::Inbox, "i".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::A, "a".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::B, "b".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::C, "c".into(), None, None).unwrap();

        let conn = pool.get().unwrap();
        let mut stmt = conn
            .prepare("SELECT tier, content FROM items ORDER BY tier")
            .unwrap();
        let rows: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("A".into(), "a".into()),
                ("B".into(), "b".into()),
                ("C".into(), "c".into()),
                ("inbox".into(), "i".into()),
            ]
        );
    }

    #[test]
    fn create_item_rejects_sixth_a_with_cap_exceeded() {
        let pool = fresh_pool();
        for i in 0..A_CAP {
            create_item_inner(&pool, Tier::A, format!("A item {i}"), None, None)
                .expect("A should accept up to cap");
        }
        let err = create_item_inner(&pool, Tier::A, "overflow".into(), None, None).unwrap_err();
        assert_eq!(err, "CAP_EXCEEDED");

        // Event + item counts didn't grow: the rejected call rolled back.
        let conn = pool.get().unwrap();
        let event_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(event_count, A_CAP as i64);
    }

    #[test]
    fn create_item_rejects_thirteenth_b_with_cap_exceeded() {
        let pool = fresh_pool();
        for i in 0..B_CAP {
            create_item_inner(&pool, Tier::B, format!("B item {i}"), None, None)
                .expect("B should accept up to cap");
        }
        let err = create_item_inner(&pool, Tier::B, "overflow".into(), None, None).unwrap_err();
        assert_eq!(err, "CAP_EXCEEDED");
    }

    #[test]
    fn inbox_and_c_are_unbounded() {
        let pool = fresh_pool();
        // Go well past A's cap of 5 to confirm Inbox has no cap.
        for i in 0..20 {
            create_item_inner(&pool, Tier::Inbox, format!("i{i}"), None, None)
                .expect("Inbox must accept arbitrary count");
        }
        for i in 0..20 {
            create_item_inner(&pool, Tier::C, format!("c{i}"), None, None)
                .expect("C must accept arbitrary count");
        }
    }

    // ── move_item ────────────────────────────────────────────────

    #[test]
    fn move_item_intra_tier_changes_rank_and_emits_event() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::Inbox, "one".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::Inbox, "two".into(), None, None).unwrap();
        // TOP placement means `b` now has the smaller rank. Move `b`
        // to a rank strictly greater than `a` — reorder to end.
        let new_rank = format!("{}{}", a.rank, "z"); // trivially greater than a.rank
        let moved = move_item_inner(&pool, b.id.clone(), Tier::Inbox, Some(new_rank.clone()), None)
            .expect("intra-tier move");

        assert_eq!(moved.tier, Tier::Inbox);
        assert_eq!(moved.rank, new_rank);

        let conn = pool.get().unwrap();
        let (ev_type, ev_item, payload_str): (String, Option<String>, String) = conn
            .query_row(
                "SELECT type, item_id, payload FROM events WHERE type = 'ITEM_MOVED'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(ev_type, "ITEM_MOVED");
        assert_eq!(ev_item.as_deref(), Some(b.id.as_str()));
        let payload: serde_json::Value = serde_json::from_str(&payload_str).unwrap();
        assert_eq!(payload["tier_before"], "inbox");
        assert_eq!(payload["tier_after"], "inbox");
        assert_eq!(payload["rank_before"], b.rank);
        assert_eq!(payload["rank_after"], new_rank);
        assert!(payload["reason"].is_null());
    }

    #[test]
    fn move_item_rejects_no_op_without_emitting() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::Inbox, "solo".into(), None, None).unwrap();

        let err = move_item_inner(&pool, a.id.clone(), a.tier, Some(a.rank.clone()), None)
            .unwrap_err();
        assert_eq!(err, "NO_OP");

        let conn = pool.get().unwrap();
        let move_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE type = 'ITEM_MOVED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(move_count, 0, "no-op must not emit ITEM_MOVED");
    }

    #[test]
    fn move_item_rejects_unknown_id() {
        let pool = fresh_pool();
        let err = move_item_inner(
            &pool,
            "missing-id".into(),
            Tier::Inbox,
            Some("z".into()),
            None,
        )
        .unwrap_err();
        assert_eq!(err, "ITEM_NOT_FOUND");
    }

    #[test]
    fn move_item_cross_tier_cap_exceeded() {
        // Fill A to cap, then try to cross-tier move a 6th active item
        // from Inbox into A.
        let pool = fresh_pool();
        for i in 0..A_CAP {
            create_item_inner(&pool, Tier::A, format!("a{i}"), None, None).unwrap();
        }
        let inbox_item =
            create_item_inner(&pool, Tier::Inbox, "overflow".into(), None, None).unwrap();

        let err = move_item_inner(
            &pool,
            inbox_item.id.clone(),
            Tier::A,
            Some("z".into()),
            None,
        )
        .unwrap_err();
        assert_eq!(err, "CAP_EXCEEDED");

        // Item stayed in Inbox — the failed tx rolled back.
        let conn = pool.get().unwrap();
        let current_tier: String = conn
            .query_row(
                "SELECT tier FROM items WHERE id = ?1",
                [&inbox_item.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(current_tier, "inbox");
    }

    // ── swap_move ────────────────────────────────────────────────

    fn fill_a_and_seed_inbox_item(pool: &SqlitePool) -> (Vec<Item>, Item) {
        // Return (a_items_ordered_by_rank_ascending, inbox_item).
        let mut a_items = Vec::new();
        for i in 0..A_CAP {
            // Insert in reverse so TOP placement stacks them in the order
            // we can reason about; the exact order isn't important for the
            // tests below, only that A is at cap.
            let item = create_item_inner(pool, Tier::A, format!("a{i}"), None, None).unwrap();
            a_items.push(item);
        }
        // Re-fetch sorted by rank asc to match frontend presentation.
        a_items.sort_by(|x, y| x.rank.cmp(&y.rank));
        let inbox_item =
            create_item_inner(pool, Tier::Inbox, "incoming".into(), None, None).unwrap();
        (a_items, inbox_item)
    }

    #[test]
    fn swap_move_happy_path_emits_two_events_atomically() {
        let pool = fresh_pool();
        let (a_items, inbox_item) = fill_a_and_seed_inbox_item(&pool);
        let demoted = a_items.last().unwrap().clone(); // any A item works

        // Entering rank: above A's current top (TOP-placement idiom).
        let entering_rank =
            crate::domain::rank_between(None, Some(a_items[0].rank.as_str()));

        let result = swap_move_inner(
            &pool,
            demoted.id.clone(),
            Tier::B,
            inbox_item.id.clone(),
            Tier::A,
            entering_rank.clone(),
            Some("reason-x".into()),
        )
        .expect("swap should succeed");

        assert_eq!(result.leaving.tier, Tier::B);
        assert_eq!(result.entering.tier, Tier::A);
        assert_eq!(result.entering.rank, entering_rank);

        let conn = pool.get().unwrap();
        // Exactly two ITEM_MOVED rows, adjacent ids, same ts.
        let rows: Vec<(i64, i64, String, Option<String>)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT id, ts, type, item_id FROM events \
                     WHERE type='ITEM_MOVED' ORDER BY id",
                )
                .unwrap();
            stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
        };
        assert_eq!(rows.len(), 2, "swap emits exactly two ITEM_MOVED events");
        assert_eq!(
            rows[1].0 - rows[0].0,
            1,
            "event ids must be adjacent (single transaction)"
        );
        assert_eq!(rows[0].1, rows[1].1, "events share the same timestamp");
        assert_eq!(rows[0].3.as_deref(), Some(demoted.id.as_str()));
        assert_eq!(rows[1].3.as_deref(), Some(inbox_item.id.as_str()));

        // A stays at cap after the swap (the demoted one left, a new
        // one arrived).
        let a_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE tier='A' AND state='active' AND deleted=0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(a_count, A_CAP as i64);
    }

    #[test]
    fn swap_move_rejects_when_target_not_at_cap() {
        let pool = fresh_pool();
        // Only 3 items in A — not at cap.
        for i in 0..3 {
            create_item_inner(&pool, Tier::A, format!("a{i}"), None, None).unwrap();
        }
        let inbox_item =
            create_item_inner(&pool, Tier::Inbox, "incoming".into(), None, None).unwrap();
        let some_a = create_item_inner(&pool, Tier::A, "pseudo-demoted".into(), None, None)
            .unwrap(); // now A=4
        assert!(some_a.tier == Tier::A);

        let err = swap_move_inner(
            &pool,
            some_a.id.clone(),
            Tier::B,
            inbox_item.id.clone(),
            Tier::A,
            "m".into(),
            None,
        )
        .unwrap_err();
        assert_eq!(err, "NO_SWAP_NEEDED");
    }

    #[test]
    fn swap_move_rejects_unknown_ids() {
        let pool = fresh_pool();
        let (_a, inbox_item) = fill_a_and_seed_inbox_item(&pool);

        let missing_leaving = swap_move_inner(
            &pool,
            "not-a-real-id".into(),
            Tier::B,
            inbox_item.id.clone(),
            Tier::A,
            "m".into(),
            None,
        )
        .unwrap_err();
        assert!(missing_leaving.starts_with("ITEM_NOT_FOUND: leaving"));

        // Valid leaving id in A, but entering id is bogus. Reads run in
        // order (leaving first), so the bogus entering surfaces
        // `ITEM_NOT_FOUND: entering`.
        let a_item = {
            let conn = pool.get().unwrap();
            let items = db::items::list_active_items(&conn).unwrap();
            items.into_iter().find(|i| i.tier == Tier::A).unwrap()
        };
        let missing_entering = swap_move_inner(
            &pool,
            a_item.id.clone(),
            Tier::B,
            "also-not-real".into(),
            Tier::A,
            "m".into(),
            None,
        )
        .unwrap_err();
        assert!(missing_entering.starts_with("ITEM_NOT_FOUND: entering"));
    }

    #[test]
    fn swap_move_rejects_when_leaving_dest_at_cap() {
        // A is at cap; B is also at cap. Demoting from A into B must
        // fail rather than silently cascading.
        let pool = fresh_pool();
        for i in 0..A_CAP {
            create_item_inner(&pool, Tier::A, format!("a{i}"), None, None).unwrap();
        }
        for i in 0..B_CAP {
            create_item_inner(&pool, Tier::B, format!("b{i}"), None, None).unwrap();
        }
        let inbox_item =
            create_item_inner(&pool, Tier::Inbox, "incoming".into(), None, None).unwrap();
        let some_a = {
            let conn = pool.get().unwrap();
            let items = db::items::list_active_items(&conn).unwrap();
            items.into_iter().find(|i| i.tier == Tier::A).unwrap()
        };

        let err = swap_move_inner(
            &pool,
            some_a.id.clone(),
            Tier::B,
            inbox_item.id.clone(),
            Tier::A,
            "z".into(),
            None,
        )
        .unwrap_err();
        assert_eq!(err, "CAP_EXCEEDED: leaving_dest");

        // Nothing changed: two_a events didn't land.
        let conn = pool.get().unwrap();
        let moved_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE type='ITEM_MOVED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(moved_count, 0);
    }

    #[test]
    fn swap_move_rejects_self_swap() {
        let pool = fresh_pool();
        let (_a, inbox_item) = fill_a_and_seed_inbox_item(&pool);
        let err = swap_move_inner(
            &pool,
            inbox_item.id.clone(),
            Tier::C,
            inbox_item.id.clone(),
            Tier::A,
            "m".into(),
            None,
        )
        .unwrap_err();
        assert!(err.starts_with("BAD_ARGS"));
    }

    #[test]
    fn swap_move_preserves_active_counts() {
        // Pre-swap: A=5, Inbox=1 incoming.
        // Post-swap (demote to B): A=5, B=1, Inbox=0.
        let pool = fresh_pool();
        let (a_items, inbox_item) = fill_a_and_seed_inbox_item(&pool);
        let demoted = a_items.last().unwrap().clone();

        let entering_rank =
            crate::domain::rank_between(None, Some(a_items[0].rank.as_str()));
        swap_move_inner(
            &pool,
            demoted.id.clone(),
            Tier::B,
            inbox_item.id.clone(),
            Tier::A,
            entering_rank,
            None,
        )
        .unwrap();

        let conn = pool.get().unwrap();
        let counts: Vec<(String, i64)> = conn
            .prepare(
                "SELECT tier, COUNT(*) FROM items \
                 WHERE state='active' AND deleted=0 GROUP BY tier ORDER BY tier",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        // alphabetical by tier value: A, B, inbox (no C, no entries when empty)
        assert_eq!(counts, vec![("A".into(), 5), ("B".into(), 1)]);
    }

    // ── edit_item ────────────────────────────────────────────────

    #[test]
    fn edit_item_happy_path_updates_content_and_emits() {
        let pool = fresh_pool();
        let created = create_item_inner(&pool, Tier::Inbox, "before".into(), None, None).unwrap();
        let edited = edit_item_inner(&pool, created.id.clone(), "after".into()).unwrap();
        assert_eq!(edited.content, "after");

        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE type = 'ITEM_EDITED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn edit_item_rejects_no_op_and_empty_and_oversize() {
        let pool = fresh_pool();
        let created = create_item_inner(&pool, Tier::Inbox, "same".into(), None, None).unwrap();

        assert_eq!(
            edit_item_inner(&pool, created.id.clone(), "same".into()).unwrap_err(),
            "NO_OP"
        );
        assert_eq!(
            edit_item_inner(&pool, created.id.clone(), "   ".into()).unwrap_err(),
            "CONTENT_EMPTY"
        );
        let oversize = "x".repeat(MAX_CONTENT_LEN + 1);
        assert_eq!(
            edit_item_inner(&pool, created.id.clone(), oversize).unwrap_err(),
            "CONTENT_TOO_LONG"
        );
    }

    #[test]
    fn edit_item_rejects_unknown_id() {
        let pool = fresh_pool();
        let err = edit_item_inner(&pool, "nope".into(), "x".into()).unwrap_err();
        assert_eq!(err, "ITEM_NOT_FOUND");
    }

    // ── set_item_state ───────────────────────────────────────────

    #[test]
    fn set_item_state_active_to_blocked_requires_reason() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        let err = set_item_state_inner(&pool, item.id.clone(), ItemState::Blocked, None)
            .unwrap_err();
        assert_eq!(err, "REASON_REQUIRED");

        let err2 = set_item_state_inner(
            &pool,
            item.id.clone(),
            ItemState::Blocked,
            Some("   ".into()),
        )
        .unwrap_err();
        assert_eq!(err2, "REASON_REQUIRED");
    }

    #[test]
    fn set_item_state_active_to_blocked_happy() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        let updated = set_item_state_inner(
            &pool,
            item.id.clone(),
            ItemState::Blocked,
            Some("waiting on Y".into()),
        )
        .unwrap();
        assert_eq!(updated.state, ItemState::Blocked);
        assert_eq!(updated.blocked_reason.as_deref(), Some("waiting on Y"));
    }

    #[test]
    fn set_item_state_active_to_done_then_back_to_active() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();

        let done = set_item_state_inner(&pool, item.id.clone(), ItemState::Done, None).unwrap();
        assert_eq!(done.state, ItemState::Done);
        assert!(done.blocked_reason.is_none());

        let back = set_item_state_inner(&pool, item.id.clone(), ItemState::Active, None).unwrap();
        assert_eq!(back.state, ItemState::Active);
    }

    #[test]
    fn set_item_state_no_op_rejected() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "x".into(), None, None).unwrap();
        let err = set_item_state_inner(&pool, item.id.clone(), ItemState::Active, None)
            .unwrap_err();
        assert_eq!(err, "NO_OP");
    }

    #[test]
    fn set_item_state_unblock_in_full_a_hits_cap() {
        // Fill A with 5 active. Block one. Try to unblock → CAP_EXCEEDED
        // because unblocking would push active count to 6.
        let pool = fresh_pool();
        let mut ids = Vec::new();
        for i in 0..A_CAP {
            let it = create_item_inner(&pool, Tier::A, format!("a{i}"), None, None).unwrap();
            ids.push(it.id);
        }
        // Block the first one → A goes to 4 active.
        set_item_state_inner(
            &pool,
            ids[0].clone(),
            ItemState::Blocked,
            Some("paused".into()),
        )
        .unwrap();
        // Now add a 5th active so A is back at cap with 1 blocked on the side.
        create_item_inner(&pool, Tier::A, "fresh-5th".into(), None, None).unwrap();
        // Try to unblock the original: active count would go 5 → 6.
        let err = set_item_state_inner(&pool, ids[0].clone(), ItemState::Active, None)
            .unwrap_err();
        assert_eq!(err, "CAP_EXCEEDED");
    }

    #[test]
    fn set_item_state_block_then_done_clears_reason() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "x".into(), None, None).unwrap();
        let blocked = set_item_state_inner(
            &pool,
            item.id.clone(),
            ItemState::Blocked,
            Some("reason-1".into()),
        )
        .unwrap();
        assert_eq!(blocked.blocked_reason.as_deref(), Some("reason-1"));
        // Transition blocked → done should null out blocked_reason.
        let done = set_item_state_inner(&pool, item.id.clone(), ItemState::Done, None).unwrap();
        assert_eq!(done.state, ItemState::Done);
        assert!(done.blocked_reason.is_none());
    }

    // ── set_item_date ────────────────────────────────────────────

    #[test]
    fn set_item_date_start_and_due() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();

        let ts1: i64 = 1_700_000_000_000;
        let with_start =
            set_item_date_inner(&pool, item.id.clone(), "start".into(), Some(ts1))
                .unwrap();
        assert_eq!(with_start.start_at, Some(ts1));
        assert_eq!(with_start.due_at, None);

        let ts2: i64 = 1_710_000_000_000;
        let with_both =
            set_item_date_inner(&pool, item.id.clone(), "due".into(), Some(ts2)).unwrap();
        assert_eq!(with_both.start_at, Some(ts1));
        assert_eq!(with_both.due_at, Some(ts2));

        // Clear start by passing None.
        let cleared =
            set_item_date_inner(&pool, item.id.clone(), "start".into(), None).unwrap();
        assert_eq!(cleared.start_at, None);
        assert_eq!(cleared.due_at, Some(ts2));
    }

    #[test]
    fn set_item_date_rejects_invalid_field() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        let err = set_item_date_inner(&pool, item.id.clone(), "finish".into(), Some(1))
            .unwrap_err();
        assert!(err.starts_with("BAD_ARGS"));
    }

    #[test]
    fn set_item_date_rejects_no_op() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        let err =
            set_item_date_inner(&pool, item.id.clone(), "start".into(), None).unwrap_err();
        assert_eq!(err, "NO_OP");
    }

    #[test]
    fn set_item_date_rejects_unknown_id() {
        let pool = fresh_pool();
        let err = set_item_date_inner(&pool, "nope".into(), "start".into(), Some(1))
            .unwrap_err();
        assert_eq!(err, "ITEM_NOT_FOUND");
    }

    // ── delete_item / restore_item ───────────────────────────────

    #[test]
    fn delete_then_restore_roundtrip() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "x".into(), None, None).unwrap();
        delete_item_inner(&pool, &item.id).unwrap();

        // Projection excludes deleted items.
        {
            let conn = pool.get().unwrap();
            let items = db::items::list_active_items(&conn).unwrap();
            assert!(items.iter().all(|i| i.id != item.id));
        }

        let restored = restore_item_inner(&pool, &item.id).unwrap();
        assert_eq!(restored.id, item.id);
        assert_eq!(restored.content, "x");
        assert!(!restored.deleted);
    }

    #[test]
    fn delete_item_event_logged() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "x".into(), None, None).unwrap();
        delete_item_inner(&pool, &item.id).unwrap();
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE type = 'ITEM_DELETED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn restore_item_rejects_not_deleted() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::Inbox, "x".into(), None, None).unwrap();
        let err = restore_item_inner(&pool, &item.id).unwrap_err();
        assert_eq!(err, "NOT_DELETED");
    }

    #[test]
    fn restore_item_rejects_unknown_id() {
        let pool = fresh_pool();
        let err = restore_item_inner(&pool, "nope").unwrap_err();
        assert_eq!(err, "ITEM_NOT_FOUND");
    }

    #[test]
    fn delete_item_rejects_unknown_id() {
        let pool = fresh_pool();
        let err = delete_item_inner(&pool, "nope").unwrap_err();
        assert_eq!(err, "ITEM_NOT_FOUND");
    }

    // ── projection replay covers all new handlers ────────────────

    #[test]
    fn projection_replay_covers_state_edit_delete_restore() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "orig".into(), None, None).unwrap();
        edit_item_inner(&pool, item.id.clone(), "edited".into()).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Done, None).unwrap();
        delete_item_inner(&pool, &item.id).unwrap();
        restore_item_inner(&pool, &item.id).unwrap();

        let before: (String, String, bool) = {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT content, state, deleted FROM items WHERE id = ?1",
                [&item.id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)? != 0,
                    ))
                },
            )
            .unwrap()
        };

        // Wipe projection; replay every event.
        {
            let mut conn = pool.get().unwrap();
            conn.execute("DELETE FROM items", []).unwrap();
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
            for (eid, ts, ts_type, item_id_opt, payload_str) in rows {
                let event_type = crate::domain::EventType::from_sql(&ts_type).unwrap();
                let payload: serde_json::Value = serde_json::from_str(&payload_str).unwrap();
                let event = crate::domain::Event {
                    id: eid,
                    ts,
                    event_type,
                    item_id: item_id_opt,
                    payload,
                };
                db::items::apply_event_to_projection(&tx, &event).unwrap();
            }
            tx.commit().unwrap();
        }

        let after: (String, String, bool) = {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT content, state, deleted FROM items WHERE id = ?1",
                [&item.id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)? != 0,
                    ))
                },
            )
            .unwrap()
        };
        assert_eq!(before, after, "replay must reproduce post-chain state");
    }

    #[test]
    fn move_item_projection_rebuilds_from_events() {
        // The move is a pure projection update; replaying the event log
        // from scratch must reproduce the post-move state byte-for-byte.
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::Inbox, "one".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::Inbox, "two".into(), None, None).unwrap();
        let new_rank_for_b = format!("{}{}", a.rank, "z");
        move_item_inner(&pool, b.id.clone(), Tier::Inbox, Some(new_rank_for_b.clone()), None)
            .unwrap();

        let before: Vec<(String, String, String)> = {
            let conn = pool.get().unwrap();
            let mut stmt = conn
                .prepare("SELECT id, tier, rank FROM items ORDER BY id")
                .unwrap();
            stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
        };

        // Wipe projection + replay every event.
        {
            let mut conn = pool.get().unwrap();
            conn.execute("DELETE FROM items", []).unwrap();
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
            for (eid, ts, type_str, item_id, payload_str) in rows {
                let event_type = crate::domain::EventType::from_sql(&type_str).unwrap();
                let payload: serde_json::Value = serde_json::from_str(&payload_str).unwrap();
                let event = crate::domain::Event {
                    id: eid,
                    ts,
                    event_type,
                    item_id,
                    payload,
                };
                db::items::apply_event_to_projection(&tx, &event).unwrap();
            }
            tx.commit().unwrap();
        }

        let after: Vec<(String, String, String)> = {
            let conn = pool.get().unwrap();
            let mut stmt = conn
                .prepare("SELECT id, tier, rank FROM items ORDER BY id")
                .unwrap();
            stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
        };

        assert_eq!(before, after, "replay must reproduce the moved state");
    }

    // ── Property tests (non-LLM oracle for swap_move + cap enforcement) ──
    //
    // These are the Externality Principle's mechanical check on
    // swap_move_inner and the cap-enforcement paths in create/move/
    // set_item_state. The existing tests pin specific scenarios; these
    // generalize: the invariants must hold for ALL valid inputs.

    use proptest::prelude::*;

    /// Count active items in a tier directly from the projection.
    fn count_active(pool: &SqlitePool, tier: Tier) -> i64 {
        let conn = pool.get().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM items WHERE tier = ?1 AND state = 'active' AND deleted = 0",
            [tier.as_sql()],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// Property: cap enforcement is invariant under any sequence of
    /// creates into A. After any number of create_item calls into A,
    /// count_active(A) <= A_CAP. Creates beyond cap must return
    /// CAP_EXCEEDED and not increment the count.
    #[test]
    fn prop_cap_a_never_exceeded_under_creates() {
        proptest!(|(n_extra in 0u32..8)| {
            let pool = fresh_pool();
            // Fill A to cap.
            for _ in 0..A_CAP {
                create_item_inner(&pool, Tier::A, "a".into(), None, None).unwrap();
            }
            assert_eq!(count_active(&pool, Tier::A), A_CAP as i64);

            // Try to create n_extra more; each must fail with CAP_EXCEEDED.
            for i in 0..n_extra {
                let err = create_item_inner(&pool, Tier::A, format!("extra-{i}"), None, None)
                    .expect_err("create beyond A cap must error");
                assert_eq!(err, "CAP_EXCEEDED", "expected CAP_EXCEEDED, got {err:?}");
            }
            // Count must still be exactly A_CAP.
            prop_assert_eq!(count_active(&pool, Tier::A), A_CAP as i64,
                "A active count must never exceed cap");
        });
    }

    /// Property: cap enforcement is invariant under any sequence of
    /// creates into B (cap 12).
    #[test]
    fn prop_cap_b_never_exceeded_under_creates() {
        proptest!(|(n_extra in 0u32..8)| {
            let pool = fresh_pool();
            for _ in 0..B_CAP {
                create_item_inner(&pool, Tier::B, "b".into(), None, None).unwrap();
            }
            assert_eq!(count_active(&pool, Tier::B), B_CAP as i64);

            for i in 0..n_extra {
                let err = create_item_inner(&pool, Tier::B, format!("extra-{i}"), None, None)
                    .expect_err("create beyond B cap must error");
                assert_eq!(err, "CAP_EXCEEDED");
            }
            prop_assert_eq!(count_active(&pool, Tier::B), B_CAP as i64,
                "B active count must never exceed cap");
        });
    }

    /// Property: Inbox and C are unbounded — any number of creates
    /// succeeds without CAP_EXCEEDED.
    #[test]
    fn prop_inbox_and_c_unbounded() {
        proptest!(|(n in 1u32..20)| {
            let pool = fresh_pool();
            for i in 0..n {
                create_item_inner(&pool, Tier::Inbox, format!("inb-{i}"), None, None)
                    .expect("Inbox create must never hit cap");
                create_item_inner(&pool, Tier::C, format!("c-{i}"), None, None)
                    .expect("C create must never hit cap");
            }
            prop_assert_eq!(count_active(&pool, Tier::Inbox), n as i64);
            prop_assert_eq!(count_active(&pool, Tier::C), n as i64);
        });
    }

    /// Property: swap_move atomicity — after a successful swap into a
    /// full A, the entering-tier active count is unchanged (one in,
    /// one out) and the leaving_dest count is +1. Two events land
    /// with adjacent ids and shared ts.
    #[test]
    fn prop_swap_move_preserves_active_counts_and_atomicity() {
        proptest!(|(leaving_dest_choice in 0u32..2, reason_present in any::<bool>())| {
            let pool = fresh_pool();
            let (a_items, inbox_item) = fill_a_and_seed_inbox_item(&pool);
            let demoted = a_items.last().unwrap().clone();
            let leaving_dest = if leaving_dest_choice == 0 { Tier::B } else { Tier::C };
            let entering_rank =
                crate::domain::rank_between(None, Some(a_items[0].rank.as_str()));
            let reason = if reason_present { Some("prop reason".to_string()) } else { None };

            let a_before = count_active(&pool, Tier::A);
            let dest_before = count_active(&pool, leaving_dest);

            let result = swap_move_inner(
                &pool,
                demoted.id.clone(),
                leaving_dest,
                inbox_item.id.clone(),
                Tier::A,
                entering_rank,
                reason,
            ).expect("swap_move must succeed when A is at cap");

            // Active counts: A unchanged (one in, one out); leaving_dest +1.
            prop_assert_eq!(count_active(&pool, Tier::A), a_before,
                "A active count must be unchanged after swap (one in, one out)");
            prop_assert_eq!(count_active(&pool, leaving_dest), dest_before + 1,
                "leaving_dest active count must be +1 after swap");

            // Atomicity: two ITEM_MOVED events with adjacent ids and shared ts.
            let conn = pool.get().unwrap();
            let (ids, ts): (Vec<i64>, Vec<i64>) = {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, ts FROM events WHERE type = 'ITEM_MOVED' ORDER BY id DESC LIMIT 2",
                    )
                    .unwrap();
                let rows = stmt
                    .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
                    .unwrap();
                let mut v: Vec<(i64, i64)> = rows.collect::<Result<_, _>>().unwrap();
                v.sort();
                v.into_iter().map(|(id, ts)| (id, ts)).unzip()
            };
            prop_assert_eq!(ids.len(), 2, "swap must emit exactly two ITEM_MOVED events");
            prop_assert_eq!(ids[1], ids[0] + 1, "the two events must have adjacent ids");
            prop_assert_eq!(ts[0], ts[1], "the two events must share a ts (single tx)");

            // The leaving item is now in leaving_dest; the entering item is in A.
            prop_assert_eq!(result.leaving.tier, leaving_dest);
            prop_assert_eq!(result.entering.tier, Tier::A);
        });
    }
}
