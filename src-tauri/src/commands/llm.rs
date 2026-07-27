//! Analyze + accept/reject LLM-suggestion commands. All writes go
//! through the event log as LLM_SUGGESTION_GENERATED /
//! LLM_SUGGESTION_ACCEPTED / LLM_SUGGESTION_REJECTED — the LLM never
//! touches the items projection (CLAUDE.md §Design philosophy #2).

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use crate::commands::settings::SettingsState;
use crate::db::{self, EventDraft, SqlitePool};
use crate::domain::{rank_between, EventType, ItemState, Tier, A_CAP, B_CAP};
use crate::llm::compression::{self, AnalyzeContext};
use crate::llm::openai_compat::OpenAiCompatClient;
use crate::llm::parse::{parse_analysis, Observation, ProposalAction, ReorgProposal};
use crate::llm::prompt::{format_user_prompt, RETRY_PREFIX, SYSTEM_PROMPT};
use crate::llm::LlmConfig;

const ANALYZE_PROGRESS_EVENT: &str = "analyze_progress";
const MAX_COMPLETION_TOKENS: i64 = 800;

#[derive(Debug, Serialize)]
pub struct AnalyzeResult {
    pub suggestion_event_id: i64,
    pub observations: Vec<Observation>,
    /// Optional re-org diff the LLM proposed (I-20). Advisory until the
    /// human accepts it via accept_suggestion(ops); empty when the model
    /// proposed nothing.
    pub proposals: Vec<ReorgProposal>,
    pub scope: AnalyzeScope,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzeScope {
    pub since_ts: i64,
    pub until_ts: i64,
    pub event_count: i64,
    pub window_days: i64,
}

#[tauri::command]
pub async fn analyze(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    settings: State<'_, SettingsState>,
    window_days: Option<i64>,
) -> Result<AnalyzeResult, String> {
    // Snapshot settings + config synchronously; mutex must not cross
    // the await boundary.
    let (config, effective_window_days) = {
        let guard = settings.lock().map_err(|e| format!("settings lock: {e}"))?;
        let config = LlmConfig::from_settings(&guard);
        let w = window_days.unwrap_or(guard.analyze_window_days).max(1);
        (config, w)
    };

    emit_progress(&app, "compressing");

    // Compress: run the SQL aggregates. Uses a blocking task because
    // rusqlite is sync.
    let pool_for_compress = (*pool).clone();
    let settings_snapshot = {
        let guard = settings.lock().map_err(|e| format!("settings lock: {e}"))?;
        guard.clone()
    };
    let now_ms = unix_ms_now();
    let ctx: AnalyzeContext = tokio::task::spawn_blocking(move || {
        compression::compress(
            &pool_for_compress,
            effective_window_days,
            &settings_snapshot,
            now_ms,
        )
    })
    .await
    .map_err(|e| format!("compression join: {e}"))??;

    // Build prompts.
    let user_prompt = format_user_prompt(&ctx);
    emit_progress(&app, "calling_llm");

    let model = config.model.clone();
    let client = OpenAiCompatClient::new(config).map_err(|e| e.into_string())?;

    // First attempt, then one retry with a "return JSON only" prefix
    // on the user prompt if parse fails.
    let known_ids: HashSet<String> = ctx
        .inbox_list
        .iter()
        .chain(ctx.a_list.iter())
        .chain(ctx.b_list.iter())
        .map(|s| s.id.clone())
        .collect();

    let first_response = client
        .chat(SYSTEM_PROMPT, &user_prompt, MAX_COMPLETION_TOKENS)
        .await
        .map_err(|e| e.into_string())?;

    emit_progress(&app, "parsing");

    let analysis = match parse_analysis(&first_response, &known_ids) {
        Ok(a) => a,
        Err(first_err) => {
            // One retry with an explicit "JSON only" prefix.
            emit_progress(&app, "retrying_parse");
            let retry_user = format!("{RETRY_PREFIX}{user_prompt}");
            let retry_response = client
                .chat(SYSTEM_PROMPT, &retry_user, MAX_COMPLETION_TOKENS)
                .await
                .map_err(|e| e.into_string())?;
            match parse_analysis(&retry_response, &known_ids) {
                Ok(a) => a,
                Err(second_err) => {
                    // Emit the suggestion event with empty observations +
                    // error hint so the audit trail reflects that we tried.
                    let err_payload = json!({
                        "kind": "analyze",
                        "scope": {
                            "since_ts": ctx.since_ts,
                            "until_ts": ctx.until_ts,
                            "event_count": ctx.event_count,
                            "window_days": effective_window_days,
                        },
                        "model": model,
                        "observations": [],
                        "error": format!("first: {first_err}; retry: {second_err}"),
                    });
                    let _ = log_suggestion(&pool, err_payload);
                    return Err(format!("LLM_PARSE_ERROR: {second_err}"));
                }
            }
        }
    };

    let observations = analysis.observations;
    let proposals = analysis.proposals;

    let scope = AnalyzeScope {
        since_ts: ctx.since_ts,
        until_ts: ctx.until_ts,
        event_count: ctx.event_count,
        window_days: effective_window_days,
    };
    let payload = json!({
        "kind": "analyze",
        "scope": scope,
        "model": model,
        "observations": observations,
        // Record the proposed re-org in the suggestion event for audit.
        // Applying it requires an explicit human accept (firewall).
        "proposals": proposals,
    });

    let suggestion_event_id = log_suggestion(&pool, payload)?;

    Ok(AnalyzeResult {
        suggestion_event_id,
        observations,
        proposals,
        scope,
        model,
    })
}

/// Accept an LLM suggestion. With no `ops`, this is an observations-only
/// acknowledgement (resulting_event_ids stays empty). With `ops`, it
/// applies the human-accepted re-org diff ATOMICALLY through the
/// deterministic write path (cap-enforced) and records the resulting
/// event ids on the LLM_SUGGESTION_ACCEPTED event — the v2 surface the
/// doctrine preserved (CLAUDE.md §2: "LLM proposes, human accepts,
/// deterministic tier owns the write"). The firewall is intact: the LLM
/// never writes; this code does, only after an explicit human accept.
#[tauri::command]
pub fn accept_suggestion(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    suggestion_event_id: i64,
    ops: Option<Vec<ReorgProposal>>,
) -> Result<(), String> {
    let ops = ops.unwrap_or_default();
    if ops.is_empty() {
        return log_response(
            &pool,
            EventType::LlmSuggestionAccepted,
            suggestion_event_id,
            None,
        );
    }

    let affected = apply_reorg_inner(&pool, suggestion_event_id, ops)?;

    // Refresh the UI for each affected item (idempotent store handlers).
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let live = db::items::list_active_items(&conn)?;
    for id in &affected {
        if let Some(item) = live.iter().find(|i| &i.id == id) {
            let _ = app.emit("item_updated", item);
        }
    }
    Ok(())
}

/// Apply a human-accepted re-org as ONE atomic transaction: the resulting
/// ITEM_MOVED / ITEM_STATE_CHANGED events plus the LLM_SUGGESTION_ACCEPTED
/// audit event (carrying the resulting event ids) all land together or
/// not at all. Returns the distinct item ids touched.
fn apply_reorg_inner(
    pool: &SqlitePool,
    suggestion_event_id: i64,
    ops: Vec<ReorgProposal>,
) -> Result<Vec<String>, String> {
    use std::collections::HashMap;

    // The suggestion must exist (same guard as log_response).
    {
        let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE id = ?1 AND type = 'LLM_SUGGESTION_GENERATED'",
                [suggestion_event_id],
                |r| r.get(0),
            )
            .map_err(|e| format!("lookup suggestion: {e}"))?;
        if found == 0 {
            return Err("EVENT_NOT_FOUND".into());
        }
    }

