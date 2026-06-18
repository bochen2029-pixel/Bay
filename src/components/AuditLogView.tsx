import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Event, EventType } from "../domain";

/**
 * I-18 Audit-log search view.
 *
 * Surfaces the event log as a first-class product surface. Full-text
 * search across event payloads (content, reasons, before/after values),
 * filter by event type and item id. The event log is the product per
 * CLAUDE.md §"Event log is the product" — undo, time-travel, analysis,
 * and now search are all queries against the log.
 *
 * Backend: search_events command (commands/events.rs). v1 is a pure-
 * Rust filter (case-insensitive substring on payload JSON + type/item/
 * date filters). FTS5 deferred per ADR (heavier migration; pure-Rust
 * is fine for single-user local-first event logs in the thousands).
 */

const EVENT_TYPES: (EventType | "")[] = [
  "",
  "ITEM_CREATED",
  "ITEM_EDITED",
  "ITEM_MOVED",
  "ITEM_STATE_CHANGED",
  "ITEM_DATE_SET",
  "ITEM_DELETED",
  "ITEM_RESTORED",
  "LLM_SUGGESTION_GENERATED",
  "LLM_SUGGESTION_ACCEPTED",
  "LLM_SUGGESTION_REJECTED",
];

const TYPE_COLORS: Record<string, string> = {
  ITEM_CREATED: "var(--accent)",
  ITEM_EDITED: "var(--fg-muted)",
  ITEM_MOVED: "#2e7d32",
  ITEM_STATE_CHANGED: "#e65100",
  ITEM_DATE_SET: "#6a1b9a",
  ITEM_DELETED: "#c0392b",
  ITEM_RESTORED: "#27ae60",
  LLM_SUGGESTION_GENERATED: "#1565c0",
  LLM_SUGGESTION_ACCEPTED: "#1565c0",
  LLM_SUGGESTION_REJECTED: "#1565c0",
};

function formatTs(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleString();
}

function summarizePayload(type: string, payload: unknown): string {
  if (typeof payload !== "object" || payload === null) return "";
  const p = payload as Record<string, unknown>;
  switch (type) {
    case "ITEM_CREATED":
      return `content="${p.content}" tier=${p.tier}`;
    case "ITEM_EDITED":
      return `"${p.content_before}" -> "${p.content_after}"`;
    case "ITEM_MOVED":
      return `${p.tier_before}/${p.rank_before} -> ${p.tier_after}/${p.rank_after}${p.reason ? ` (reason: ${p.reason})` : ""}`;
    case "ITEM_STATE_CHANGED":
      return `${p.state_before} -> ${p.state_after}${p.blocked_reason ? ` (reason: ${p.blocked_reason})` : ""}`;
    case "ITEM_DATE_SET":
      return `${p.field}: ${p.value_before ?? "null"} -> ${p.value_after ?? "null"}`;
    case "ITEM_DELETED":
      return `soft=${p.soft}`;
    case "ITEM_RESTORED":
      return "restored";
    case "LLM_SUGGESTION_GENERATED": {
      const obs = p.observations as Array<unknown> | undefined;
      return `${obs?.length ?? 0} observation(s)`;
    }
    case "LLM_SUGGESTION_ACCEPTED":
      return `suggestion ${p.suggestion_event_id} accepted`;
    case "LLM_SUGGESTION_REJECTED":
      return `suggestion ${p.suggestion_event_id} rejected`;
    default:
      return JSON.stringify(p);
  }
}

export function AuditLogView() {
  const [query, setQuery] = useState("");
  const [eventType, setEventType] = useState<string>("");
  const [itemId, setItemId] = useState("");
  const [results, setResults] = useState<Event[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searched, setSearched] = useState(false);

  async function runSearch() {
    setLoading(true);
    setError(null);
    try {
      const raw = await invoke<unknown[]>("search_events", {
        query: query.trim() || null,
        event_type: eventType || null,
        itemId: itemId.trim() || null,
        sinceTs: null,
        untilTs: null,
        limit: 200,
      });
      setResults(raw as Event[]);
      setSearched(true);
    } catch (err) {
      setError(typeof err === "string" ? err : String(err));
      setResults([]);
    } finally {
      setLoading(false);
    }
  }

  // Load initial results on mount (all events, limit 200).
  useEffect(() => {
    void runSearch();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      void runSearch();
    }
  }

  return (
    <section className="audit-log" aria-label="Audit log search">
      <header className="audit-log-header">
        <h2>Audit Log</h2>
        <span className="audit-log-count">
          {results.length} event{results.length === 1 ? "" : "s"}
        </span>
      </header>

      <div className="audit-log-filters">
        <input
          className="audit-log-search-input"
          placeholder="Search content, reasons, before/after… (Enter to search)"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          aria-label="Search query"
        />
        <select
          className="audit-log-type-select"
          value={eventType}
          onChange={(e) => setEventType(e.target.value)}
          aria-label="Filter by event type"
        >
          {EVENT_TYPES.map((t) => (
            <option key={t} value={t}>
              {t === "" ? "All types" : t}
            </option>
          ))}
        </select>
        <input
          className="audit-log-item-input"
          placeholder="Item ID (optional)"
          value={itemId}
          onChange={(e) => setItemId(e.target.value)}
          onKeyDown={onKeyDown}
          aria-label="Filter by item id"
        />
        <button
          type="button"
          className="audit-log-run-button"
          onClick={() => void runSearch()}
          disabled={loading}
        >
          {loading ? "Searching…" : "Search"}
        </button>
      </div>

      {error ? <div className="audit-log-error">{error}</div> : null}

      <ul className="audit-log-list">
        {results.map((event) => (
          <li key={event.id} className="audit-log-row">
            <span className="audit-log-id">#{event.id}</span>
            <span className="audit-log-ts">{formatTs(event.ts)}</span>
            <span
              className="audit-log-type"
              style={{ color: TYPE_COLORS[event.type] ?? "var(--fg)" }}
            >
              {event.type}
            </span>
            <span className="audit-log-item">
              {event.item_id ? event.item_id.slice(0, 8) + "…" : "—"}
            </span>
            <span className="audit-log-payload">
              {summarizePayload(event.type, event.payload)}
            </span>
          </li>
        ))}
        {searched && results.length === 0 && !error ? (
          <li className="audit-log-empty">No events match.</li>
        ) : null}
      </ul>
    </section>
  );
}
