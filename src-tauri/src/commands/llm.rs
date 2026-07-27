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

    let outcome = apply_reorg_inner(&pool, suggestion_event_id, ops)?;

    // Refresh the UI for each affected item (idempotent store handlers).
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let live = db::items::list_active_items(&conn)?;
    for id in &outcome.affected {
        if let Some(item) = live.iter().find(|i| &i.id == id) {
            let _ = app.emit("item_updated", item);
        }
    }
    // Announce spawned recurrence children too — `AnalyzePanel` closes
    // without refetching, so without this the next instance would sit
    // in the database, invisible until the app restarts.
    for id in &outcome.spawned_ids {
        if let Some(item) = live.iter().find(|i| &i.id == id) {
            let _ = app.emit("item_created", item);
        }
    }
    Ok(())
}

/// Apply a human-accepted re-org as ONE atomic transaction: the resulting
/// ITEM_MOVED / ITEM_STATE_CHANGED events plus the LLM_SUGGESTION_ACCEPTED
/// audit event (carrying the resulting event ids) all land together or
/// not at all. Returns the distinct item ids touched.
#[derive(Debug)]
pub(crate) struct ReorgOutcome {
    /// Item ids named by the accepted ops, first-seen order.
    affected: Vec<String>,
    /// Recurrence children spawned by accepted `done` ops. They reach
    /// the UI through `item_created`, like every other spawn path — an
    /// item that exists in SQLite but not on the board until restart is
    /// a bug the user experiences as "my repeating task vanished".
    spawned_ids: Vec<String>,
}

