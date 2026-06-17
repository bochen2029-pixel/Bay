import { useEffect, useMemo, useRef, useState } from "react";
import { Tier } from "../domain";
import { useStore } from "../store";

/**
 * I-15 Command Palette (Cmd/Ctrl+K).
 *
 * The biggest UX upgrade for a keyboard-driven single-user app. Opens
 * on Cmd/Ctrl+K; fuzzy-searches across: jump-to-item, create-in-tier,
 * set-view, run-analyze, open-settings, restore-from-archive. Reuses
 * existing store actions; no new write paths.
 *
 * Design: the palette is a pure UI surface over existing commands.
 * It dispatches store actions and invoke() calls that already exist;
 * it introduces no new event types, no new backend commands, no new
 * write paths. The LLM firewall and caps hold (create-in-tier routes
 * through create_item which enforces caps; restore routes through
 * restore_item which respects NOT_DELETED).
 *
 * SPEC §2.10 (Analyze panel) and §1.2 (component tree) — the palette
 * is a new always-mounted overlay like the modals; opened via keyboard,
 * not a button.
 */

type View = "board" | "calendar" | "timetravel" | "archive" | "settings";

interface Command {
  id: string;
  label: string;
  hint?: string;
  group: "navigate" | "create" | "item" | "action";
  run: () => void;
}

const VIEW_LABELS: Record<View, string> = {
  board: "Board",
  calendar: "Calendar",
  timetravel: "Time-travel",
  archive: "Archive",
  settings: "Settings",
};

const TIER_LABELS: Record<Tier, string> = {
  inbox: "Inbox",
  A: "A",
  B: "B",
  C: "C",
};

