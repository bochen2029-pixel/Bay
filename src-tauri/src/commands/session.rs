//! v0.3 focus sessions: the verb the app never had — START.
//!
//! Starting is the cheapest action in Bay (VISION law 8): one call
//! opens a session on an active item; the open session IS the "Now"
//! slot (at most one — command check + partial unique index). Ending
//! takes one tap: `done` finishes the item in the SAME transaction
//! (recurrence spawns ride along, so a Ctrl+Z after a mis-tap reverts
//! the board effect while the behavior record stands); `progress` is
//! an honest pause; `interrupted` records why focus broke (the
//! five-word taxonomy).
//!
//! Sessions are BEHAVIOR records: undo never touches them (you cannot
//! un-spend attention). They are still projection rows — rebuildable
//! from the log under the same purity law as `items`.

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::db::{self, EventDraft, SqlitePool, WriteCtx};
use crate::domain::{
    EventType, Item, ItemState, Session, SessionOutcome, INTERRUPT_REASONS,
};

const ITEM_UPDATED_EVENT: &str = "item_updated";
const ITEM_CREATED_EVENT: &str = "item_created";

// ── start ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn start_session(
    pool: State<'_, SqlitePool>,
    item_id: String,
) -> Result<Session, String> {
    start_session_inner(&pool, item_id)
}

pub fn start_session_inner(pool: &SqlitePool, item_id: String) -> Result<Session, String> {
    let session_id = Uuid::now_v7().to_string();
    let _ = db::write_event_ctx(
        pool,
        WriteCtx {
            origin: Some("session_start".into()),
            ..Default::default()
        },
        |tx, _ts| {
            let current = db::items::read_item_by_id_tx(tx, &item_id)?
                .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;
            if current.state != ItemState::Active {
                return Err("NOT_ACTIVE".into());
            }
            if db::items::open_session_tx(tx)?.is_some() {
                return Err("SESSION_ALREADY_OPEN".into());
            }
            Ok(EventDraft {
                event_type: EventType::SessionStarted,
                item_id: Some(item_id.clone()),
                payload: json!({ "session_id": session_id }),
            })
        },
    )?;
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    db::items::open_session_conn(&conn)?.ok_or_else(|| "session not found post-start".to_string())
}

// ── end ─────────────────────────────────────────────────────────────

/// What `end_session` hands back so the command layer (and the store)
/// can update every touched surface: the closed session, the item as
/// it now stands, and any recurrence child spawned by a `done` ending.
#[derive(Debug, Serialize)]
pub struct EndSessionResult {
    pub session: Session,
    pub item: Item,
    pub spawned: Vec<Item>,
}

#[tauri::command]
pub fn end_session(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    outcome: SessionOutcome,
    reason: Option<String>,
    note: Option<String>,
) -> Result<EndSessionResult, String> {
    let result = end_session_inner(&pool, outcome, reason, note)?;
    let _ = app.emit(ITEM_UPDATED_EVENT, &result.item);
    for child in &result.spawned {
        let _ = app.emit(ITEM_CREATED_EVENT, child);
    }
    Ok(result)
}