    let mut affected: Vec<String> = Vec::new();
    // Provenance: this is a HUMAN write (the user accepted); origin
    // records which suggestion it executed (envelope v2). The LLM is
    // never an actor — it has no write path.
    let ctx = db::WriteCtx {
        origin: Some(format!("llm_accept:{suggestion_event_id}")),
        ..Default::default()
    };
    let _ = db::write_events_ctx(pool, ctx, |tx, _ts| {
        // resulting_event_ids must be known when we build the acceptance
        // event's payload, but ids aren't assigned until append. events is
        // append-only (no deletes, trigger-enforced), so AUTOINCREMENT ids
        // are gap-free: the next N ids are MAX(id)+1..MAX(id)+N.
        let base_id: i64 = tx
            .query_row("SELECT COALESCE(MAX(id), 0) FROM events", [], |r| r.get(0))
            .map_err(|e| format!("max event id: {e}"))?;

        // Working simulation of each referenced item (read once), mutated
        // as ops apply — drives both cap accounting and rank_before.
        let mut sim: HashMap<String, crate::domain::Item> = HashMap::new();
        for op in &ops {
            if !sim.contains_key(&op.item_id) {
                let it = db::items::read_item_by_id_tx(tx, &op.item_id)?
                    .ok_or_else(|| "ITEM_NOT_FOUND".to_string())?;
                sim.insert(op.item_id.clone(), it);
            }
        }
        let orig = sim.clone();

        // Chain ranks for multiple moves into the same tier (the projection
        // isn't updated until after this closure, so max_rank_in_tier would
        // return the same value for each move otherwise).
        let mut tier_last_rank: HashMap<Tier, Option<String>> = HashMap::new();
        // Shared across the accepted batch, exactly as in
        // batch_set_state: recurrence spawns stay cap-correct and
        // rank-distinct, and Today reactivations can't exceed 3.
        let mut spawn_acct = crate::commands::items::SpawnAccounting::default();
        let mut today_acct = crate::commands::day::TodayAccounting::default();

        let mut drafts: Vec<EventDraft> = Vec::new();
        for op in &ops {
            let cur = sim.get(&op.item_id).cloned().unwrap();
            match op.action {
                ProposalAction::Move => {
                    let to_tier = Tier::from_sql(op.to_tier.as_deref().unwrap_or(""))
                        .ok_or_else(|| "BAD_TIER".to_string())?;
                    if to_tier == cur.tier {
                        continue; // no-op move
                    }
                    let last = match tier_last_rank.get(&to_tier) {
                        Some(r) => r.clone(),
                        None => db::items::max_rank_in_tier(tx, to_tier)?,
                    };
                    let new_rank = rank_between(last.as_deref(), None);
                    tier_last_rank.insert(to_tier, Some(new_rank.clone()));
                    drafts.push(EventDraft {
                        event_type: EventType::ItemMoved,
                        item_id: Some(op.item_id.clone()),
                        payload: json!({
                            "tier_before": cur.tier,
                            "rank_before": cur.rank,
                            "tier_after": to_tier,
                            "rank_after": new_rank,
                            "reason": op.rationale.clone().unwrap_or_else(|| "LLM re-org".into()),
                        }),
                    });
                    let s = sim.get_mut(&op.item_id).unwrap();
                    s.tier = to_tier;
                    s.rank = new_rank;
                }
                ProposalAction::Done => {
                    if cur.state == ItemState::Done {
                        continue;
                    }
                    drafts.push(state_change_draft(&op.item_id, cur.state, ItemState::Done, None));
                    // I-21: completing through the accept-diff must
                    // behave like every other done-door, or a recurring
                    // item accepted here would silently stop recurring.
                    if let Some(spawn) = crate::commands::items::build_recurrence_spawn(
                        tx,
                        &cur,
                        _ts,
                        &mut spawn_acct,
                    )? {
                        drafts.extend(spawn);
                    }
                    sim.get_mut(&op.item_id).unwrap().state = ItemState::Done;
                }
                ProposalAction::Active => {
                    if cur.state == ItemState::Active {
                        continue;
                    }
                    // Today cap on this re-activation door (see
                    // day::today_overflow_draft).
                    if let Some(drop) = crate::commands::day::today_overflow_draft(
                        tx,
                        &cur,
                        &mut today_acct,
                    )? {
                        drafts.push(drop);
                    }
                    // Preserve the outgoing blocked reason so an undo can
                    // restore the blocked row (migration-002 CHECK).
                    let reason = if cur.state == ItemState::Blocked {
                        cur.blocked_reason.clone()
                    } else {
                        None
                    };
                    drafts.push(state_change_draft(
                        &op.item_id,
                        cur.state,
                        ItemState::Active,
                        reason,
                    ));
                    let s = sim.get_mut(&op.item_id).unwrap();
                    s.state = ItemState::Active;
                    s.blocked_reason = None;
                }
            }
        }

        // Cap check on the FINAL active counts of A and B. (Intermediate
        // over-cap is fine — the projection has no cap constraint; only the
        // committed end state must hold.)
        for (tier, cap) in [(Tier::A, A_CAP as i64), (Tier::B, B_CAP as i64)] {
            let base = db::items::count_active_in_tier(tx, tier)?;
            let orig_in = orig
                .values()
                .filter(|it| it.tier == tier && it.state == ItemState::Active)
                .count() as i64;
            let final_in = sim
                .values()
                .filter(|it| it.tier == tier && it.state == ItemState::Active)
                .count() as i64;
            if base - orig_in + final_in > cap {
                return Err("CAP_EXCEEDED".into());
            }
        }

        // Predict the ids the reorg drafts will receive, then append the
        // acceptance event last with that list.
        let n = drafts.len() as i64;
        let resulting: Vec<i64> = (1..=n).map(|k| base_id + k).collect();
        drafts.push(EventDraft {
            event_type: EventType::LlmSuggestionAccepted,
            item_id: None,
            payload: json!({
                "suggestion_event_id": suggestion_event_id,
                "resulting_event_ids": resulting,
            }),
        });

        // Distinct affected item ids, in first-seen order.
        for op in &ops {
            if !affected.contains(&op.item_id) {
                affected.push(op.item_id.clone());
            }
        }
        Ok(drafts)
    })?;