export function CommandPalette({
  view,
  onView,
  onAnalyze,
}: {
  view: View;
  onView: (v: View) => void;
  onAnalyze: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const items = useStore((s) => s.items);
  const itemsByTier = useStore((s) => s.itemsByTier);
  const setSelectedItemId = useStore((s) => s.setSelectedItemId);
  const openQuickCapture = useStore((s) => s.openQuickCapture);

  // Cmd/Ctrl+K toggles the palette. Esc closes (handled by the input
  // onKeyDown below). The listener is on document so it works from any
  // view.
  useEffect(() => {
    function handler(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((v) => !v);
      }
    }
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  // When opening, reset state and focus the input. When closing, clear.
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIndex(0);
      // Microtask to beat the dialog focus race (same pattern as
      // QuickCaptureModal).
      queueMicrotask(() => inputRef.current?.focus());
    }
  }, [open]);

  // Build the command list. Static commands first (navigate/create/
  // action), then jump-to-item (dynamic). The list is rebuilt on every
  // query keystroke but filtered below.
  const allCommands = useMemo<Command[]>(() => {
    const cmds: Command[] = [];

    // Navigate
    (Object.keys(VIEW_LABELS) as View[]).forEach((v) => {
      cmds.push({
        id: `nav-${v}`,
        label: `Go to ${VIEW_LABELS[v]}`,
        hint: v === view ? "current" : undefined,
        group: "navigate",
        run: () => {
          onView(v);
          setOpen(false);
        },
      });
    });

    // Create in tier (routes through QuickCaptureModal for inbox; for
    // A/B/C it would need a content prompt — defer to the bay inline
    // add by switching to board + focusing the bay add input is more
    // complex than the palette should own. Simplest: open quick-capture
    // for inbox; for A/B/C, switch to board view where the + Add
    // button is reachable.)
    cmds.push({
      id: "create-inbox",
      label: "Quick capture to Inbox",
      hint: "hotkey modal",
      group: "create",
      run: () => {
        openQuickCapture();
        setOpen(false);
      },
    });
    (["A", "B", "C"] as Tier[]).forEach((t) => {
      cmds.push({
        id: `create-${t}`,
        label: `Add item to ${TIER_LABELS[t]}`,
        hint: "switch to board",
        group: "create",
        run: () => {
          onView("board");
          setOpen(false);
        },
      });
    });

    // Actions
    cmds.push({
      id: "action-analyze",
      label: "Run Analyze",
      group: "action",
      run: () => {
        onAnalyze();
        setOpen(false);
      },
    });
    cmds.push({
      id: "action-archive",
      label: "Open Archive (restore deleted items)",
      group: "action",
      run: () => {
        onView("archive");
        setOpen(false);
      },
    });

    // Jump to item (dynamic). Show up to 8 matches; selecting one sets
    // selectedItemId (opens inspector) and switches to board.
    const allIds = (["inbox", "A", "B", "C"] as Tier[]).flatMap((t) => itemsByTier[t]);
    allIds.slice(0, 50).forEach((id) => {
      const item = items[id];
      if (!item || item.deleted) return;
      cmds.push({
        id: `item-${id}`,
        label: item.content,
        hint: `${TIER_LABELS[item.tier]} · ${item.state}`,
        group: "item",
        run: () => {
          setSelectedItemId(id);
          onView("board");
          setOpen(false);
        },
      });
    });

    return cmds;
  }, [view, items, itemsByTier, onView, onAnalyze, openQuickCapture, setSelectedItemId]);

  // Fuzzy filter: case-insensitive substring match on label. Good
  // enough for v1; a real fuzzy scorer can come later. Filter static
  // commands always; item commands only when query is non-empty (so
  // the palette doesn't drown in 50 items on open).
  const filtered = useMemo<Command[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return allCommands.filter((c) => c.group !== "item").slice(0, 12);
    return allCommands
      .filter((c) => c.label.toLowerCase().includes(q) || (c.hint?.toLowerCase().includes(q) ?? false))
      .slice(0, 20);
  }, [allCommands, query]);

  // Clamp activeIndex when filtered changes.
  useEffect(() => {
    setActiveIndex((i) => Math.min(i, Math.max(0, filtered.length - 1)));
  }, [filtered.length]);

  // Scroll active item into view.
  useEffect(() => {
    if (!open) return;
    const el = listRef.current?.children[activeIndex] as HTMLElement | undefined;
    el?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, open]);

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((i) => Math.min(i + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      filtered[activeIndex]?.run();
    } else if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
    }
  }

  if (!open) return null;

  const GROUP_LABELS: Record<Command["group"], string> = {
    navigate: "Navigate",
    create: "Create",
    item: "Jump to item",
    action: "Actions",
  };

  // Render with group headers. Track running index to map back to
  // filtered[] for activeIndex.
  let runningIndex = 0;
  const groups: { label: string; cmds: Command[] }[] = [];
  let current: { label: string; cmds: Command[] } | null = null;
  for (const cmd of filtered) {
    if (!current || current.label !== GROUP_LABELS[cmd.group]) {
      current = { label: GROUP_LABELS[cmd.group], cmds: [] };
      groups.push(current);
    }
    current.cmds.push(cmd);
  }

  return (
    <div className="palette-backdrop" onClick={() => setOpen(false)}>
      <div
        className="palette-card"
        role="dialog"
        aria-label="Command palette"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          ref={inputRef}
          className="palette-input"
          placeholder="Type a command or search items… (Esc to close)"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          aria-label="Command search"
        />
        <ul ref={listRef} className="palette-list" role="listbox">
          {groups.map((g) => (
            <li key={g.label} className="palette-group">
              <div className="palette-group-label">{g.label}</div>
              <ul>
                {g.cmds.map((cmd) => {
                  const idx = runningIndex++;
                  return (
                    <li
                      key={cmd.id}
                      role="option"
                      aria-selected={idx === activeIndex}
                      className={"palette-option" + (idx === activeIndex ? " is-active" : "")}
                      onMouseEnter={() => setActiveIndex(idx)}
                      onClick={() => cmd.run()}
                    >
                      <span className="palette-option-label">{cmd.label}</span>
                      {cmd.hint ? <span className="palette-option-hint">{cmd.hint}</span> : null}
                    </li>
                  );
                })}
              </ul>
            </li>
          ))}
          {filtered.length === 0 ? (
            <li className="palette-empty">No matches.</li>
          ) : null}
        </ul>
        <div className="palette-footer">
          <kbd>↑↓</kbd> navigate · <kbd>Enter</kbd> run · <kbd>Esc</kbd> close · <kbd>Ctrl/Cmd+K</kbd> toggle
        </div>
      </div>
    </div>
  );
}
