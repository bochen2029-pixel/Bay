//! System + user prompt templates for the analyze flow. Kept as
//! const strings to avoid accidental drift; SPEC §8.2 is the
//! reference.

use std::fmt::Write;

use super::compression::AnalyzeContext;
use crate::domain::{A_CAP, B_CAP};

pub const SYSTEM_PROMPT: &str = "You are an analyst observing a single user's task management event log.\n\
Your job: identify patterns the user would benefit from seeing, and OPTIONALLY propose a small re-org the user can accept or reject.\n\
\n\
Rules:\n\
- Observe first. Prefer sharp, specific observations over generic advice.\n\
- You MAY propose concrete re-org actions, but they are SUGGESTIONS ONLY — the user explicitly accepts or rejects them; you never apply anything.\n\
- Only propose an action when the data clearly justifies it (e.g. an item untouched far past its tier's staleness threshold, or an A-tier inflated relative to throughput). Prefer few high-confidence proposals over many.\n\
- Valid proposal actions: \"move\" (with to_tier one of \"inbox\"|\"A\"|\"B\"|\"C\"), \"done\", \"active\".\n\
- Use only item ids that appear in the data below.\n\
- You can see RECORDED BEHAVIOUR, not just the board: focus sessions, what broke them, and which committed items have never been started. Prefer an observation grounded in what the user actually did over one grounded in how a board looks. \"You have not started this in the three weeks since you called it critical\" beats \"this item is stale\".\n\
- Zero sessions on a committed item is evidence of avoidance, not of laziness, and there is usually a reason: the item may be too large, too vague, or missing a first physical step. Say the useful version.\n\
- Report; do not exhort. No praise, no encouragement, no motivational framing, and never a streak or a score. The user asked for a mirror, not a coach with opinions about their character.\n\
- Output strictly valid JSON matching the schema. If nothing is interesting, return empty arrays.\n\
- Do not repeat what the user can trivially see by looking at the board.\n\
\n\
Output schema:\n\
{\n\
  \"observations\": [\n\
    { \"severity\": \"info\" | \"warn\", \"text\": \"string (<= 200 chars)\",\n\
      \"affected_item_ids\": [ \"string\" ] }\n\
  ],\n\
  \"proposals\": [\n\
    { \"item_id\": \"string\", \"action\": \"move\" | \"done\" | \"active\",\n\
      \"to_tier\": \"inbox\" | \"A\" | \"B\" | \"C\" (only for move),\n\
      \"rationale\": \"string (<= 120 chars)\" }\n\
  ]\n\
}\n\
\n\
Respond with the JSON object only — no surrounding prose, no code fences.";

pub const RETRY_PREFIX: &str =
    "Your previous response was not valid JSON. Return only the JSON object \
     described in the schema, no prose, no code fences.\n\n";