    Ok(affected)
}

fn state_change_draft(
    item_id: &str,
    before: ItemState,
    after: ItemState,
    blocked_reason: Option<String>,
) -> EventDraft {
    EventDraft {
        event_type: EventType::ItemStateChanged,
        item_id: Some(item_id.to_string()),
        payload: json!({
            "state_before": before,
            "state_after": after,
            "blocked_reason": blocked_reason,
        }),
    }
}

#[tauri::command]
pub fn reject_suggestion(
    pool: State<'_, SqlitePool>,
    suggestion_event_id: i64,
    reason: Option<String>,
) -> Result<(), String> {
    log_response(
        &pool,
        EventType::LlmSuggestionRejected,
        suggestion_event_id,
        reason,
    )
}

// ── helpers ──────────────────────────────────────────────────────

fn log_suggestion(pool: &SqlitePool, payload: serde_json::Value) -> Result<i64, String> {
    let event = db::write_event(pool, |_tx, _ts| {
        Ok(EventDraft {
            event_type: EventType::LlmSuggestionGenerated,
            item_id: None,
            payload,
        })
    })?;
    Ok(event.id)
}

fn log_response(
    pool: &SqlitePool,
    event_type: EventType,
    suggestion_event_id: i64,
    reason: Option<String>,
) -> Result<(), String> {
    // Verify the referenced suggestion event exists.
    {
        let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE id = ?1 AND type = 'LLM_SUGGESTION_GENERATED'",
                [suggestion_event_id],
                |r| r.get(0),
            )
            .map_err(|e| format!("lookup suggestion: {e}"))?;
        if found == 0 {
            return Err("EVENT_NOT_FOUND".into());
        }
    }

    let payload = match event_type {
        EventType::LlmSuggestionAccepted => json!({
            "suggestion_event_id": suggestion_event_id,
            // Observations-only accept: no item mutations, so no resulting
            // events. A re-org accept (with ops) goes through
            // apply_reorg_inner, which populates resulting_event_ids.
            "resulting_event_ids": Vec::<i64>::new(),
        }),
        EventType::LlmSuggestionRejected => json!({
            "suggestion_event_id": suggestion_event_id,
            "reason": reason,
        }),
        _ => unreachable!("log_response called with wrong event_type"),
    };

    let _ = db::write_event(pool, |_tx, _ts| {
        Ok(EventDraft {
            event_type,
            item_id: None,
            payload,
        })
    })?;
    Ok(())
}

