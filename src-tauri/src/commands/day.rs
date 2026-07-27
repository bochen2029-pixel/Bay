//! v0.3 execution-core commands: the Today overlay and the day rituals.
//!
//! Today is an EXECUTION OVERLAY over the tiers (VISION law 4): items
//! keep their tier; `today_on = 'YYYY-MM-DD'` marks "chosen for this
//! day", cap **3 active**, decided once at day-open so the rest of the
//! day is re-decision-free. Caps bind flow here the way A/B bind stock.
//!
//! The FRONTEND owns "what day is it" (it has the local timezone; ISO
//! date strings compare lexicographically, which is all the roll
//! needs). `roll_day` executes the human-configured day boundary as
//! `actor: system` — the ONE sanctioned machine write (VISION law 6):
//! it touches Today membership only, never tier, state, or content,
//! and Ctrl+Z looks straight past it.
//!
//! DAY_OPENED / DAY_CLOSED are audit events with NULL `item_id` (the
//! log's first): the per-item TODAY_ADDED/REMOVED rows carry all
//! projection changes; the DAY_* rows record the ceremony — including
//! "tomorrow's first move", the evening's implementation intention
//! that day-open hands back in the morning.

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use crate::db::{self, EventDraft, SqlitePool, WriteCtx};
use crate::domain::{Actor, EventType, Item, ItemState};

/// Flow cap: at most 3 ACTIVE items on Today (done Today items are
/// finished work, not held slots — mirroring the tier caps'
/// active-only rule).
pub const TODAY_CAP: i64 = 3;

const ITEM_UPDATED_EVENT: &str = "item_updated";

fn validate_date(s: &str) -> Result<(), String> {
    let ok = s.len() == 10
        && s.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        });
    if ok {
        Ok(())
    } else {
        Err(format!("BAD_DATE: expected YYYY-MM-DD, got {s:?}"))
    }
}

// ── Today cap on the re-entry doors ─────────────────────────────────

/// Per-date bookkeeping while a multi-draft closure builds. As with
/// `SpawnAccounting`, `write_events` applies drafts only AFTER the
/// closure returns, so `count_active_today` reports the pre-transaction
/// count throughout and the closure must account for its own effects.
#[derive(Default)]
pub struct TodayAccounting {
    net_active: std::collections::HashMap<String, i64>,
}

/// Enforce the Today cap on every path that makes an item ACTIVE again
/// — reactivation from done/blocked, and archive-restore.
///
/// A done/blocked item **keeps** its Today membership (progress stays
/// in sight) but frees its slot, so bringing it back to active could
/// otherwise push the date past 3 (the P2e `restore_item` bug class:
/// an entry path that skips a cap). The cap is enforced here by
/// dropping the item's Today membership in the SAME transaction, not
/// by refusing the transition:
///
/// - Today is an **execution overlay**, not a commitment tier. Failing
///   "mark active" because of an overlay would be surprising.
/// - More decisively: **undo must never fail.** Undoing a completion
///   re-activates the item; if that could return `TODAY_FULL`, Ctrl+Z
///   would break — and undo is the one operation the whole event-log
///   architecture promises always works.
///
/// The drop is logged (`cause: "user"` — the human's own reactivation
/// caused it), so the board never silently disagrees with the log.
/// Returns `None` when the item isn't on Today or the date has room.
pub fn today_overflow_draft(
    tx: &rusqlite::Transaction<'_>,
    item: &Item,
    acct: &mut TodayAccounting,
) -> Result<Option<EventDraft>, String> {
    let date = match &item.today_on {
        Some(d) => d.clone(),
        None => return Ok(None),
    };
    let net = acct.net_active.get(&date).copied().unwrap_or(0);
    if db::items::count_active_today(tx, &date)? + net >= TODAY_CAP {
        return Ok(Some(EventDraft {
            event_type: EventType::TodayRemoved,
            item_id: Some(item.id.clone()),
            payload: json!({ "date": date, "cause": "user" }),
        }));
    }
    *acct.net_active.entry(date).or_insert(0) += 1;
    Ok(None)
}

// ── add / remove ────────────────────────────────────────────────────

