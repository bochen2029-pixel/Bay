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

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    observations: Vec<Observation>,
}

pub fn parse_observations(
    raw: &str,
    known_ids: &HashSet<String>,
) -> Result<Vec<Observation>, String> {
    let stripped = strip_fences(raw.trim());
    let env: Envelope = serde_json::from_str(stripped)
        .map_err(|e| format!("parse json: {e} (input starts: {:?})", head(stripped, 160)))?;

    let mut out = Vec::with_capacity(env.observations.len());
    for obs in env.observations {
        let filtered_ids: Vec<String> = obs
            .affected_item_ids
            .into_iter()
            .filter(|id| {
                let ok = known_ids.contains(id);
                if !ok {
                    eprintln!(
                        "analyze parse: dropping unknown affected_item_id {id:?}"
                    );
                }
                ok
            })
            .collect();
        out.push(Observation {
            severity: obs.severity,
            text: obs.text,
            affected_item_ids: filtered_ids,
        });
    }
    Ok(out)
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
        let out = parse_observations(raw, &ids()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, Severity::Info);
        assert_eq!(out[0].affected_item_ids, vec!["a".to_string()]);
    }

    #[test]
    fn strips_code_fences() {
        let raw = "```json\n{\"observations\":[]}\n```";
        let out = parse_observations(raw, &ids()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn filters_unknown_ids() {
        let raw = r#"{"observations":[{"severity":"warn","text":"t","affected_item_ids":["a","ghost"]}]}"#;
        let out = parse_observations(raw, &ids()).unwrap();
        assert_eq!(out[0].affected_item_ids, vec!["a".to_string()]);
    }

    #[test]
    fn rejects_malformed() {
        let raw = "not json";
        assert!(parse_observations(raw, &ids()).is_err());
    }

    #[test]
    fn accepts_missing_affected_ids() {
        let raw = r#"{"observations":[{"severity":"info","text":"t"}]}"#;
        let out = parse_observations(raw, &ids()).unwrap();
        assert!(out[0].affected_item_ids.is_empty());
    }

    #[test]
    fn accepts_empty_observations() {
        let raw = r#"{"observations":[]}"#;
        assert!(parse_observations(raw, &ids()).unwrap().is_empty());
    }
}
