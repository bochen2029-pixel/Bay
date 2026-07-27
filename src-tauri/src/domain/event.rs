use serde::{Deserialize, Serialize};

/// The event-type vocabulary: v1's ten plus I-21's two (v0.3). String
/// wire format matches the `type` column of the `events` table (see
/// CLAUDE.md §Data model) and the SCREAMING_SNAKE_CASE convention in
/// SPEC §4.3.
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
    /// I-21: sets/clears `items.recurrence`. Projection event.
    #[serde(rename = "ITEM_RECURRENCE_SET")]
    ItemRecurrenceSet,
    /// I-21: audit link written when completing a recurring item spawns
    /// its next instance (`{parent_id, child_id, next_due_at}`, one
    /// transaction with the parent's ITEM_STATE_CHANGED and the child's
    /// ITEM_CREATED). No projection effect — the child's existence is
    /// carried by its own ITEM_CREATED.
    #[serde(rename = "ITEM_RECURRED")]
    ItemRecurred,
    /// v0.3 execution core: sets/clears `items.first_step` — the single
    /// next physical action (<=140 chars). Projection event.
    #[serde(rename = "ITEM_FIRST_STEP_SET")]
    ItemFirstStepSet,
    /// v0.3: the item joins the Today execution overlay for one local
    /// date (`{date}`). Projection event (`items.today_on`). Cap 3.
    #[serde(rename = "TODAY_ADDED")]
    TodayAdded,
    /// v0.3: the item leaves Today (`{date, cause: "expired"|"user"}`).
    /// `expired` rows are written by the day-roll with `actor: system`
    /// — the one sanctioned machine write (VISION law 6).
    #[serde(rename = "TODAY_REMOVED")]
    TodayRemoved,
    /// v0.3 ritual audit: the day was opened with a chosen Today set
    /// (`{date, today_ids}`). NULL item_id; no projection effect.
    #[serde(rename = "DAY_OPENED")]
    DayOpened,
    /// v0.3 ritual audit: the day was closed (`{date, tomorrow_first?,
    /// note?}`) — "tomorrow's first move" chosen tonight. NULL item_id;
    /// no projection effect.
    #[serde(rename = "DAY_CLOSED")]
    DayClosed,
    /// v0.3: a focus session opened on an item (`{session_id}`; the
    /// item on the envelope). Projects into `sessions` — the open
    /// session is the "Now" slot (at most one, index-enforced).
    #[serde(rename = "SESSION_STARTED")]
    SessionStarted,
    /// v0.3: the open session closed (`{session_id, outcome, reason?,
    /// note?}`). `outcome: done` co-writes the item's state change in
    /// the same transaction. Projects into `sessions`.
    #[serde(rename = "SESSION_ENDED")]
    SessionEnded,
    #[serde(rename = "LLM_SUGGESTION_GENERATED")]
    LlmSuggestionGenerated,
    #[serde(rename = "LLM_SUGGESTION_ACCEPTED")]
    LlmSuggestionAccepted,
    #[serde(rename = "LLM_SUGGESTION_REJECTED")]
    LlmSuggestionRejected,
}