#[tauri::command]
pub fn add_to_today(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    id: String,
    date: String,
) -> Result<Item, String> {
    let item = add_to_today_inner(&pool, id, date)?;
    app.emit(ITEM_UPDATED_EVENT, &item)
        .map_err(|e| format!("emit {ITEM_UPDATED_EVENT}: {e}"))?;
    Ok(item)
}

pub fn add_to_today_inner(pool: &SqlitePool, id: String, date: String) -> Result<Item, String> {
    validate_date(&date)?;
    let _ = db::write_events(pool, |tx, _ts| {
        let current = db::items::read_item_by_id_tx(tx, &id)?
            .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;
        if current.state != ItemState::Active {
            return Err("NOT_ACTIVE".into());
        }
        if current.today_on.as_deref() == Some(date.as_str()) {
            return Err("NO_OP".into());
        }
        if db::items::count_active_today(tx, &date)? >= TODAY_CAP {
            return Err("TODAY_FULL".into());
        }
        // Moving between dates: close out the old date's membership so
        // the log's add/remove pairs balance per date (otherwise the
        // old date silently loses a member with no event to show for it).
        let mut drafts = Vec::new();
        if let Some(old) = &current.today_on {
            drafts.push(EventDraft {
                event_type: EventType::TodayRemoved,
                item_id: Some(id.clone()),
                payload: json!({ "date": old, "cause": "user" }),
            });
        }
        drafts.push(EventDraft {
            event_type: EventType::TodayAdded,
            item_id: Some(id.clone()),
            payload: json!({ "date": date }),
        });
        Ok(drafts)
    })?;
    read_back(pool, &id)
}

#[tauri::command]
pub fn remove_from_today(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    id: String,
) -> Result<Item, String> {
    let item = remove_from_today_inner(&pool, id)?;
    app.emit(ITEM_UPDATED_EVENT, &item)
        .map_err(|e| format!("emit {ITEM_UPDATED_EVENT}: {e}"))?;
    Ok(item)
}

pub fn remove_from_today_inner(pool: &SqlitePool, id: String) -> Result<Item, String> {
    let _ = db::write_event(pool, |tx, _ts| {
        let current = db::items::read_item_by_id_tx(tx, &id)?
            .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;
        let date = match &current.today_on {
            Some(d) => d.clone(),
            None => return Err("NO_OP".into()),
        };
        Ok(EventDraft {
            event_type: EventType::TodayRemoved,
            item_id: Some(id.clone()),
            payload: json!({ "date": date, "cause": "user" }),
        })
    })?;
    read_back(pool, &id)
}

// ── day open / close ────────────────────────────────────────────────

#[tauri::command]
pub fn open_day(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    date: String,
    today_ids: Vec<String>,
) -> Result<Vec<Item>, String> {
    let items = open_day_inner(&pool, date, today_ids)?;
    for item in &items {
        let _ = app.emit(ITEM_UPDATED_EVENT, item);
    }
    Ok(items)
}

/// The morning ceremony: choose Today (≤3), one atomic transaction —
/// N × TODAY_ADDED + one DAY_OPENED audit row. Items already on this
/// date's Today are skipped (idempotent re-open); everything else must
/// be active and fit under the cap or the whole ceremony rolls back.
pub fn open_day_inner(
    pool: &SqlitePool,
    date: String,
    today_ids: Vec<String>,
) -> Result<Vec<Item>, String> {
    validate_date(&date)?;
    let mut seen = std::collections::HashSet::new();
    let today_ids: Vec<String> = today_ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect();

    let events = db::write_events_ctx(
        pool,
        WriteCtx {
            origin: Some("day_open".into()),
            ..Default::default()
        },
        |tx, _ts| {
            let mut drafts = Vec::new();
            let mut adds: i64 = 0;
            let base = db::items::count_active_today(tx, &date)?;
            for id in &today_ids {
                let current = db::items::read_item_by_id_tx(tx, id)?
                    .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;
                if current.today_on.as_deref() == Some(date.as_str()) {
                    continue; // already chosen — idempotent
                }
                if current.state != ItemState::Active {
                    return Err("NOT_ACTIVE".into());
                }
                if base + adds >= TODAY_CAP {
                    return Err("TODAY_FULL".into());
                }
                adds += 1;
                // Balance the old date's membership (see add_to_today).
                if let Some(old) = &current.today_on {
                    drafts.push(EventDraft {
                        event_type: EventType::TodayRemoved,
                        item_id: Some(id.clone()),
                        payload: json!({ "date": old, "cause": "user" }),
                    });
                }
                drafts.push(EventDraft {
                    event_type: EventType::TodayAdded,
                    item_id: Some(id.clone()),
                    payload: json!({ "date": date }),
                });
            }
            drafts.push(EventDraft {
                event_type: EventType::DayOpened,
                item_id: None,
                payload: json!({ "date": date, "today_ids": today_ids }),
            });
            Ok(drafts)
        },
    )?;

    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let live = db::items::list_active_items(&conn)?;
    let added: std::collections::HashSet<String> = events
        .iter()
        .filter(|e| e.event_type == EventType::TodayAdded)
        .filter_map(|e| e.item_id.clone())
        .collect();
    Ok(live.into_iter().filter(|i| added.contains(&i.id)).collect())
}

