//! Item-mutation commands. Each opens a transaction via
//! `db::write_event`, never touching the events or items tables
//! outside that wrapper.

use serde_json::json;
use tauri::State;
use uuid::Uuid;

use crate::db::{self, EventDraft, SqlitePool};
use crate::domain::{rank_between, EventType, Item, Tier};

/// 1 ≤ content length ≤ MAX_CONTENT_LEN characters (SPEC §4.3
/// ITEM_CREATED payload shape). Counted as Unicode scalar values to
/// match what the user sees, not bytes.
const MAX_CONTENT_LEN: usize = 4096;

#[tauri::command]
pub fn create_item(
    pool: State<'_, SqlitePool>,
    tier: Tier,
    content: String,
    start_at: Option<i64>,
    due_at: Option<i64>,
) -> Result<Item, String> {
    create_item_inner(&pool, tier, content, start_at, due_at)
}

/// Pure function behind the `create_item` Tauri command. Extracted so
/// unit tests can drive the full write path without constructing a
/// Tauri `State<T>`.
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
        // Compute end-of-tier rank inside the same transaction so the
        // max-rank read and the event append share isolation. Blocked
        // and done items still occupy rank space; rank is orthogonal to
        // state.
        let max_rank = db::items::max_rank_in_tier(tx, tier)?;
        let rank = rank_between(max_rank.as_deref(), None);

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
    fn repeated_creates_in_same_tier_yield_ordered_ranks() {
        // End-of-tier appends must produce a strictly-increasing rank
        // sequence — that's what keeps subsequent drag-reorder math sane.
        let pool = fresh_pool();
        let mut prev_rank = String::new();
        for i in 0..5 {
            let item = create_item_inner(
                &pool,
                Tier::Inbox,
                format!("item {i}"),
                None,
                None,
            )
            .unwrap();
            if !prev_rank.is_empty() {
                assert!(
                    item.rank.as_str() > prev_rank.as_str(),
                    "rank must grow: prev={prev_rank:?} new={:?}",
                    item.rank
                );
            }
            prev_rank = item.rank;
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
}
