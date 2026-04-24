import { useEffect, useRef, useState } from "react";
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
import { useStore } from "./store";
import { Strip } from "./components/Strip";
import { TauriEventBridge } from "./components/TauriEventBridge";
import { QuickCaptureModal } from "./components/QuickCaptureModal";

type View = "board" | "calendar" | "timetravel";

const VIEW_LABELS: Record<View, string> = {
  board: "Board",
  calendar: "Calendar",
  timetravel: "Time-travel",
};

const TIER_CAP: Record<Tier, number | undefined> = {
  inbox: undefined,
  A: A_CAP,
  B: B_CAP,
  C: undefined,
};

const CAP_FULL_TOOLTIP = "Full — drag an item in (swap will be offered)";

export default function App() {
  const [view, setView] = useState<View>("board");
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
    if (v === "board") {
      setView("board");
      return;
    }
    console.log(`${VIEW_LABELS[v]}: not yet implemented`);
  }

  return (
    <div className="app">
      <TauriEventBridge />
      <BackendWarningBanner />
      <TopBar view={view} onView={handleView} />
      <main className="main">{view === "board" ? <Board /> : null}</main>
      <QuickCaptureModal />
    </div>
  );
}

function TopBar({ view, onView }: { view: View; onView: (v: View) => void }) {
  return (
    <header className="topbar">
      <div className="topbar-brand">Bay</div>
      <nav className="view-switcher" aria-label="View switcher">
        {(Object.keys(VIEW_LABELS) as View[]).map((v) => (
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

    // Cross-tier rejection (I-06 scope). SwapModal + cross-tier
    // ITEM_MOVED land in I-07. @dnd-kit's default visual revert handles
    // the user-visible snap-back.
    if (activeItem.tier !== overItem.tier) return;

    const tier = activeItem.tier;
    const ids = state.itemsByTier[tier];
    const oldIndex = ids.indexOf(String(active.id));
    const newIndex = ids.indexOf(String(over.id));
    if (oldIndex < 0 || newIndex < 0 || oldIndex === newIndex) return;

    // Apply the reorder locally so we can look up the moved item's
    // neighbors at its new position — the three drop cases
    // (start / middle / end) fall out of prev/next null-ness below.
    const reordered = arrayMove(ids, oldIndex, newIndex);
    const movedIdx = reordered.indexOf(String(active.id));
    const prevId = movedIdx > 0 ? reordered[movedIdx - 1] : null;
    const nextId =
      movedIdx < reordered.length - 1 ? reordered[movedIdx + 1] : null;
    const prevRank = prevId ? state.items[prevId].rank : null;
    const nextRank = nextId ? state.items[nextId].rank : null;

    const newRank = rankBetween(prevRank, nextRank);

    // Frontend no-op pre-check. Backend also rejects NO_OP defensively;
    // catching it here avoids an unnecessary round-trip + invoke error
    // surface.
    if (newRank === activeItem.rank) return;

    invoke<unknown>("move_item", {
      id: String(active.id),
      toTier: tier,
      toRank: newRank,
    })
      .then((raw) => {
        // Best-effort immediate store update; the backend's
        // item_updated event also calls onItemUpdated, and the
        // idempotency check on `updated_at` makes double-delivery a
        // no-op.
        const item = Item.parse(raw);
        onItemUpdated(item);
      })
      .catch((err) => {
        const msg = typeof err === "string" ? err : String(err);
        if (msg === "NO_OP") return; // race against concurrent updates
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
  const cap = TIER_CAP[tier];
  // For I-06 every item is active (state transitions land in I-08), so
  // the full count equals the active count. Diverges in I-08.
  const activeCount = itemIds.length;
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
      <SortableContext items={itemIds} strategy={verticalListSortingStrategy}>
        <div className="bay-body">
          {itemIds.map((id) => (
            <Strip key={id} itemId={id} />
          ))}
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
