import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { A_CAP, B_CAP, BootstrapResult, Item, Tier } from "./domain";

type View = "board" | "calendar" | "timetravel";

const VIEW_LABELS: Record<View, string> = {
  board: "Board",
  calendar: "Calendar",
  timetravel: "Time-travel",
};

export default function App() {
  const [view, setView] = useState<View>("board");

  useEffect(() => {
    invoke<unknown>("bootstrap")
      .then((raw) => {
        // Zod-parse the response to catch any frontend/backend type drift
        // at the single IPC boundary rather than silently downstream.
        const parsed = BootstrapResult.parse(raw);
        console.log("bootstrap:", parsed);
      })
      .catch((err) => {
        console.error("bootstrap failed:", err);
      });
  }, []);

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
      const item = await invoke<unknown>("create_item", {
        tier,
        content: "new item",
      });
      const parsed = Item.parse(item);
      console.log("create_item:", parsed);
    } catch (err) {
      console.error("create_item failed:", err);
    }
  }

  return (
    <div className="app">
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
      <BayColumn tier="inbox" label="Inbox" count={0} onDevAdd={onDevAdd} />
      <BayColumn tier="A" label="A" count={0} cap={A_CAP} onDevAdd={onDevAdd} />
      <BayColumn tier="B" label="B" count={0} cap={B_CAP} onDevAdd={onDevAdd} />
      <BayColumn tier="C" label="C" count={0} onDevAdd={onDevAdd} />
    </div>
  );
}

function BayColumn({
  tier,
  label,
  count,
  cap,
  onDevAdd,
}: {
  tier: Tier;
  label: string;
  count: number;
  cap?: number;
  onDevAdd: (tier: Tier) => void;
}) {
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
      <div className="bay-body" />
    </section>
  );
}
