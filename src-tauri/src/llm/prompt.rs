//! System + user prompt templates for the analyze flow. Kept as
//! const strings to avoid accidental drift; SPEC §8.2 is the
//! reference.

use std::fmt::Write;

use super::compression::AnalyzeContext;
use crate::domain::{A_CAP, B_CAP};

pub const SYSTEM_PROMPT: &str = "You are an analyst observing a single user's task management event log.\n\
Your job: identify patterns the user would benefit from seeing.\n\
\n\
Rules:\n\
- Observe only. Do not suggest specific reassignments.\n\
- Output strictly valid JSON matching the schema provided.\n\
- Prefer sharp, specific observations over generic advice.\n\
- If the data shows no interesting patterns, return an empty array.\n\
- Do not repeat what the user can trivially see by looking at the board.\n\
\n\
Output schema:\n\
{\n\
  \"observations\": [\n\
    { \"severity\": \"info\" | \"warn\", \"text\": \"string (<= 200 chars)\",\n\
      \"affected_item_ids\": [ \"string\" ] }\n\
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
