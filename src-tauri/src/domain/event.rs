use serde::{Deserialize, Serialize};

/// The ten event types exhaustive for v1. String wire format matches the
/// `type` column of the `events` table (see CLAUDE.md §Data model) and
/// the SCREAMING_SNAKE_CASE convention in SPEC §4.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    #[serde(rename = "ITEM_CREATED")]
    ItemCreated,
    #[serde(rename = "ITEM_EDITED")]
    ItemEdited,
    #[serde(rename = "ITEM_MOVED")]
    ItemMoved,
    #[serde(rename = "ITEM_STATE_CHANGED")]
    ItemStateChanged,
    #[serde(rename = "ITEM_DATE_SET")]
    ItemDateSet,
    #[serde(rename = "ITEM_DELETED")]
    ItemDeleted,
    #[serde(rename = "ITEM_RESTORED")]
    ItemRestored,
    #[serde(rename = "LLM_SUGGESTION_GENERATED")]
    LlmSuggestionGenerated,
    #[serde(rename = "LLM_SUGGESTION_ACCEPTED")]
    LlmSuggestionAccepted,
    #[serde(rename = "LLM_SUGGESTION_REJECTED")]
    LlmSuggestionRejected,
}

/// One row of the append-only `events` table. Payload is kept as untyped
/// JSON here; typed-payload variants arrive in I-03+ when event writers
/// need them. SPEC §4.3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: i64,
    pub ts: i64,
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub item_id: Option<String>,
    pub payload: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_wire_format_matches_spec() {
        let cases = [
            (EventType::ItemCreated, "\"ITEM_CREATED\""),
            (EventType::ItemEdited, "\"ITEM_EDITED\""),
            (EventType::ItemMoved, "\"ITEM_MOVED\""),
            (EventType::ItemStateChanged, "\"ITEM_STATE_CHANGED\""),
            (EventType::ItemDateSet, "\"ITEM_DATE_SET\""),
            (EventType::ItemDeleted, "\"ITEM_DELETED\""),
            (EventType::ItemRestored, "\"ITEM_RESTORED\""),
            (EventType::LlmSuggestionGenerated, "\"LLM_SUGGESTION_GENERATED\""),
            (EventType::LlmSuggestionAccepted, "\"LLM_SUGGESTION_ACCEPTED\""),
            (EventType::LlmSuggestionRejected, "\"LLM_SUGGESTION_REJECTED\""),
        ];
        for (variant, expected) in cases {
            assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
        }
    }
}
