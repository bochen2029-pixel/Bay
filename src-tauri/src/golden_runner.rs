//! Golden-case RUNNER (P5a) — EXECUTES `contracts/golden/*.json`
//! against the real command layer + projection under `cargo test`.
//!
//! Until this module existed, `scripts/check-golden.py` verified only
//! that golden files EXIST with >=1 case — the exact gap the P2e
//! JOINT_WRONG (restore cap) slipped through: 143 tests were green
//! while the implementation contradicted `caps.json` #12. This runner
//! closes the gap: every case in `projection.json`, `swap.json`, and
//! `caps.json` runs against a fresh in-memory DB through the same
//! `*_inner` functions the Tauri commands call. (`rank.json` mirrors
//! `scripts/rank-fixtures.json`, which is already executed by the
//! Rust + TS parity suites.)
//!
//! Discipline:
//! - The runner FAILS LOUDLY on any op type or expectation key it does
//!   not recognize. Silent skips are how JOINT_WRONGs hide.
//! - The runner never writes golden files. Corrections to
//!   `_status: proposed` cases are ordinary source edits flagged for
//!   operator freeze; frozen files are operator-owned (charter §12).

use serde_json::Value;

use crate::commands::events::rebuild_projection_inner;
use crate::commands::items::{
    create_item_inner, delete_item_inner, edit_item_inner, move_item_inner, restore_item_inner,
    set_item_date_inner, set_item_state_inner, swap_move_inner,
};
use crate::db::{self, EventDraft, SqlitePool};
use crate::domain::{EventType, ItemState, Tier};

const PROJECTION_JSON: &str = include_str!("../../contracts/golden/projection.json");
const SWAP_JSON: &str = include_str!("../../contracts/golden/swap.json");
const CAPS_JSON: &str = include_str!("../../contracts/golden/caps.json");

// ── shared helpers ──────────────────────────────────────────────────

fn fresh_pool() -> SqlitePool {
    let manager = r2d2_sqlite::SqliteConnectionManager::memory();
    let pool = r2d2::Pool::builder().max_size(1).build(manager).unwrap();
    db::run_migrations(&pool).unwrap();
    pool
}

fn tier_of(s: &str) -> Tier {
    Tier::from_sql(s).unwrap_or_else(|| panic!("golden: unknown tier {s:?}"))
}

fn state_of(s: &str) -> ItemState {
    ItemState::from_sql(s).unwrap_or_else(|| panic!("golden: unknown state {s:?}"))
}

/// Golden files may pin either the exact error ("CAP_EXCEEDED") or a
/// code family ("BAD_ARGS" matching "BAD_ARGS: leaving and entering
/// must differ"). Prefix match covers both without loosening exact pins.
fn error_matches(expected: &str, actual: &str) -> bool {
    actual == expected || actual.starts_with(expected)
}

/// Full-width projection snapshot for rebuild-determinism comparison —
/// every `items` column, nested so each tuple stays within Rust's
/// 12-arity trait-impl limit.
type ItemRow = (
    (
        String,         // id
        String,         // content
        String,         // tier
        String,         // rank
        String,         // state
        Option<String>, // blocked_reason
        Option<i64>,    // start_at
    ),
    (
        Option<i64>,    // due_at
        Option<String>, // recurrence
        Option<String>, // first_step
        Option<String>, // today_on
        i64,            // created_at
        i64,            // updated_at
        i64,            // deleted
    ),
);

fn snapshot_items_full(pool: &SqlitePool) -> Vec<ItemRow> {
    let conn = pool.get().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, content, tier, rank, state, blocked_reason, start_at, due_at, \
             recurrence, first_step, today_on, created_at, updated_at, deleted \
             FROM items ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                (
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ),
                (
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                    r.get(12)?,
                    r.get(13)?,
                ),
            ))
        })
        .unwrap();
    rows.collect::<Result<_, _>>().unwrap()
}

