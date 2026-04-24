use serde::{Deserialize, Serialize};

/// The four vertical bays per CLAUDE.md §Design philosophy.
/// Serializes as `"inbox" | "A" | "B" | "C"` to match SPEC §4.1 and
/// the `tier` CHECK constraint in `items`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tier {
    #[serde(rename = "inbox")]
    Inbox,
    A,
    B,
    C,
}

impl Tier {
    pub fn as_sql(self) -> &'static str {
        match self {
            Tier::Inbox => "inbox",
            Tier::A => "A",
            Tier::B => "B",
            Tier::C => "C",
        }
    }
}

/// Lifecycle state for a single item. Orthogonal to `Tier`.
/// Serializes lowercase to match SPEC §4.1 and the `state` CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemState {
    Active,
    Blocked,
    Done,
}

impl ItemState {
    pub fn as_sql(self) -> &'static str {
        match self {
            ItemState::Active => "active",
            ItemState::Blocked => "blocked",
            ItemState::Done => "done",
        }
    }
}

/// One row of the `items` projection. Rebuildable from the event log; the
/// projection is the serving copy, events are the source of truth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub content: String,
    pub tier: Tier,
    pub rank: String,
    pub state: ItemState,
    pub blocked_reason: Option<String>,
    pub start_at: Option<i64>,
    pub due_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_serializes_per_spec() {
        assert_eq!(serde_json::to_string(&Tier::Inbox).unwrap(), "\"inbox\"");
        assert_eq!(serde_json::to_string(&Tier::A).unwrap(), "\"A\"");
        assert_eq!(serde_json::to_string(&Tier::B).unwrap(), "\"B\"");
        assert_eq!(serde_json::to_string(&Tier::C).unwrap(), "\"C\"");
    }

    #[test]
    fn item_state_serializes_per_spec() {
        assert_eq!(serde_json::to_string(&ItemState::Active).unwrap(), "\"active\"");
        assert_eq!(
            serde_json::to_string(&ItemState::Blocked).unwrap(),
            "\"blocked\""
        );
        assert_eq!(serde_json::to_string(&ItemState::Done).unwrap(), "\"done\"");
    }

    #[test]
    fn tier_as_sql_matches_ddl_check_constraint() {
        assert_eq!(Tier::Inbox.as_sql(), "inbox");
        assert_eq!(Tier::A.as_sql(), "A");
        assert_eq!(Tier::B.as_sql(), "B");
        assert_eq!(Tier::C.as_sql(), "C");
    }
}
