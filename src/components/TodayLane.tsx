// v0.3 execution core: the Today lane — the day's ≤3 commitments,
// chosen once at day-open so the rest of the day is re-decision-free
// (VISION law 4). Sits above the board; each row starts a focus
// session in one click.
//
// The FRONTEND owns "what day is it" — only it knows the local
// timezone. It calls roll_day on mount (expiring yesterday's
// membership as a system write) and reads get_day_state to decide
// whether to show the morning invitation.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { Item, Session } from "../domain";
import { useStore } from "../store";

/** Local calendar date as YYYY-MM-DD (never UTC — the day boundary is
 *  the user's, not Greenwich's). */
export function localDate(d: Date = new Date()): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

type DayState = {
  today_ids: string[];
  day_opened: boolean;
  tomorrow_first: string | null;
};

export function TodayLane() {
  const items = useStore((s) => s.items);
  const bootstrapped = useStore((s) => s.bootstrapped);
  const onItemUpdated = useStore((s) => s.onItemUpdated);
  const setOpenSession = useStore((s) => s.setOpenSession);
  const hasOpenSession = useStore((s) => s.openSession !== null);
  const [date] = useState(localDate());
  const [dayState, setDayState] = useState<DayState | null>(null);
  const [picking, setPicking] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const raw = await invoke<DayState>("get_day_state", { date });
      setDayState(raw);
    } catch (err) {
      console.error("get_day_state failed:", err);
    }
  }, [date]);

  // Roll first (expire stale membership), then read the day's state.
  useEffect(() => {
    if (!bootstrapped) return;
    invoke("roll_day", { today: date })
      .catch((err) => console.error("roll_day failed:", err))
      .then(refresh);
  }, [bootstrapped, date, refresh]);

  if (!bootstrapped) return null;

  const todayItems = (dayState?.today_ids ?? [])
    .map((id) => items[id])
    .filter((it): it is Item => Boolean(it));
  const activeCount = todayItems.filter((i) => i.state === "active").length;

  return (
    <section className="today-lane" aria-label="Today">
      <header className="today-header">
        <h2 className="today-title">Today</h2>
        <span className="today-counter">{activeCount} / 3</span>
        <button type="button" onClick={() => setPicking(true)}>
          {dayState?.day_opened ? "Adjust…" : "Plan today…"}
        </button>
        <DayCloseButton date={date} />
      </header>

      {todayItems.length === 0 ? (
        <p className="today-empty">
          Nothing chosen yet.{" "}
          {dayState?.tomorrow_first && items[dayState.tomorrow_first]
            ? `Last night you named "${items[dayState.tomorrow_first].content}" as the first move.`
            : "Pick up to three things you will actually do."}
        </p>
      ) : (
        <ul className="today-list">
          {todayItems.map((item) => (
            <li
              key={item.id}
              className={"today-row" + (item.state === "done" ? " is-done" : "")}
            >
              <span className="today-tier">{item.tier}</span>
              <span className="today-content">{item.content}</span>
              {item.first_step ? (
                <span className="today-first-step">→ {item.first_step}</span>
              ) : null}
              {item.state === "active" && !hasOpenSession ? (
                <button
                  type="button"
                  className="today-start"
                  onClick={async () => {
                    try {
                      const raw = await invoke<unknown>("start_session", {
                        itemId: item.id,
                      });
                      setOpenSession(Session.parse(raw));
                    } catch (err) {
                      console.error("start_session failed:", err);
                    }
                  }}
                >
                  ▶ Start
                </button>
              ) : null}
              <button
                type="button"
                className="today-remove"
                aria-label={`Remove ${item.content} from Today`}
                onClick={async () => {
                  try {
                    const raw = await invoke<unknown>("remove_from_today", {
                      id: item.id,
                    });
                    onItemUpdated(Item.parse(raw));
                    void refresh();
                  } catch (err) {
                    console.error("remove_from_today failed:", err);
                  }
                }}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}

      {picking ? (
        <TodayPicker
          date={date}
          chosen={dayState?.today_ids ?? []}
          suggested={dayState?.tomorrow_first ?? null}
          onClose={() => {
            setPicking(false);
            void refresh();
          }}
        />
      ) : null}
    </section>
  );
}

function TodayPicker({
  date,
  chosen,
  suggested,
  onClose,
}: {
  date: string;
  chosen: string[];
  suggested: string | null;
  onClose: () => void;
}) {
  const items = useStore((s) => s.items);
  const itemsByTier = useStore((s) => s.itemsByTier);
  const onItemUpdated = useStore((s) => s.onItemUpdated);
  const [selected, setSelected] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  // Candidates: active A first (the committed tier), then B. Already-
  // chosen items are excluded; the suggested "tomorrow's first move"
  // floats to the top.
  const candidates = [...itemsByTier.A, ...itemsByTier.B]
    .map((id) => items[id])
    .filter((it): it is Item => Boolean(it) && it.state === "active")
    .filter((it) => !chosen.includes(it.id))
    .sort((a, b) => (a.id === suggested ? -1 : b.id === suggested ? 1 : 0));

  const remaining = 3 - chosen.length - selected.length;

  async function commit() {
    setError(null);
    try {
      const raw = await invoke<unknown[]>("open_day", {
        date,
        todayIds: [...chosen, ...selected],
      });
      for (const r of raw) onItemUpdated(Item.parse(r));
      onClose();
    } catch (err) {
      setError(typeof err === "string" ? err : String(err));
    }
  }

  return (
    <div className="today-picker" role="dialog" aria-label="Plan today">
      <p className="today-picker-prompt">
        What will you actually do today? {remaining > 0 ? `${remaining} slot${remaining === 1 ? "" : "s"} left.` : "Today is full."}
      </p>
      <ul className="today-picker-list">
        {candidates.length === 0 ? (
          <li className="is-dim">No active A or B items to choose from.</li>
        ) : null}
        {candidates.map((item) => {
          const isSelected = selected.includes(item.id);
          return (
            <li key={item.id}>
              <label>
                <input
                  type="checkbox"
                  checked={isSelected}
                  disabled={!isSelected && remaining <= 0}
                  onChange={() =>
                    setSelected((prev) =>
                      prev.includes(item.id)
                        ? prev.filter((id) => id !== item.id)
                        : [...prev, item.id],
                    )
                  }
                />
                <span className="today-picker-tier">{item.tier}</span>
                {item.content}
                {item.id === suggested ? (
                  <span className="today-picker-hint"> · your first move</span>
                ) : null}
              </label>
            </li>
          );
        })}
      </ul>
      {error ? <div className="modal-error">{error}</div> : null}
      <div className="today-picker-actions">
        <button type="button" onClick={onClose}>
          Cancel
        </button>
        <button type="button" onClick={() => void commit()} disabled={selected.length === 0}>
          Commit to {selected.length}
        </button>
      </div>
    </div>
  );
}

function DayCloseButton({ date }: { date: string }) {
  const items = useStore((s) => s.items);
  const itemsByTier = useStore((s) => s.itemsByTier);
  const [open, setOpen] = useState(false);
  const [first, setFirst] = useState<string>("");

  const candidates = [...itemsByTier.A, ...itemsByTier.B]
    .map((id) => items[id])
    .filter((it): it is Item => Boolean(it) && it.state === "active");

  async function commit() {
    try {
      await invoke("close_day", {
        date,
        tomorrowFirst: first || null,
        note: null,
      });
      setOpen(false);
    } catch (err) {
      console.error("close_day failed:", err);
    }
  }

  if (!open) {
    return (
      <button type="button" onClick={() => setOpen(true)} title="Close the day">
        Close day…
      </button>
    );
  }
  return (
    <span className="day-close" role="dialog" aria-label="Close day">
      <label>
        Tomorrow&rsquo;s first move:{" "}
        <select value={first} onChange={(e) => setFirst(e.target.value)}>
          <option value="">(decide tomorrow)</option>
          {candidates.map((it) => (
            <option key={it.id} value={it.id}>
              {it.content}
            </option>
          ))}
        </select>
      </label>
      <button type="button" onClick={() => void commit()}>
        Close day
      </button>
      <button type="button" onClick={() => setOpen(false)}>
        Cancel
      </button>
    </span>
  );
}
