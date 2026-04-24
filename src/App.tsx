import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { shallow } from "zustand/shallow";
import { A_CAP, B_CAP, BootstrapResult, Item, Tier } from "./domain";
import { useStore } from "./store";
import { Strip } from "./components/Strip";
import { TauriEventBridge } from "./components/TauriEventBridge";

type View = "board" | "calendar" | "timetravel";

const VIEW_LABELS: Record<View, string> = {
  board: "Board",
  calendar: "Calendar",
  timetravel: "Time-travel",
};

export default function App() {
  const [view, setView] = useState<View>("board");
  const bootstrap = useStore((s) => s.bootstrap);
  const onItemCreated = useStore((s) => s.onItemCreated);

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

  // Dev-only handler for the "+" button on each bay. I-05 replaces this
  // with a proper capacity-aware inline-input flow.
  async function handleDevAdd(tier: Tier) {
    try {
      const raw = await invoke<unknown>("create_item", {
        tier,
        content: "new item",
      });
      const parsed = Item.parse(raw);
      onItemCreated(parsed);
    } catch (err) {
      console.error("create_item failed:", err);
    }
  }

  return (
    <div className="app">
      <TauriEventBridge />
      <TopBar view={view} onView={handleView} />
      <main className="main">
        {view === "board" ? <Board onDevAdd={handleDevAdd} /> : null}
      </main>
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

function Board({ onDevAdd }: { onDevAdd: (tier: Tier) => void }) {
  return (
    <div className="board">
      <BayColumn tier="inbox" label="Inbox" onDevAdd={onDevAdd} />
      <BayColumn tier="A" label="A" cap={A_CAP} onDevAdd={onDevAdd} />
      <BayColumn tier="B" label="B" cap={B_CAP} onDevAdd={onDevAdd} />
      <BayColumn tier="C" label="C" onDevAdd={onDevAdd} />
    </div>
  );
}

function BayColumn({
  tier,
  label,
  cap,
  onDevAdd,
}: {
  tier: Tier;
  label: string;
  cap?: number;
  onDevAdd: (tier: Tier) => void;
}) {
  // `itemsByTier[tier]` is stored as derived state; `shallow` makes this
  // subscription re-render only when the id-array contents change.
  const itemIds = useStore((s) => s.itemsByTier[tier], shallow);
  const count = itemIds.length;
  const counter = cap !== undefined ? `${count} / ${cap}` : `${count} items`;

  return (
    <section className="bay" data-tier={tier} aria-label={`${label} bay`}>
      <header className="bay-header">
        <h2 className="bay-title">{label}</h2>
        <span className="bay-counter" aria-label={`${counter} in ${label}`}>
          {counter}
        </span>
        {/* Dev-only scaffolding; I-05 replaces this with real capacity-aware UI. */}
        <button
          type="button"
          className="bay-dev-add"
          onClick={() => onDevAdd(tier)}
          aria-label={`Dev add item to ${label}`}
        >
          +
        </button>
      </header>
      <div className="bay-body">
        {itemIds.map((id) => (
          <Strip key={id} itemId={id} />
        ))}
      </div>
    </section>
  );
}
