// LLM analyze panel: side drawer summarizing observations from the
// most recent analyze run. LLM is advisory-only (CLAUDE.md §Design
// philosophy #2); the buttons at the bottom emit
// LLM_SUGGESTION_ACCEPTED or LLM_SUGGESTION_REJECTED to the event
// log, never any item mutations.

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { useStore } from "../store";
import { Item } from "../domain";

type Severity = "info" | "warn";

type Observation = {
  severity: Severity;
  text: string;
  affected_item_ids: string[];
};

type ProposalAction = "move" | "done" | "active";

// A re-org the LLM proposed (I-20). Advisory until the user accepts it;
// shape matches the Rust ReorgProposal (snake_case) so it can be passed
// straight back to accept_suggestion as `ops`.
type Proposal = {
  item_id: string;
  action: ProposalAction;
  to_tier: string | null;
  rationale: string | null;
};

type AnalyzeResult = {
  suggestion_event_id: number;
  observations: Observation[];
  proposals: Proposal[];
  scope: {
    since_ts: number;
    until_ts: number;
    event_count: number;
    window_days: number;
  };
  model: string;
};

type Stage =
  | { kind: "idle" }
  | { kind: "running"; stage: string }
  | { kind: "ok"; result: AnalyzeResult }
  | { kind: "error"; message: string };