pub fn end_session_inner(
    pool: &SqlitePool,
    outcome: SessionOutcome,
    reason: Option<String>,
    note: Option<String>,
) -> Result<EndSessionResult, String> {
    let reason = reason.map(|r| r.trim().to_string()).filter(|r| !r.is_empty());
    match outcome {
        SessionOutcome::Interrupted => {
            let r = reason.as_deref().unwrap_or("");
            if !INTERRUPT_REASONS.contains(&r) {
                return Err("REASON_REQUIRED".into());
            }
        }
        _ => {
            if reason.is_some() {
                return Err("BAD_ARGS: reason only accompanies interrupted".into());
            }
        }
    }
    let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());

    let events = db::write_events_ctx(
        pool,
        WriteCtx {
            origin: Some("session_end".into()),
            ..Default::default()
        },
        |tx, ts| {
            let open = db::items::open_session_tx(tx)?
                .ok_or_else(|| "NO_OPEN_SESSION".to_string())?;
            let mut drafts = vec![EventDraft {
                event_type: EventType::SessionEnded,
                item_id: Some(open.item_id.clone()),
                payload: json!({
                    "session_id": open.id,
                    "outcome": outcome,
                    "reason": reason,
                    "note": note,
                }),
            }];
            if outcome == SessionOutcome::Done {
                // Finish the item in the SAME transaction — the whole
                // point of the one-tap Done. The item may have changed
                // mid-session: an already-done item skips the co-write,
                // and a soft-deleted one (read_item_by_id_tx returns
                // None) does too — ending the session must never be
                // blocked by what happened to the item, or the Now slot
                // would be stuck open with no way out.
                let current = match db::items::read_item_by_id_tx(tx, &open.item_id)? {
                    Some(item) => item,
                    None => return Ok(drafts),
                };
                if current.state != ItemState::Done {
                    drafts.push(EventDraft {
                        event_type: EventType::ItemStateChanged,
                        item_id: Some(current.id.clone()),
                        payload: json!({
                            "state_before": current.state,
                            "state_after": ItemState::Done,
                            // Leaving blocked preserves the outgoing
                            // reason for undo (P2e fix (a) semantics).
                            "blocked_reason": if current.state == ItemState::Blocked {
                                current.blocked_reason.clone()
                            } else {
                                None
                            },
                        }),
                    });
                    let mut acct = crate::commands::items::SpawnAccounting::default();
                    if let Some(spawn) =
                        crate::commands::items::build_recurrence_spawn(tx, &current, ts, &mut acct)?
                    {
                        drafts.extend(spawn);
                    }
                }
            }
            Ok(drafts)
        },
    )?;

    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let session_id = events
        .iter()
        .find(|e| e.event_type == EventType::SessionEnded)
        .and_then(|e| e.payload.get("session_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "ended session id missing".to_string())?
        .to_string();
    let session: Session = conn
        .query_row(
            "SELECT id, item_id, started_at, ended_at, outcome, reason, note \
             FROM sessions WHERE id = ?1",
            [&session_id],
            |row| {
                Ok(Session {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    started_at: row.get(2)?,
                    ended_at: row.get(3)?,
                    outcome: row
                        .get::<_, Option<String>>(4)?
                        .and_then(|s| SessionOutcome::from_sql(&s)),
                    reason: row.get(5)?,
                    note: row.get(6)?,
                })
            },
        )
        .map_err(|e| format!("read ended session: {e}"))?;
    let item = db::items::read_item_by_id_any_conn(&conn, &session.item_id)?
        .ok_or_else(|| "session item missing post-end".to_string())?;
    // Checked in every build, not just debug: a stuck-open Now slot
    // locks the user out of starting anything with no way back, and it
    // is cheap to rule out. (Evaluating this inside `debug_assert!`
    // would both skip the check in the shipped binary and give the
    // error path different behavior in debug vs release.)
    if db::items::open_session_conn(&conn)?.is_some() {
        return Err("SESSION_STILL_OPEN: ending a session must free the Now slot".into());
    }
    let child_ids: Vec<String> = events
        .iter()
        .filter(|e| e.event_type == EventType::ItemCreated)
        .filter_map(|e| e.item_id.clone())
        .collect();
    let spawned = child_ids
        .iter()
        .filter_map(|id| db::items::read_item_by_id_any_conn(&conn, id).ok().flatten())
        .collect();

    Ok(EndSessionResult {
        session,
        item,
        spawned,
    })
}

// ── read side ───────────────────────────────────────────────────────

#[tauri::command]
pub fn get_open_session(pool: State<'_, SqlitePool>) -> Result<Option<Session>, String> {
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    db::items::open_session_conn(&conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::{create_item_inner, set_item_recurrence_inner, set_item_state_inner};
    use crate::domain::Tier;
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    fn fresh_pool() -> SqlitePool {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        db::run_migrations(&pool).unwrap();
        pool
    }

    #[test]
    fn start_and_end_progress_session() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "deep work".into(), None, None).unwrap();
        let session = start_session_inner(&pool, item.id.clone()).unwrap();
        assert_eq!(session.item_id, item.id);
        assert_eq!(session.ended_at, None);

        // Second start refused: one Now slot.
        let other = create_item_inner(&pool, Tier::B, "other".into(), None, None).unwrap();
        let err = start_session_inner(&pool, other.id.clone()).unwrap_err();
        assert_eq!(err, "SESSION_ALREADY_OPEN");

        let result = end_session_inner(&pool, SessionOutcome::Progress, None, None).unwrap();
        assert_eq!(result.session.outcome, Some(SessionOutcome::Progress));
        assert!(result.session.ended_at.is_some());
        assert_eq!(result.item.state, ItemState::Active, "progress leaves the item active");
        assert!(result.spawned.is_empty());

        // Now the slot is free again.
        start_session_inner(&pool, other.id.clone()).unwrap();
    }

    #[test]
    fn start_requires_active_item() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Blocked, Some("stuck".into()))
            .unwrap();
        let err = start_session_inner(&pool, item.id.clone()).unwrap_err();
        assert_eq!(err, "NOT_ACTIVE");
    }

    #[test]
    fn interrupted_requires_a_taxonomy_reason() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        start_session_inner(&pool, item.id.clone()).unwrap();
        let err =
            end_session_inner(&pool, SessionOutcome::Interrupted, None, None).unwrap_err();
        assert_eq!(err, "REASON_REQUIRED");
        let err = end_session_inner(
            &pool,
            SessionOutcome::Interrupted,
            Some("cosmic rays".into()),
            None,
        )
        .unwrap_err();
        assert_eq!(err, "REASON_REQUIRED", "reason must come from the taxonomy");
        let ok = end_session_inner(
            &pool,
            SessionOutcome::Interrupted,
            Some("meeting".into()),
            None,
        )
        .unwrap();
        assert_eq!(ok.session.reason.as_deref(), Some("meeting"));
    }

    #[test]
    fn done_ending_finishes_item_and_spawns_recurrence_in_one_txn() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "weekly".into(), None, None).unwrap();
        set_item_recurrence_inner(&pool, item.id.clone(), Some("FREQ=WEEKLY".into())).unwrap();
        start_session_inner(&pool, item.id.clone()).unwrap();

        let result = end_session_inner(&pool, SessionOutcome::Done, None, None).unwrap();
        assert_eq!(result.item.state, ItemState::Done);
        assert_eq!(result.spawned.len(), 1, "recurrence child rides the same txn");

        // SESSION_ENDED + STATE_CHANGED + CREATED + RECURRED share one txn.
        let conn = pool.get().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE txn_id = \
                 (SELECT txn_id FROM events WHERE type = 'SESSION_ENDED')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 4);
    }

    #[test]
    fn undo_after_done_ending_reverts_board_but_keeps_the_behavior_record() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "task".into(), None, None).unwrap();
        start_session_inner(&pool, item.id.clone()).unwrap();
        end_session_inner(&pool, SessionOutcome::Done, None, None).unwrap();

        let result = crate::commands::events::undo_last_action_inner(&pool).unwrap();
        assert_eq!(
            result.undone_event_types,
            vec!["ITEM_STATE_CHANGED".to_string()],
            "only the board effect is compensated; the session record stands"
        );
        let conn = pool.get().unwrap();
        let state: String = conn
            .query_row("SELECT state FROM items WHERE id = ?1", [&item.id], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "active");
        let (ended, outcome): (Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT ended_at, outcome FROM sessions LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(ended.is_some(), "the session stays ended — attention was spent");
        assert_eq!(outcome.as_deref(), Some("done"));
    }

    #[test]
    fn undo_looks_past_pure_session_txns_to_the_prior_board_action() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "task".into(), None, None).unwrap();
        start_session_inner(&pool, item.id.clone()).unwrap();

        // Last txn is a pure SESSION_STARTED; undo must reverse the
        // create, not error out on an empty compensation set.
        let result = crate::commands::events::undo_last_action_inner(&pool).unwrap();
        assert_eq!(result.undone_event_types, vec!["ITEM_CREATED".to_string()]);
        let conn = pool.get().unwrap();
        let deleted: i64 = conn
            .query_row("SELECT deleted FROM items WHERE id = ?1", [&item.id], |r| r.get(0))
            .unwrap();
        assert_eq!(deleted, 1);
    }

    #[test]
    fn sessions_rebuild_identically_from_the_log() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        start_session_inner(&pool, item.id.clone()).unwrap();
        end_session_inner(&pool, SessionOutcome::Progress, None, Some("half the draft".into()))
            .unwrap();
        start_session_inner(&pool, item.id.clone()).unwrap();

        let snapshot = |pool: &SqlitePool| -> Vec<(String, String, i64, Option<i64>, Option<String>, Option<String>, Option<String>)> {
            let conn = pool.get().unwrap();
            let mut stmt = conn
                .prepare("SELECT id, item_id, started_at, ended_at, outcome, reason, note FROM sessions ORDER BY id")
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
                })
                .unwrap();
            rows.collect::<Result<_, _>>().unwrap()
        };
        let before = snapshot(&pool);
        assert_eq!(before.len(), 2);
        crate::commands::events::rebuild_projection_inner(&pool).unwrap();
        let after = snapshot(&pool);
        assert_eq!(before, after, "sessions obey the same purity law as items");
    }
}
