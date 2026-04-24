// Monthly calendar grid. Pills represent items with start_at or due_at
// falling in the visible month. Click a day to open the day sheet
// listing all items anchored to that day (start or due). Click a pill
// to jump back to the board with selectedItemId set so the inspector
// panel (I-10) can focus on that item.

import { useMemo, useState } from "react";
import {
  addMonths,
  eachDayOfInterval,
  endOfMonth,
  endOfWeek,
  format,
  isSameDay,
  isSameMonth,
  isToday,
  startOfMonth,
  startOfWeek,
} from "date-fns";

import { Item, Tier } from "../domain";
import { useStore } from "../store";

type DayEntry = { item: Item; field: "start" | "due" };

const TIER_ORDER: Record<Tier, number> = { inbox: 0, A: 1, B: 2, C: 3 };

export function CalendarView({ onFocusItem }: { onFocusItem: () => void }) {
  const [monthAnchor, setMonthAnchor] = useState<Date>(startOfMonth(new Date()));
  const items = useStore((s) => s.items);
  const setSelectedItemId = useStore((s) => s.setSelectedItemId);

  const days = useMemo(() => {
    const gridStart = startOfWeek(monthAnchor, { weekStartsOn: 0 });
    const gridEnd = endOfWeek(endOfMonth(monthAnchor), { weekStartsOn: 0 });
    return eachDayOfInterval({ start: gridStart, end: gridEnd });
  }, [monthAnchor]);

  const byDay = useMemo(() => {
    const map = new Map<string, DayEntry[]>();
    for (const item of Object.values(items)) {
      if (item.deleted) continue;
      if (item.start_at !== null) {
        pushIntoDayMap(map, item.start_at, { item, field: "start" });
      }
      if (item.due_at !== null) {
        pushIntoDayMap(map, item.due_at, { item, field: "due" });
      }
    }
    // Within a day, sort: start before due, then tier, then content.
    for (const entries of map.values()) {
      entries.sort((a, b) => {
        if (a.field !== b.field) return a.field === "start" ? -1 : 1;
        if (a.item.tier !== b.item.tier)
          return TIER_ORDER[a.item.tier] - TIER_ORDER[b.item.tier];
        return a.item.content.localeCompare(b.item.content);
      });
    }
    return map;
  }, [items]);

  const [daySheet, setDaySheet] = useState<Date | null>(null);
  const daySheetEntries = daySheet ? (byDay.get(dayKey(daySheet)) ?? []) : [];

  function jumpToItem(id: string) {
    setSelectedItemId(id);
    onFocusItem();
  }

  return (
    <div className="calendar">
      <header className="calendar-header">
        <button
          type="button"
          onClick={() => setMonthAnchor((m) => addMonths(m, -1))}
          aria-label="Previous month"
        >
          ◂
        </button>
        <h2 className="calendar-title">{format(monthAnchor, "MMMM yyyy")}</h2>
        <button
          type="button"
          onClick={() => setMonthAnchor((m) => addMonths(m, 1))}
          aria-label="Next month"
        >
          ▸
        </button>
        <button
          type="button"
          className="calendar-today"
          onClick={() => setMonthAnchor(startOfMonth(new Date()))}
        >
          Today
        </button>
      </header>

      <div className="calendar-dowrow" aria-hidden="true">
        {["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].map((d) => (
          <div key={d} className="calendar-dow">
            {d}
          </div>
        ))}
      </div>

      <div className="calendar-grid">
        {days.map((day) => {
          const entries = byDay.get(dayKey(day)) ?? [];
          const inMonth = isSameMonth(day, monthAnchor);
          return (
            <button
              key={day.toISOString()}
              type="button"
              className={
                "calendar-day" +
                (inMonth ? "" : " is-outside") +
                (isToday(day) ? " is-today" : "")
              }
              onClick={() => setDaySheet(day)}
            >
              <div className="calendar-day-number">{format(day, "d")}</div>
              <div className="calendar-day-pills">
                {entries.slice(0, 3).map((e, i) => (
                  <span
                    key={`${e.item.id}-${e.field}-${i}`}
                    className={"calendar-pill pill-" + e.field}
                    title={e.item.content}
                  >
                    {e.field === "start" ? "▸" : "●"} {e.item.content}
                  </span>
                ))}
                {entries.length > 3 ? (
                  <span className="calendar-pill-overflow">
                    +{entries.length - 3} more
                  </span>
                ) : null}
              </div>
            </button>
          );
        })}
      </div>

      <footer className="calendar-legend">
        <span>▸ start date</span>
        <span>● due date</span>
      </footer>

      {daySheet !== null ? (
        <DaySheet
          day={daySheet}
          entries={daySheetEntries}
          onClose={() => setDaySheet(null)}
          onJump={jumpToItem}
        />
      ) : null}
    </div>
  );
}

function DaySheet({
  day,
  entries,
  onClose,
  onJump,
}: {
  day: Date;
  entries: DayEntry[];
  onClose: () => void;
  onJump: (id: string) => void;
}) {
  return (
    <div
      className="day-sheet-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="day-sheet" role="dialog" aria-label="Day items">
        <div className="day-sheet-header">
          <h3>{format(day, "EEEE, MMMM d, yyyy")}</h3>
          <button type="button" onClick={onClose} aria-label="Close day sheet">
            ×
          </button>
        </div>
        {entries.length === 0 ? (
          <p className="day-sheet-empty">No items for this day.</p>
        ) : (
          <ul className="day-sheet-list">
            {entries.map((e, i) => (
              <li key={`${e.item.id}-${e.field}-${i}`}>
                <button
                  type="button"
                  onClick={() => {
                    onJump(e.item.id);
                    onClose();
                  }}
                >
                  <span className="day-sheet-tag">
                    {e.field === "start" ? "▸" : "●"}
                  </span>
                  <span className="day-sheet-tier">{e.item.tier}</span>
                  <span className="day-sheet-content">{e.item.content}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function pushIntoDayMap(
  map: Map<string, DayEntry[]>,
  ms: number,
  entry: DayEntry,
): void {
  const key = dayKey(new Date(ms));
  const existing = map.get(key);
  if (existing) existing.push(entry);
  else map.set(key, [entry]);
}

function dayKey(d: Date): string {
  return (
    d.getFullYear() +
    "-" +
    String(d.getMonth() + 1).padStart(2, "0") +
    "-" +
    String(d.getDate()).padStart(2, "0")
  );
}

function isSameDayExported(a: Date, b: Date): boolean {
  return isSameDay(a, b);
}
// expose to avoid "unused" on the date-fns import while keeping the
// ceremony minimal in this file. Not used externally.
void isSameDayExported;
