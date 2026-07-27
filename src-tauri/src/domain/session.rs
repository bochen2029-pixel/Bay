//! v0.3 sessions: one continuous stretch of attention on one item.
//! Rows of the `sessions` projection table, rebuildable from
//! SESSION_STARTED / SESSION_ENDED exactly like `items` is from item
//! events. The open session (ended_at NULL) is the "Now" slot — at
//! most one, index-enforced.

use serde::{Deserialize, Serialize};

/// How a session ended. `Done` co-writes the item's state change in
/// the same transaction; `Interrupted` carries a reason from the
/// five-word taxonomy (the honest interruption record).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionOutcome {
    Done,
    Progress,
    Interrupted,
}

impl SessionOutcome {
    pub fn as_sql(self) -> &'static str {
        match self {
            SessionOutcome::Done => "done",
            SessionOutcome::Progress => "progress",
            SessionOutcome::Interrupted => "interrupted",
        }
    }

    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "done" => Some(SessionOutcome::Done),
            "progress" => Some(SessionOutcome::Progress),
            "interrupted" => Some(SessionOutcome::Interrupted),
            _ => None,
        }
    }
}

/// Why focus broke. Deliberately five words, one tap — a taxonomy the
/// user never has to configure and therefore actually uses.
pub const INTERRUPT_REASONS: &[&str] = &["meeting", "person", "self_switch", "blocked", "energy"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub item_id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub outcome: Option<SessionOutcome>,
    pub reason: Option<String>,
    pub note: Option<String>,
}