pub(crate) fn apply_reorg_inner(
    pool: &SqlitePool,
    suggestion_event_id: i64,
    ops: Vec<ReorgProposal>,
) -> Result<ReorgOutcome, String> {
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

    // Several ops on one item are legal and coherent — "unblock it and
    // demote it" is a proposal pair a model produces routinely, and the
    // simulation applies them in order. (An earlier build rejected them
    // outright, which turned one such pair into a total accept failure
    // that discarded every other accepted op.) Repeated completions
    // cannot double-spawn: `completed` is a set, resolved once, after
    // every op has been applied.
    let mut affected: Vec<String> = Vec::new();
    let mut spawned_ids: Vec<String> = Vec::new();
    // Provenance: this is a HUMAN write (the user accepted); origin
    // records which suggestion it executed (envelope v2). The LLM is
    // never an actor — it has no write path.
    let ctx = db::WriteCtx {
        origin: Some(format!("llm_accept:{suggestion_event_id}")),
        ..Default::default()
    };
    let _ = db::write_events_ctx(pool, ctx, |tx, ts| {
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

        // ONE rank ledger for the whole transaction. Moves and
        // recurrence spawns both place items at the end of a tier, and
        // `rank_between` is deterministic — so two maps seeded from the
        // same untouched projection hand out byte-identical ranks, and
        // a later drag between the two collides. (There is no UNIQUE
        // constraint on (tier, rank); it commits silently.)
        let mut tier_last_rank: HashMap<Tier, Option<String>> = HashMap::new();
        // Items this diff leaves newly done / newly active. Derived
        // effects are resolved from these AFTER every op is applied.
        let mut completed: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut reactivated: Vec<String> = Vec::new();

        // End-of-tier rank in the SIMULATED world.
        let next_rank = |tier: Tier,
                             tier_last_rank: &mut HashMap<Tier, Option<String>>,
                             tx: &rusqlite::Transaction<'_>|
         -> Result<String, String> {
            let last = match tier_last_rank.get(&tier) {
                Some(r) => r.clone(),
                None => db::items::max_rank_in_tier(tx, tier)?,
            };
            let rank = rank_between(last.as_deref(), None);
            tier_last_rank.insert(tier, Some(rank.clone()));
            Ok(rank)
        };

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
                    let new_rank = next_rank(to_tier, &mut tier_last_rank, tx)?;
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
                    // Carry the OUTGOING blocked reason, exactly as
                    // every other done-door does (items.rs single +
                    // batch, session.rs). Without it, undoing a
                    // blocked→done accept writes `state = 'blocked'`
                    // with a null reason, trips the migration-002 CHECK,
                    // and the undo transaction rolls back — leaving
                    // Ctrl+Z permanently dead on that transaction, since
                    // undo keeps targeting it. This is the P2e
                    // BLOCKING-1 bug class, and it was the last door
                    // still missing the fix.
                    let reason = if cur.state == ItemState::Blocked {
                        cur.blocked_reason.clone()
                    } else {
                        None
                    };
                    drafts.push(state_change_draft(
                        &op.item_id,
                        cur.state,
                        ItemState::Done,
                        reason,
                    ));
                    sim.get_mut(&op.item_id).unwrap().state = ItemState::Done;
                    completed.insert(op.item_id.clone());
                }
                ProposalAction::Active => {
                    if cur.state == ItemState::Active {
                        continue;
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
                    completed.remove(&op.item_id); // done-then-active: not a completion
                    if !reactivated.contains(&op.item_id) {
                        reactivated.push(op.item_id.clone());
                    }
                }
            }
        }

        // ── Cap check on the human's own ops ────────────────────────
        // Intermediate over-cap is fine — the projection has no cap
        // constraint; only the committed end state must hold. Derived
        // effects (spawns) are resolved AFTER this and are placed where
        // they fit, so they can never turn a legal diff into a failure.
        for (tier, cap) in [(Tier::A, A_CAP as i64), (Tier::B, B_CAP as i64)] {
            if effective_active(tx, &orig, &sim, tier)? > cap {
                return Err("CAP_EXCEEDED".into());
            }
        }

        // ── Pass 2: derived effects, from the FINISHED simulation ───
        //
        // Everything above is what the human accepted. Everything below
        // is what those acceptances imply. Resolving the implications
        // incrementally — inside the loop, against a half-applied
        // simulation — made the outcome depend on the ORDER the model
        // happened to list its proposals: an op not yet visited still
        // reads as a no-op, so the same accepted set could commit or
        // fail, and could keep or lose a Today slot, purely on array
        // order. The LLM has no write path, but that would have handed
        // it a lever on the deterministic tier's result, which is the
        // spirit of the firewall if not its letter.

        // I-21: a completed recurring item spawns its next instance
        // here, exactly as every other done-door does. Placement reads
        // the finished simulation, and a full tier overflows to Inbox
        // (SPEC §8.7) — a spawn must NEVER fail the accept.
        //
        // Ordered by BOARD POSITION, not by the ops array. When two
        // recurring items complete into a tier with one free slot,
        // somebody's child overflows to Inbox — and that decision must
        // not be the model's to make by listing order. Sorting by
        // (tier, rank) gives the slot to the item the HUMAN ranked
        // higher, which is both deterministic and the answer they would
        // defend.
        let mut spawn_candidates: Vec<&String> = completed.iter().collect();
        spawn_candidates.sort_by(|a, b| board_order(&orig, &sim, a).cmp(&board_order(&orig, &sim, b)));
        for id in spawn_candidates {
            let parent = sim.get(id).cloned().unwrap();
            let rule = match crate::commands::items::recurrence_rule_of(&parent) {
                Some(r) => r,
                None => continue,
            };
            let cap = match parent.tier {
                Tier::A => Some(A_CAP as i64),
                Tier::B => Some(B_CAP as i64),
                Tier::Inbox | Tier::C => None,
            };
            let child_tier = match cap {
                Some(cap) if effective_active(tx, &orig, &sim, parent.tier)? >= cap => Tier::Inbox,
                _ => parent.tier,
            };
            let child_rank = next_rank(child_tier, &mut tier_last_rank, tx)?;
            let (child_id, spawn) = crate::commands::items::recurrence_child_drafts(
                &parent, ts, rule, child_tier, child_rank.clone(),
            );
            // Register the child so a later sibling spawn counts it.
            // Modelled faithfully rather than cloned wholesale: a stale
            // date or first_step inherited here would be invisible today
            // (only tier+state are read) and wrong the moment anything
            // else consults the simulation.
            let mut child = parent.clone();
            child.id = child_id.clone();
            child.tier = child_tier;
            child.rank = child_rank;
            child.state = ItemState::Active;
            child.today_on = None;
            child.first_step = None;
            child.blocked_reason = None;
            child.due_at = Some(rule.next_after(parent.due_at.unwrap_or(ts)));
            child.start_at = parent.start_at.map(|s| rule.next_after(s));
            child.created_at = ts;
            child.updated_at = ts;
            sim.insert(child_id.clone(), child);
            spawned_ids.push(child_id);
            drafts.extend(spawn);
        }

        // Today cap on the re-activation door. Counted against the
        // finished simulation, so a slot freed by a completion in this
        // same diff is available regardless of op order.
        //
        // Two filters and an ordering, each load-bearing:
        //  * only items that actually END active — an item reactivated
        //    and then completed in the same diff is NOT competing for a
        //    Today slot, and dropping it would strip a finished item's
        //    membership (contradicting golden today.json case 3, "a
        //    finished Today item keeps its membership") while freeing
        //    nothing, since done items are not counted.
        //  * WORST board position first, because this loop drops until
        //    the date fits: the item the human ranked lowest should be
        //    the one that loses the slot, and the choice must not come
        //    from the model's array order.
        let mut today_candidates: Vec<&String> = reactivated
            .iter()
            .filter(|id| {
                sim.get(*id)
                    .map(|it| it.state == ItemState::Active && it.today_on.is_some())
                    .unwrap_or(false)
            })
            .collect();
        today_candidates.sort_by(|a, b| board_order(&orig, &sim, b).cmp(&board_order(&orig, &sim, a)));
        for id in today_candidates {
            let item = sim.get(id).cloned().unwrap();
            let date = match &item.today_on {
                Some(d) => d.clone(),
                None => continue,
            };
            if effective_active_today(tx, &orig, &sim, &date)? > crate::commands::day::TODAY_CAP {
                drafts.push(EventDraft {
                    event_type: EventType::TodayRemoved,
                    item_id: Some(id.clone()),
                    payload: json!({ "date": date, "cause": "user" }),
                });
                sim.get_mut(id).unwrap().today_on = None;
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

    Ok(ReorgOutcome {
        affected,
        spawned_ids,
    })
}

/// Active count of `tier` as this transaction will leave it:
/// the live projection, minus the referenced items that were active in
/// it, plus the referenced items that end active in it. Spawned
/// children live only in `sim`, so they read as a net +1 — which is
/// exactly what the cap must see.
fn effective_active(
    tx: &rusqlite::Transaction<'_>,
    orig: &std::collections::HashMap<String, crate::domain::Item>,
    sim: &std::collections::HashMap<String, crate::domain::Item>,
    tier: Tier,
) -> Result<i64, String> {
    let base = db::items::count_active_in_tier(tx, tier)?;
    let count_in = |m: &std::collections::HashMap<String, crate::domain::Item>| {
        m.values()
            .filter(|it| it.tier == tier && it.state == ItemState::Active)
            .count() as i64
    };
    Ok(base - count_in(orig) + count_in(sim))
}

/// A total order over items by their position on the board the human
/// REVIEWED: tier first, then rank, then id as the tiebreak.
///
/// Used to order pass-2's derived effects. Anything that must pick a
/// winner between two items — which spawn keeps the last tier slot,
/// which reactivation loses its Today slot — has to pick by something
/// the human controls. Iterating the ops array instead lets the model
/// decide by listing order, which SPEC §8.7 forbids.
///
/// **Keyed on `orig`, deliberately, not on the mutated simulation.**
/// `next_rank` hands out end-of-tier ranks in ops-array order, so two
/// items moved into one tier in the same diff have a relative rank
/// that the model's listing order decided. Keying on the post-diff
/// board would therefore smuggle that order straight back into the
/// contest — the same defect one layer down. `orig` is the board as it
/// stood when the human read the diff, which is also the board SPEC
/// §8.7 appeals to ("answers the human can predict from their own
/// board"). Items absent from `orig` — only ever spawned children,
/// which are never candidates — fall back to the simulation.
fn board_order(
    orig: &std::collections::HashMap<String, crate::domain::Item>,
    sim: &std::collections::HashMap<String, crate::domain::Item>,
    id: &str,
) -> (u8, String, String) {
    match orig.get(id).or_else(|| sim.get(id)) {
        Some(it) => (
            match it.tier {
                Tier::A => 0,
                Tier::B => 1,
                Tier::C => 2,
                Tier::Inbox => 3,
            },
            it.rank.clone(),
            it.id.clone(),
        ),
        None => (u8::MAX, String::new(), id.to_string()),
    }
}

/// The same reckoning for a Today date: how many ACTIVE items this
/// transaction will leave committed to `date`.
fn effective_active_today(
    tx: &rusqlite::Transaction<'_>,
    orig: &std::collections::HashMap<String, crate::domain::Item>,
    sim: &std::collections::HashMap<String, crate::domain::Item>,
    date: &str,
) -> Result<i64, String> {
    let base = db::items::count_active_today(tx, date)?;
    let count_in = |m: &std::collections::HashMap<String, crate::domain::Item>| {
        m.values()
            .filter(|it| {
                it.today_on.as_deref() == Some(date) && it.state == ItemState::Active
            })
            .count() as i64
    };
    Ok(base - count_in(orig) + count_in(sim))
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

        let outcome = apply_reorg_inner(
            &pool,
            sug,
            vec![move_op(&a.id, "C"), done_op(&b.id)],
        )
        .unwrap();
        assert_eq!(outcome.affected.len(), 2);

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

    /// THE property for this path: an accepted diff is a function of the
    /// op SET, not its order. Both order-dependent defects pass 3 found
    /// die here — the example-based tests around it each pin exactly one
    /// ordering, which is how the defects survived a green suite.
    ///
    /// Exhaustive over all 3! orderings rather than sampled: with six
    /// cases, "every permutation" is both stronger than a generator and
    /// cheaper. (An earlier version of this test used
    /// `proptest::sample::subsequence`, which preserves order — so it
    /// compared `[0,1,2]` against `[0,1,2]` and asserted nothing. It
    /// passed a negative control it should have failed, which is the
    /// only reason the vacuity was noticed.)
    #[test]
    fn accept_reorg_outcome_is_a_function_of_the_op_set_not_its_order() {
        use crate::commands::items::set_item_recurrence_inner;
        const DATE: &str = "2026-07-26";

        /// Order-insensitive board fingerprint. Includes `rank`: an
        /// earlier version omitted it, which made rank-order defects
        /// invisible to a test whose whole purpose is order-sensitivity.
        fn fingerprint(
            pool: &SqlitePool,
        ) -> Vec<(String, String, String, Option<String>, String)> {
            let conn = pool.get().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT content, tier, state, today_on, rank FROM items \
                     WHERE deleted = 0 ORDER BY content, tier, state, today_on, rank",
                )
                .unwrap();
            stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
        }

        // The scenario has to make the derived effects CONTEND for a
        // scarce slot, or ordering cannot manifest. An earlier version
        // used one recurring completion and one reactivation — exactly
        // the configuration in which no winner has to be picked — and
        // its blind spot turned out to contain three real defects.
        //
        // Here: TWO recurring items complete into an A tier with one
        // free slot (so one child must overflow to Inbox), and TWO
        // blocked items reactivate onto a Today holding one free slot
        // (so one must lose its membership). Both winners must be
        // decided by the board, never by the ops array.
        let build = || {
            let pool = fresh_pool();
            // Two blocked items on the same Today date — one in A (so
            // its reactivation also consumes an A slot), one in B (so
            // it competes only for Today). Created FIRST, because
            // creation is itself cap-gated.
            let mut blocked = Vec::new();
            for (i, tier) in [Tier::A, Tier::B].into_iter().enumerate() {
                let b = create_item_inner(&pool, tier, format!("blocked-{i}"), None, None).unwrap();
                crate::commands::day::add_to_today_inner(&pool, b.id.clone(), DATE.into()).unwrap();
                crate::commands::items::set_item_state_inner(
                    &pool,
                    b.id.clone(),
                    ItemState::Blocked,
                    Some("stuck".into()),
                )
                .unwrap();
                blocked.push(b.id);
            }
            // Two recurring items, active in A.
            let mut recurring = Vec::new();
            for i in 0..2 {
                let r =
                    create_item_inner(&pool, Tier::A, format!("recurring-{i}"), None, None).unwrap();
                set_item_recurrence_inner(&pool, r.id.clone(), Some("FREQ=DAILY".into())).unwrap();
                recurring.push(r.id);
            }
            // Fill A to cap; put two fills on Today.
            //
            // The arithmetic is the whole point of the fixture:
            //   A active before = 2 recurring + 3 fills = 5 = A_CAP.
            //   After the diff  = 5 − 2 (completions) + 1 (blocked-0
            //                     reactivating in A) = 4, i.e. ONE free
            //                     slot for TWO children.
            //   Today active before = 2 fills; after = 2 + 2
            //                     reactivations = 4 > 3, i.e. ONE of the
            //                     two must lose its membership.
            for i in 0..(A_CAP - 2) {
                let f =
                    create_item_inner(&pool, Tier::A, format!("fill-{i}"), None, None).unwrap();
                if i < 2 {
                    crate::commands::day::add_to_today_inner(&pool, f.id.clone(), DATE.into())
                        .unwrap();
                }
            }
            let sug = seed_suggestion(&pool);
            (pool, recurring, blocked, sug)
        };

        // All 4! orderings of [done(r0), done(r1), active(b0), active(b1)].
        let mut perms: Vec<[usize; 4]> = Vec::new();
        for a in 0..4 {
            for b in 0..4 {
                for c in 0..4 {
                    for d in 0..4 {
                        let p = [a, b, c, d];
                        let mut seen = [false; 4];
                        if p.iter().all(|i| !std::mem::replace(&mut seen[*i], true)) {
                            perms.push(p);
                        }
                    }
                }
            }
        }
        assert_eq!(perms.len(), 24);

        type Board = Vec<(String, String, String, Option<String>, String)>;
        let mut results: Vec<(bool, Board)> = Vec::new();
        for order in &perms {
            let (pool, recurring, blocked, sug) = build();
            let all_ops = [
                done_op(&recurring[0]),
                done_op(&recurring[1]),
                ReorgProposal {
                    item_id: blocked[0].clone(),
                    action: ProposalAction::Active,
                    to_tier: None,
                    rationale: None,
                },
                ReorgProposal {
                    item_id: blocked[1].clone(),
                    action: ProposalAction::Active,
                    to_tier: None,
                    rationale: None,
                },
            ];
            let ops: Vec<ReorgProposal> = order.iter().map(|i| all_ops[*i].clone()).collect();
            let ok = apply_reorg_inner(&pool, sug, ops).is_ok();
            results.push((ok, fingerprint(&pool)));
        }
        for (i, r) in results.iter().enumerate().skip(1) {
            assert_eq!(
                &results[0], r,
                "permutation {:?} produced a different board than {:?} — the model's \
                 array order is deciding the outcome",
                perms[i], perms[0]
            );
        }

        // Sanity: the scenario must actually force both contests.
        assert!(results[0].0, "the diff is legal and must commit");
        let (_, board) = &results[0];
        let children: Vec<&(String, String, String, Option<String>, String)> = board
            .iter()
            .filter(|r| r.0.starts_with("recurring-") && r.2 == "active")
            .collect();
        assert_eq!(children.len(), 2, "both recurring items spawned");
        assert!(
            children.iter().any(|r| r.1 == "A") && children.iter().any(|r| r.1 == "inbox"),
            "one child took the free A slot and one overflowed — the contest happened: {children:?}"
        );
        let on_today = board
            .iter()
            .filter(|r| r.0.starts_with("blocked-") && r.3.as_deref() == Some(DATE))
            .count();
        assert_eq!(
            on_today, 1,
            "exactly one reactivated item kept the last Today slot — the contest happened"
        );
        assert!(
            board.iter().all(|r| !r.0.starts_with("blocked-") || r.2 == "active"),
            "both blocked items were reactivated"
        );
    }

    /// The declared policy, pinned by OUTCOME rather than by
    /// determinism. Its absence was a real hole: flipping either pass-2
    /// sort, or replacing `board_order` with a raw UUID sort, left the
    /// whole suite green — the permutation test asserts only that a
    /// contest *happened*, never who won, and a cross-permutation
    /// comparison is satisfied by ANY deterministic key.
    #[test]
    fn accept_reorg_contests_are_decided_by_board_position() {
        use crate::commands::items::set_item_recurrence_inner;
        const DATE: &str = "2026-07-26";

        // ── spawn contest: one free A slot, two recurring completions.
        // `top` is ranked above `bottom` in A, so TOP's child keeps the
        // slot and BOTTOM's child is exiled to Inbox.
        {
            let pool = fresh_pool();
            // create_item_inner places new items at TOP of tier, so the
            // LAST created has the smallest rank. Create bottom first.
            let bottom =
                create_item_inner(&pool, Tier::A, "bottom".into(), None, None).unwrap();
            let top = create_item_inner(&pool, Tier::A, "top".into(), None, None).unwrap();
            assert!(top.rank < bottom.rank, "fixture: top must outrank bottom");
            for id in [&top.id, &bottom.id] {
                set_item_recurrence_inner(&pool, id.clone(), Some("FREQ=DAILY".into())).unwrap();
            }
            // Arithmetic of the fixture: A holds 2 recurring + 3 fills
            // = 5 active = A_CAP. The diff completes both recurring
            // items (−2) and reactivates one blocked item INTO A via a
            // move (+1), so A ends at 4 — exactly ONE free slot for TWO
            // children. Without that reactivation the two completions
            // would free two slots and both children would fit, and
            // there would be no contest to observe.
            for i in 0..(A_CAP - 2) {
                create_item_inner(&pool, Tier::A, format!("fill-{i}"), None, None).unwrap();
            }
            // Created in B and moved into A by the diff (A is at cap, so
            // it could not be created there).
            let blocked =
                create_item_inner(&pool, Tier::B, "blocked-a".into(), None, None).unwrap();
            crate::commands::items::set_item_state_inner(
                &pool,
                blocked.id.clone(),
                ItemState::Blocked,
                Some("stuck".into()),
            )
            .unwrap();
            let sug = seed_suggestion(&pool);
            // done(top), done(bottom), move(blocked→A)+active: A ends
            // 5-2+1 = 4 → one free slot for two children.
            let outcome = apply_reorg_inner(
                &pool,
                sug,
                vec![
                    done_op(&bottom.id), // listed FIRST on purpose
                    done_op(&top.id),
                    move_op(&blocked.id, "A"),
                    ReorgProposal {
                        item_id: blocked.id.clone(),
                        action: ProposalAction::Active,
                        to_tier: None,
                        rationale: None,
                    },
                ],
            )
            .unwrap();
            assert_eq!(outcome.spawned_ids.len(), 2);

            let conn = pool.get().unwrap();
            let tier_of = |content: &str| -> Vec<String> {
                let mut stmt = conn
                    .prepare(
                        "SELECT tier FROM items WHERE content = ?1 AND state = 'active' \
                         AND deleted = 0",
                    )
                    .unwrap();
                stmt.query_map([content], |r| r.get(0))
                    .unwrap()
                    .collect::<Result<_, _>>()
                    .unwrap()
            };
            assert_eq!(
                tier_of("top"),
                vec!["A".to_string()],
                "the higher-ranked parent's child takes the free slot"
            );
            assert_eq!(
                tier_of("bottom"),
                vec!["inbox".to_string()],
                "the lower-ranked parent's child is the one exiled — and note it was \
                 listed FIRST in the diff, so ops order did not decide this"
            );
        }

        // ── Today contest: one free slot, two reactivations in ONE tier
        // (so `rank` is the deciding component, not `tier`).
        {
            let pool = fresh_pool();
            let mut ids = Vec::new();
            for name in ["bottom", "top"] {
                let it = create_item_inner(&pool, Tier::B, name.into(), None, None).unwrap();
                crate::commands::day::add_to_today_inner(&pool, it.id.clone(), DATE.into())
                    .unwrap();
                crate::commands::items::set_item_state_inner(
                    &pool,
                    it.id.clone(),
                    ItemState::Blocked,
                    Some("stuck".into()),
                )
                .unwrap();
                ids.push(it);
            }
            let (bottom, top) = (ids[0].clone(), ids[1].clone());
            assert!(top.rank < bottom.rank, "fixture: top must outrank bottom");
            // Two actives already on the date → one free slot.
            for i in 0..2 {
                let f = create_item_inner(&pool, Tier::A, format!("fill-{i}"), None, None).unwrap();
                crate::commands::day::add_to_today_inner(&pool, f.id.clone(), DATE.into()).unwrap();
            }
            let sug = seed_suggestion(&pool);
            let active_op = |id: &String| ReorgProposal {
                item_id: id.clone(),
                action: ProposalAction::Active,
                to_tier: None,
                rationale: None,
            };
            apply_reorg_inner(
                &pool,
                sug,
                // `top` listed FIRST — if ops order decided, top would lose.
                vec![active_op(&top.id), active_op(&bottom.id)],
            )
            .unwrap();

            let conn = pool.get().unwrap();
            let today_of = |id: &String| -> Option<String> {
                conn.query_row("SELECT today_on FROM items WHERE id = ?1", [id], |r| r.get(0))
                    .unwrap()
            };
            assert_eq!(
                today_of(&top.id).as_deref(),
                Some(DATE),
                "the higher-ranked item keeps the last Today slot"
            );
            assert_eq!(
                today_of(&bottom.id),
                None,
                "the lower-ranked item is the one that yields"
            );
        }
    }

    #[test]
    fn accept_reorg_cross_tier_today_contest_favours_the_higher_tier() {
        // `board_order`'s TIER component was decoration: making it a
        // constant, or inverting it, left all 240 tests green. Every
        // contest fixture put both contenders in the SAME tier, so the
        // highest-order byte of the key was never observed.
        //
        // Here the two contenders are in different tiers. A commitment
        // in A outranks one in B, so the B item is the one that yields
        // its Today slot.
        const DATE: &str = "2026-07-26";
        let pool = fresh_pool();
        let mut ids = Vec::new();
        for (name, tier) in [("in-a", Tier::A), ("in-b", Tier::B)] {
            let it = create_item_inner(&pool, tier, name.into(), None, None).unwrap();
            crate::commands::day::add_to_today_inner(&pool, it.id.clone(), DATE.into()).unwrap();
            crate::commands::items::set_item_state_inner(
                &pool,
                it.id.clone(),
                ItemState::Blocked,
                Some("stuck".into()),
            )
            .unwrap();
            ids.push(it.id);
        }
        let (in_a, in_b) = (ids[0].clone(), ids[1].clone());
        // Two actives already on the date → exactly one free slot.
        for i in 0..2 {
            let f = create_item_inner(&pool, Tier::A, format!("fill-{i}"), None, None).unwrap();
            crate::commands::day::add_to_today_inner(&pool, f.id.clone(), DATE.into()).unwrap();
        }
        let sug = seed_suggestion(&pool);
        let active_op = |id: &String| ReorgProposal {
            item_id: id.clone(),
            action: ProposalAction::Active,
            to_tier: None,
            rationale: None,
        };
        // The expected winner is listed FIRST, so ops order would give
        // the opposite answer.
        apply_reorg_inner(&pool, sug, vec![active_op(&in_a), active_op(&in_b)]).unwrap();

        let conn = pool.get().unwrap();
        let today_of = |id: &String| -> Option<String> {
            conn.query_row("SELECT today_on FROM items WHERE id = ?1", [id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(
            today_of(&in_a).as_deref(),
            Some(DATE),
            "the A-tier commitment keeps the day"
        );
        assert_eq!(
            today_of(&in_b),
            None,
            "the B-tier item yields — tier is the highest-order component of the contest key"
        );
    }

    #[test]
    fn accept_reorg_today_contest_reads_the_pre_diff_board() {
        // The Today door's `orig`-keying had no independent pin: the
        // gate's key mutation swapped the lookup GLOBALLY and was caught
        // only through the spawn door, so changing the Today sort alone
        // survived.
        //
        // Fixture built so the two rules DISAGREE. `q` outranks `p`,
        // both blocked in C, both on a date with one free slot. The
        // diff also moves `p` into A.
        //   pre-diff board  (correct): both are C items; `p` is
        //     lower-ranked, so `p` yields.
        //   post-diff board (wrong):  `p` reads as an A item and `q` as
        //     a C item, so `q` would yield instead.
        const DATE: &str = "2026-07-26";
        let pool = fresh_pool();
        // create_item_inner places top-of-tier, so create p first to
        // leave q better-ranked.
        let mut made = Vec::new();
        for name in ["p", "q"] {
            let it = create_item_inner(&pool, Tier::C, name.into(), None, None).unwrap();
            crate::commands::day::add_to_today_inner(&pool, it.id.clone(), DATE.into()).unwrap();
            crate::commands::items::set_item_state_inner(
                &pool,
                it.id.clone(),
                ItemState::Blocked,
                Some("stuck".into()),
            )
            .unwrap();
            made.push(it);
        }
        let (p, q) = (made[0].clone(), made[1].clone());
        assert!(q.rank < p.rank, "fixture: q must outrank p in C");
        for i in 0..2 {
            let f = create_item_inner(&pool, Tier::A, format!("fill-{i}"), None, None).unwrap();
            crate::commands::day::add_to_today_inner(&pool, f.id.clone(), DATE.into()).unwrap();
        }
        let sug = seed_suggestion(&pool);
        let active_op = |id: &String| ReorgProposal {
            item_id: id.clone(),
            action: ProposalAction::Active,
            to_tier: None,
            rationale: None,
        };
        apply_reorg_inner(
            &pool,
            sug,
            vec![move_op(&p.id, "A"), active_op(&p.id), active_op(&q.id)],
        )
        .unwrap();

        let conn = pool.get().unwrap();
        let today_of = |id: &String| -> Option<String> {
            conn.query_row("SELECT today_on FROM items WHERE id = ?1", [id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(
            today_of(&q.id).as_deref(),
            Some(DATE),
            "q outranked p on the board the human reviewed, so q keeps the day"
        );
        assert_eq!(
            today_of(&p.id),
            None,
            "p yields — its move to A in this same diff must not buy it priority"
        );
    }

    #[test]
    fn accept_reorg_moves_do_not_decide_a_contest() {
        // Pass 5's MAJOR: `board_order` used to read the MUTATED
        // simulation, and `next_rank` hands out end-of-tier ranks in ops
        // order — so two items moved into one tier had a relative rank
        // the model chose, and if they also contended, the model chose
        // the winner. Keying on the pre-diff board removes the lever.
        use crate::commands::items::set_item_recurrence_inner;
        let build = || {
            let pool = fresh_pool();
            // x outranks y in C (created last = top of tier).
            let y = create_item_inner(&pool, Tier::C, "y".into(), None, None).unwrap();
            let x = create_item_inner(&pool, Tier::C, "x".into(), None, None).unwrap();
            assert!(x.rank < y.rank, "fixture: x must outrank y in C");
            for id in [&x.id, &y.id] {
                set_item_recurrence_inner(&pool, id.clone(), Some("FREQ=DAILY".into())).unwrap();
            }
            for i in 0..(A_CAP - 1) {
                create_item_inner(&pool, Tier::A, format!("fill-{i}"), None, None).unwrap();
            }
            let sug = seed_suggestion(&pool);
            (pool, x.id, y.id, sug)
        };

        // A holds 4 actives; both x and y move in and complete, so A
        // ends at 4 + one surviving child = 5, and one child overflows.
        let mut outcomes = Vec::new();
        for reversed in [false, true] {
            let (pool, x_id, y_id, sug) = build();
            let mut ops = vec![
                move_op(&x_id, "A"),
                move_op(&y_id, "A"),
                done_op(&x_id),
                done_op(&y_id),
            ];
            if reversed {
                ops.swap(0, 1); // list y's move first
            }
            apply_reorg_inner(&pool, sug, ops).unwrap();
            let conn = pool.get().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT content, tier FROM items WHERE state = 'active' AND deleted = 0 \
                     AND content IN ('x','y') ORDER BY content",
                )
                .unwrap();
            let rows: Vec<(String, String)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            outcomes.push(rows);
        }
        assert_eq!(
            outcomes[0], outcomes[1],
            "reordering two move ops changed which child was exiled — the model is \
             deciding the contest through rank allocation"
        );
        assert_eq!(
            outcomes[0],
            vec![
                ("x".to_string(), "A".to_string()),
                ("y".to_string(), "inbox".to_string())
            ],
            "and the winner is the one the human ranked higher on the board they reviewed"
        );
    }

    #[test]
    fn accept_reorg_done_on_a_blocked_item_stays_undoable() {
        // BLOCKING: the accept path was the last done-door still
        // dropping the outgoing blocked reason. Undo then wrote
        // `state='blocked'` with a null reason, tripped the
        // migration-002 CHECK, and rolled back — and because undo keeps
        // targeting the same transaction, Ctrl+Z stayed dead until the
        // user did something else undoable. P2e BLOCKING-1, reopened at
        // a door the original fix never reached.
        let pool = fresh_pool();
        let item = create_item_inner(&pool, Tier::A, "stuck work".into(), None, None).unwrap();
        crate::commands::items::set_item_state_inner(
            &pool,
            item.id.clone(),
            ItemState::Blocked,
            Some("waiting on legal".into()),
        )
        .unwrap();
        let sug = seed_suggestion(&pool);
        apply_reorg_inner(&pool, sug, vec![done_op(&item.id)]).unwrap();

        // The event must carry the reason it cleared.
        {
            let conn = pool.get().unwrap();
            let reason: Option<String> = conn
                .query_row(
                    "SELECT json_extract(payload, '$.blocked_reason') FROM events \
                     WHERE type = 'ITEM_STATE_CHANGED' ORDER BY id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(reason.as_deref(), Some("waiting on legal"));
        }

        // And undo must actually work — the law undo has no exception to.
        crate::commands::events::undo_last_action_inner(&pool)
            .expect("undo must never fail");
        let conn = pool.get().unwrap();
        let (state, reason): (String, Option<String>) = conn
            .query_row(
                "SELECT state, blocked_reason FROM items WHERE id = ?1",
                [&item.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, "blocked");
        assert_eq!(reason.as_deref(), Some("waiting on legal"));
    }

    #[test]
    fn accept_reorg_does_not_strip_today_from_an_item_it_also_completes() {
        // An item reactivated and then completed in the same diff is
        // not competing for a Today slot — done items are not counted —
        // so dropping it frees nothing and contradicts golden
        // today.json case 3 ("a finished Today item keeps its
        // membership but frees its slot").
        let pool = fresh_pool();
        const DATE: &str = "2026-07-26";
        // Two blocked items join Today FIRST and are then blocked —
        // joining is itself cap-gated, so neither could get on a full
        // date afterwards. `y` will be reactivated AND completed; `z`
        // will only be reactivated, so `z` is the one genuinely
        // competing for the last slot.
        let mut ids = Vec::new();
        for name in ["y", "z"] {
            let it = create_item_inner(&pool, Tier::B, name.into(), None, None).unwrap();
            crate::commands::day::add_to_today_inner(&pool, it.id.clone(), DATE.into()).unwrap();
            crate::commands::items::set_item_state_inner(
                &pool,
                it.id.clone(),
                ItemState::Blocked,
                Some("stuck".into()),
            )
            .unwrap();
            ids.push(it.id);
        }
        let (y, z) = (ids[0].clone(), ids[1].clone());
        // Fill Today to cap with three actives (y and z are blocked, so
        // they hold no slot).
        for i in 0..3 {
            let f = create_item_inner(&pool, Tier::A, format!("fill-{i}"), None, None).unwrap();
            crate::commands::day::add_to_today_inner(&pool, f.id.clone(), DATE.into()).unwrap();
        }
        let sug = seed_suggestion(&pool);

        // Unblock y and finish it; unblock z. The date now wants 4
        // actives and holds 3, so exactly ONE membership must go — and
        // it must be z's, because y is done and holds no slot.
        let active_op = |id: &String| ReorgProposal {
            item_id: id.clone(),
            action: ProposalAction::Active,
            to_tier: None,
            rationale: None,
        };
        apply_reorg_inner(
            &pool,
            sug,
            vec![active_op(&y), done_op(&y), active_op(&z)],
        )
        .unwrap();

        let conn = pool.get().unwrap();
        let row = |id: &String| -> (String, Option<String>) {
            conn.query_row(
                "SELECT state, today_on FROM items WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        let (y_state, y_today) = row(&y);
        assert_eq!(y_state, "done");
        assert_eq!(
            y_today.as_deref(),
            Some(DATE),
            "finished work stays visible on Today; it frees its slot without losing its place"
        );
        let (z_state, z_today) = row(&z);
        assert_eq!(z_state, "active");
        assert_eq!(z_today, None, "z is the one actually competing for the slot");

        let spurious: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE type = 'TODAY_REMOVED' AND item_id = ?1",
                [&y],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(spurious, 0, "no removal the human never caused");
        let total_removals: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE type = 'TODAY_REMOVED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total_removals, 1, "exactly one membership was given up");
        let active_today: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE today_on = ?1 AND state='active' AND deleted=0",
                [DATE],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_today, 3, "and the cap holds");
    }

    #[test]
    fn accept_reorg_unknown_suggestion_is_event_not_found() {
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        let err = apply_reorg_inner(&pool, 9999, vec![move_op(&a.id, "C")]).unwrap_err();
        assert_eq!(err, "EVENT_NOT_FOUND");
    }

    #[test]
    fn accept_reorg_outcome_does_not_depend_on_op_order() {
        // Pass 3's headline: derived effects used to be resolved
        // incrementally, so an op not yet visited read as a no-op and
        // the SAME accepted set could commit or fail depending on the
        // order the model happened to list its proposals. The LLM has
        // no write path, but that handed it a lever on the
        // deterministic tier's result.
        use crate::commands::items::set_item_recurrence_inner;
        let build = || {
            let pool = fresh_pool();
            let blocked = create_item_inner(&pool, Tier::A, "blocked".into(), None, None).unwrap();
            crate::commands::items::set_item_state_inner(
                &pool,
                blocked.id.clone(),
                ItemState::Blocked,
                Some("stuck".into()),
            )
            .unwrap();
            let mut a_ids = Vec::new();
            for i in 0..A_CAP {
                a_ids.push(
                    create_item_inner(&pool, Tier::A, format!("a-{i}"), None, None)
                        .unwrap()
                        .id,
                );
            }
            set_item_recurrence_inner(&pool, a_ids[0].clone(), Some("FREQ=WEEKLY".into())).unwrap();
            let sug = seed_suggestion(&pool);
            (pool, a_ids, blocked.id, sug)
        };

        // Same set, both orders. Each must commit with identical shape.
        for reversed in [false, true] {
            let (pool, a_ids, blocked_id, sug) = build();
            let mut ops = vec![
                done_op(&a_ids[0]),
                ReorgProposal {
                    item_id: blocked_id.clone(),
                    action: ProposalAction::Active,
                    to_tier: None,
                    rationale: None,
                },
            ];
            if reversed {
                ops.reverse();
            }
            let outcome = apply_reorg_inner(&pool, sug, ops)
                .unwrap_or_else(|e| panic!("reversed={reversed}: accept must not fail: {e}"));
            assert_eq!(outcome.spawned_ids.len(), 1, "reversed={reversed}");
            assert_eq!(
                count_active(&pool, Tier::A),
                A_CAP as i64,
                "reversed={reversed}: cap holds"
            );
            let conn = pool.get().unwrap();
            let child_tier: String = conn
                .query_row(
                    "SELECT tier FROM items WHERE id = ?1",
                    [&outcome.spawned_ids[0]],
                    |r| r.get(0),
                )
                .unwrap();
            // SPEC §8.7: a spawn overflows to Inbox, it never fails the
            // accept. An earlier build returned CAP_EXCEEDED here — and
            // its own regression test enshrined that, against the SPEC
            // line the commit message cited.
            assert_eq!(child_tier, "inbox", "reversed={reversed}");
        }
    }

    #[test]
    fn accept_reorg_today_slot_freed_in_the_same_diff_survives_either_order() {
        // The mirror-image ordering defect: "finish X, start Y" (both on
        // one Today) kept Y's membership, while "start Y, finish X" —
        // the identical set — dropped it with a TODAY_REMOVED the human
        // never caused.
        let date = "2026-07-26";
        let build = || {
            let pool = fresh_pool();
            let mut ids = Vec::new();
            for i in 0..3 {
                let it = create_item_inner(&pool, Tier::A, format!("t{i}"), None, None).unwrap();
                crate::commands::day::add_to_today_inner(&pool, it.id.clone(), date.into())
                    .unwrap();
                ids.push(it.id);
            }
            let y = create_item_inner(&pool, Tier::A, "y".into(), None, None).unwrap();
            crate::commands::items::set_item_state_inner(&pool, y.id.clone(), ItemState::Done, None)
                .unwrap();
            {
                let conn = pool.get().unwrap();
                conn.execute(
                    "UPDATE items SET today_on = ?1 WHERE id = ?2",
                    rusqlite::params![date, &y.id],
                )
                .unwrap();
            }
            let sug = seed_suggestion(&pool);
            (pool, ids, y.id, sug)
        };

        for reversed in [false, true] {
            let (pool, ids, y_id, sug) = build();
            let mut ops = vec![
                done_op(&ids[0]),
                ReorgProposal {
                    item_id: y_id.clone(),
                    action: ProposalAction::Active,
                    to_tier: None,
                    rationale: None,
                },
            ];
            if reversed {
                ops.reverse();
            }
            apply_reorg_inner(&pool, sug, ops).unwrap();

            let conn = pool.get().unwrap();
            let y_today: Option<String> = conn
                .query_row("SELECT today_on FROM items WHERE id = ?1", [&y_id], |r| r.get(0))
                .unwrap();
            assert_eq!(
                y_today.as_deref(),
                Some(date),
                "reversed={reversed}: the freed slot is available in either order"
            );
            let active_today: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM items WHERE today_on = ?1 AND state='active' AND deleted=0",
                    [date],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(active_today, 3, "reversed={reversed}: and the cap still holds");
        }
    }

    #[test]
    fn accept_reorg_spawn_never_fails_a_legal_diff() {
        // The v0.3 BLOCKING escape, verbatim from the cold review. The
        // spawn used to consult the live projection while the final cap
        // check reasoned over the simulation, so the child was invisible
        // to both ledgers: A committed at 6 active with A_CAP = 5.
        use crate::commands::items::set_item_recurrence_inner;
        let pool = fresh_pool();
        // The blocked item is created FIRST — creation is itself
        // cap-gated, so it could not be added once A is full.
        let blocked = create_item_inner(&pool, Tier::A, "blocked".into(), None, None).unwrap();
        crate::commands::items::set_item_state_inner(
            &pool,
            blocked.id.clone(),
            ItemState::Blocked,
            Some("stuck".into()),
        )
        .unwrap();
        let mut a_ids = Vec::new();
        for i in 0..A_CAP {
            a_ids.push(
                create_item_inner(&pool, Tier::A, format!("a-{i}"), None, None)
                    .unwrap()
                    .id,
            );
        }
        set_item_recurrence_inner(&pool, a_ids[0].clone(), Some("FREQ=WEEKLY".into())).unwrap();
        let sug = seed_suggestion(&pool);

        // "Finish the recurring report, and unblock the other thing."
        // The human's own ops are net-neutral (one leaves active, one
        // joins), so the diff is legal and MUST commit. Only the
        // automatic spawn would push A over — and a spawn may never be
        // the reason an accept fails (SPEC §8.7), so it overflows.
        let outcome = apply_reorg_inner(
            &pool,
            sug,
            vec![
                done_op(&a_ids[0]),
                ReorgProposal {
                    item_id: blocked.id.clone(),
                    action: ProposalAction::Active,
                    to_tier: None,
                    rationale: None,
                },
            ],
        )
        .expect("a legal diff must not be failed by its own derived spawn");
        assert_eq!(count_active(&pool, Tier::A), A_CAP as i64, "cap holds");
        assert_eq!(outcome.spawned_ids.len(), 1);
        let conn = pool.get().unwrap();
        let child_tier: String = conn
            .query_row(
                "SELECT tier FROM items WHERE id = ?1",
                [&outcome.spawned_ids[0]],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(child_tier, "inbox");
    }

    #[test]
    fn accept_reorg_spawn_overflows_to_inbox_when_the_tier_is_genuinely_full() {
        // Same shape, but the parent is BLOCKED — it frees no slot, so
        // the child cannot fit A and must land in Inbox rather than
        // failing the accept (marking done never fails).
        use crate::commands::items::set_item_recurrence_inner;
        let pool = fresh_pool();
        // Blocked recurring item first (creation is cap-gated), then
        // fill A with actives around it.
        let rec = create_item_inner(&pool, Tier::A, "recurring".into(), None, None).unwrap();
        set_item_recurrence_inner(&pool, rec.id.clone(), Some("FREQ=DAILY".into())).unwrap();
        crate::commands::items::set_item_state_inner(
            &pool,
            rec.id.clone(),
            ItemState::Blocked,
            Some("stuck".into()),
        )
        .unwrap();
        for i in 0..A_CAP {
            create_item_inner(&pool, Tier::A, format!("a-{i}"), None, None).unwrap();
        }
        let sug = seed_suggestion(&pool);

        let outcome = apply_reorg_inner(&pool, sug, vec![done_op(&rec.id)]).unwrap();
        assert_eq!(outcome.spawned_ids.len(), 1);
        let conn = pool.get().unwrap();
        let child_tier: String = conn
            .query_row(
                "SELECT tier FROM items WHERE id = ?1",
                [&outcome.spawned_ids[0]],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(child_tier, "inbox", "doctrine-consistent overflow");
        drop(conn); // max_size(1) pool: release before the next helper
        assert_eq!(count_active(&pool, Tier::A), A_CAP as i64);
    }

    #[test]
    fn accept_reorg_move_and_spawn_never_collide_on_rank() {
        // Two independent "last rank in tier" maps, both seeded from the
        // untouched projection, hand out byte-identical ranks — and
        // rankBetween(R, R) throws on the next drag between them. One
        // ledger per transaction is the fix.
        use crate::commands::items::set_item_recurrence_inner;
        let pool = fresh_pool();
        let rec = create_item_inner(&pool, Tier::A, "recurring".into(), None, None).unwrap();
        set_item_recurrence_inner(&pool, rec.id.clone(), Some("FREQ=DAILY".into())).unwrap();
        let c1 = create_item_inner(&pool, Tier::C, "promote me".into(), None, None).unwrap();
        let sug = seed_suggestion(&pool);

        // Move c1 into A and complete the recurring A item: both place
        // an item at the end of A in the same transaction.
        apply_reorg_inner(&pool, sug, vec![move_op(&c1.id, "A"), done_op(&rec.id)]).unwrap();

        let conn = pool.get().unwrap();
        let mut stmt = conn
            .prepare("SELECT rank FROM items WHERE tier = 'A' AND deleted = 0")
            .unwrap();
        let ranks: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let unique: std::collections::HashSet<&String> = ranks.iter().collect();
        assert_eq!(ranks.len(), unique.len(), "ranks collided: {ranks:?}");
    }

    #[test]
    fn accept_reorg_reactivation_uses_a_slot_freed_in_the_same_diff() {
        // "Finish X, start Y" where both are on the same Today. X's
        // completion frees the slot, so Y must KEEP its membership —
        // reading the pre-transaction count would drop Y with a
        // TODAY_REMOVED the human never caused.
        let pool = fresh_pool();
        let date = "2026-07-26";
        let mut ids = Vec::new();
        for i in 0..3 {
            let it = create_item_inner(&pool, Tier::A, format!("t{i}"), None, None).unwrap();
            crate::commands::day::add_to_today_inner(&pool, it.id.clone(), date.into()).unwrap();
            ids.push(it.id);
        }
        // A 4th item, done but still on Today (it kept membership).
        let y = create_item_inner(&pool, Tier::A, "y".into(), None, None).unwrap();
        crate::commands::items::set_item_state_inner(&pool, y.id.clone(), ItemState::Done, None)
            .unwrap();
        {
            let conn = pool.get().unwrap();
            conn.execute("UPDATE items SET today_on = ?1 WHERE id = ?2", rusqlite::params![date, &y.id])
                .unwrap();
        }
        let sug = seed_suggestion(&pool);

        // Today is at 3 active. Finishing ids[0] frees one for y.
        apply_reorg_inner(
            &pool,
            sug,
            vec![
                done_op(&ids[0]),
                ReorgProposal {
                    item_id: y.id.clone(),
                    action: ProposalAction::Active,
                    to_tier: None,
                    rationale: None,
                },
            ],
        )
        .unwrap();

        let conn = pool.get().unwrap();
        let y_today: Option<String> = conn
            .query_row("SELECT today_on FROM items WHERE id = ?1", [&y.id], |r| r.get(0))
            .unwrap();
        assert_eq!(
            y_today.as_deref(),
            Some(date),
            "the freed slot was counted; y keeps its Today membership"
        );
        let active_today: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE today_on = ?1 AND state='active' AND deleted=0",
                [date],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_today, 3, "and the cap still holds");
    }

    #[test]
    fn accept_reorg_applies_two_coherent_ops_on_one_item() {
        // "Unblock it and demote it" is a pair models produce routinely,
        // and nothing tells them one proposal per item. An earlier build
        // rejected the whole accept with BAD_ARGS, discarding every
        // other accepted op — a worse failure than the double-count it
        // was guarding against (which the two-pass structure now makes
        // impossible anyway).
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "x".into(), None, None).unwrap();
        crate::commands::items::set_item_state_inner(
            &pool,
            a.id.clone(),
            ItemState::Blocked,
            Some("stuck".into()),
        )
        .unwrap();
        let sug = seed_suggestion(&pool);

        apply_reorg_inner(
            &pool,
            sug,
            vec![
                ReorgProposal {
                    item_id: a.id.clone(),
                    action: ProposalAction::Active,
                    to_tier: None,
                    rationale: None,
                },
                move_op(&a.id, "B"),
            ],
        )
        .expect("a coherent pair on one item must apply");

        let conn = pool.get().unwrap();
        let (tier, state): (String, String) = conn
            .query_row("SELECT tier, state FROM items WHERE id = ?1", [&a.id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((tier.as_str(), state.as_str()), ("B", "active"));
    }

    #[test]
    fn accept_reorg_completing_an_item_twice_spawns_one_child() {
        // The double-spawn the duplicate guard existed to prevent, now
        // prevented structurally: completions are a SET, resolved once,
        // after every op has been applied.
        use crate::commands::items::set_item_recurrence_inner;
        let pool = fresh_pool();
        let a = create_item_inner(&pool, Tier::A, "weekly".into(), None, None).unwrap();
        set_item_recurrence_inner(&pool, a.id.clone(), Some("FREQ=WEEKLY".into())).unwrap();
        let sug = seed_suggestion(&pool);

        let outcome = apply_reorg_inner(
            &pool,
            sug,
            vec![
                done_op(&a.id),
                ReorgProposal {
                    item_id: a.id.clone(),
                    action: ProposalAction::Active,
                    to_tier: None,
                    rationale: None,
                },
                done_op(&a.id),
            ],
        )
        .unwrap();
        assert_eq!(outcome.spawned_ids.len(), 1, "one completion, one child");
    }

    #[test]
    fn accept_reorg_reports_spawned_children_so_the_ui_can_show_them() {
        // AnalyzePanel closes without refetching, so a child that isn't
        // announced exists in SQLite and nowhere on screen until restart.
        use crate::commands::items::set_item_recurrence_inner;
        let pool = fresh_pool();
        let rec = create_item_inner(&pool, Tier::B, "weekly".into(), None, None).unwrap();
        set_item_recurrence_inner(&pool, rec.id.clone(), Some("FREQ=WEEKLY".into())).unwrap();
        let sug = seed_suggestion(&pool);

        let outcome = apply_reorg_inner(&pool, sug, vec![done_op(&rec.id)]).unwrap();
        assert_eq!(outcome.affected, vec![rec.id.clone()]);
        assert_eq!(outcome.spawned_ids.len(), 1, "the child is reported for emission");
        assert_ne!(outcome.spawned_ids[0], rec.id);
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
