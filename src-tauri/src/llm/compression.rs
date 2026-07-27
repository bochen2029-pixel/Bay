//! Event-log compression into a compact prompt input. SQL-driven
//! aggregates, never raw events (SPEC §8.3). The goal is <=2500
//! tokens on the system+user side so Ollama's default context is
//! sufficient and latency stays ~5s on llama3.2.

use std::collections::HashMap;

use serde::Serialize;

use crate::db::{items, SqlitePool};
use crate::domain::{Item, ItemState, Tier};
use crate::settings::Settings;

const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;
/// Max Inbox items to enumerate in the prompt. Inbox is unbounded;
/// feeding hundreds of strings to the LLM blows past the context
/// budget with little incremental insight.
const MAX_INBOX_LIST: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzeContext {
    pub window_days: i64,
    pub since_ts: i64,
    pub until_ts: i64,
    pub event_count: i64,
    pub created_in_window: i64,
    pub created_by_tier: HashMap<String, i64>,
    pub done_in_window: i64,
    pub blocked_current: i64,
    pub inbox_count: i64,
    pub inbox_list: Vec<ItemSummary>,
    pub a_list: Vec<ItemSummary>,
    pub b_list: Vec<ItemSummary>,
    pub c_count: i64,
    pub stale_list: Vec<StaleItem>,
    // ── behavior (v0.3) ────────────────────────────────────────────
    // Until v0.3 the coach could only see board TOPOLOGY, so its
    // sharpest possible observation was "this item is old". With
    // sessions recorded it can see what actually happened: time spent,
    // what broke focus, and — the one that matters — committed items
    // with no recorded attention at all.
    pub sessions_in_window: i64,
    pub session_minutes_in_window: i64,
    pub sessions_by_outcome: HashMap<String, i64>,
    pub interruptions_by_cause: HashMap<String, i64>,
    /// Committed (A/B) active items with ZERO sessions ever, oldest
    /// untouched first. THE procrastination signal.
    pub never_started: Vec<StaleItem>,
    pub today_planned_in_window: i64,
    pub today_finished_in_window: i64,
    pub today_expired_in_window: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemSummary {
    pub id: String,
    pub content: String,
    pub tier: String,
    pub state: String,
    pub days_in_tier: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StaleItem {
    pub id: String,
    pub content: String,
    pub tier: String,
    pub days_untouched: i64,
    pub threshold_days: i64,
}

pub fn compress(
    pool: &SqlitePool,
    window_days: i64,
    settings: &Settings,
    now_ms: i64,
) -> Result<AnalyzeContext, String> {
    let since_ts = now_ms - window_days * MS_PER_DAY;
    let until_ts = now_ms;

    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;

    // ── Event counts in window ────────────────────────────────────
    let event_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE ts >= ?1 AND ts <= ?2",
            [since_ts, until_ts],
            |r| r.get(0),
        )
        .map_err(|e| format!("event count: {e}"))?;

    let created_in_window: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE type = 'ITEM_CREATED' \
             AND ts >= ?1 AND ts <= ?2",
            [since_ts, until_ts],
            |r| r.get(0),
        )
        .map_err(|e| format!("created count: {e}"))?;

    let done_in_window: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE type = 'ITEM_STATE_CHANGED' \
             AND ts >= ?1 AND ts <= ?2 \
             AND json_extract(payload, '$.state_after') = 'done'",
            [since_ts, until_ts],
            |r| r.get(0),
        )
        .map_err(|e| format!("done count: {e}"))?;

    // Created-by-tier from ITEM_CREATED payloads. Tier lives in
    // payload.tier so we can't aggregate via a column directly.
    let mut created_by_tier: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT json_extract(payload, '$.tier') AS tier, COUNT(*) \
                 FROM events WHERE type = 'ITEM_CREATED' \
                 AND ts >= ?1 AND ts <= ?2 GROUP BY tier",
            )
            .map_err(|e| format!("prep created_by_tier: {e}"))?;
        let rows = stmt
            .query_map([since_ts, until_ts], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| format!("query created_by_tier: {e}"))?;
        for row in rows {
            let (tier, count) = row.map_err(|e| format!("row: {e}"))?;
            created_by_tier.insert(tier, count);
        }
    }

    // ── Current projection ────────────────────────────────────────
    let items = items::list_active_items(&conn)?;

    let mut inbox: Vec<&Item> = Vec::new();
    let mut a_items: Vec<&Item> = Vec::new();
    let mut b_items: Vec<&Item> = Vec::new();
    let mut c_count: i64 = 0;
    let mut inbox_count_total: i64 = 0;
    let mut blocked_current: i64 = 0;

    for item in &items {
        if item.state == ItemState::Blocked {
            blocked_current += 1;
        }
        match item.tier {
            Tier::Inbox => {
                inbox_count_total += 1;
                inbox.push(item);
            }
            Tier::A => a_items.push(item),
            Tier::B => b_items.push(item),
            Tier::C => c_count += 1,
        }
    }

    // Sort each non-C list by rank for deterministic prompt order.
    inbox.sort_by(|a, b| a.rank.cmp(&b.rank));
    a_items.sort_by(|a, b| a.rank.cmp(&b.rank));
    b_items.sort_by(|a, b| a.rank.cmp(&b.rank));

    let inbox_list = inbox
        .iter()
        .take(MAX_INBOX_LIST)
        .map(|it| summarize(it, now_ms))
        .collect();
    let a_list = a_items.iter().map(|it| summarize(it, now_ms)).collect();
    let b_list = b_items.iter().map(|it| summarize(it, now_ms)).collect();

    // ── Stale items (active only, tier threshold) ─────────────────
    let mut stale_list: Vec<StaleItem> = Vec::new();
    for item in &items {
        if item.state != ItemState::Active {
            continue;
        }
        let threshold = match item.tier {
            Tier::Inbox => settings.staleness_inbox_days,
            Tier::A => settings.staleness_a_days,
            Tier::B => settings.staleness_b_days,
            Tier::C => settings.staleness_c_days,
        };
        let Some(days) = threshold else { continue };
        let untouched = (now_ms - item.updated_at) / MS_PER_DAY;
        if untouched > days {
            stale_list.push(StaleItem {
                id: item.id.clone(),
                content: truncate(&item.content, 80),
                tier: item.tier.as_sql().into(),
                days_untouched: untouched,
                threshold_days: days,
            });
        }
    }

    // ── behavior aggregates (v0.3) ────────────────────────────────
    let (sessions_in_window, session_ms): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(ended_at - started_at), 0) FROM sessions \
             WHERE started_at >= ?1 AND started_at <= ?2 AND ended_at IS NOT NULL",
            [since_ts, until_ts],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("session totals: {e}"))?;

    let mut sessions_by_outcome: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT outcome, COUNT(*) FROM sessions \
                 WHERE started_at >= ?1 AND started_at <= ?2 AND outcome IS NOT NULL \
                 GROUP BY outcome",
            )
            .map_err(|e| format!("prepare outcomes: {e}"))?;
        let rows = stmt
            .query_map([since_ts, until_ts], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| format!("query outcomes: {e}"))?;
        for row in rows {
            let (k, n) = row.map_err(|e| format!("outcome row: {e}"))?;
            sessions_by_outcome.insert(k, n);
        }
    }

    let mut interruptions_by_cause: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT reason, COUNT(*) FROM sessions \
                 WHERE started_at >= ?1 AND started_at <= ?2 AND reason IS NOT NULL \
                 GROUP BY reason",
            )
            .map_err(|e| format!("prepare causes: {e}"))?;
        let rows = stmt
            .query_map([since_ts, until_ts], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| format!("query causes: {e}"))?;
        for row in rows {
            let (k, n) = row.map_err(|e| format!("cause row: {e}"))?;
            interruptions_by_cause.insert(k, n);
        }
    }

    // Committed work with no recorded attention, ever. Deliberately
    // NOT windowed: "you have never started this" is the claim, and a
    // 30-day window would quietly weaken it to "not lately".
    let never_started = {
        let mut stmt = conn
            .prepare(
                "SELECT i.id, i.content, i.tier, i.updated_at FROM items i \
                 WHERE i.state = 'active' AND i.deleted = 0 AND i.tier IN ('A','B') \
                   AND NOT EXISTS (SELECT 1 FROM sessions s WHERE s.item_id = i.id) \
                 ORDER BY i.updated_at ASC LIMIT 10",
            )
            .map_err(|e| format!("prepare never_started: {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                let updated_at: i64 = r.get(3)?;
                Ok(StaleItem {
                    id: r.get(0)?,
                    content: truncate(&r.get::<_, String>(1)?, 80),
                    tier: r.get(2)?,
                    days_untouched: (now_ms - updated_at).max(0) / MS_PER_DAY,
                    threshold_days: 0, // not a threshold signal; zero sessions is the signal
                })
            })
            .map_err(|e| format!("query never_started: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("never_started row: {e}"))?
    };

    let today_event_count = |event_type: &str, cause: Option<&str>| -> Result<i64, String> {
        let sql = match cause {
            Some(_) => format!(
                "SELECT COUNT(*) FROM events WHERE type = '{event_type}' \
                 AND ts >= ?1 AND ts <= ?2 AND json_extract(payload, '$.cause') = ?3"
            ),
            None => format!(
                "SELECT COUNT(*) FROM events WHERE type = '{event_type}' \
                 AND ts >= ?1 AND ts <= ?2"
            ),
        };
        match cause {
            Some(c) => conn
                .query_row(&sql, rusqlite::params![since_ts, until_ts, c], |r| r.get(0))
                .map_err(|e| format!("today count: {e}")),
            None => conn
                .query_row(&sql, rusqlite::params![since_ts, until_ts], |r| r.get(0))
                .map_err(|e| format!("today count: {e}")),
        }
    };
    let today_planned_in_window = today_event_count("TODAY_ADDED", None)?;
    let today_expired_in_window = today_event_count("TODAY_REMOVED", Some("expired"))?;
    // Finished-while-on-Today: a done transition on an item that still
    // carries a today_on (membership survives completion by design).
    let today_finished_in_window: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT e.item_id) FROM events e \
             JOIN items i ON i.id = e.item_id \
             WHERE e.type = 'ITEM_STATE_CHANGED' AND e.ts >= ?1 AND e.ts <= ?2 \
               AND json_extract(e.payload, '$.state_after') = 'done' \
               AND i.today_on IS NOT NULL",
            [since_ts, until_ts],
            |r| r.get(0),
        )
        .map_err(|e| format!("today finished: {e}"))?;

    Ok(AnalyzeContext {
        window_days,
        since_ts,
        until_ts,
        event_count,
        created_in_window,
        created_by_tier,
        done_in_window,
        blocked_current,
        inbox_count: inbox_count_total,
        inbox_list,
        a_list,
        b_list,
        c_count,
        stale_list,
        sessions_in_window,
        session_minutes_in_window: session_ms / 60_000,
        sessions_by_outcome,
        interruptions_by_cause,
        never_started,
        today_planned_in_window,
        today_finished_in_window,
        today_expired_in_window,
    })
}