fn emit_progress(app: &AppHandle, stage: &str) {
    let _ = app.emit(
        ANALYZE_PROGRESS_EVENT,
        json!({ "stage": stage }),
    );
}

fn unix_ms_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::create_item_inner;
    use crate::domain::Tier;
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    fn fresh_pool() -> SqlitePool {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        db::run_migrations(&pool).unwrap();
        pool
    }

    fn seed_suggestion(pool: &SqlitePool) -> i64 {
        log_suggestion(
            pool,
            json!({
                "kind": "analyze",
                "scope": { "since_ts": 0, "until_ts": 0, "event_count": 0, "window_days": 30 },
                "model": "test",
                "observations": [],
                "proposals": [],
            }),
        )
        .unwrap()
    }

    fn move_op(id: &str, to_tier: &str) -> ReorgProposal {
        ReorgProposal {
            item_id: id.to_string(),
            action: ProposalAction::Move,
            to_tier: Some(to_tier.to_string()),
            rationale: Some("test".into()),
        }
    }

    fn done_op(id: &str) -> ReorgProposal {
        ReorgProposal {
            item_id: id.to_string(),
            action: ProposalAction::Done,
            to_tier: None,
            rationale: None,
        }
    }

    fn count_active(pool: &SqlitePool, tier: Tier) -> i64 {
        let conn = pool.get().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM items WHERE tier = ?1 AND state = 'active' AND deleted = 0",
            [tier.as_sql()],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn accept_reorg_applies_moves_and_states_atomically() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "demote me".into(), None, None).unwrap();
        let b = create_item_inner(&pool, Tier::A, "finish me".into(), None, None).unwrap();
        let sug = seed_suggestion(&pool);

        let affected = apply_reorg_inner(
            &pool,
            sug,
            vec![move_op(&a.id, "C"), done_op(&b.id)],
        )
        .unwrap();
        assert_eq!(affected.len(), 2);

        let conn = pool.get().unwrap();
        let (a_tier, a_state): (String, String) = conn
            .query_row("SELECT tier, state FROM items WHERE id=?1", [&a.id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(a_tier, "C", "the move op must relocate the item");
        assert_eq!(a_state, "active");
        let b_state: String = conn
            .query_row("SELECT state FROM items WHERE id=?1", [&b.id], |r| r.get(0))
            .unwrap();
        assert_eq!(b_state, "done", "the done op must mark the item done");
    }

    #[test]
    fn accept_reorg_populates_resulting_event_ids() {
        // The LLM_SUGGESTION_ACCEPTED event's resulting_event_ids must
        // equal the ids of the reorg events it produced — the doctrine
        // surface that was empty forever before I-20.
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        let sug = seed_suggestion(&pool);
        apply_reorg_inner(&pool, sug, vec![move_op(&a.id, "C")]).unwrap();

        let conn = pool.get().unwrap();
        // The single reorg event is the ITEM_MOVED.
        let moved_id: i64 = conn
            .query_row(
                "SELECT id FROM events WHERE type='ITEM_MOVED' ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let accepted_payload: String = conn
            .query_row(
                "SELECT payload FROM events WHERE type='LLM_SUGGESTION_ACCEPTED' ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&accepted_payload).unwrap();
        let resulting = v["resulting_event_ids"].as_array().unwrap();
        assert_eq!(resulting.len(), 1, "one reorg event → one resulting id");
        assert_eq!(
            resulting[0].as_i64().unwrap(),
            moved_id,
            "resulting_event_ids must reference the actual ITEM_MOVED event"
        );
        assert_eq!(v["suggestion_event_id"].as_i64().unwrap(), sug);
    }

    #[test]
    fn accept_reorg_rolls_back_when_it_would_exceed_a_cap() {
        // A has 4 active; proposing to move 2 items from C into A would make
        // 6 active (cap 5) → CAP_EXCEEDED, whole accept rolls back.
        let pool = fresh_pool();
        for i in 0..4 {
            create_item_inner(&pool, Tier::A, format!("a-{i}"), None, None).unwrap();
        }
        let c1 = create_item_inner(&pool, Tier::C, "c1".into(), None, None).unwrap();
        let c2 = create_item_inner(&pool, Tier::C, "c2".into(), None, None).unwrap();
        let sug = seed_suggestion(&pool);

        let err = apply_reorg_inner(&pool, sug, vec![move_op(&c1.id, "A"), move_op(&c2.id, "A")])
            .unwrap_err();
        assert_eq!(err, "CAP_EXCEEDED");

        // Atomic rollback: A still 4 active, both items still in C, and NO
        // acceptance event was written.
        assert_eq!(count_active(&pool, Tier::A), 4);
        let conn = pool.get().unwrap();
        let in_c: i64 = conn
            .query_row("SELECT COUNT(*) FROM items WHERE tier='C' AND deleted=0", [], |r| r.get(0))
            .unwrap();
        assert_eq!(in_c, 2);
        let accepted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE type='LLM_SUGGESTION_ACCEPTED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(accepted, 0, "no acceptance event on a rolled-back reorg");
    }

    #[test]
    fn accept_reorg_swap_style_within_cap_succeeds() {
        // Net-zero on A: demote one A item to C and promote one C item to A
        // in the same accept. Final A active stays 5 → allowed even though
        // a naive per-op check might trip.
        let pool = fresh_pool();
        let mut a_ids = Vec::new();
        for i in 0..5 {
            a_ids.push(create_item_inner(&pool, Tier::A, format!("a-{i}"), None, None).unwrap().id);
        }
        let c = create_item_inner(&pool, Tier::C, "promote me".into(), None, None).unwrap();
        let sug = seed_suggestion(&pool);

        apply_reorg_inner(&pool, sug, vec![move_op(&a_ids[0], "C"), move_op(&c.id, "A")])
            .unwrap();
        assert_eq!(count_active(&pool, Tier::A), 5, "net active count unchanged");
    }

    #[test]
    fn accept_reorg_unknown_suggestion_is_event_not_found() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        let err = apply_reorg_inner(&pool, 9999, vec![move_op(&a.id, "C")]).unwrap_err();
        assert_eq!(err, "EVENT_NOT_FOUND");
    }

    #[test]
    fn accept_reorg_done_spawns_the_recurrence_like_every_other_done_door() {
        // Regression: the accept-diff is an entry path like any other.
        // If it completes a recurring item without spawning the next
        // instance, accepting one LLM suggestion silently stops the
        // recurrence — a bug the user would discover weeks later.
        use crate::commands::items::set_item_recurrence_inner;
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "weekly report".into(), None, None).unwrap();
        set_item_recurrence_inner(&pool, item.id.clone(), Some("FREQ=WEEKLY".into())).unwrap();
        let sug = seed_suggestion(&pool);

        apply_reorg_inner(
            &pool,
            sug,
            vec![ReorgProposal {
                item_id: item.id.clone(),
                action: ProposalAction::Done,
                to_tier: None,
                rationale: None,
            }],
        )
        .unwrap();

        let conn = pool.get().unwrap();
        let children: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE content = 'weekly report' AND state = 'active'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(children, 1, "the next instance was spawned");
        let linked: i64 = conn
            .query_row("SELECT COUNT(*) FROM events WHERE type = 'ITEM_RECURRED'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(linked, 1, "and the audit link records it");
    }
}
