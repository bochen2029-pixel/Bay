// v0.3 execution core: the in-app "Now" HUD. Renders only while a
// session is open — item content + first step in plain sight, elapsed
// time, and the three one-tap endings (Done / Pause / Interrupt-with-
// reason). Presence without interruption: Bay never pings; the bar is
// simply THERE while you work (VISION §3.2, law 8).

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { EndSessionResult, INTERRUPT_REASONS, SessionOutcome } from "../domain";
import { useStore } from "../store";

export function FocusBar() {
  const session = useStore((s) => s.openSession);
  const item = useStore((s) =>
    s.openSession ? (s.items[s.openSession.item_id] ?? null) : null,
  );
  const setOpenSession = useStore((s) => s.setOpenSession);
  const onItemUpdated = useStore((s) => s.onItemUpdated);
  const onItemCreated = useStore((s) => s.onItemCreated);
  const [reasonOpen, setReasonOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const reasonRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!reasonOpen) return;
    function handleOutside(e: MouseEvent) {
      if (reasonRef.current && !reasonRef.current.contains(e.target as Node)) {
        setReasonOpen(false);
      }
    }
    document.addEventListener("mousedown", handleOutside);
    return () => document.removeEventListener("mousedown", handleOutside);
  }, [reasonOpen]);

  if (!session) return null;

  async function end(outcome: SessionOutcome, reason?: string) {
    if (busy) return;
    setBusy(true);
    try {
      const raw = await invoke<unknown>("end_session", {
        outcome,
        reason: reason ?? null,
        note: null,
      });
      const result = EndSessionResult.parse(raw);
      setOpenSession(null);
      onItemUpdated(result.item);
      for (const child of result.spawned) onItemCreated(child);
    } catch (err) {
      console.error("end_session failed:", err);
    } finally {
      setBusy(false);
      setReasonOpen(false);
    }
  }

  return (
    <div className="focus-bar" role="status" aria-label="Focus session">
      <span className="focus-live" aria-hidden="true">
        ▶
      </span>
      <Elapsed startedAt={session.started_at} />
      <span className="focus-content" title={item?.content}>
        {item ? item.content : "(item unavailable)"}
      </span>
      {item?.first_step ? (
        <span className="focus-first-step" title="First step">
          → {item.first_step}
        </span>
      ) : null}
      <span className="focus-actions">
        <button
          type="button"
          disabled={busy}
          onClick={() => void end("done")}
          title="Finish the item and end the session"
        >
          Done
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => void end("progress")}
          title="Honest pause — work advanced"
        >
          Pause
        </button>
        <span className="focus-interrupt-wrap" ref={reasonRef}>
          <button
            type="button"
            disabled={busy}
            aria-expanded={reasonOpen}
            onClick={() => setReasonOpen((v) => !v)}
            title="Focus broke — record why"
          >
            Interrupt…
          </button>
          {reasonOpen ? (
            <span className="focus-interrupt-menu" role="menu">
              {INTERRUPT_REASONS.map((r) => (
                <button
                  key={r}
                  type="button"
                  role="menuitem"
                  onClick={() => void end("interrupted", r)}
                >
                  {r.replace("_", " ")}
                </button>
              ))}
            </span>
          ) : null}
        </span>
      </span>
    </div>
  );
}

function Elapsed({ startedAt }: { startedAt: number }) {
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => tick((n) => n + 1), 1_000);
    return () => clearInterval(t);
  }, []);
  const total = Math.max(0, Math.floor((Date.now() - startedAt) / 1_000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const text =
    h > 0
      ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
      : `${m}:${String(s).padStart(2, "0")}`;
  return <span className="focus-elapsed">{text}</span>;
}
