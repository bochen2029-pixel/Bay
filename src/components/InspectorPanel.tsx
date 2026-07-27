// Right-side drawer showing one item's full event history. Opens when
// selectedItemId is set (from clicking a strip, a calendar pill, or
// Ctrl+Enter in QuickCapture). Fetches get_events({itemId}) on open
// and re-fetches when the item changes (updated_at bump).

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { format } from "date-fns";
import { z } from "zod";

import { Event, Tier } from "../domain";
import { useStore } from "../store";

const EventList = z.array(Event);

export function InspectorPanel() {
  const selectedId = useStore((s) => s.selectedItemId);
  const setSelected = useStore((s) => s.setSelectedItemId);
  const item = useStore((s) =>
    s.selectedItemId ? (s.items[s.selectedItemId] ?? null) : null,
  );
  const [events, setEvents] = useState<Event[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Re-fetch on id change or updated_at tick (any event that touched
  // this item bumps updated_at).
  useEffect(() => {
    if (!selectedId) {
      setEvents(null);
      return;
    }
    let cancelled = false;
    invoke<unknown>("get_events", { itemId: selectedId })
      .then((raw) => {
        if (cancelled) return;
        setEvents(EventList.parse(raw));
        setError(null);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(typeof err === "string" ? err : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, item?.updated_at]);

  if (!selectedId) return null;

  return (
    <aside className="inspector" role="complementary" aria-label="Item history">
      <header className="inspector-header">
        <h3 className="inspector-title">Item history</h3>
        <button
          type="button"
          onClick={() => setSelected(null)}
          aria-label="Close inspector"
        >
          ×
        </button>
      </header>

      {item ? (
        <div className="inspector-summary">
          <div className="inspector-content">"{item.content}"</div>
          <dl className="inspector-meta">
            <dt>Tier</dt>
            <dd>{tierLabel(item.tier)}</dd>
            <dt>Rank</dt>
            <dd className="inspector-mono">{item.rank}</dd>
            <dt>State</dt>
            <dd>{item.state}</dd>
            <dt>Created</dt>
            <dd>{formatTs(item.created_at)}</dd>
            <dt>Updated</dt>
            <dd>{formatTs(item.updated_at)}</dd>
            {item.start_at !== null ? (
              <>
                <dt>Start</dt>
                <dd>{format(item.start_at, "yyyy-MM-dd")}</dd>
              </>
            ) : null}
            {item.due_at !== null ? (
              <>
                <dt>Due</dt>
                <dd>{format(item.due_at, "yyyy-MM-dd")}</dd>
              </>
            ) : null}
            {item.recurrence !== null ? (
              <>
                <dt>Repeats</dt>
                <dd className="inspector-mono">{item.recurrence}</dd>
              </>
            ) : null}
            {item.deleted ? (
              <>
                <dt>Deleted</dt>
                <dd>yes</dd>
              </>
            ) : null}
          </dl>
        </div>
      ) : (
        <div className="inspector-summary">
          <div className="inspector-content is-dim">
            (item is not in the current projection — may have been deleted
            or never existed at this time)
          </div>
        </div>
      )}

      <section className="inspector-events">
        <h4>Events</h4>
        {error ? <div className="modal-error">{error}</div> : null}
        {events === null ? (
          <div className="is-dim">Loading…</div>
        ) : events.length === 0 ? (
          <div className="is-dim">No events for this item.</div>
        ) : (
          <ol className="inspector-event-list">
            {events.map((ev) => (
              <EventRow key={ev.id} event={ev} />
            ))}
          </ol>
        )}
      </section>
    </aside>
  );
}

function EventRow({ event }: { event: Event }) {
  const when = formatTs(event.ts);
  return (
    <li className="inspector-event-row">
      <div className="inspector-event-when">{when}</div>
      <div className="inspector-event-body">
        <div className="inspector-event-type">{event.type}</div>
        <div className="inspector-event-payload">{renderPayload(event)}</div>
      </div>
    </li>
  );
}

function renderPayload(event: Event): string {
  const p = event.payload as Record<string, unknown>;
  switch (event.type) {
    case "ITEM_CREATED":
      return `tier=${p.tier}, rank=${p.rank}`;
    case "ITEM_EDITED":
      return `"${p.content_before}" → "${p.content_after}"`;
    case "ITEM_MOVED":
      return (
        `${p.tier_before}/${p.rank_before} → ${p.tier_after}/${p.rank_after}` +
        (p.reason ? ` · reason: ${p.reason}` : "")
      );
    case "ITEM_STATE_CHANGED":
      return (
        `${p.state_before} → ${p.state_after}` +
        (p.blocked_reason ? ` · reason: ${p.blocked_reason}` : "")
      );
    case "ITEM_DATE_SET":
      return `${p.field}: ${p.value_before ?? "∅"} → ${p.value_after ?? "∅"}`;
    case "ITEM_DELETED":
      return "soft delete";
    case "ITEM_RESTORED":
      return "restored";
    case "ITEM_RECURRENCE_SET":
      return `${p.before ?? "∅"} → ${p.after ?? "∅"}`;
    case "ITEM_RECURRED":
      return `spawned next instance (${p.child_id})`;
    case "ITEM_FIRST_STEP_SET":
      return `"${p.before ?? "∅"}" → "${p.after ?? "∅"}"`;
    case "TODAY_ADDED":
      return `→ Today ${p.date}`;
    case "TODAY_REMOVED":
      return `← Today ${p.date} (${p.cause})`;
    case "SESSION_STARTED":
      return "focus session started";
    case "SESSION_ENDED":
      return `session ${p.outcome}${p.reason ? ` (${p.reason})` : ""}`;
    default:
      try {
        return JSON.stringify(p);
      } catch {
        return "(unrenderable)";
      }
  }
}

function formatTs(ms: number): string {
  return format(ms, "yyyy-MM-dd HH:mm:ss");
}

function tierLabel(tier: Tier): string {
  return tier === "inbox" ? "Inbox" : tier;
}