/// Count items in a tier by metric: active|blocked|done (state, non-deleted),
/// total (non-deleted), deleted.
fn count_tier_metric(pool: &SqlitePool, tier: Tier, metric: &str) -> i64 {
    let conn = pool.get().unwrap();
    match metric {
        "active" | "blocked" | "done" => conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE tier = ?1 AND state = ?2 AND deleted = 0",
                rusqlite::params![tier.as_sql(), metric],
                |r| r.get(0),
            )
            .unwrap(),
        "total" => conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE tier = ?1 AND deleted = 0",
                rusqlite::params![tier.as_sql()],
                |r| r.get(0),
            )
            .unwrap(),
        "deleted" => conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE tier = ?1 AND deleted = 1",
                rusqlite::params![tier.as_sql()],
                |r| r.get(0),
            )
            .unwrap(),
        other => panic!("golden: unknown count metric {other:?}"),
    }
}

fn assert_rebuild_matches(pool: &SqlitePool, case: &str) {
    let before = snapshot_items_full(pool);
    rebuild_projection_inner(pool)
        .unwrap_or_else(|e| panic!("[{case}] rebuild_projection failed: {e}"));
    let after = snapshot_items_full(pool);
    assert_eq!(
        before, after,
        "[{case}] rebuild_projection must reproduce the items table exactly"
    );
}

// ── projection.json ─────────────────────────────────────────────────

#[test]
fn golden_projection_cases_execute() {
    let root: Value = serde_json::from_str(PROJECTION_JSON).expect("projection.json parses");
    let cases = root["cases"].as_array().expect("projection.json has cases");
    assert!(!cases.is_empty());
    for case in cases {
        run_projection_case(case);
    }
}

const PROJECTION_CASE_KEYS: &[&str] = &[
    "name",
    "description",
    "ops",
    "expect_items_non_deleted",
    "expect_items_non_deleted_count",
    "expect_items_deleted_count",
    "expect_active_counts",
    "expect_rebuild_matches",
    "expect_projection_unchanged_by_llm_event",
];

