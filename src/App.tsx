import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  DndContext,
  DragEndEvent,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { shallow } from "zustand/shallow";

import { A_CAP, B_CAP, BootstrapResult, Item, Tier } from "./domain";
import { rankBetween } from "./rank";
import { needsSwap } from "./swap";
import { useStore } from "./store";
import { Strip } from "./components/Strip";
import { TauriEventBridge } from "./components/TauriEventBridge";
import { QuickCaptureModal } from "./components/QuickCaptureModal";
import { MoveReasonModal } from "./components/MoveReasonModal";
import { SwapModal } from "./components/SwapModal";
import { BlockModal } from "./components/BlockModal";
import { UndoToast } from "./components/UndoToast";
import { LanCaptureToast } from "./components/LanCaptureToast";
import { CalendarView } from "./components/CalendarView";
import { InspectorPanel } from "./components/InspectorPanel";
import { TimeTravelView } from "./components/TimeTravelView";
import { SettingsView } from "./components/SettingsView";
import { AnalyzePanel } from "./components/AnalyzePanel";

type View = "board" | "calendar" | "timetravel" | "settings";

const VIEW_LABELS: Record<View, string> = {
  board: "Board",
  calendar: "Calendar",
  timetravel: "Time-travel",
  settings: "Settings",
};

// The top-bar navigation switcher excludes Settings; Settings is
// reached via the gear icon instead.
const SWITCHER_VIEWS: View[] = ["board", "calendar", "timetravel"];

const TIER_CAP: Record<Tier, number | undefined> = {
  inbox: undefined,
  A: A_CAP,
  B: B_CAP,
  C: undefined,
};

const CAP_FULL_TOOLTIP = "Full — drag an item in (swap will be offered)";

export default function App() {
  const [view, setView] = useState<View>("board");
  const [analyzeOpen, setAnalyzeOpen] = useState(false);
  const bootstrap = useStore((s) => s.bootstrap);

  useEffect(() => {
    invoke<unknown>("bootstrap")
      .then((raw) => {
        const parsed = BootstrapResult.parse(raw);
        bootstrap(parsed.items, parsed.settings);
      })
      .catch((err) => {
        console.error("bootstrap failed:", err);
      });
  }, [bootstrap]);

  function handleView(v: View) {
    setView(v);
  }

  return (
    <div className="app">
      <TauriEventBridge />
      <BackendWarningBanner />
      <TopBar
        view={view}
        onView={handleView}
        onAnalyze={() => setAnalyzeOpen(true)}
      />
      <main className="main">
        {view === "board" ? <Board /> : null}
        {view === "calendar" ? (
          <CalendarView onFocusItem={() => setView("board")} />
        ) : null}
        {view === "timetravel" ? (
          <TimeTravelView onExit={() => setView("board")} />
        ) : null}
        {view === "settings" ? <SettingsView /> : null}
      </main>
      <InspectorPanel />
      <AnalyzePanel open={analyzeOpen} onClose={() => setAnalyzeOpen(false)} />
      <QuickCaptureModal />
      <MoveReasonModal />
      <SwapModal />
      <BlockModal />
      <UndoToast />
      <LanCaptureToast />
    </div>
  );
}

function TopBar({
  view,
  onView,
  onAnalyze,
}: {
  view: View;
  onView: (v: View) => void;
  onAnalyze: () => void;
}) {
  return (
    <header className="topbar">
      <div className="topbar-brand">Bay</div>
      <nav className="view-switcher" aria-label="View switcher">
        {SWITCHER_VIEWS.map((v) => (
          <button
            key={v}
            type="button"
            className={"view-button" + (view === v ? " is-active" : "")}
            aria-pressed={view === v}
            onClick={() => onView(v)}
          >
            {VIEW_LABELS[v]}
          </button>
        ))}
        <button type="button" className="view-button" onClick={onAnalyze}>
          Analyze
        </button>
        <button
          type="button"
          className={
            "view-button view-settings" + (view === "settings" ? " is-active" : "")
          }
          aria-pressed={view === "settings"}
          onClick={() => onView("settings")}
          aria-label="Settings"
          title="Settings"
        >
          ⚙
        </button>
      </nav>
    </header>
  );
}