pub fn format_user_prompt(ctx: &AnalyzeContext) -> String {
    let mut out = String::new();
    writeln!(
        &mut out,
        "Window: last {} days (since_ts={} until_ts={}). Total events in window: {}.",
        ctx.window_days, ctx.since_ts, ctx.until_ts, ctx.event_count
    )
    .unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "=== AGGREGATE ===").unwrap();
    writeln!(
        &mut out,
        "- Items created in window: {} total; by tier: {}",
        ctx.created_in_window,
        render_map(&ctx.created_by_tier)
    )
    .unwrap();
    writeln!(
        &mut out,
        "- Items marked done in window: {}",
        ctx.done_in_window
    )
    .unwrap();
    writeln!(
        &mut out,
        "- Items currently blocked: {}",
        ctx.blocked_current
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    writeln!(&mut out, "=== CURRENT BOARD ===").unwrap();
    writeln!(
        &mut out,
        "Inbox: {} items (showing up to {})",
        ctx.inbox_count,
        ctx.inbox_list.len()
    )
    .unwrap();
    for s in &ctx.inbox_list {
        writeln!(
            &mut out,
            "  - id={} state={} days_in_tier={} content={}",
            s.id, s.state, s.days_in_tier, s.content
        )
        .unwrap();
    }
    writeln!(&mut out, "A ({} / {} active):", active_of(&ctx.a_list), A_CAP).unwrap();
    for s in &ctx.a_list {
        writeln!(
            &mut out,
            "  - id={} state={} days_in_tier={} content={}",
            s.id, s.state, s.days_in_tier, s.content
        )
        .unwrap();
    }
    writeln!(&mut out, "B ({} / {} active):", active_of(&ctx.b_list), B_CAP).unwrap();
    for s in &ctx.b_list {
        writeln!(
            &mut out,
            "  - id={} state={} days_in_tier={} content={}",
            s.id, s.state, s.days_in_tier, s.content
        )
        .unwrap();
    }
    writeln!(&mut out, "C: {} items (not enumerated)", ctx.c_count).unwrap();
    writeln!(&mut out).unwrap();

    if !ctx.stale_list.is_empty() {
        writeln!(&mut out, "=== STALE ITEMS ===").unwrap();
        for s in &ctx.stale_list {
            writeln!(
                &mut out,
                "  - id={} tier={} untouched={}d (threshold {}d) content={}",
                s.id, s.tier, s.days_untouched, s.threshold_days, s.content
            )
            .unwrap();
        }
        writeln!(&mut out).unwrap();
    }

    // ── behavior (v0.3) ───────────────────────────────────────────
    // The log records WORK now, not only management. Without this the
    // sharpest thing the model could say was "this item is old"; with
    // it, it can say "you have never started the thing you called
    // critical", which is a different and much more useful sentence.
    writeln!(&mut out, "=== ATTENTION (recorded focus sessions) ===").unwrap();
    writeln!(
        &mut out,
        "- Sessions in window: {} totalling {} minutes",
        ctx.sessions_in_window, ctx.session_minutes_in_window
    )
    .unwrap();
    writeln!(
        &mut out,
        "- Session outcomes: {}",
        render_map(&ctx.sessions_by_outcome)
    )
    .unwrap();
    if !ctx.interruptions_by_cause.is_empty() {
        writeln!(
            &mut out,
            "- What broke focus: {}",
            render_map(&ctx.interruptions_by_cause)
        )
        .unwrap();
    }
    writeln!(
        &mut out,
        "- Today: {} planned, {} finished, {} rolled over unfinished",
        ctx.today_planned_in_window, ctx.today_finished_in_window, ctx.today_expired_in_window
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    if !ctx.never_started.is_empty() {
        writeln!(
            &mut out,
            "=== COMMITTED BUT NEVER STARTED (zero focus sessions, ever) ==="
        )
        .unwrap();
        for s in &ctx.never_started {
            writeln!(
                &mut out,
                "  - id={} tier={} untouched={}d content={}",
                s.id, s.tier, s.days_untouched, s.content
            )
            .unwrap();
        }
        writeln!(&mut out).unwrap();
    }

    out.push_str("Return observations JSON.");
    out
}

fn render_map(m: &std::collections::HashMap<String, i64>) -> String {
    if m.is_empty() {
        return "{}".into();
    }
    let mut pairs: Vec<(&String, &i64)> = m.iter().collect();
    // Sort by reference comparison — avoids the (*k).clone() allocation
    // and silences clippy's `suspicious_double_ref_op` on `k.clone()`
    // (which would have cloned the &String, not the inner String).
    pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
    let parts: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect();
    parts.join(", ")
}

fn active_of(list: &[super::compression::ItemSummary]) -> usize {
    list.iter().filter(|s| s.state == "active").count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_prompt_never_grants_the_model_write_authority() {
        // The firewall is structural — ProjectionEvent has no LLM
        // variant, so an LLM event cannot reach the projection whatever
        // the prompt says. But the prompt is what stops the model
        // NARRATING as though it had applied something ("I moved three
        // items to C"), which a user would reasonably read as a report
        // of fact rather than a proposal.
        //
        // prompt.rs had no tests until v0.3 pass 10.
        assert!(
            SYSTEM_PROMPT.contains("SUGGESTIONS ONLY"),
            "the proposal constraint must stay in the prompt"
        );
        assert!(
            SYSTEM_PROMPT.contains("you never apply anything"),
            "the prompt must state plainly that the model has no write path"
        );
        assert!(
            SYSTEM_PROMPT.contains("accepts or rejects"),
            "the accept/reject contract is what makes a proposal safe to make"
        );
    }

    #[test]
    fn the_prompt_reports_rather_than_exhorts() {
        // CLAUDE law 9: the mirror confronts, it never shames. A coach
        // that tells the user to try harder is the thing this product
        // is a reaction against.
        assert!(
            SYSTEM_PROMPT.to_lowercase().contains("report"),
            "the report-not-exhort rule must stay in the prompt"
        );
    }
}
