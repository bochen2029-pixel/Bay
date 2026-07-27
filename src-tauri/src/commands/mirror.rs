//! v0.3 Mirror: deterministic feedback, computed in SQL + one pass over
//! the log. **No LLM required** — VISION §3.5 inverts the old dependency
//! order (facts are free; interpretation is the optional add-on). The
//! coach may narrate these numbers later; it never produces them.
//!
//! Every figure here is derived from recorded behavior, never from
//! self-report (principle 5): throughput and lead time from the event
//! log, avoidance from the sessions table, block cost from blocked
//! durations, Today honesty from planned-vs-expired-vs-finished.
//!
//! Little's law is the spine: lead time = WIP / throughput. Bay already
//! caps WIP (A≤5, B≤12, Today≤3); the Mirror shows what those caps buy.

use std::collections::HashMap;

use rusqlite::OptionalExtension;
use serde::Serialize;
use tauri::State;

use crate::db::{self, SqlitePool};
use crate::domain::{EventType, Tier};

const DAY_MS: i64 = 86_400_000;
/// A→C/inbox demotion within this window of entering A is the "A is
/// being used as an inbox" signal (CLAUDE.md LLM-scope example #3).
const LEAK_WINDOW_MS: i64 = 2 * DAY_MS;
const DEFAULT_WINDOW_DAYS: i64 = 30;
const RECEIPT_LIMIT: usize = 10;
const AVOIDANCE_LIMIT: usize = 10;