#[tauri::command]
pub fn close_day(
    pool: State<'_, SqlitePool>,
    date: String,
    tomorrow_first: Option<String>,
    note: Option<String>,
) -> Result<(), String> {
    close_day_inner(&pool, date, tomorrow_first, note)
}

/// The evening ceremony: one audit row, at most one question answered —
/// "tomorrow's first move?" (the cheapest known defeat of
/// tomorrow-morning activation energy). Never required, never nagged.
pub fn close_day_inner(
    pool: &SqlitePool,
    date: String,
    tomorrow_first: Option<String>,
    note: Option<String>,
) -> Result<(), String> {
    validate_date(&date)?;
    let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    let _ = db::write_event_ctx(
        pool,
        WriteCtx {
            origin: Some("day_close".into()),
            ..Default::default()
        },
        |tx, _ts| {
            if let Some(first) = &tomorrow_first {
                db::items::read_item_by_id_tx(tx, first)?
                    .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;
            }
            Ok(EventDraft {
                event_type: EventType::DayClosed,
                item_id: None,
                payload: json!({
                    "date": date,
                    "tomorrow_first": tomorrow_first,
                    "note": note,
                }),
            })
        },
    )?;
    Ok(())
}

// ── day roll (the one sanctioned system write) ──────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RollResult {
    pub expired_ids: Vec<String>,
}

#[tauri::command]
pub fn roll_day(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    today: String,
) -> Result<RollResult, String> {
    let result = roll_day_inner(&pool, today)?;
    if !result.expired_ids.is_empty() {
        let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
        for id in &result.expired_ids {
            if let Ok(Some(item)) = db::items::read_item_by_id_any_conn(&conn, id) {
                let _ = app.emit(ITEM_UPDATED_EVENT, &item);
            }
        }
    }
    Ok(result)
}

/// Expire stale Today membership: every item whose `today_on` is
/// strictly before the frontend-supplied local date returns to plain
/// tier life via TODAY_REMOVED{cause: expired} — one atomic system
/// transaction, `origin: day_roll`. No rollover, no guilt banner: the
/// expiry is *logged*, and the Mirror will show the planned-vs-done
/// delta, which is confrontation enough. Idempotent: a second roll on
/// the same date writes nothing (empty transactions leave no events).
pub fn roll_day_inner(pool: &SqlitePool, today: String) -> Result<RollResult, String> {
    validate_date(&today)?;
    let events = db::write_events_ctx(
        pool,
        WriteCtx {
            actor: Actor::System,
            origin: Some("day_roll".into()),
        },
        |tx, _ts| {
            let mut stmt = tx
                .prepare(
                    "SELECT id, today_on FROM items \
                     WHERE today_on IS NOT NULL AND today_on < ?1 AND deleted = 0 \
                     ORDER BY id",
                )
                .map_err(|e| format!("prepare roll query: {e}"))?;
            let rows: Vec<(String, String)> = stmt
                .query_map([&today], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| format!("query roll rows: {e}"))?
                .collect::<Result<_, _>>()
                .map_err(|e| format!("roll row: {e}"))?;
            Ok(rows
                .into_iter()
                .map(|(id, date)| EventDraft {
                    event_type: EventType::TodayRemoved,
                    item_id: Some(id),
                    payload: json!({ "date": date, "cause": "expired" }),
                })
                .collect())
        },
    )?;
    Ok(RollResult {
        expired_ids: events.into_iter().filter_map(|e| e.item_id).collect(),
    })
}

