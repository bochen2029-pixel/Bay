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