impl EventType {
    pub fn as_sql(self) -> &'static str {
        match self {
            EventType::ItemCreated => "ITEM_CREATED",
            EventType::ItemEdited => "ITEM_EDITED",
            EventType::ItemMoved => "ITEM_MOVED",
            EventType::ItemStateChanged => "ITEM_STATE_CHANGED",
            EventType::ItemDateSet => "ITEM_DATE_SET",
            EventType::ItemDeleted => "ITEM_DELETED",
            EventType::ItemRestored => "ITEM_RESTORED",
            EventType::ItemRecurrenceSet => "ITEM_RECURRENCE_SET",
            EventType::ItemRecurred => "ITEM_RECURRED",
            EventType::ItemFirstStepSet => "ITEM_FIRST_STEP_SET",
            EventType::TodayAdded => "TODAY_ADDED",
            EventType::TodayRemoved => "TODAY_REMOVED",
            EventType::DayOpened => "DAY_OPENED",
            EventType::DayClosed => "DAY_CLOSED",
            EventType::SessionStarted => "SESSION_STARTED",
            EventType::SessionEnded => "SESSION_ENDED",
            EventType::LlmSuggestionGenerated => "LLM_SUGGESTION_GENERATED",
            EventType::LlmSuggestionAccepted => "LLM_SUGGESTION_ACCEPTED",
            EventType::LlmSuggestionRejected => "LLM_SUGGESTION_REJECTED",
        }
    }

    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "ITEM_CREATED" => Some(EventType::ItemCreated),
            "ITEM_EDITED" => Some(EventType::ItemEdited),
            "ITEM_MOVED" => Some(EventType::ItemMoved),
            "ITEM_STATE_CHANGED" => Some(EventType::ItemStateChanged),
            "ITEM_DATE_SET" => Some(EventType::ItemDateSet),
            "ITEM_DELETED" => Some(EventType::ItemDeleted),
            "ITEM_RESTORED" => Some(EventType::ItemRestored),
            "ITEM_RECURRENCE_SET" => Some(EventType::ItemRecurrenceSet),
            "ITEM_RECURRED" => Some(EventType::ItemRecurred),
            "ITEM_FIRST_STEP_SET" => Some(EventType::ItemFirstStepSet),
            "TODAY_ADDED" => Some(EventType::TodayAdded),
            "TODAY_REMOVED" => Some(EventType::TodayRemoved),
            "DAY_OPENED" => Some(EventType::DayOpened),
            "DAY_CLOSED" => Some(EventType::DayClosed),
            "SESSION_STARTED" => Some(EventType::SessionStarted),
            "SESSION_ENDED" => Some(EventType::SessionEnded),
            "LLM_SUGGESTION_GENERATED" => Some(EventType::LlmSuggestionGenerated),
            "LLM_SUGGESTION_ACCEPTED" => Some(EventType::LlmSuggestionAccepted),
            "LLM_SUGGESTION_REJECTED" => Some(EventType::LlmSuggestionRejected),
            _ => None,
        }
    }
}

/// Who initiated a write transaction (envelope v2, migration 003).
///
/// Deliberately TWO variants. The LLM is not an actor because it has no
/// write path (the `ProjectionEvent` firewall): an accepted suggestion
/// is a HUMAN write whose `origin` records `llm_accept:<event id>`.
/// `System` is reserved for the deterministic execution of a timer the
/// human configured (VISION law 6 — e.g. the Today day-roll), never for
/// autonomous decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    #[default]
    Human,
    System,
}

impl Actor {
    pub fn as_sql(self) -> &'static str {
        match self {
            Actor::Human => "human",
            Actor::System => "system",
        }
    }

    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "human" => Some(Actor::Human),
            "system" => Some(Actor::System),
            _ => None,
        }
    }
}

/// The eight event types that MAY affect the `items` projection.
///
/// This is the **type-level LLM firewall** (Phase 2d / CLAUDE.md
/// "LLM firewalled out of state"). The three `LlmSuggestion*` variants
/// on `EventType` are deliberately NOT present here: an LLM event
/// cannot be fed into the projection logic because there is no
/// `ProjectionEvent` variant for it. The projection's `apply` function
/// dispatches on `ProjectionEvent`, not `EventType`, so the compiler
/// enforces that LLM events never mutate the projection — the firewall
/// is "the type system won't let you," not "the match arm returns
/// Ok(())".
///
/// The conversion `EventType -> Option<ProjectionEvent>` is the single
/// boundary. `None` now reads as "no projection effect" and covers TWO
/// classes: the three LLM advisory events (the firewall proper) and
/// pure audit/link events like `ITEM_RECURRED` (I-21 — the spawned
/// child's existence is carried by its own `ITEM_CREATED`; the link is
/// audit-only). The firewall's structural claim is unchanged: there is
/// no `ProjectionEvent::LlmSuggestion*` variant, so LLM events cannot
/// reach the projection under any refactor. Events mapped to `None`
/// still land in the append-only `events` log; they just never touch
/// `items`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionEvent {
    ItemCreated,
    ItemEdited,
    ItemMoved,
    ItemStateChanged,
    ItemDateSet,
    ItemDeleted,
    ItemRestored,
    ItemRecurrenceSet,
    ItemFirstStepSet,
    TodayAdded,
    TodayRemoved,
    SessionStarted,
    SessionEnded,
}