// ── day state (read side) ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DayState {
    /// Item ids currently on this date's Today (any state, non-deleted).
    pub today_ids: Vec<String>,
    /// Whether a DAY_OPENED ceremony was recorded for this date.
    pub day_opened: bool,
    /// The most recent prior day-close's "tomorrow's first move", if
    /// that item is still alive — the morning hand-back.
    pub tomorrow_first: Option<String>,
}

#[tauri::command]
pub fn get_day_state(pool: State<'_, SqlitePool>, date: String) -> Result<DayState, String> {
    get_day_state_inner(&pool, date)
}

pub fn get_day_state_inner(pool: &SqlitePool, date: String) -> Result<DayState, String> {
    validate_date(&date)?;
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT id FROM items WHERE today_on = ?1 AND deleted = 0 ORDER BY rank")
        .map_err(|e| format!("prepare today ids: {e}"))?;
    let today_ids: Vec<String> = stmt
        .query_map([&date], |r| r.get(0))
        .map_err(|e| format!("query today ids: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("today id row: {e}"))?;

    let day_opened: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM events WHERE type = 'DAY_OPENED' \
             AND json_extract(payload, '$.date') = ?1)",
            [&date],
            |r| r.get(0),
        )
        .map_err(|e| format!("query day_opened: {e}"))?;

    // Most recent close BEFORE this date whose named item is still
    // alive — a deleted or missing "first move" silently degrades to
    // None rather than pointing at a ghost.
    let tomorrow_first: Option<String> = conn
        .query_row(
            "SELECT json_extract(e.payload, '$.tomorrow_first') FROM events e \
             WHERE e.type = 'DAY_CLOSED' AND json_extract(e.payload, '$.date') < ?1 \
             ORDER BY e.id DESC LIMIT 1",
            [&date],
            |r| r.get(0),
        )
        .unwrap_or(None)
        .filter(|id: &String| {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1 AND deleted = 0)",
                [id],
                |r| r.get::<_, bool>(0),
            )
            .unwrap_or(false)
        });

    Ok(DayState {
        today_ids,
        day_opened,
        tomorrow_first,
    })
}

// ── helpers ─────────────────────────────────────────────────────────