export function AnalyzePanel({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const [state, setState] = useState<Stage>({ kind: "idle" });
  const setSelectedItemId = useStore((s) => s.setSelectedItemId);
  const items = useStore((s) => s.items);
  const [selectedProposals, setSelectedProposals] = useState<Set<number>>(
    new Set(),
  );
  const [applyError, setApplyError] = useState<string | null>(null);

  // When a result arrives, pre-select every proposal and clear any prior
  // apply error.
  useEffect(() => {
    if (state.kind === "ok") {
      setSelectedProposals(new Set(state.result.proposals.map((_, i) => i)));
      setApplyError(null);
    }
  }, [state]);

  // Reset on open; subscribe to progress events while mounted.
  useEffect(() => {
    if (!open) return;
    setState({ kind: "idle" });
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<{ stage: string }>("analyze_progress", (ev) => {
      setState((prev) =>
        prev.kind === "running" ? { kind: "running", stage: ev.payload.stage } : prev,
      );
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [open]);

  async function run() {
    setState({ kind: "running", stage: "starting" });
    try {
      const raw = await invoke<AnalyzeResult>("analyze", {});
      setState({ kind: "ok", result: raw });
    } catch (err) {
      const message = typeof err === "string" ? err : String(err);
      setState({ kind: "error", message });
    }
  }

  async function markReviewed(id: number) {
    try {
      await invoke("accept_suggestion", { suggestionEventId: id });
      onClose();
    } catch (err) {
      console.error("accept_suggestion failed:", err);
    }
  }

  async function dismiss(id: number) {
    try {
      await invoke("reject_suggestion", {
        suggestionEventId: id,
        reason: null,
      });
      onClose();
    } catch (err) {
      console.error("reject_suggestion failed:", err);
    }
  }

  // Apply the user-selected subset of the LLM's proposed re-org. The
  // backend re-validates and writes atomically under cap enforcement
  // (firewall: LLM proposed, human accepted, deterministic tier writes).
  async function applyReorg(id: number, ops: Proposal[]) {
    setApplyError(null);
    try {
      await invoke("accept_suggestion", { suggestionEventId: id, ops });
      onClose();
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      setApplyError(
        msg.includes("CAP_EXCEEDED")
          ? "That re-org would exceed an A/B cap. Deselect a promotion or free a slot first."
          : `Couldn't apply re-org: ${msg}`,
      );
    }
  }

  function toggleProposal(i: number) {
    setSelectedProposals((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      return next;
    });
  }

  if (!open) return null;

  return (
    <aside
      className="analyze-panel"
      role="complementary"
      aria-label="Analyze observations"
    >
      <header className="analyze-header">
        <h3>Analyze</h3>
        <button type="button" onClick={onClose} aria-label="Close analyze">
          ×
        </button>
      </header>

      <div className="analyze-body">
        {state.kind === "idle" ? (
          <div className="analyze-intro">
            <p>
              Sends a compressed summary of your last analyze-window
              days of activity to the configured LLM. It never mutates
              the board — only returns observations.
            </p>
            <button type="button" className="is-primary" onClick={run}>
              Run analyze
            </button>
          </div>
        ) : null}

        {state.kind === "running" ? (
          <div className="analyze-running">
            <div className="analyze-spinner" aria-hidden="true" />
            <div className="analyze-stage">
              {humanStage(state.stage)}…
            </div>
          </div>
        ) : null}

        {state.kind === "error" ? (
          <div className="analyze-error">
            <p>{humanError(state.message)}</p>
            <button type="button" onClick={run}>
              Try again
            </button>
          </div>
        ) : null}

        {state.kind === "ok" ? (
          <>
            <div className="analyze-scope">
              Model: <code>{state.result.model}</code> · window{" "}
              {state.result.scope.window_days}d ·{" "}
              {state.result.scope.event_count} events
            </div>
            {state.result.observations.length === 0 ? (
              <div className="analyze-empty">
                The LLM returned no observations. Either your activity
                doesn't show a notable pattern, or the model couldn't
                find one worth flagging.
              </div>
            ) : (
              <ul className="analyze-list">
                {state.result.observations.map((obs, i) => (
                  <li key={i} className={`analyze-obs is-${obs.severity}`}>
                    <span className="analyze-obs-icon" aria-hidden="true">
                      {obs.severity === "warn" ? "⚠" : "ℹ"}
                    </span>
                    <div className="analyze-obs-body">
                      <div className="analyze-obs-text">{obs.text}</div>
                      {obs.affected_item_ids.length > 0 ? (
                        <div className="analyze-obs-links">
                          {obs.affected_item_ids.map((id) => (
                            <button
                              key={id}
                              type="button"
                              className="analyze-obs-link"
                              onClick={() => setSelectedItemId(id)}
                            >
                              view item
                            </button>
                          ))}
                        </div>
                      ) : null}
                    </div>
                  </li>
                ))}
              </ul>
            )}
            {state.result.proposals.length > 0 ? (
              <div className="analyze-reorg">
                <div className="analyze-reorg-head">
                  Suggested re-org{" "}
                  <span className="analyze-reorg-note">
                    (you choose what to apply — nothing happens until you do)
                  </span>
                </div>
                <ul className="analyze-reorg-list">
                  {state.result.proposals.map((p, i) => (
                    <li key={i} className="analyze-reorg-item">
                      <label className="analyze-reorg-label">
                        <input
                          type="checkbox"
                          checked={selectedProposals.has(i)}
                          onChange={() => toggleProposal(i)}
                        />
                        <span className="analyze-reorg-desc">
                          {describeProposal(p, items)}
                        </span>
                      </label>
                      {p.rationale ? (
                        <div className="analyze-reorg-why">{p.rationale}</div>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}

            {applyError ? (
              <div className="analyze-error analyze-apply-error">
                {applyError}
              </div>
            ) : null}

            <div className="analyze-actions">
              <button
                type="button"
                onClick={() => dismiss(state.result.suggestion_event_id)}
              >
                Dismiss
              </button>
              {state.result.proposals.length > 0 &&
              selectedProposals.size > 0 ? (
                <button
                  type="button"
                  className="is-primary"
                  onClick={() =>
                    applyReorg(
                      state.result.suggestion_event_id,
                      state.result.proposals.filter((_, i) =>
                        selectedProposals.has(i),
                      ),
                    )
                  }
                >
                  Apply {selectedProposals.size} change
                  {selectedProposals.size === 1 ? "" : "s"}
                </button>
              ) : (
                <button
                  type="button"
                  className="is-primary"
                  onClick={() => markReviewed(state.result.suggestion_event_id)}
                >
                  Mark reviewed
                </button>
              )}
            </div>
          </>
        ) : null}
      </div>
    </aside>
  );
}

function describeProposal(p: Proposal, items: Record<string, Item>): string {
  const raw = items[p.item_id]?.content ?? p.item_id.slice(0, 8);
  const short = raw.length > 44 ? `${raw.slice(0, 44)}…` : raw;
  switch (p.action) {
    case "move":
      return `Move “${short}” → ${p.to_tier}`;
    case "done":
      return `Mark “${short}” done`;
    case "active":
      return `Mark “${short}” active`;
  }
}

function humanStage(s: string): string {
  switch (s) {
    case "compressing":
      return "Compressing event log";
    case "calling_llm":
      return "Calling LLM";
    case "parsing":
      return "Parsing response";
    case "retrying_parse":
      return "Retrying (first response wasn't valid JSON)";
    case "starting":
      return "Starting";
    default:
      return s;
  }
}

function humanError(msg: string): string {
  if (msg.startsWith("LLM_UNREACHABLE"))
    return "Can't reach the LLM endpoint. Check that Ollama is running or your API endpoint is correct.";
  if (msg === "LLM_AUTH_FAILED")
    return "LLM rejected the API key. Re-enter it in Settings → LLM.";
  if (msg === "LLM_TIMEOUT")
    return "LLM took too long. Increase the timeout in Settings or try a smaller model.";
  if (msg.startsWith("LLM_PARSE_ERROR"))
    return "LLM returned an unparseable response. This may be a model quality issue — try a different model.";
  if (msg === "LLM_RATE_LIMITED")
    return "LLM endpoint is rate limiting. Try again in a moment.";
  return msg;
}
