// Read-only historical board view. Pick a timestamp on the scrubber;
// backend replays events up to that ts into an ephemeral in-memory
// projection and returns the items. No mutations possible here; all
// drag/edit/menu affordances are omitted by rendering a stripped-down
// strip directly rather than reusing the interactive Strip component.
//
// SPEC §2.8, §5.1 get_items_at, §10.6 resolution (fully read-only).

import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { format } from "date-fns";
import { z } from "zod";

import { A_CAP, B_CAP, Item, Tier } from "../domain";

const ItemList = z.array(Item);

export function TimeTravelView({ onExit }: { onExit: () => void }) {
  const now = useMemo(() => Date.now(), []);
  const [earliestTs, setEarliestTs] = useState<number | null>(null);
  const [ts, setTs] = useState<number>(now);
  const [items, setItems] = useState<Item[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Determine scrubber min: earliest event ts. If the log is empty we
  // fall back to "now minus one hour" so the slider still moves.
  useEffect(() => {
    invoke<unknown>("get_events", { limit: 1 })
      .then((raw) => {
        const parsed = z
          .array(z.object({ ts: z.number() }))
          .parse(raw);
        if (parsed.length > 0) setEarliestTs(parsed[0].ts);
        else setEarliestTs(now - 60 * 60 * 1000);
      })
      .catch((err) => {
        console.error("time-travel range fetch failed:", err);
        setEarliestTs(now - 60 * 60 * 1000);
      });
  }, [now]);

  // Re-replay when ts moves.
  useEffect(() => {
    let cancelled = false;
    invoke<unknown>("get_items_at", { ts })
      .then((raw) => {
        if (cancelled) return;
        setItems(ItemList.parse(raw));
        setError(null);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(typeof err === "string" ? err : String(err));
        setItems(null);
      });
    return () => {
      cancelled = true;
    };
  }, [ts]);

  const itemsByTier = useMemo(() => {
    const buckets: Record<Tier, Item[]> = { inbox: [], A: [], B: [], C: [] };
    if (!items) return buckets;
    for (const it of items) buckets[it.tier].push(it);
    for (const t of Object.keys(buckets) as Tier[]) {
      buckets[t].sort((a, b) => (a.rank < b.rank ? -1 : a.rank > b.rank ? 1 : 0));
    }
    return buckets;
  }, [items]);

  return (
    <div className="timetravel">
      <header className="timetravel-header">
        <span className="timetravel-label">TIME-TRAVEL · READ-ONLY</span>
        <span className="timetravel-ts">
          {format(ts, "yyyy-MM-dd HH:mm:ss")}
        </span>
        <input
          type="range"
          className="timetravel-slider"
          min={earliestTs ?? 0}
          max={now}
          step={60_000}
          value={ts}
          onChange={(e) => setTs(Number(e.target.value))}
          aria-label="Time-travel timestamp"
        />
        <button type="button" onClick={() => setTs(now)}>
          Now
        </button>
        <button type="button" onClick={onExit}>
          Exit
        </button>
      </header>

      {error ? <div className="modal-error">{error}</div> : null}

      <div className="board">
        <HistoricalBay tier="inbox" label="Inbox" items={itemsByTier.inbox} />
        <HistoricalBay
          tier="A"
          label="A"
          cap={A_CAP}
          items={itemsByTier.A}
        />
        <HistoricalBay
          tier="B"
          label="B"
          cap={B_CAP}
          items={itemsByTier.B}
        />
        <HistoricalBay tier="C" label="C" items={itemsByTier.C} />
      </div>
    </div>
  );
}

function HistoricalBay({
  tier,
  label,
  cap,
  items,
}: {
  tier: Tier;
  label: string;
  cap?: number;
  items: Item[];
}) {
  const active = items.filter((i) => i.state === "active").length;
  const counter =
    cap !== undefined ? `${active} / ${cap}` : `${items.length} items`;
  return (
    <section className="bay" data-tier={tier} aria-label={`${label} bay`}>
      <header className="bay-header">
        <h2 className="bay-title">{label}</h2>
        <span className="bay-counter">{counter}</span>
      </header>
      <div className="bay-body">
        {items.map((it) => (
          <div
            key={it.id}
            className={
              "strip is-readonly" +
              (it.state === "done" ? " is-done" : "") +
              (it.state === "blocked" ? " is-blocked" : "")
            }
          >
            <span className="strip-handle">
              {it.state === "blocked" ? "⏸" : "·"}
            </span>
            <span className="strip-content">
              {it.content}
              {it.blocked_reason ? (
                <span className="strip-blocked-reason">
                  {" · "}
                  {it.blocked_reason}
                </span>
              ) : null}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}