fn run_projection_case(case: &Value) {
    let name = case["name"].as_str().expect("case name");
    for key in case.as_object().unwrap().keys() {
        assert!(
            PROJECTION_CASE_KEYS.contains(&key.as_str()),
            "[{name}] unhandled projection case key {key:?} — extend the runner, never skip"
        );
    }

    let pool = fresh_pool();
    // Created ids in op order; `item_index` refs index into this.
    let mut ids: Vec<String> = Vec::new();
    let mut llm_left_projection_unchanged: Option<bool> = None;

    for op in case["ops"].as_array().expect("ops") {
        let op_type = op["type"].as_str().expect("op type");
        let expect_error = op["expect_error"].as_str();
        let result: Result<(), String> = match op_type {
            "create" => create_item_inner(
                &pool,
                tier_of(op["tier"].as_str().unwrap()),
                op["content"].as_str().unwrap().to_string(),
                None,
                None,
            )
            .map(|item| ids.push(item.id)),
            "edit" => edit_item_inner(
                &pool,
                ids[op["item_index"].as_u64().unwrap() as usize].clone(),
                op["content"].as_str().unwrap().to_string(),
            )
            .map(|_| ()),
            "move" => move_item_inner(
                &pool,
                ids[op["item_index"].as_u64().unwrap() as usize].clone(),
                tier_of(op["to_tier"].as_str().unwrap()),
                op["to_rank"].as_str().map(String::from),
                None,
            )
            .map(|_| ()),
            "set_state" => set_item_state_inner(
                &pool,
                ids[op["item_index"].as_u64().unwrap() as usize].clone(),
                state_of(op["state"].as_str().unwrap()),
                op["blocked_reason"].as_str().map(String::from),
            )
            .map(|_| ()),
            "set_date" => set_item_date_inner(
                &pool,
                ids[op["item_index"].as_u64().unwrap() as usize].clone(),
                op["field"].as_str().unwrap().to_string(),
                op["value_ms"].as_i64(),
            )
            .map(|_| ()),
            "delete" => {
                delete_item_inner(&pool, &ids[op["item_index"].as_u64().unwrap() as usize])
            }
            "restore" => restore_item_inner(
                &pool,
                &ids[op["item_index"].as_u64().unwrap() as usize],
            )
            .map(|_| ()),
            "swap" => swap_move_inner(
                &pool,
                ids[op["leaving_item_index"].as_u64().unwrap() as usize].clone(),
                tier_of(op["leaving_dest"].as_str().unwrap()),
                ids[op["entering_item_index"].as_u64().unwrap() as usize].clone(),
                tier_of(op["entering_tier"].as_str().unwrap()),
                "m".to_string(),
                None,
            )
            .map(|_| ()),
            "append_llm_suggestion_generated" => {
                let before = snapshot_items_full(&pool);
                let observations = op["observations"].clone();
                let r = db::write_event(&pool, |_tx, _ts| {
                    Ok(EventDraft {
                        event_type: EventType::LlmSuggestionGenerated,
                        item_id: None,
                        payload: serde_json::json!({
                            "kind": "analyze",
                            "scope": { "since_ts": 0, "until_ts": 0, "event_count": 0 },
                            "model": "golden-runner",
                            "observations": observations,
                        }),
                    })
                })
                .map(|_| ());
                let after = snapshot_items_full(&pool);
                llm_left_projection_unchanged = Some(before == after);
                r
            }
            other => panic!("[{name}] unhandled op type {other:?} — extend the runner"),
        };

        match (expect_error, result) {
            (Some(code), Err(actual)) => assert!(
                error_matches(code, &actual),
                "[{name}] op {op_type} expected error {code:?}, got {actual:?}"
            ),
            (Some(code), Ok(())) => {
                panic!("[{name}] op {op_type} expected error {code:?} but succeeded")
            }
            (None, Err(actual)) => {
                panic!("[{name}] op {op_type} unexpectedly failed: {actual}")
            }
            (None, Ok(())) => {}
        }
    }

    // ── expectations ────────────────────────────────────────────────
    if let Some(expected_list) = case["expect_items_non_deleted"].as_array() {
        let conn = pool.get().unwrap();
        let live = db::items::list_active_items(&conn).unwrap();
        assert_eq!(
            live.len(),
            expected_list.len(),
            "[{name}] non-deleted item count mismatch"
        );
        for expected in expected_list {
            let found = live.iter().any(|item| {
                expected["content"].as_str().is_none_or(|v| item.content == v)
                    && expected["tier"].as_str().is_none_or(|v| item.tier.as_sql() == v)
                    && expected["state"].as_str().is_none_or(|v| item.state.as_sql() == v)
                    && expected["deleted"].as_bool().is_none_or(|v| item.deleted == v)
                    && (expected.get("due_at").is_none() || item.due_at == expected["due_at"].as_i64())
                    && (expected.get("start_at").is_none() || item.start_at == expected["start_at"].as_i64())
            });
            assert!(found, "[{name}] no live item matches {expected}");
        }
    }
    if let Some(n) = case["expect_items_non_deleted_count"].as_i64() {
        let conn = pool.get().unwrap();
        let live = db::items::list_active_items(&conn).unwrap();
        assert_eq!(live.len() as i64, n, "[{name}] non-deleted count");
    }
    if let Some(n) = case["expect_items_deleted_count"].as_i64() {
        let conn = pool.get().unwrap();
        let deleted = db::items::list_deleted_items(&conn).unwrap();
        assert_eq!(deleted.len() as i64, n, "[{name}] deleted count");
    }
    if let Some(counts) = case["expect_active_counts"].as_object() {
        for (tier_str, expected) in counts {
            let actual = count_tier_metric(&pool, tier_of(tier_str), "active");
            assert_eq!(
                actual,
                expected.as_i64().unwrap(),
                "[{name}] active count for tier {tier_str}"
            );
        }
    }
    if case["expect_projection_unchanged_by_llm_event"].as_bool() == Some(true) {
        assert_eq!(
            llm_left_projection_unchanged,
            Some(true),
            "[{name}] LLM event must leave the projection byte-identical (firewall)"
        );
    }
    if case["expect_rebuild_matches"].as_bool() == Some(true) {
        assert_rebuild_matches(&pool, name);
    }
}

// ── swap.json ───────────────────────────────────────────────────────

#[test]
fn golden_swap_cases_execute() {
    let root: Value = serde_json::from_str(SWAP_JSON).expect("swap.json parses");
    let cases = root["cases"].as_array().expect("swap.json has cases");
    assert!(!cases.is_empty());
    for case in cases {
        run_swap_case(case);
    }
}

