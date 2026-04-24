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
}
