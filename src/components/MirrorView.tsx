// v0.3 Mirror: deterministic feedback, no LLM in the path. Every
// number here comes from get_mirror_stats (SQL + one log pass).
//
// Tone rule (VISION principle 6 / law 9): this view confronts, it never
// shames. No red, no badges, no streaks — plain figures, plainly
// stated, with completed work kept visible as evidence.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { format } from "date-fns";

type MirrorStats = {
  window_days: number;
  generated_at: number;
  wip: { inbox: number; a: number; b: number; c: number };
  flow: {
    created: number;
    completed: number;
    throughput_per_week: number;
    lead_time_p50_days: number | null;
    lead_time_p90_days: number | null;
    littles_law_days: number | null;
  };
  a_leak: { departures: number; fast_leaks: number; rate: number };
  avoidance: Array<{
    item_id: string;
    content: string;
    tier: string;
    days_since_touch: number;
    sessions: number;
    has_first_step: boolean;
  }>;
  blocks: Array<{ reason: string; count: number; total_days: number }>;
  sessions: {
    count: number;
    total_minutes: number;
    median_minutes: number | null;
    done: number;
    progress: number;
    interrupted: number;
    interruptions: Array<[string, number]>;
  };
  today: { planned: number; finished: number; expired: number };
  receipts: Array<{
    item_id: string;
    content: string;
    tier: string;
    done_at: number;
    days_to_done: number;
    sessions: number;
    minutes: number;
  }>;
};

const WINDOWS = [7, 30, 90];