const SWAP_CASE_KEYS: &[&str] = &[
    "name",
    "description",
    "setup",
    "input",
    "expect_success",
    "expect_error",
    "expect_events_emitted",
    "expect_events_adjacent_ids",
    "expect_events_shared_ts",
    "expect_event_types",
    "expect_active_counts_after",
    "expect_leaving_item_tier_after",
    "expect_entering_item_tier_after",
];

fn run_swap_case(case: &Value) {
    let name = case["name"].as_str().expect("case name");
    for key in case.as_object().unwrap().keys() {
        assert!(
            SWAP_CASE_KEYS.contains(&key.as_str()),
            "[{name}] unhandled swap case key {key:?} — extend the runner, never skip"
        );
    }

    let pool = fresh_pool();
    let setup = &case["setup"];
    let mut a_ids: Vec<String> = Vec::new();
    let mut b_ids: Vec<String> = Vec::new();
    let mut inbox_ids: Vec<String> = Vec::new();

    // Setup vocabulary: a_items / b_items / inbox_items (all active), or
    // a_active + a_blocked (create the sum, block the tail). leaving_dest
    // in setup is informational (input repeats it).
    for key in setup.as_object().unwrap().keys() {
        assert!(
            ["a_items", "b_items", "inbox_items", "a_active", "a_blocked", "leaving_dest"]
                .contains(&key.as_str()),
            "[{name}] unhandled swap setup key {key:?}"
        );
    }
    let a_total = setup["a_items"]
        .as_i64()
        .unwrap_or(setup["a_active"].as_i64().unwrap_or(0) + setup["a_blocked"].as_i64().unwrap_or(0));
    for i in 0..a_total {
        a_ids.push(
            create_item_inner(&pool, Tier::A, format!("A-{i}"), None, None)
                .unwrap_or_else(|e| panic!("[{name}] setup create A-{i}: {e}"))
                .id,
        );
    }
    for i in 0..setup["a_blocked"].as_i64().unwrap_or(0) {
        let id = a_ids[a_ids.len() - 1 - i as usize].clone();
        set_item_state_inner(&pool, id, ItemState::Blocked, Some("setup-block".into()))
            .unwrap_or_else(|e| panic!("[{name}] setup block: {e}"));
    }
    for i in 0..setup["b_items"].as_i64().unwrap_or(0) {
        b_ids.push(
            create_item_inner(&pool, Tier::B, format!("B-{i}"), None, None)
                .unwrap_or_else(|e| panic!("[{name}] setup create B-{i}: {e}"))
                .id,
        );
    }
    for i in 0..setup["inbox_items"].as_i64().unwrap_or(0) {
        inbox_ids.push(
            create_item_inner(&pool, Tier::Inbox, format!("inbox-{i}"), None, None)
                .unwrap_or_else(|e| panic!("[{name}] setup create inbox-{i}: {e}"))
                .id,
        );
    }

    let max_event_id_after_setup: i64 = {
        let conn = pool.get().unwrap();
        conn.query_row("SELECT COALESCE(MAX(id), 0) FROM events", [], |r| r.get(0))
            .unwrap()
    };

    // Input resolution. leaving refs the entering tier's item list
    // ("last A item", "an A item", "n/a (swap not triggered)"); entering
    // is the inbox item or SAME (self-swap case).
    let input = &case["input"];
    let entering_tier = tier_of(input["entering_tier"].as_str().unwrap());
    let leaving_desc = input["leaving_item"].as_str().unwrap();
    let tier_list = match entering_tier {
        Tier::A => &a_ids,
        Tier::B => &b_ids,
        _ => panic!("[{name}] swap entering_tier must be A or B"),
    };
    let leaving_id = if leaving_desc.contains("last") {
        tier_list.last().expect("setup created tier items").clone()
    } else {
        tier_list.first().expect("setup created tier items").clone()
    };
    let entering_desc = input["entering_item"].as_str().unwrap();
    let entering_id = if entering_desc.contains("SAME") {
        leaving_id.clone()
    } else if entering_desc.contains("inbox") {
        inbox_ids.first().expect("setup created inbox item").clone()
    } else {
        panic!("[{name}] unhandled entering_item description {entering_desc:?}");
    };
    let leaving_dest = tier_of(input["leaving_dest"].as_str().unwrap());
    let reason = input["reason"].as_str().map(String::from);

    let result = swap_move_inner(
        &pool,
        leaving_id.clone(),
        leaving_dest,
        entering_id.clone(),
        entering_tier,
        "m".to_string(),
        reason,
    );

    // ── expectations ────────────────────────────────────────────────
    let expect_success = case["expect_success"].as_bool().expect("expect_success");
    match (&result, expect_success) {
        (Ok(_), true) => {}
        (Err(e), false) => {
            if let Some(code) = case["expect_error"].as_str() {
                assert!(
                    error_matches(code, e),
                    "[{name}] expected error {code:?}, got {e:?}"
                );
            }
        }
        (Ok(_), false) => panic!("[{name}] swap succeeded but golden expects failure"),
        (Err(e), true) => panic!("[{name}] swap failed but golden expects success: {e}"),
    }

    let new_events: Vec<(i64, i64, String)> = {
        let conn = pool.get().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, ts, type FROM events WHERE id > ?1 ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map([max_event_id_after_setup], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    };

    if let Some(n) = case["expect_events_emitted"].as_i64() {
        assert_eq!(new_events.len() as i64, n, "[{name}] events emitted");
    }
    if let Some(types) = case["expect_event_types"].as_array() {
        let actual: Vec<&str> = new_events.iter().map(|(_, _, t)| t.as_str()).collect();
        let expected: Vec<&str> = types.iter().map(|t| t.as_str().unwrap()).collect();
        assert_eq!(actual, expected, "[{name}] event types");
    }
    if case["expect_events_adjacent_ids"].as_bool() == Some(true) {
        for pair in new_events.windows(2) {
            assert_eq!(
                pair[1].0,
                pair[0].0 + 1,
                "[{name}] swap events must have adjacent ids"
            );
        }
    }
    if case["expect_events_shared_ts"].as_bool() == Some(true) {
        assert!(
            new_events.windows(2).all(|p| p[0].1 == p[1].1),
            "[{name}] swap events must share one ts"
        );
    }
    if let Some(counts) = case["expect_active_counts_after"].as_object() {
        for (key, expected) in counts {
            // "A" → active count; "A_active"/"A_blocked" → explicit metric.
            let (tier_str, metric) = match key.split_once('_') {
                Some((t, m)) => (t, m),
                None => (key.as_str(), "active"),
            };
            let actual = count_tier_metric(&pool, tier_of(tier_str), metric);
            assert_eq!(
                actual,
                expected.as_i64().unwrap(),
                "[{name}] count {key} after swap"
            );
        }
    }
    let conn = pool.get().unwrap();
    if let Some(t) = case["expect_leaving_item_tier_after"].as_str() {
        let actual: String = conn
            .query_row("SELECT tier FROM items WHERE id = ?1", [&leaving_id], |r| r.get(0))
            .unwrap();
        assert_eq!(actual, t, "[{name}] leaving item tier after swap");
    }
    if let Some(t) = case["expect_entering_item_tier_after"].as_str() {
        let actual: String = conn
            .query_row("SELECT tier FROM items WHERE id = ?1", [&entering_id], |r| r.get(0))
            .unwrap();
        assert_eq!(actual, t, "[{name}] entering item tier after swap");
    }
}

// ── caps.json ───────────────────────────────────────────────────────

#[test]
fn golden_caps_cases_execute() {
    let root: Value = serde_json::from_str(CAPS_JSON).expect("caps.json parses");
    let cases = root["cases"].as_array().expect("caps.json has cases");
    assert!(!cases.is_empty());
    for case in cases {
        run_caps_case(case);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpKind {
    Create,
    Move,
    SetState,
    Delete,
    Restore,
    Swap,
}

/// Map an `*_error` expectation key to the op kind it pins.
/// Examples seen in caps.json: "6th_error" / "13th_error" (the Nth
/// create), "6th_active_create_error", "undo_a0_active_error" (a
/// set_state back to active), "move_error", "restore_error".
fn kind_hint(key: &str) -> OpKind {
    let k = key.to_ascii_lowercase();
    if k.contains("create") {
        OpKind::Create
    } else if k.contains("move") {
        OpKind::Move
    } else if k.contains("restore") {
        OpKind::Restore
    } else if k.contains("swap") {
        OpKind::Swap
    } else if k.contains("delete") {
        OpKind::Delete
    } else if k.contains("undo") || k.contains("active") || k.contains("state") {
        OpKind::SetState
    } else if k
        .split('_')
        .next()
        .is_some_and(|t| t.ends_with("th") && t[..t.len() - 2].chars().all(|c| c.is_ascii_digit()))
    {
        // "6th_error", "13th_error" — ordinal creates.
        OpKind::Create
    } else {
        panic!("golden caps: cannot infer op kind from expectation key {key:?}");
    }
}

struct RecordedFailure {
    kind: OpKind,
    code: String,
    desc: String,
}

fn run_caps_case(case: &Value) {
    let name = case["name"].as_str().expect("case name");
    for key in case.as_object().unwrap().keys() {
        assert!(
            ["name", "description", "_corrected", "input", "expect"].contains(&key.as_str()),
            "[{name}] unhandled caps case key {key:?}"
        );
    }

    let pool = fresh_pool();
    // Per-tier created ids in creation order: refs like "a0"/"b0" index these.
    let mut created: std::collections::HashMap<char, Vec<String>> = std::collections::HashMap::new();
    let mut failures: Vec<RecordedFailure> = Vec::new();

    let resolve = |created: &std::collections::HashMap<char, Vec<String>>, r: &str| -> String {
        if r == "incoming" {
            return created.get(&'i').and_then(|v| v.first()).cloned().unwrap_or_else(|| {
                panic!("[{name}] ref 'incoming' but no inbox item created")
            });
        }
        let (letter, idx_str) = r.split_at(1);
        let idx: usize = idx_str.parse().unwrap_or_else(|_| panic!("[{name}] bad ref {r:?}"));
        let list = created
            .get(&letter.chars().next().unwrap())
            .unwrap_or_else(|| panic!("[{name}] ref {r:?}: no items created in tier"));
        list.get(idx)
            .cloned()
            .unwrap_or_else(|| panic!("[{name}] ref {r:?}: index out of range"))
    };

    for op_str in case["input"]["ops"].as_array().expect("ops").iter() {
        let op = op_str.as_str().expect("op string");
        let tokens: Vec<&str> = op.split_whitespace().collect();
        match tokens[0] {
            // "create <tier> x<N>"
            "create" => {
                let tier = tier_of(tokens[1]);
                let letter = match tier {
                    Tier::Inbox => 'i',
                    Tier::A => 'a',
                    Tier::B => 'b',
                    Tier::C => 'c',
                };
                let n: usize = tokens[2]
                    .strip_prefix('x')
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| panic!("[{name}] bad create count in {op:?}"));
                for i in 0..n {
                    let count_so_far = created.get(&letter).map_or(0, |v| v.len());
                    match create_item_inner(
                        &pool,
                        tier,
                        format!("{}-{}", tokens[1], count_so_far + i),
                        None,
                        None,
                    ) {
                        Ok(item) => created.entry(letter).or_default().push(item.id),
                        Err(code) => failures.push(RecordedFailure {
                            kind: OpKind::Create,
                            code,
                            desc: format!("{op} (create #{})", i + 1),
                        }),
                    }
                }
            }
            // "set_state <ref> <state> ['reason...']"
            "set_state" => {
                let id = resolve(&created, tokens[1]);
                let state = state_of(tokens[2]);
                let reason = if tokens.len() > 3 {
                    Some(tokens[3..].join(" ").trim_matches('\'').to_string())
                } else {
                    None
                };
                if let Err(code) = set_item_state_inner(&pool, id, state, reason) {
                    failures.push(RecordedFailure {
                        kind: OpKind::SetState,
                        code,
                        desc: op.to_string(),
                    });
                }
            }
            // "move <ref> -> <tier>"
            "move" => {
                let id = resolve(&created, tokens[1]);
                assert_eq!(tokens[2], "->", "[{name}] bad move syntax {op:?}");
                let to_tier = tier_of(tokens[3]);
                if let Err(code) = move_item_inner(&pool, id, to_tier, None, None) {
                    failures.push(RecordedFailure {
                        kind: OpKind::Move,
                        code,
                        desc: op.to_string(),
                    });
                }
            }
            // "delete <ref>"
            "delete" => {
                let id = resolve(&created, tokens[1]);
                if let Err(code) = delete_item_inner(&pool, &id) {
                    failures.push(RecordedFailure {
                        kind: OpKind::Delete,
                        code,
                        desc: op.to_string(),
                    });
                }
            }
            // "restore <ref>"
            "restore" => {
                let id = resolve(&created, tokens[1]);
                if let Err(code) = restore_item_inner(&pool, &id) {
                    failures.push(RecordedFailure {
                        kind: OpKind::Restore,
                        code,
                        desc: op.to_string(),
                    });
                }
            }
            // "swap incoming-><T>, <ref>-><T>"  (entering first, leaving second)
            "swap" => {
                let rest = op.strip_prefix("swap ").unwrap();
                let (entering_part, leaving_part) = rest
                    .split_once(", ")
                    .unwrap_or_else(|| panic!("[{name}] bad swap syntax {op:?}"));
                let (entering_ref, entering_tier_str) = entering_part
                    .split_once("->")
                    .unwrap_or_else(|| panic!("[{name}] bad swap entering {op:?}"));
                let (leaving_ref, leaving_dest_str) = leaving_part
                    .split_once("->")
                    .unwrap_or_else(|| panic!("[{name}] bad swap leaving {op:?}"));
                let entering_id = resolve(&created, entering_ref.trim());
                let leaving_id = resolve(&created, leaving_ref.trim());
                if let Err(code) = swap_move_inner(
                    &pool,
                    leaving_id,
                    tier_of(leaving_dest_str.trim()),
                    entering_id,
                    tier_of(entering_tier_str.trim()),
                    "m".to_string(),
                    None,
                ) {
                    failures.push(RecordedFailure {
                        kind: OpKind::Swap,
                        code,
                        desc: op.to_string(),
                    });
                }
            }
            other => panic!("[{name}] unhandled caps op {other:?} in {op:?}"),
        }
    }

    // ── expectations ────────────────────────────────────────────────
    // 1) The set of recorded failures must EXACTLY match the *_error
    //    expectations (kind + code). 2) Every count key must match.
    //    3) Boolean *_succeed(s) keys must be true (they are implied by
    //    the exact-failure-set rule, but assert the value to catch a
    //    golden file that ever pins `false`).
    let mut expected_failures: Vec<(OpKind, String, String)> = Vec::new();
    for (key, value) in case["expect"].as_object().expect("expect").iter() {
        if key.ends_with("_error") {
            expected_failures.push((
                kind_hint(key),
                value.as_str().expect("error code").to_string(),
                key.clone(),
            ));
        } else if key.ends_with("_after") {
            // "<tier>_<metric>_after", e.g. "a_active_after", "inbox_active_after".
            let parts: Vec<&str> = key.split('_').collect();
            assert_eq!(parts.len(), 3, "[{name}] bad count key {key:?}");
            let tier = match parts[0] {
                "a" => Tier::A,
                "b" => Tier::B,
                "c" => Tier::C,
                "inbox" => Tier::Inbox,
                other => panic!("[{name}] bad tier in count key {other:?}"),
            };
            let actual = count_tier_metric(&pool, tier, parts[1]);
            assert_eq!(
                actual,
                value.as_i64().unwrap(),
                "[{name}] {key}"
            );
        } else if key.ends_with("_succeed") || key.ends_with("_succeeds") {
            assert_eq!(
                value.as_bool(),
                Some(true),
                "[{name}] runner only supports affirmative success pins for {key:?}"
            );
        } else {
            panic!("[{name}] unhandled caps expectation key {key:?} — extend the runner");
        }
    }

    for (kind, code, key) in &expected_failures {
        let pos = failures
            .iter()
            .position(|f| f.kind == *kind && error_matches(code, &f.code))
            .unwrap_or_else(|| {
                panic!(
                    "[{name}] expected a {kind:?} failure with code {code:?} ({key}), \
                     recorded failures: {:?}",
                    failures.iter().map(|f| format!("{:?}:{} ({})", f.kind, f.code, f.desc)).collect::<Vec<_>>()
                )
            });
        failures.remove(pos);
    }
    assert!(
        failures.is_empty(),
        "[{name}] unexpected op failures beyond the golden expectations: {:?}",
        failures
            .iter()
            .map(|f| format!("{:?}:{} ({})", f.kind, f.code, f.desc))
            .collect::<Vec<_>>()
    );
}
