import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type View = "board" | "calendar" | "timetravel";

type Tier = "inbox" | "A" | "B" | "C";

interface BootstrapResult {
  items: unknown[];
  settings: unknown;
}

const VIEW_LABELS: Record<View, string> = {
  board: "Board",
  calendar: "Calendar",
  timetravel: "Time-travel",
};

export default function App() {
  const [view, setView] = useState<View>("board");

  useEffect(() => {
    invoke<BootstrapResult>("bootstrap")
      .then((b) => {
        console.log("bootstrap:", b);
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

  return (
    <div className="app">
      <TopBar view={view} onView={handleView} />
      <main className="main">{view === "board" ? <Board /> : null}</main>
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

function Board() {
  return (
    <div className="board">
      <BayColumn tier="inbox" label="Inbox" count={0} />
      <BayColumn tier="A" label="A" count={0} cap={5} />
      <BayColumn tier="B" label="B" count={0} cap={12} />
      <BayColumn tier="C" label="C" count={0} />
    </div>
  );
}

function BayColumn({
  tier,
  label,
  count,
  cap,
}: {
  tier: Tier;
  label: string;
  count: number;
  cap?: number;
}) {
  const counter = cap !== undefined ? `${count} / ${cap}` : `${count} items`;
  return (
    <section className="bay" data-tier={tier} aria-label={`${label} bay`}>
      <header className="bay-header">
        <h2 className="bay-title">{label}</h2>
        <span className="bay-counter" aria-label={`${counter} in ${label}`}>
          {counter}
        </span>
      </header>
      <div className="bay-body" />
    </section>
  );
}