#[derive(Debug, Default, Serialize)]
pub struct TierCounts {
    pub inbox: i64,
    pub a: i64,
    pub b: i64,
    pub c: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct FlowStats {
    pub created: i64,
    pub completed: i64,
    /// Completions per 7 days across the window — the denominator in
    /// Little's law.
    pub throughput_per_week: f64,
    /// Median days from creation to completion, over items completed in
    /// the window. `None` when nothing completed.
    pub lead_time_p50_days: Option<f64>,
    pub lead_time_p90_days: Option<f64>,
    /// WIP / throughput_per_week, in days — the queueing-theory
    /// prediction for how long a newly promoted item will take. Shown
    /// beside the measured lead time; a wide gap means the board holds
    /// work it never actually starts.
    pub littles_law_days: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
pub struct LeakStats {
    /// Active-item departures from A to C/inbox in the window.
    pub departures: i64,
    /// …of which happened within 48h of the item entering A.
    pub fast_leaks: i64,
    pub rate: f64,
}

#[derive(Debug, Serialize)]
pub struct AvoidanceRow {
    pub item_id: String,
    pub content: String,
    pub tier: String,
    pub days_since_touch: i64,
    pub sessions: i64,
    pub has_first_step: bool,
}

#[derive(Debug, Serialize)]
pub struct BlockRow {
    pub reason: String,
    pub count: i64,
    pub total_days: f64,
}

#[derive(Debug, Default, Serialize)]
pub struct SessionStats {
    pub count: i64,
    pub total_minutes: f64,
    pub median_minutes: Option<f64>,
    pub done: i64,
    pub progress: i64,
    pub interrupted: i64,
    /// Interruption cause → count. The personal taxonomy, unprompted.
    pub interruptions: Vec<(String, i64)>,
}

#[derive(Debug, Default, Serialize)]
pub struct TodayHonesty {
    pub planned: i64,
    pub finished: i64,
    /// Planned days that ended without the item being done — logged by
    /// the day-roll, reported here. No guilt banner; just the delta.
    pub expired: i64,
}

#[derive(Debug, Serialize)]
pub struct ReceiptRow {
    pub item_id: String,
    pub content: String,
    pub tier: String,
    pub done_at: i64,
    pub days_to_done: f64,
    pub sessions: i64,
    pub minutes: f64,
}

#[derive(Debug, Serialize)]
pub struct MirrorStats {
    pub window_days: i64,
    pub generated_at: i64,
    pub wip: TierCounts,
    pub flow: FlowStats,
    pub a_leak: LeakStats,
    pub avoidance: Vec<AvoidanceRow>,
    pub blocks: Vec<BlockRow>,
    pub sessions: SessionStats,
    pub today: TodayHonesty,
    pub receipts: Vec<ReceiptRow>,
}

#[tauri::command]
pub fn get_mirror_stats(
    pool: State<'_, SqlitePool>,
    window_days: Option<i64>,
) -> Result<MirrorStats, String> {
    get_mirror_stats_inner(&pool, window_days)
}

/// Per-item state accumulated while walking the log once. Everything
/// the Mirror needs that a point-in-time query can't answer (how long
/// an item sat in A before being demoted; whether it was on Today when
/// it was finished) lives here.
#[derive(Default)]
struct Walk {
    created_at: Option<i64>,
    entered_a_at: Option<i64>,
    done_at: Option<i64>,
    on_today: bool,
    blocked_since: Option<i64>,
    blocked_reason: Option<String>,
}

pub fn get_mirror_stats_inner(
    pool: &SqlitePool,
    window_days: Option<i64>,
) -> Result<MirrorStats, String> {
    let window_days = window_days.unwrap_or(DEFAULT_WINDOW_DAYS).clamp(1, 3650);
    let now = db::unix_ms_now();
    let since = now - window_days * DAY_MS;
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;

    // ── one pass over the whole log ─────────────────────────────────
    // The full log (not just the window) is walked so that an item
    // created before the window still has a known creation time and A
    // entry — the window only gates what gets COUNTED.
    let mut stmt = conn
        .prepare("SELECT ts, type, item_id, payload FROM events ORDER BY id")
        .map_err(|e| format!("prepare mirror walk: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| format!("query mirror walk: {e}"))?;

    let mut walks: HashMap<String, Walk> = HashMap::new();
    let mut flow = FlowStats::default();
    let mut leak = LeakStats::default();
    let mut today = TodayHonesty::default();
    let mut block_totals: HashMap<String, (i64, i64)> = HashMap::new(); // reason → (count, ms)
    // Completions are keyed by item, not appended per event: a
    // mis-tapped done → Ctrl+Z → real done must count as ONE completion,
    // not two (Bay actively encourages that undo). The last completion
    // per item wins; an item whose completion was undone and never
    // redone drops out entirely.
    let mut completions: HashMap<String, (i64, Option<i64>)> = HashMap::new(); // id → (done_at, lead_ms?)
    let mut today_finished: std::collections::HashSet<String> = std::collections::HashSet::new();

    for row in rows {
        let (ts, type_str, item_id, payload_str) =
            row.map_err(|e| format!("mirror row: {e}"))?;
        let event_type = match EventType::from_sql(&type_str) {
            Some(t) => t,
            None => continue, // forward-compat: unknown type, skip
        };
        let payload: serde_json::Value = serde_json::from_str(&payload_str)
            .map_err(|e| format!("mirror payload parse: {e}"))?;
        let id = match item_id {
            Some(id) => id,
            None => {
                // Ritual audit rows carry no item; DAY_* need no
                // per-item accounting here.
                continue;
            }
        };
        let w = walks.entry(id.clone()).or_default();

        match event_type {
            EventType::ItemCreated => {
                w.created_at = Some(ts);
                if payload["tier"].as_str() == Some(Tier::A.as_sql()) {
                    w.entered_a_at = Some(ts);
                }
                if ts >= since {
                    flow.created += 1;
                }
            }
            EventType::ItemMoved => {
                let before = payload["tier_before"].as_str().unwrap_or("");
                let after = payload["tier_after"].as_str().unwrap_or("");
                if after == "A" && before != "A" {
                    w.entered_a_at = Some(ts);
                }
                if before == "A" && (after == "C" || after == "inbox") && ts >= since {
                    leak.departures += 1;
                    if let Some(entered) = w.entered_a_at {
                        if ts - entered < LEAK_WINDOW_MS {
                            leak.fast_leaks += 1;
                        }
                    }
                }
            }
            EventType::ItemStateChanged => {
                let after = payload["state_after"].as_str().unwrap_or("");
                let before = payload["state_before"].as_str().unwrap_or("");
                if before == "blocked" {
                    // Leaving blocked closes the interval.
                    if let (Some(start), Some(reason)) =
                        (w.blocked_since.take(), w.blocked_reason.take())
                    {
                        if ts >= since {
                            let e = block_totals.entry(reason).or_insert((0, 0));
                            e.0 += 1;
                            e.1 += ts - start;
                        }
                    }
                }
                if after == "blocked" {
                    w.blocked_since = Some(ts);
                    w.blocked_reason = Some(
                        payload["blocked_reason"]
                            .as_str()
                            .unwrap_or("(no reason)")
                            .to_string(),
                    );
                }
                if after == "done" {
                    w.done_at = Some(ts);
                    if ts >= since {
                        if w.on_today {
                            today_finished.insert(id.clone());
                        }
                        // Recorded even when the creation event is
                        // unknown: the completion happened, so it counts
                        // toward throughput. Only the lead time is
                        // unknowable, and `None` drops it from the
                        // percentiles rather than the count.
                        completions.insert(id.clone(), (ts, w.created_at.map(|c| ts - c)));
                    }
                } else if before == "done" {
                    // Un-done (undo or reactivation): the item is back
                    // in flight, so it is not a completion right now.
                    // A later re-completion re-inserts it.
                    w.done_at = None;
                    completions.remove(&id);
                    today_finished.remove(&id);
                }
            }
            EventType::TodayAdded => {
                w.on_today = true;
                if ts >= since {
                    today.planned += 1;
                }
            }
            EventType::TodayRemoved => {
                w.on_today = false;
                // "Rolled over" means *planned but not finished*. An
                // item that was completed keeps its membership until
                // the roll, so counting every expiry would double-count
                // finished work as slippage — the opposite of honest.
                if ts >= since
                    && payload["cause"].as_str() == Some("expired")
                    && w.done_at.is_none()
                {
                    today.expired += 1;
                }
            }
            _ => {}
        }
    }
    drop(stmt);

    // Still-open blocked intervals count up to now.
    for w in walks.values() {
        if let (Some(start), Some(reason)) = (w.blocked_since, w.blocked_reason.clone()) {
            let e = block_totals.entry(reason).or_insert((0, 0));
            e.0 += 1;
            e.1 += now - start;
        }
    }

    leak.rate = if leak.departures > 0 {
        leak.fast_leaks as f64 / leak.departures as f64
    } else {
        0.0
    };

    // ── flow figures ────────────────────────────────────────────────
    flow.completed = completions.len() as i64;
    today.finished = today_finished.len() as i64;
    flow.throughput_per_week = flow.completed as f64 * 7.0 / window_days as f64;
    let mut leads: Vec<f64> = completions
        .values()
        .filter_map(|(_, ms)| ms.map(|ms| ms as f64 / DAY_MS as f64))
        .collect();
    leads.sort_by(|a, b| a.partial_cmp(b).unwrap());
    flow.lead_time_p50_days = percentile(&leads, 0.50);
    flow.lead_time_p90_days = percentile(&leads, 0.90);

    // ── WIP (point-in-time, from the projection) ────────────────────
    let mut wip = TierCounts::default();
    {
        let mut stmt = conn
            .prepare(
                "SELECT tier, COUNT(*) FROM items \
                 WHERE state = 'active' AND deleted = 0 GROUP BY tier",
            )
            .map_err(|e| format!("prepare wip: {e}"))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| format!("query wip: {e}"))?;
        for row in rows {
            let (tier, n) = row.map_err(|e| format!("wip row: {e}"))?;
            match tier.as_str() {
                "inbox" => wip.inbox = n,
                "A" => wip.a = n,
                "B" => wip.b = n,
                "C" => wip.c = n,
                _ => {}
            }
        }
    }
    let committed_wip = (wip.a + wip.b) as f64;
    flow.littles_law_days = if flow.throughput_per_week > 0.0 {
        Some(committed_wip / flow.throughput_per_week * 7.0)
    } else {
        None
    };

    // ── avoidance: committed work with no recorded attention ────────
    // THE procrastination metric, and the one v0.2 structurally could
    // not answer. Ordered by longest untouched.
    let avoidance = {
        let mut stmt = conn
            .prepare(
                "SELECT i.id, i.content, i.tier, i.updated_at, i.first_step IS NOT NULL, \
                        (SELECT COUNT(*) FROM sessions s WHERE s.item_id = i.id) \
                 FROM items i \
                 WHERE i.state = 'active' AND i.deleted = 0 AND i.tier IN ('A','B') \
                 ORDER BY i.updated_at ASC",
            )
            .map_err(|e| format!("prepare avoidance: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AvoidanceRow {
                    item_id: r.get(0)?,
                    content: r.get(1)?,
                    tier: r.get(2)?,
                    days_since_touch: (now - r.get::<_, i64>(3)?) / DAY_MS,
                    has_first_step: r.get(4)?,
                    sessions: r.get(5)?,
                })
            })
            .map_err(|e| format!("query avoidance: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("avoidance row: {e}"))?
            .into_iter()
            .filter(|row| row.sessions == 0)
            .take(AVOIDANCE_LIMIT)
            .collect::<Vec<_>>()
    };

    // ── blocks: what actually holds work up, and for how long ───────
    let mut blocks: Vec<BlockRow> = block_totals
        .into_iter()
        .map(|(reason, (count, ms))| BlockRow {
            reason,
            count,
            total_days: ms as f64 / DAY_MS as f64,
        })
        .collect();
    blocks.sort_by(|a, b| b.total_days.partial_cmp(&a.total_days).unwrap());
    blocks.truncate(10);

    // ── sessions: recorded attention ────────────────────────────────
    let sessions = {
        let mut stmt = conn
            .prepare(
                "SELECT started_at, ended_at, outcome, reason FROM sessions \
                 WHERE started_at >= ?1 AND ended_at IS NOT NULL",
            )
            .map_err(|e| format!("prepare sessions: {e}"))?;
        let rows = stmt
            .query_map([since], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|e| format!("query sessions: {e}"))?;
        let mut stats = SessionStats::default();
        let mut minutes: Vec<f64> = Vec::new();
        let mut causes: HashMap<String, i64> = HashMap::new();
        for row in rows {
            let (start, end, outcome, reason) = row.map_err(|e| format!("session row: {e}"))?;
            stats.count += 1;
            let mins = (end - start) as f64 / 60_000.0;
            stats.total_minutes += mins;
            minutes.push(mins);
            match outcome.as_str() {
                "done" => stats.done += 1,
                "progress" => stats.progress += 1,
                "interrupted" => {
                    stats.interrupted += 1;
                    if let Some(r) = reason {
                        *causes.entry(r).or_insert(0) += 1;
                    }
                }
                _ => {}
            }
        }
        minutes.sort_by(|a, b| a.partial_cmp(b).unwrap());
        stats.median_minutes = percentile(&minutes, 0.50);
        let mut causes: Vec<(String, i64)> = causes.into_iter().collect();
        causes.sort_by(|a, b| b.1.cmp(&a.1));
        stats.interruptions = causes;
        stats
    };

    // ── receipts: finished work, kept visible as evidence ───────────
    // Receipts are driven by the SAME completion ledger as the flow
    // figures, not by a separate `updated_at` query. Sorting or
    // filtering on last-touch while displaying completion time made the
    // list disagree with itself: an item finished long ago but edited
    // yesterday sorted to the top showing an old date, and an item
    // completed *before* the window could appear in it.
    let receipts = {
        let mut recent: Vec<(&String, i64, Option<i64>)> = completions
            .iter()
            .map(|(id, (done_at, lead))| (id, *done_at, *lead))
            .collect();
        // Tie-break by id: batch completions share one `ts`, and
        // `completions` is a HashMap, so without this the order within
        // a tied group — and therefore which of them survives the
        // limit — reshuffles between calls.
        recent.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

        // NOTE: no `truncate` before the loop. Soft-deleted items drop
        // out below, so truncating first would let deletions shrink the
        // list — delete the ten most recent completions and the panel
        // would go empty while dozens of surviving ones sat in the
        // window. Completed work stays visible as evidence (law 9).
        let mut out = Vec::new();
        for (id, done_at, lead_ms) in recent {
            if out.len() >= RECEIPT_LIMIT {
                break;
            }
            let row: Option<(String, String, i64, i64)> = conn
                .query_row(
                    "SELECT i.content, i.tier, \
                            (SELECT COUNT(*) FROM sessions s WHERE s.item_id = i.id), \
                            (SELECT COALESCE(SUM(s.ended_at - s.started_at), 0) FROM sessions s \
                             WHERE s.item_id = i.id AND s.ended_at IS NOT NULL) \
                     FROM items i WHERE i.id = ?1 AND i.deleted = 0",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()
                .map_err(|e| format!("query receipt: {e}"))?;
            // A completed item that was later deleted drops out — the
            // receipts list is evidence of work you still have.
            let (content, tier, sessions, ms) = match row {
                Some(r) => r,
                None => continue,
            };
            out.push(ReceiptRow {
                item_id: id.clone(),
                content,
                tier,
                done_at,
                days_to_done: lead_ms.map(|ms| ms as f64 / DAY_MS as f64).unwrap_or(0.0),
                sessions,
                minutes: ms as f64 / 60_000.0,
            });
        }
        out
    };

    Ok(MirrorStats {
        window_days,
        generated_at: now,
        wip,
        flow,
        a_leak: leak,
        avoidance,
        blocks,
        sessions,
        today,
        receipts,
    })
}

/// Linear-interpolation percentile over a pre-sorted slice.
fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    if sorted.len() == 1 {
        return Some(sorted[0]);
    }
    let pos = p * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    Some(sorted[lo] + (sorted[hi] - sorted[lo]) * frac)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::day::add_to_today_inner;
    use crate::commands::items::{create_item_inner, move_item_inner, set_item_state_inner};
    use crate::commands::session::{end_session_inner, start_session_inner};
    use crate::domain::{ItemState, SessionOutcome};
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    fn fresh_pool() -> SqlitePool {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        db::run_migrations(&pool).unwrap();
        pool
    }

    #[test]
    fn empty_log_yields_zeroed_stats_not_an_error() {
        let pool = fresh_pool();
        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.flow.created, 0);
        assert_eq!(stats.flow.completed, 0);
        assert_eq!(stats.flow.lead_time_p50_days, None);
        assert_eq!(stats.a_leak.rate, 0.0);
        assert!(stats.avoidance.is_empty());
        assert_eq!(stats.window_days, DEFAULT_WINDOW_DAYS);
    }

    #[test]
    fn counts_creation_completion_and_wip() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "one".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::B, "two".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::C, "three".into(), None, None).unwrap();
        set_item_state_inner(&pool, a.id.clone(), ItemState::Done, None).unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.flow.created, 3);
        assert_eq!(stats.flow.completed, 1);
        assert_eq!((stats.wip.a, stats.wip.b, stats.wip.c), (0, 1, 1));
        // Completed in-window with a known creation → a lead time exists.
        assert!(stats.flow.lead_time_p50_days.is_some());
        // Little's law needs throughput > 0; 1 completion in 30d gives it.
        assert!(stats.flow.littles_law_days.is_some());
    }

    #[test]
    fn a_leak_counts_only_fast_demotions_out_of_a() {
        let pool = fresh_pool();
        let fast = create_item_inner(&pool, Tier::A, "leaked".into(), None, None).unwrap();
        move_item_inner(&pool, fast.id.clone(), Tier::C, None, None).unwrap();
        // A B→C move is not an A departure and must not count at all.
        let other = create_item_inner(&pool, Tier::B, "unrelated".into(), None, None).unwrap();
        move_item_inner(&pool, other.id.clone(), Tier::C, None, None).unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.a_leak.departures, 1);
        assert_eq!(stats.a_leak.fast_leaks, 1, "same-moment demotion is inside 48h");
        assert_eq!(stats.a_leak.rate, 1.0);
    }

    #[test]
    fn avoidance_lists_only_committed_items_with_zero_sessions() {
        let pool = fresh_pool();
        let worked = create_item_inner(&pool, Tier::A, "worked on".into(), None, None).unwrap();
        let avoided = create_item_inner(&pool, Tier::A, "avoided".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::C, "someday".into(), None, None).unwrap();
        start_session_inner(&pool, worked.id.clone()).unwrap();
        end_session_inner(&pool, SessionOutcome::Progress, None, None).unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        let ids: Vec<&str> = stats.avoidance.iter().map(|r| r.item_id.as_str()).collect();
        assert_eq!(ids, vec![avoided.id.as_str()], "C is not committed; worked has a session");
    }

    #[test]
    fn block_map_aggregates_by_reason() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::A, "y".into(), None, None).unwrap();
        set_item_state_inner(&pool, a.id.clone(), ItemState::Blocked, Some("waiting on Marco".into()))
            .unwrap();
        set_item_state_inner(&pool, b.id.clone(), ItemState::Blocked, Some("waiting on Marco".into()))
            .unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.blocks.len(), 1, "one reason, aggregated");
        assert_eq!(stats.blocks[0].reason, "waiting on Marco");
        assert_eq!(stats.blocks[0].count, 2, "still-open intervals count to now");
    }

    #[test]
    fn session_stats_split_by_outcome_and_cause() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        start_session_inner(&pool, a.id.clone()).unwrap();
        end_session_inner(&pool, SessionOutcome::Progress, None, None).unwrap();
        start_session_inner(&pool, a.id.clone()).unwrap();
        end_session_inner(&pool, SessionOutcome::Interrupted, Some("meeting".into()), None).unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.sessions.count, 2);
        assert_eq!((stats.sessions.progress, stats.sessions.interrupted), (1, 1));
        assert_eq!(stats.sessions.interruptions, vec![("meeting".to_string(), 1)]);
        assert!(stats.sessions.median_minutes.is_some());
    }

    #[test]
    fn today_honesty_counts_only_unfinished_work_as_rolled_over() {
        // "Rolled over" must mean *planned but not finished*. A
        // completed item keeps its Today membership until the roll
        // (progress stays visible), so counting every expiry would
        // report finished work as slippage — the opposite of honest.
        let pool = fresh_pool();
        let done = create_item_inner(&pool, Tier::A, "finish me".into(), None, None).unwrap();
        let slip = create_item_inner(&pool, Tier::A, "slipped".into(), None, None).unwrap();
        add_to_today_inner(&pool, done.id.clone(), "2026-07-25".into()).unwrap();
        add_to_today_inner(&pool, slip.id.clone(), "2026-07-25".into()).unwrap();
        set_item_state_inner(&pool, done.id.clone(), ItemState::Done, None).unwrap();
        crate::commands::day::roll_day_inner(&pool, "2026-07-26".into()).unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.today.planned, 2);
        assert_eq!(stats.today.finished, 1, "done while on Today");
        assert_eq!(stats.today.expired, 1, "only the unfinished item rolled over");
    }

    #[test]
    fn a_completion_that_was_undone_and_redone_counts_once() {
        // Bay actively encourages Ctrl+Z after a mis-tapped done
        // (session.rs regression-tests it), so throughput and lead time
        // must not inflate every time it happens.
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "mis-tapped".into(), None, None).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Done, None).unwrap();
        crate::commands::events::undo_last_action_inner(&pool).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Done, None).unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.flow.completed, 1, "one item finished, not two");
        assert_eq!(stats.receipts.len(), 1);
    }

    #[test]
    fn an_undone_completion_stops_counting_until_it_is_redone() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "reopened".into(), None, None).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Done, None).unwrap();
        crate::commands::events::undo_last_action_inner(&pool).unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.flow.completed, 0, "it is back in flight, not finished");
        assert_eq!(stats.flow.lead_time_p50_days, None);
    }

    #[test]
    fn receipts_are_ordered_by_completion_not_by_last_touch() {
        // The list is evidence of what you finished and when. Ordering
        // it by `updated_at` while displaying `done_at` made it
        // disagree with itself: edit an old item and it jumps to the
        // top wearing an old date.
        let pool = fresh_pool();
        let first = create_item_inner(&pool, Tier::A, "finished first".into(), None, None).unwrap();
        set_item_state_inner(&pool, first.id.clone(), ItemState::Done, None).unwrap();
        let second = create_item_inner(&pool, Tier::A, "finished second".into(), None, None).unwrap();
        set_item_state_inner(&pool, second.id.clone(), ItemState::Done, None).unwrap();
        // Touch the OLDER completion last.
        crate::commands::items::edit_item_inner(&pool, first.id.clone(), "finished first (v2)".into())
            .unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.receipts.len(), 2);
        assert_eq!(
            stats.receipts[0].item_id, second.id,
            "most recently COMPLETED comes first, regardless of later edits"
        );
    }

    #[test]
    fn a_deleted_completion_leaves_the_receipts_list() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "shipped then binned".into(), None, None).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Done, None).unwrap();
        crate::commands::items::delete_item_inner(&pool, &item.id).unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert!(stats.receipts.is_empty(), "receipts show work you still have");
        assert_eq!(stats.flow.completed, 1, "but the completion still happened");
    }

    #[test]
    fn receipt_done_at_is_the_completion_not_a_later_edit() {
        // updated_at moves on any later touch; the receipt must report
        // when the work actually finished.
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "shipped".into(), None, None).unwrap();
        set_item_state_inner(&pool, item.id.clone(), ItemState::Done, None).unwrap();
        let done_ts: i64 = {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT ts FROM events WHERE type = 'ITEM_STATE_CHANGED' ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        // Touch the item afterwards (an edit is legal on a done item).
        crate::commands::items::edit_item_inner(&pool, item.id.clone(), "shipped (v2)".into())
            .unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.receipts.len(), 1);
        assert_eq!(
            stats.receipts[0].done_at, done_ts,
            "receipt reports the completion timestamp, not the last touch"
        );
    }

    #[test]
    fn receipts_carry_the_journey_of_finished_work() {
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "shipped it".into(), None, None).unwrap();
        start_session_inner(&pool, item.id.clone()).unwrap();
        end_session_inner(&pool, SessionOutcome::Done, None, None).unwrap();

        let stats = get_mirror_stats_inner(&pool, None).unwrap();
        assert_eq!(stats.receipts.len(), 1);
        let r = &stats.receipts[0];
        assert_eq!(r.content, "shipped it");
        assert_eq!(r.sessions, 1);
        assert!(r.days_to_done >= 0.0);
    }

    #[test]
    fn percentiles_interpolate() {
        assert_eq!(percentile(&[], 0.5), None);
        assert_eq!(percentile(&[4.0], 0.9), Some(4.0));
        assert_eq!(percentile(&[0.0, 10.0], 0.5), Some(5.0));
        assert_eq!(percentile(&[0.0, 10.0, 20.0], 0.5), Some(10.0));
    }
}