function BackendWarningBanner() {
  const warning = useStore((s) => s.backendWarning);
  const setWarning = useStore((s) => s.setBackendWarning);
  if (!warning) return null;
  return (
    <div className="warning-banner" role="alert">
      <span>{warning.message}</span>
      <button
        type="button"
        className="warning-dismiss"
        onClick={() => setWarning(null)}
        aria-label="Dismiss warning"
      >
        ×
      </button>
    </div>
  );
}

function Board() {
  // Sensors: PointerSensor covers mouse + touch with a small activation
  // distance so clicks on the drag handle don't start spurious drags.
  // KeyboardSensor pairs with @dnd-kit's sortable-coordinate adapter
  // for basic accessibility.
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const onItemUpdated = useStore((s) => s.onItemUpdated);
  const openMoveReason = useStore((s) => s.openMoveReason);
  const openSwap = useStore((s) => s.openSwap);

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over) return; // dropped outside any droppable
    if (active.id === over.id) return; // dropped on self

    // Live store snapshot at drop time — do NOT use drag-start snapshot.
    // A concurrent item_created (from LAN capture or another hotkey
    // firing) during this drag would have shifted neighbor ranks.
    const state = useStore.getState();
    const activeItem = state.items[String(active.id)];
    const overItem = state.items[String(over.id)];
    if (!activeItem || !overItem) return;

    // ── cross-tier ────────────────────────────────────────────────
    if (activeItem.tier !== overItem.tier) {
      const targetTier = overItem.tier;
      const targetIds = state.itemsByTier[targetTier];
      const overIdx = targetIds.indexOf(String(over.id));
      if (overIdx < 0) return;

      // Drop-above semantics: active lands at overIdx in target,
      // pushing over and everything below down one slot. Rank
      // neighbors: targetIds[overIdx - 1] (or none) above,
      // overItem below.
      const prevId = overIdx > 0 ? targetIds[overIdx - 1] : null;
      const prevRank = prevId ? state.items[prevId].rank : null;
      const newRank = rankBetween(prevRank, overItem.rank);

      // Active count excludes blocked + done per CLAUDE.md §Design
      // philosophy #1. This must be computed from store state, not
      // proxied by itemsByTier.length, because blocked/done items
      // still occupy the id list.
      const targetActiveCount = targetIds.reduce((n, id) => {
        const it = state.items[id];
        return n + (it && it.state === "active" ? 1 : 0);
      }, 0);

      if (needsSwap(activeItem, targetTier, targetActiveCount)) {
        openSwap({
          kind: "swap",
          enteringId: String(active.id),
          enteringTier: targetTier,
          enteringRank: newRank,
        });
      } else {
        openMoveReason({
          kind: "reason",
          activeId: String(active.id),
          toTier: targetTier,
          toRank: newRank,
        });
      }
      return;
    }

    // ── intra-tier reorder (I-06 behavior) ─────────────────────────
    const tier = activeItem.tier;
    const ids = state.itemsByTier[tier];
    const oldIndex = ids.indexOf(String(active.id));
    const newIndex = ids.indexOf(String(over.id));
    if (oldIndex < 0 || newIndex < 0 || oldIndex === newIndex) return;

    const reordered = arrayMove(ids, oldIndex, newIndex);
    const movedIdx = reordered.indexOf(String(active.id));
    const prevId = movedIdx > 0 ? reordered[movedIdx - 1] : null;
    const nextId =
      movedIdx < reordered.length - 1 ? reordered[movedIdx + 1] : null;
    const prevRank = prevId ? state.items[prevId].rank : null;
    const nextRank = nextId ? state.items[nextId].rank : null;

    const newRank = rankBetween(prevRank, nextRank);
    if (newRank === activeItem.rank) return;

    invoke<unknown>("move_item", {
      id: String(active.id),
      toTier: tier,
      toRank: newRank,
    })
      .then((raw) => {
        const item = Item.parse(raw);
        onItemUpdated(item);
      })
      .catch((err) => {
        const msg = typeof err === "string" ? err : String(err);
        if (msg === "NO_OP") return;
        console.error("move_item failed:", err);
      });
  }

  return (
    <DndContext sensors={sensors} onDragEnd={handleDragEnd}>
      <div className="board">
        <BayColumn tier="inbox" label="Inbox" />
        <BayColumn tier="A" label="A" />
        <BayColumn tier="B" label="B" />
        <BayColumn tier="C" label="C" />
      </div>
    </DndContext>
  );
}