fn summarize(item: &Item, now_ms: i64) -> ItemSummary {
    let days_in_tier = (now_ms - item.created_at).max(0) / MS_PER_DAY;
    ItemSummary {
        id: item.id.clone(),
        content: truncate(&item.content, 120),
        tier: item.tier.as_sql().into(),
        state: item.state.as_sql().into(),
        days_in_tier,
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars - 1).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::{create_item_inner, set_item_state_inner};
    use crate::commands::session::{end_session_inner, start_session_inner};
    use crate::db;
    use crate::domain::{ItemState, SessionOutcome, Tier, A_CAP, B_CAP};
    use crate::llm::prompt::format_user_prompt;

    fn fresh_pool() -> SqlitePool {
        let manager = r2d2_sqlite::SqliteConnectionManager::memory();
        let pool = r2d2::Pool::builder().max_size(1).build(manager).unwrap();
        db::run_migrations(&pool).unwrap();
        pool
    }

    fn ctx_of(pool: &SqlitePool) -> AnalyzeContext {
        compress(pool, 30, &Settings::default(), db::unix_ms_now()).unwrap()
    }

    #[test]
    fn behavior_aggregates_reach_the_context_and_the_prompt() {
        let pool = fresh_pool();
        let worked = create_item_inner(&pool, Tier::A, "worked on".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::A, "never touched".into(), None, None).unwrap();
        start_session_inner(&pool, worked.id.clone()).unwrap();
        end_session_inner(&pool, SessionOutcome::Progress, None, None).unwrap();
        start_session_inner(&pool, worked.id.clone()).unwrap();
        end_session_inner(&pool, SessionOutcome::Interrupted, Some("meeting".into()), None)
            .unwrap();

        let ctx = ctx_of(&pool);
        assert_eq!(ctx.sessions_in_window, 2);
        assert_eq!(ctx.sessions_by_outcome.get("progress"), Some(&1));
        assert_eq!(ctx.interruptions_by_cause.get("meeting"), Some(&1));

        // The item with sessions must NOT appear as never-started; the
        // untouched one must.
        let never: Vec<&str> = ctx.never_started.iter().map(|s| s.content.as_str()).collect();
        assert_eq!(never, vec!["never touched"]);

        // And all of it has to actually reach the model.
        let prompt = format_user_prompt(&ctx);
        assert!(prompt.contains("ATTENTION"), "{prompt}");
        assert!(prompt.contains("meeting"), "{prompt}");
        assert!(prompt.contains("COMMITTED BUT NEVER STARTED"), "{prompt}");
        assert!(prompt.contains("never touched"), "{prompt}");
    }

    #[test]
    fn never_started_is_committed_tiers_only_and_survives_the_window() {
        // "You have never started this" must not quietly weaken to "not
        // lately", and C/Inbox are not commitments.
        let pool = fresh_pool();
        create_item_inner(&pool, Tier::C, "someday".into(), None, None).unwrap();
        create_item_inner(&pool, Tier::Inbox, "untriaged".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::B, "staged".into(), None, None).unwrap();

        // A 1-day window must still report the never-started B item.
        let ctx = compress(&pool, 1, &Settings::default(), db::unix_ms_now()).unwrap();
        let never: Vec<&str> = ctx.never_started.iter().map(|s| s.content.as_str()).collect();
        assert_eq!(never, vec!["staged"]);
        assert_eq!(ctx.sessions_in_window, 0);
        drop(b);
    }

    #[test]
    fn today_honesty_reaches_the_prompt() {
        let pool = fresh_pool();
        let done = create_item_inner(&pool, Tier::A, "finish me".into(), None, None).unwrap();
        let slip = create_item_inner(&pool, Tier::A, "slipped".into(), None, None).unwrap();
        crate::commands::day::add_to_today_inner(&pool, done.id.clone(), "2026-07-25".into())
            .unwrap();
        crate::commands::day::add_to_today_inner(&pool, slip.id.clone(), "2026-07-25".into())
            .unwrap();
        set_item_state_inner(&pool, done.id.clone(), ItemState::Done, None).unwrap();
        crate::commands::day::roll_day_inner(&pool, "2026-07-26".into()).unwrap();

        let ctx = ctx_of(&pool);
        assert_eq!(ctx.today_planned_in_window, 2);
        assert_eq!(ctx.today_expired_in_window, 2, "both memberships aged out");
        let prompt = format_user_prompt(&ctx);
        assert!(prompt.contains("2 planned"), "{prompt}");
    }

    #[test]
    fn prompt_stays_within_the_token_budget_on_a_busy_board() {
        // SPEC §8.3 targets <= 2500 tokens of system+user. The v0.3
        // behavior section adds bounded lists (<=10 never-started, plus
        // fixed-size aggregate lines), so this must not blow the budget
        // on a realistic board. ~4 chars/token is the usual estimate.
        let pool = fresh_pool();
        for i in 0..A_CAP {
            create_item_inner(&pool, Tier::A, format!("A item number {i}"), None, None).unwrap();
        }
        for i in 0..B_CAP {
            create_item_inner(&pool, Tier::B, format!("B item number {i}"), None, None).unwrap();
        }
        for i in 0..40 {
            create_item_inner(&pool, Tier::C, format!("C item number {i}"), None, None).unwrap();
            create_item_inner(&pool, Tier::Inbox, format!("inbox item {i}"), None, None).unwrap();
        }
        let ctx = ctx_of(&pool);
        let chars = crate::llm::prompt::SYSTEM_PROMPT.len() + format_user_prompt(&ctx).len();
        let approx_tokens = chars / 4;
        assert!(
            approx_tokens < 2500,
            "prompt grew to ~{approx_tokens} tokens (SPEC §8.3 budget is 2500)"
        );
    }
}