fn read_back(pool: &SqlitePool, id: &str) -> Result<Item, String> {
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    db::items::read_item_by_id_any_conn(&conn, id)?
        .ok_or_else(|| "item not found in projection".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::{create_item_inner, set_item_state_inner};
    use crate::domain::Tier;
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    fn fresh_pool() -> SqlitePool {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        db::run_migrations(&pool).unwrap();
        pool
    }

    const D1: &str = "2026-07-25";
    const D2: &str = "2026-07-26";

    #[test]
    fn add_to_today_happy_path_and_cap() {
        let pool = fresh_pool();
        let mut ids = Vec::new();
        for i in 0..4 {
            ids.push(create_item_inner(&pool, Tier::A, format!("t{i}"), None, None).unwrap().id);
        }
        for id in ids.iter().take(3) {
            let item = add_to_today_inner(&pool, id.clone(), D2.into()).unwrap();
            assert_eq!(item.today_on.as_deref(), Some(D2));
        }
        // 4th active add on the same date must refuse: flow cap = 3.
        let err = add_to_today_inner(&pool, ids[3].clone(), D2.into()).unwrap_err();
        assert_eq!(err, "TODAY_FULL");
        // Duplicate add is a NO_OP, not an error that eats a slot.
        let err = add_to_today_inner(&pool, ids[0].clone(), D2.into()).unwrap_err();
        assert_eq!(err, "NO_OP");
    }

    #[test]
    fn add_to_today_requires_active_state() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::B, "blocked".into(), None, None).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Blocked, Some("waiting".into()))
            .unwrap();
        let err = add_to_today_inner(&pool, item.id.clone(), D2.into()).unwrap_err();
        assert_eq!(err, "NOT_ACTIVE");
    }

    #[test]
    fn done_today_item_frees_its_slot_but_keeps_membership() {
        // A finished Today item stays VISIBLE on Today (progress kept
        // in sight) but stops holding a slot (cap counts active only).
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "a".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::A, "b".into(), None, None).unwrap();
        let c = create_item_inner(&pool, Tier::A, "c".into(), None, None).unwrap();
        let d = create_item_inner(&pool, Tier::A, "d".into(), None, None).unwrap();
        for it in [&a, &b, &c] {
            add_to_today_inner(&pool, it.id.clone(), D2.into()).unwrap();
        }
        set_item_state_inner(&pool, a.id.clone(), ItemState::Done, None).unwrap();
        // a still on today…
        let conn = pool.get().unwrap();
        let a_today: Option<String> = conn
            .query_row("SELECT today_on FROM items WHERE id = ?1", [&a.id], |r| r.get(0))
            .unwrap();
        assert_eq!(a_today.as_deref(), Some(D2));
        drop(conn);
        // …and its slot is free for d.
        add_to_today_inner(&pool, d.id.clone(), D2.into()).unwrap();
    }

    #[test]
    fn remove_from_today_records_user_cause() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        add_to_today_inner(&pool, item.id.clone(), D2.into()).unwrap();
        let removed = remove_from_today_inner(&pool, item.id.clone()).unwrap();
        assert_eq!(removed.today_on, None);
        let conn = pool.get().unwrap();
        let cause: String = conn
            .query_row(
                "SELECT json_extract(payload, '$.cause') FROM events \
                 WHERE type = 'TODAY_REMOVED' ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cause, "user");
        drop(conn); // max_size=1 pool: release before the next write
        // Removing again: NO_OP.
        let err = remove_from_today_inner(&pool, item.id.clone()).unwrap_err();
        assert_eq!(err, "NO_OP");
    }

    #[test]
    fn open_day_is_atomic_and_writes_the_ceremony_row() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "a".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::B, "b".into(), None, None).unwrap();
        let items = open_day_inner(&pool, D2.into(), vec![a.id.clone(), b.id.clone()]).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.today_on.as_deref() == Some(D2)));

        let conn = pool.get().unwrap();
        // DAY_OPENED: NULL item_id, same txn as the adds.
        let (item_id, txn_matches): (Option<String>, bool) = conn
            .query_row(
                "SELECT item_id, txn_id = (SELECT txn_id FROM events WHERE type = 'TODAY_ADDED' LIMIT 1) \
                 FROM events WHERE type = 'DAY_OPENED'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(item_id, None, "DAY_OPENED is a NULL-item_id audit row");
        assert!(txn_matches, "ceremony + adds share one transaction");
        drop(conn); // max_size=1 pool: release before the next write

        // Re-open with the same ids: idempotent (no new TODAY_ADDED).
        let again = open_day_inner(&pool, D2.into(), vec![a.id.clone(), b.id.clone()]).unwrap();
        assert_eq!(again.len(), 0);
    }

    #[test]
    fn open_day_over_cap_rolls_back_whole_ceremony() {
        let pool = fresh_pool();
        let mut ids = Vec::new();
        for i in 0..4 {
            ids.push(create_item_inner(&pool, Tier::A, format!("t{i}"), None, None).unwrap().id);
        }
        let err = open_day_inner(&pool, D2.into(), ids.clone()).unwrap_err();
        assert_eq!(err, "TODAY_FULL");
        let conn = pool.get().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE type IN ('TODAY_ADDED','DAY_OPENED')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "atomic: nothing lands when the ceremony overflows");
    }

    #[test]
    fn roll_day_expires_past_membership_as_system_actor_and_is_idempotent() {
        let pool = fresh_pool();
        let old = create_item_inner(&pool, Tier::A, "yesterday".into(), None, None).unwrap();
        let cur = create_item_inner(&pool, Tier::A, "today".into(), None, None).unwrap();
        add_to_today_inner(&pool, old.id.clone(), D1.into()).unwrap();
        add_to_today_inner(&pool, cur.id.clone(), D2.into()).unwrap();

        let result = roll_day_inner(&pool, D2.into()).unwrap();
        assert_eq!(result.expired_ids, vec![old.id.clone()]);

        let conn = pool.get().unwrap();
        let (actor, origin, cause): (String, String, String) = conn
            .query_row(
                "SELECT actor, origin, json_extract(payload, '$.cause') FROM events \
                 WHERE type = 'TODAY_REMOVED' ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((actor.as_str(), origin.as_str(), cause.as_str()),
                   ("system", "day_roll", "expired"));
        let cur_today: Option<String> = conn
            .query_row("SELECT today_on FROM items WHERE id = ?1", [&cur.id], |r| r.get(0))
            .unwrap();
        assert_eq!(cur_today.as_deref(), Some(D2), "current-date membership survives");
        let n_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        drop(conn);

        // Second roll: nothing to expire, nothing written.
        let again = roll_day_inner(&pool, D2.into()).unwrap();
        assert_eq!(again.expired_ids.len(), 0);
        let conn = pool.get().unwrap();
        let n_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_before, n_after, "idempotent: empty roll writes no events");
    }

    #[test]
    fn close_day_hands_tomorrow_first_to_the_next_day() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "first move".into(), None, None).unwrap();
        close_day_inner(&pool, D1.into(), Some(item.id.clone()), Some("good day".into())).unwrap();

        let state = get_day_state_inner(&pool, D2.into()).unwrap();
        assert_eq!(state.tomorrow_first.as_deref(), Some(item.id.as_str()));
        assert!(!state.day_opened);

        // A deleted "first move" degrades to None instead of a ghost.
        crate::commands::items::delete_item_inner(&pool, &item.id).unwrap();
        let state = get_day_state_inner(&pool, D2.into()).unwrap();
        assert_eq!(state.tomorrow_first, None);
    }

    #[test]
    fn undo_skips_the_system_roll_and_targets_the_prior_human_action() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        add_to_today_inner(&pool, item.id.clone(), D1.into()).unwrap();
        let rolled = roll_day_inner(&pool, D2.into()).unwrap();
        assert_eq!(rolled.expired_ids.len(), 1);

        // Ctrl+Z after the roll: the roll is system-actor, so undo
        // reverses the ADD (the last human action) — net: the item is
        // off Today, and the roll's own write is untouched in the log.
        let result = crate::commands::events::undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types, vec!["TODAY_ADDED".to_string()]);
        let conn = pool.get().unwrap();
        let today: Option<String> = conn
            .query_row("SELECT today_on FROM items WHERE id = ?1", [&item.id], |r| r.get(0))
            .unwrap();
        assert_eq!(today, None);
    }

    // ── the Today cap on the RE-ENTRY doors (verifier BLOCKING) ─────
    //
    // A done/blocked item keeps its Today membership but frees its
    // slot. Every path that makes it active again must therefore be
    // cap-aware, or one click yields 4 active on a date. Same class as
    // the P2e restore_item bug: an entry path that skipped a cap.

    #[test]
    fn reactivating_a_done_today_item_into_a_full_day_drops_its_membership() {
        let pool = fresh_pool();
        let ids: Vec<String> = (0..4)
            .map(|i| create_item_inner(&pool, Tier::A, format!("t{i}"), None, None).unwrap().id)
            .collect();
        // 3 on Today; finish one, which frees its slot but keeps it visible.
        for id in ids.iter().take(3) {
            add_to_today_inner(&pool, id.clone(), D2.into()).unwrap();
        }
        set_item_state_inner(&pool, ids[0].clone(), ItemState::Done, None).unwrap();
        // A fourth item takes the freed slot: the date is full again.
        add_to_today_inner(&pool, ids[3].clone(), D2.into()).unwrap();

        // Un-done the first item. The transition MUST succeed (undo can
        // never fail), and the cap must hold — so it leaves Today.
        let item = set_item_state_inner(&pool, ids[0].clone(), ItemState::Active, None).unwrap();
        assert_eq!(item.state, ItemState::Active);
        assert_eq!(item.today_on, None, "membership dropped to keep the cap");

        let conn = pool.get().unwrap();
        let active_today: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE today_on = ?1 AND state='active' AND deleted=0",
                [D2],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_today, TODAY_CAP, "Today cap holds on the re-entry door");
        // The drop is logged, so the board never silently disagrees.
        let cause: String = conn
            .query_row(
                "SELECT json_extract(payload, '$.cause') FROM events \
                 WHERE type = 'TODAY_REMOVED' ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cause, "user");
    }

    #[test]
    fn reactivating_keeps_membership_when_the_day_has_room() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "a".into(), None, None).unwrap();
        add_to_today_inner(&pool, a.id.clone(), D2.into()).unwrap();
        set_item_state_inner(&pool, a.id.clone(), ItemState::Done, None).unwrap();
        let back = set_item_state_inner(&pool, a.id.clone(), ItemState::Active, None).unwrap();
        assert_eq!(
            back.today_on.as_deref(),
            Some(D2),
            "no overflow, no drop — the guard must not fire needlessly"
        );
    }

    #[test]
    fn batch_reactivation_cannot_smuggle_a_day_over_cap() {
        let pool = fresh_pool();
        let ids: Vec<String> = (0..5)
            .map(|i| create_item_inner(&pool, Tier::A, format!("t{i}"), None, None).unwrap().id)
            .collect();
        // Three on Today, all finished (slots free, membership kept).
        for id in ids.iter().take(3) {
            add_to_today_inner(&pool, id.clone(), D2.into()).unwrap();
            set_item_state_inner(&pool, id.clone(), ItemState::Done, None).unwrap();
        }
        // Two fresh items fill the date.
        add_to_today_inner(&pool, ids[3].clone(), D2.into()).unwrap();
        add_to_today_inner(&pool, ids[4].clone(), D2.into()).unwrap();

        // Reactivate all three at once: one slot is free, so exactly
        // one may keep its membership.
        crate::commands::items::batch_set_state_inner(
            &pool,
            ids[..3].to_vec(),
            ItemState::Active,
            None,
        )
        .unwrap();

        let conn = pool.get().unwrap();
        let active_today: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE today_on = ?1 AND state='active' AND deleted=0",
                [D2],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_today, TODAY_CAP, "shared accounting holds across the batch");
    }

    #[test]
    fn restoring_an_active_today_item_into_a_full_day_drops_its_membership() {
        let pool = fresh_pool();
        let ids: Vec<String> = (0..4)
            .map(|i| create_item_inner(&pool, Tier::A, format!("t{i}"), None, None).unwrap().id)
            .collect();
        for id in ids.iter().take(3) {
            add_to_today_inner(&pool, id.clone(), D2.into()).unwrap();
        }
        // Delete one (frees its slot, keeps today_on), refill the date.
        crate::commands::items::delete_item_inner(&pool, &ids[0]).unwrap();
        add_to_today_inner(&pool, ids[3].clone(), D2.into()).unwrap();

        let restored = crate::commands::items::restore_item_inner(&pool, &ids[0]).unwrap();
        assert_eq!(restored.today_on, None, "restore is cap-gated for Today too");
        let conn = pool.get().unwrap();
        let active_today: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE today_on = ?1 AND state='active' AND deleted=0",
                [D2],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_today, TODAY_CAP);
    }

    #[test]
    fn moving_an_item_between_days_balances_the_old_date() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "a".into(), None, None).unwrap();
        add_to_today_inner(&pool, a.id.clone(), D1.into()).unwrap();
        let moved = add_to_today_inner(&pool, a.id.clone(), D2.into()).unwrap();
        assert_eq!(moved.today_on.as_deref(), Some(D2));

        // The old date gets an explicit removal, so add/remove pairs
        // balance per date and the roll has nothing stale to find.
        let conn = pool.get().unwrap();
        let removals: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE type = 'TODAY_REMOVED' \
                 AND json_extract(payload, '$.date') = ?1",
                [D1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(removals, 1);
    }

    #[test]
    fn prop_today_cap_never_exceeded_under_any_add_remove_interleaving() {
        use proptest::prelude::*;
        proptest!(|(ops in proptest::collection::vec((0usize..6, proptest::bool::ANY), 1..40))| {
            let pool = fresh_pool();
            let ids: Vec<String> = (0..6)
                .map(|i| create_item_inner(&pool, Tier::C, format!("p{i}"), None, None).unwrap().id)
                .collect();
            for (idx, add) in ops {
                if add {
                    let _ = add_to_today_inner(&pool, ids[idx].clone(), D2.into());
                } else {
                    let _ = remove_from_today_inner(&pool, ids[idx].clone());
                }
                let conn = pool.get().unwrap();
                let n: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM items WHERE today_on = ?1 AND state='active' AND deleted=0",
                        [D2],
                        |r| r.get(0),
                    )
                    .unwrap();
                prop_assert!(n <= TODAY_CAP, "active Today count {n} exceeded the cap");
            }
        });
    }
}