impl EventType {
    /// Convert an `EventType` to a `ProjectionEvent`, returning `None`
    /// for LLM suggestion events. This is the LLM firewall's single
    /// boundary: item events pass through; LLM events don't.
    ///
    /// The projection's `apply` function takes a `ProjectionEvent`, so
    /// once an event has been converted, the type system guarantees it
    /// cannot be an LLM event. There is no `LlmSuggestion*` variant on
    /// `ProjectionEvent` to even reach the projection logic.
    pub fn to_projection_event(self) -> Option<ProjectionEvent> {
        match self {
            EventType::ItemCreated => Some(ProjectionEvent::ItemCreated),
            EventType::ItemEdited => Some(ProjectionEvent::ItemEdited),
            EventType::ItemMoved => Some(ProjectionEvent::ItemMoved),
            EventType::ItemStateChanged => Some(ProjectionEvent::ItemStateChanged),
            EventType::ItemDateSet => Some(ProjectionEvent::ItemDateSet),
            EventType::ItemDeleted => Some(ProjectionEvent::ItemDeleted),
            EventType::ItemRestored => Some(ProjectionEvent::ItemRestored),
            EventType::ItemRecurrenceSet => Some(ProjectionEvent::ItemRecurrenceSet),
            EventType::ItemFirstStepSet => Some(ProjectionEvent::ItemFirstStepSet),
            EventType::TodayAdded => Some(ProjectionEvent::TodayAdded),
            EventType::TodayRemoved => Some(ProjectionEvent::TodayRemoved),
            // Sessions project into the `sessions` table — same purity
            // law as `items` (behavior records; undo never touches
            // them, but rebuild reproduces them).
            EventType::SessionStarted => Some(ProjectionEvent::SessionStarted),
            EventType::SessionEnded => Some(ProjectionEvent::SessionEnded),
            // Audit/link event: no projection effect (the spawned
            // child's row comes from its own ITEM_CREATED in the same
            // transaction). I-21.
            EventType::ItemRecurred => None,
            // Ritual audit events (v0.3): NULL item_id, no projection
            // effect — the per-item TODAY_ADDED/REMOVED rows carry the
            // projection changes; these record the ceremony itself.
            EventType::DayOpened | EventType::DayClosed => None,
            // LLM events are advisory-only (CLAUDE.md §LLM scope v1,
            // SPEC §4.3). They land in the event log but never the
            // projection. Returning None here is the firewall.
            EventType::LlmSuggestionGenerated
            | EventType::LlmSuggestionAccepted
            | EventType::LlmSuggestionRejected => None,
        }
    }
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
    // ── envelope v2 (migration 003) ─────────────────────────────────
    // All None on legacy (pre-envelope) rows; stamped by
    // `db::write_events_ctx` on every row written since. Omitted from
    // the wire when absent so old frontends/logs parse unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_ver: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_hash: Option<String>,
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
            (EventType::ItemRecurrenceSet, "\"ITEM_RECURRENCE_SET\""),
            (EventType::ItemRecurred, "\"ITEM_RECURRED\""),
            (EventType::ItemFirstStepSet, "\"ITEM_FIRST_STEP_SET\""),
            (EventType::TodayAdded, "\"TODAY_ADDED\""),
            (EventType::TodayRemoved, "\"TODAY_REMOVED\""),
            (EventType::DayOpened, "\"DAY_OPENED\""),
            (EventType::DayClosed, "\"DAY_CLOSED\""),
            (EventType::SessionStarted, "\"SESSION_STARTED\""),
            (EventType::SessionEnded, "\"SESSION_ENDED\""),
            (EventType::LlmSuggestionGenerated, "\"LLM_SUGGESTION_GENERATED\""),
            (EventType::LlmSuggestionAccepted, "\"LLM_SUGGESTION_ACCEPTED\""),
            (EventType::LlmSuggestionRejected, "\"LLM_SUGGESTION_REJECTED\""),
        ];
        for (variant, expected) in cases {
            assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
        }
    }

    // ── Type-level LLM firewall tests (Phase 2d) ───────────────────
    //
    // These tests pin the firewall's behavior: the seven item event
    // types convert to Some(ProjectionEvent); the three LLM event
    // types convert to None. If someone ever adds an LLM variant to
    // ProjectionEvent by mistake, these tests catch it. If someone
    // adds a new item event type but forgets to add it to
    // ProjectionEvent, the compiler catches it (the match in
    // to_projection_event becomes non-exhaustive).

    #[test]
    fn item_event_types_convert_to_projection_event() {
        assert_eq!(
            EventType::ItemCreated.to_projection_event(),
            Some(ProjectionEvent::ItemCreated)
        );
        assert_eq!(
            EventType::ItemEdited.to_projection_event(),
            Some(ProjectionEvent::ItemEdited)
        );
        assert_eq!(
            EventType::ItemMoved.to_projection_event(),
            Some(ProjectionEvent::ItemMoved)
        );
        assert_eq!(
            EventType::ItemStateChanged.to_projection_event(),
            Some(ProjectionEvent::ItemStateChanged)
        );
        assert_eq!(
            EventType::ItemDateSet.to_projection_event(),
            Some(ProjectionEvent::ItemDateSet)
        );
        assert_eq!(
            EventType::ItemDeleted.to_projection_event(),
            Some(ProjectionEvent::ItemDeleted)
        );
        assert_eq!(
            EventType::ItemRestored.to_projection_event(),
            Some(ProjectionEvent::ItemRestored)
        );
        assert_eq!(
            EventType::ItemRecurrenceSet.to_projection_event(),
            Some(ProjectionEvent::ItemRecurrenceSet)
        );
        assert_eq!(
            EventType::ItemFirstStepSet.to_projection_event(),
            Some(ProjectionEvent::ItemFirstStepSet)
        );
        assert_eq!(
            EventType::TodayAdded.to_projection_event(),
            Some(ProjectionEvent::TodayAdded)
        );
        assert_eq!(
            EventType::TodayRemoved.to_projection_event(),
            Some(ProjectionEvent::TodayRemoved)
        );
        assert_eq!(
            EventType::SessionStarted.to_projection_event(),
            Some(ProjectionEvent::SessionStarted)
        );
        assert_eq!(
            EventType::SessionEnded.to_projection_event(),
            Some(ProjectionEvent::SessionEnded)
        );
    }

    #[test]
    fn llm_event_types_do_not_convert_to_projection_event() {
        // The LLM firewall: these three event types return None,
        // meaning they cannot reach the projection's apply logic.
        // There is no ProjectionEvent::LlmSuggestion* variant.
        assert_eq!(EventType::LlmSuggestionGenerated.to_projection_event(), None);
        assert_eq!(EventType::LlmSuggestionAccepted.to_projection_event(), None);
        assert_eq!(EventType::LlmSuggestionRejected.to_projection_event(), None);
    }

    #[test]
    fn audit_link_event_types_do_not_convert_to_projection_event() {
        // ITEM_RECURRED (I-21) and the DAY_* ritual events (v0.3) are
        // pure audit events: projection changes travel on their own
        // per-item events. Same None mechanism as the firewall,
        // different rationale.
        assert_eq!(EventType::ItemRecurred.to_projection_event(), None);
        assert_eq!(EventType::DayOpened.to_projection_event(), None);
        assert_eq!(EventType::DayClosed.to_projection_event(), None);
    }

    #[test]
    fn projection_event_has_no_llm_variants() {
        // Sanity: the ProjectionEvent enum must have exactly 8 variants
        // (the projection-affecting item event types), never the 3 LLM
        // types and never the audit-link ITEM_RECURRED. This is a
        // structural assertion — if someone adds an LLM variant, this
        // test fails.
        let all = [
            ProjectionEvent::ItemCreated,
            ProjectionEvent::ItemEdited,
            ProjectionEvent::ItemMoved,
            ProjectionEvent::ItemStateChanged,
            ProjectionEvent::ItemDateSet,
            ProjectionEvent::ItemDeleted,
            ProjectionEvent::ItemRestored,
            ProjectionEvent::ItemRecurrenceSet,
            ProjectionEvent::ItemFirstStepSet,
            ProjectionEvent::TodayAdded,
            ProjectionEvent::TodayRemoved,
            ProjectionEvent::SessionStarted,
            ProjectionEvent::SessionEnded,
        ];
        // Each converts back to its EventType counterpart.
        for pe in all {
            let et = match pe {
                ProjectionEvent::ItemCreated => EventType::ItemCreated,
                ProjectionEvent::ItemEdited => EventType::ItemEdited,
                ProjectionEvent::ItemMoved => EventType::ItemMoved,
                ProjectionEvent::ItemStateChanged => EventType::ItemStateChanged,
                ProjectionEvent::ItemDateSet => EventType::ItemDateSet,
                ProjectionEvent::ItemDeleted => EventType::ItemDeleted,
                ProjectionEvent::ItemRestored => EventType::ItemRestored,
                ProjectionEvent::ItemRecurrenceSet => EventType::ItemRecurrenceSet,
                ProjectionEvent::ItemFirstStepSet => EventType::ItemFirstStepSet,
                ProjectionEvent::TodayAdded => EventType::TodayAdded,
                ProjectionEvent::TodayRemoved => EventType::TodayRemoved,
                ProjectionEvent::SessionStarted => EventType::SessionStarted,
                ProjectionEvent::SessionEnded => EventType::SessionEnded,
            };
            assert_eq!(et.to_projection_event(), Some(pe));
        }
        // 13 variants — no LLM types, no audit types. The match above
        // is exhaustive on ProjectionEvent; adding a variant forces
        // this test (and the apply function) to handle it.
    }
}