export function MirrorView() {
  const [windowDays, setWindowDays] = useState(30);
  const [stats, setStats] = useState<MirrorStats | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const raw = await invoke<MirrorStats>("get_mirror_stats", { windowDays });
      setStats(raw);
    } catch (err) {
      setError(typeof err === "string" ? err : String(err));
    }
  }, [windowDays]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="mirror">
      <header className="mirror-header">
        <h2>Mirror</h2>
        <span className="mirror-windows">
          {WINDOWS.map((w) => (
            <button
              key={w}
              type="button"
              className={"view-button" + (w === windowDays ? " is-active" : "")}
              onClick={() => setWindowDays(w)}
            >
              {w}d
            </button>
          ))}
        </span>
        <span className="is-dim">computed locally · no model involved</span>
      </header>

      {error ? <div className="modal-error">{error}</div> : null}
      {!stats ? (
        <p className="is-dim">Reading the log…</p>
      ) : (
        <>
          <section className="mirror-section">
            <h3>Flow</h3>
            <div className="mirror-figures">
              <Figure label="Created" value={stats.flow.created} />
              <Figure label="Completed" value={stats.flow.completed} />
              <Figure
                label="Throughput"
                value={`${stats.flow.throughput_per_week.toFixed(1)}/wk`}
              />
              <Figure
                label="Lead time (median)"
                value={fmtDays(stats.flow.lead_time_p50_days)}
              />
              <Figure
                label="Lead time (p90)"
                value={fmtDays(stats.flow.lead_time_p90_days)}
              />
              <Figure
                label="Committed WIP"
                value={stats.wip.a + stats.wip.b}
                hint={`A ${stats.wip.a} · B ${stats.wip.b} · inbox ${stats.wip.inbox} · C ${stats.wip.c}`}
              />
            </div>
            {stats.flow.littles_law_days !== null ? (
              <p className="mirror-note">
                At this throughput, your {stats.wip.a + stats.wip.b} committed
                items imply roughly{" "}
                <strong>{fmtDays(stats.flow.littles_law_days)}</strong> before a
                newly promoted item finishes (Little&rsquo;s law: WIP ÷
                throughput).
                {stats.flow.lead_time_p50_days !== null &&
                stats.flow.littles_law_days > stats.flow.lead_time_p50_days * 2
                  ? " Your measured lead time is much shorter — the board holds work you are not actually starting."
                  : ""}
              </p>
            ) : null}
          </section>

          <section className="mirror-section">
            <h3>A-tier discipline</h3>
            {stats.a_leak.departures === 0 ? (
              <p className="is-dim">No items left A in this window.</p>
            ) : (
              <p className="mirror-note">
                {stats.a_leak.fast_leaks} of {stats.a_leak.departures} departures
                from A happened within 48 hours of arriving (
                {Math.round(stats.a_leak.rate * 100)}%).
                {stats.a_leak.rate >= 0.4
                  ? " A is functioning as a second inbox."
                  : ""}
              </p>
            )}
          </section>

          <section className="mirror-section">
            <h3>Attention</h3>
            <div className="mirror-figures">
              <Figure label="Sessions" value={stats.sessions.count} />
              <Figure
                label="Time in focus"
                value={fmtMinutes(stats.sessions.total_minutes)}
              />
              <Figure
                label="Typical session"
                value={
                  stats.sessions.median_minutes === null
                    ? "—"
                    : fmtMinutes(stats.sessions.median_minutes)
                }
              />
              <Figure
                label="Ended"
                value={`${stats.sessions.done} done · ${stats.sessions.progress} paused · ${stats.sessions.interrupted} broken`}
              />
            </div>
            {stats.sessions.interruptions.length > 0 ? (
              <ul className="mirror-bars">
                {stats.sessions.interruptions.map(([cause, n]) => (
                  <Bar
                    key={cause}
                    label={cause.replace("_", " ")}
                    value={n}
                    max={stats.sessions.interrupted}
                    suffix={`${n}`}
                  />
                ))}
              </ul>
            ) : null}
          </section>

          <section className="mirror-section">
            <h3>Not started</h3>
            {stats.avoidance.length === 0 ? (
              <p className="is-dim">
                Every committed item has recorded attention. That is rare.
              </p>
            ) : (
              <>
                <p className="mirror-note">
                  Committed items with no focus session on record — the honest
                  options are promote, demote, or delete.
                </p>
                <ul className="mirror-rows">
                  {stats.avoidance.map((row) => (
                    <li key={row.item_id}>
                      <span className="mirror-tier">{row.tier}</span>
                      <span className="mirror-row-content">{row.content}</span>
                      <span className="is-dim">
                        {row.days_since_touch}d untouched
                        {row.has_first_step ? "" : " · no first step"}
                      </span>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </section>

          <section className="mirror-section">
            <h3>What holds work up</h3>
            {stats.blocks.length === 0 ? (
              <p className="is-dim">Nothing blocked in this window.</p>
            ) : (
              <ul className="mirror-bars">
                {stats.blocks.map((b) => (
                  <Bar
                    key={b.reason}
                    label={b.reason}
                    value={b.total_days}
                    max={Math.max(...stats.blocks.map((x) => x.total_days))}
                    suffix={`${b.total_days.toFixed(1)}d · ${b.count}×`}
                  />
                ))}
              </ul>
            )}
          </section>

          <section className="mirror-section">
            <h3>Today, honestly</h3>
            <div className="mirror-figures">
              <Figure label="Planned" value={stats.today.planned} />
              <Figure label="Finished" value={stats.today.finished} />
              <Figure label="Rolled over" value={stats.today.expired} />
            </div>
          </section>

          <section className="mirror-section">
            <h3>Finished</h3>
            {stats.receipts.length === 0 ? (
              <p className="is-dim">Nothing completed in this window yet.</p>
            ) : (
              <ul className="mirror-rows">
                {stats.receipts.map((r) => (
                  <li key={r.item_id}>
                    <span className="mirror-tier">{r.tier}</span>
                    <span className="mirror-row-content">{r.content}</span>
                    <span className="is-dim">
                      {format(r.done_at, "MMM d")} · {r.days_to_done.toFixed(1)}d
                      to done
                      {r.sessions > 0
                        ? ` · ${r.sessions} session${r.sessions === 1 ? "" : "s"}, ${fmtMinutes(r.minutes)}`
                        : ""}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      )}
    </div>
  );
}

function Figure({
  label,
  value,
  hint,
}: {
  label: string;
  value: string | number;
  hint?: string;
}) {
  return (
    <div className="mirror-figure" title={hint}>
      <div className="mirror-figure-value">{value}</div>
      <div className="mirror-figure-label">{label}</div>
    </div>
  );
}

function Bar({
  label,
  value,
  max,
  suffix,
}: {
  label: string;
  value: number;
  max: number;
  suffix: string;
}) {
  const pct = max > 0 ? Math.max(4, (value / max) * 100) : 0;
  return (
    <li className="mirror-bar-row">
      <span className="mirror-bar-label">{label}</span>
      <span className="mirror-bar-track">
        <span className="mirror-bar-fill" style={{ width: `${pct}%` }} />
      </span>
      <span className="mirror-bar-suffix is-dim">{suffix}</span>
    </li>
  );
}

function fmtDays(d: number | null): string {
  if (d === null) return "—";
  if (d < 1) return `${Math.round(d * 24)}h`;
  return `${d.toFixed(1)}d`;
}

function fmtMinutes(m: number): string {
  if (m < 60) return `${Math.round(m)}m`;
  const h = Math.floor(m / 60);
  const rem = Math.round(m % 60);
  return rem === 0 ? `${h}h` : `${h}h ${rem}m`;
}