function BayColumn({ tier, label }: { tier: Tier; label: string }) {
  // `itemsByTier[tier]` is stored as derived state; `shallow` makes this
  // subscription re-render only when the id-array contents change.
  const itemIds = useStore((s) => s.itemsByTier[tier], shallow);
  const items = useStore((s) => s.items);
  const sessionDoneIds = useStore((s) => s.sessionDoneIds);
  const revealed = useStore((s) => s.doneRevealed[tier]);
  const toggleDoneRevealed = useStore((s) => s.toggleDoneRevealed);
  const cap = TIER_CAP[tier];

  // Visible ids: active + blocked always; done only if (a) marked done
  // this session, or (b) the per-bay reveal is on. Hidden-done count
  // drives the "Show N earlier done items" link.
  const { visibleIds, activeCount, hiddenDoneCount } = useMemo(() => {
    let active = 0;
    let hiddenDone = 0;
    const visible: string[] = [];
    for (const id of itemIds) {
      const it = items[id];
      if (!it) continue;
      if (it.state === "active") {
        active++;
        visible.push(id);
      } else if (it.state === "blocked") {
        visible.push(id);
      } else {
        // done
        if (sessionDoneIds.has(id) || revealed) visible.push(id);
        else hiddenDone++;
      }
    }
    return { visibleIds: visible, activeCount: active, hiddenDoneCount: hiddenDone };
  }, [itemIds, items, sessionDoneIds, revealed]);

  const counter =
    cap !== undefined ? `${activeCount} / ${cap}` : `${activeCount} items`;
  const atCap = cap !== undefined && activeCount >= cap;

  const [adding, setAdding] = useState(false);

  return (
    <section className="bay" data-tier={tier} aria-label={`${label} bay`}>
      <header className="bay-header">
        <h2 className="bay-title">{label}</h2>
        <span className="bay-counter" aria-label={`${counter} in ${label}`}>
          {counter}
        </span>
        <button
          type="button"
          className="bay-add-button"
          disabled={atCap || adding}
          title={atCap ? CAP_FULL_TOOLTIP : undefined}
          onClick={() => setAdding(true)}
          aria-label={`Add item to ${label}`}
        >
          + Add
        </button>
      </header>
      {adding ? (
        <BayAddInput tier={tier} onClose={() => setAdding(false)} />
      ) : null}
      <SortableContext items={visibleIds} strategy={verticalListSortingStrategy}>
        <div className="bay-body">
          {visibleIds.map((id) => (
            <Strip key={id} itemId={id} />
          ))}
          {hiddenDoneCount > 0 ? (
            <button
              type="button"
              className="bay-show-done"
              onClick={() => toggleDoneRevealed(tier)}
            >
              Show {hiddenDoneCount} earlier done item
              {hiddenDoneCount === 1 ? "" : "s"}
            </button>
          ) : null}
          {revealed && hiddenDoneCount === 0 ? null : null}
        </div>
      </SortableContext>
    </section>
  );
}

function BayAddInput({
  tier,
  onClose,
}: {
  tier: Tier;
  onClose: () => void;
}) {
  const onItemCreated = useStore((s) => s.onItemCreated);
  const [content, setContent] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  async function commit() {
    const text = content.trim();
    if (!text) {
      onClose();
      return;
    }
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const raw = await invoke<unknown>("create_item", { tier, content: text });
      const item = Item.parse(raw);
      onItemCreated(item);
      onClose();
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      setError(msg);
      setBusy(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void commit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  return (
    <div className="bay-add-input">
      <textarea
        ref={textareaRef}
        value={content}
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={handleKeyDown}
        rows={1}
        placeholder="New item…"
        disabled={busy}
      />
      {error ? <div className="bay-add-error">{error}</div> : null}
    </div>
  );
}
