//! Strict observation parser for LLM responses. One retry allowed;
//! second failure surfaces as LLM_PARSE_ERROR. SPEC §8.4.
//!
//! The parser is deliberately tolerant of code-fence wrapping (the
//! most common "non-JSON" mistake) but strict about the schema shape.
//! Unknown affected_item_ids are dropped with a warn log — the prompt
//! includes ids but a distracted model could invent one.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub severity: Severity,
    pub text: String,
    #[serde(default)]
    pub affected_item_ids: Vec<String>,
}

/// A re-org action the LLM may PROPOSE (I-20). This is advisory data: the
/// LLM never applies it. The human reviews the diff and accepts, at which
/// point the deterministic tier (accept_suggestion) re-validates and
/// writes the resulting events under cap enforcement. The firewall holds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProposalAction {
    /// Move the item to `to_tier`.
    Move,
    /// Mark the item done.
    Done,
    /// Mark the item active (un-done; un-block is not proposable since a
    /// re-block would need a reason).
    Active,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorgProposal {
    pub item_id: String,
    pub action: ProposalAction,
    /// Target tier for a `move` ("inbox" | "A" | "B" | "C"). Ignored for
    /// state actions.
    #[serde(default)]
    pub to_tier: Option<String>,
    /// One-line reason the LLM gives for this op — shown in the diff.
    #[serde(default)]
    pub rationale: Option<String>,
}

/// Parsed analyze output: advisory observations + optional re-org proposals.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisOutput {
    pub observations: Vec<Observation>,
    pub proposals: Vec<ReorgProposal>,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    observations: Vec<Observation>,
    #[serde(default)]
    proposals: Vec<ReorgProposal>,
}

const VALID_TIERS: [&str; 4] = ["inbox", "A", "B", "C"];

/// Parse the full analyze response: observations + re-org proposals.
/// Unknown / malformed entries are dropped (with a warn) rather than
/// failing the whole parse — a distracted model shouldn't sink the run.
pub fn parse_analysis(
    raw: &str,
    known_ids: &HashSet<String>,
) -> Result<AnalysisOutput, String> {
    let stripped = strip_fences(raw.trim());
    let env: Envelope = serde_json::from_str(stripped)
        .map_err(|e| format!("parse json: {e} (input starts: {:?})", head(stripped, 160)))?;

    let mut observations = Vec::with_capacity(env.observations.len());
    for obs in env.observations {
        let filtered_ids: Vec<String> = obs
            .affected_item_ids
            .into_iter()
            .filter(|id| {
                let ok = known_ids.contains(id);
                if !ok {
                    eprintln!("analyze parse: dropping unknown affected_item_id {id:?}");
                }
                ok
            })
            .collect();
        observations.push(Observation {
            severity: obs.severity,
            text: obs.text,
            affected_item_ids: filtered_ids,
        });
    }

    // Validate proposals: item_id must be known; a Move must carry a valid
    // to_tier. Drop anything else — the accept path re-validates anyway,
    // but a clean proposal list makes the preview honest.
    let mut proposals = Vec::with_capacity(env.proposals.len());
    for p in env.proposals {
        if !known_ids.contains(&p.item_id) {
            eprintln!("analyze parse: dropping proposal for unknown item {:?}", p.item_id);
            continue;
        }
        if p.action == ProposalAction::Move {
            match p.to_tier.as_deref() {
                Some(t) if VALID_TIERS.contains(&t) => {}
                _ => {
                    eprintln!(
                        "analyze parse: dropping move proposal with invalid to_tier {:?}",
                        p.to_tier
                    );
                    continue;
                }
            }
        }
        proposals.push(p);
    }

    Ok(AnalysisOutput {
        observations,
        proposals,
    })
}

fn strip_fences(s: &str) -> &str {
    // Strip ```json ... ``` or ``` ... ``` wrappings.
    let s = s.trim();
    if let Some(inner) = s.strip_prefix("```json") {
        inner.trim_end_matches("```").trim()
    } else if let Some(inner) = s.strip_prefix("```") {
        inner.trim_end_matches("```").trim()
    } else {
        s
    }
}

fn head(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> HashSet<String> {
        ["a", "b"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_plain_json() {
        let raw = r#"{"observations":[{"severity":"info","text":"hi","affected_item_ids":["a"]}]}"#;
        let out = parse_analysis(raw, &ids()).unwrap().observations;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Info);
        assert_eq!(out[0].affected_item_ids, vec!["a".to_string()]);
    }

    #[test]
    fn strips_code_fences() {
        let raw = "```json\n{\"observations\":[]}\n```";
        let out = parse_analysis(raw, &ids()).unwrap().observations;
        assert!(out.is_empty());
    }

    #[test]
    fn filters_unknown_ids() {
        let raw = r#"{"observations":[{"severity":"warn","text":"t","affected_item_ids":["a","ghost"]}]}"#;
        let out = parse_analysis(raw, &ids()).unwrap().observations;
        assert_eq!(out[0].affected_item_ids, vec!["a".to_string()]);
    }

    #[test]
    fn rejects_malformed() {
        let raw = "not json";
        assert!(parse_analysis(raw, &ids()).is_err());
    }

    #[test]
    fn accepts_missing_affected_ids() {
        let raw = r#"{"observations":[{"severity":"info","text":"t"}]}"#;
        let out = parse_analysis(raw, &ids()).unwrap().observations;
        assert!(out[0].affected_item_ids.is_empty());
    }

    #[test]
    fn accepts_empty_observations() {
        let raw = r#"{"observations":[]}"#;
        assert!(parse_analysis(raw, &ids()).unwrap().observations.is_empty());
    }

    // ── re-org proposals (I-20) ────────────────────────────────────

    #[test]
    fn parses_reorg_proposals() {
        let raw = r#"{"observations":[],"proposals":[
            {"item_id":"a","action":"move","to_tier":"C","rationale":"stale in A"},
            {"item_id":"b","action":"done","rationale":"looks complete"}
        ]}"#;
        let out = parse_analysis(raw, &ids()).unwrap();
        assert_eq!(out.proposals.len(), 2);
        assert_eq!(out.proposals[0].action, ProposalAction::Move);
        assert_eq!(out.proposals[0].to_tier.as_deref(), Some("C"));
        assert_eq!(out.proposals[1].action, ProposalAction::Done);
    }

    #[test]
    fn drops_proposal_for_unknown_item() {
        let raw = r#"{"proposals":[{"item_id":"ghost","action":"done"}]}"#;
        let out = parse_analysis(raw, &ids()).unwrap();
        assert!(out.proposals.is_empty(), "unknown-item proposal must be dropped");
    }

    #[test]
    fn drops_move_proposal_with_invalid_tier() {
        let raw = r#"{"proposals":[{"item_id":"a","action":"move","to_tier":"Z"}]}"#;
        let out = parse_analysis(raw, &ids()).unwrap();
        assert!(out.proposals.is_empty(), "invalid to_tier must be dropped");
    }

    #[test]
    fn analysis_absent_proposals_is_empty_not_error() {
        let raw = r#"{"observations":[{"severity":"info","text":"t"}]}"#;
        let out = parse_analysis(raw, &ids()).unwrap();
        assert_eq!(out.observations.len(), 1);
        assert!(out.proposals.is_empty());
    }
}
