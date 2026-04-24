// Cross-tier swap confirmation. Opened when dropping an active item
// into an A or B that's already at cap. User picks (a) which active
// item to demote and (b) where to demote it (B or C for A; C only for
// B to avoid cascade). Per the I-07 retrospective: all radio buttons
// are UNCHECKED by default, and the Confirm button is disabled until
// an explicit selection exists — forcing the user to pick deliberately
// rather than click-through to a default "lowest-rank → B" demotion.

import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";

import { Item, Tier } from "../domain";
import { useStore } from "../store";

// Response shape from swap_move.
const SwapResultSchema = z.object({
  leaving: Item,
  entering: Item,
});

type Choice = { leavingId: string; leavingDest: "B" | "C" };

function destOptionsFor(enteringTier: Tier): Array<"B" | "C"> {
  // For a swap into A, the demoted item can go to B or C. For a swap
  // into B (v2 case; rare but supported), the only non-cascading dest
  // is C. Cascading (B-swap demoting to A) is not v1.
  return enteringTier === "A" ? ["B", "C"] : ["C"];
}

export function SwapModal() {
  const pending = useStore((s) => s.swapPending);
  const close = useStore((s) => s.closeSwap);
  const onItemUpdated = useStore((s) => s.onItemUpdated);

  const enteringContent = useStore((s) =>
    pending ? (s.items[pending.enteringId]?.content ?? "") : "",
  );
  // Active items currently occupying the target tier — each is a
  // candidate for demotion. We pull all of itemsByTier here rather than
  // filtering at render to keep the SortableContext-friendly subscription
  // shallow.
  const targetIds = useStore((s) =>
    pending ? s.itemsByTier[pending.enteringTier] : ([] as string[]),
  );
  const items = useStore((s) => s.items);

  const targetActiveItems = useMemo(() => {
    return targetIds
      .map((id) => items[id])
      .filter((it): it is Item => !!it && it.state === "active");
  }, [targetIds, items]);

  const [choice, setChoice] = useState<Choice | null>(null);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const el = dialogRef.current;
    if (!el) return;
    if (pending && !el.open) {
      setChoice(null);
      setReason("");
      setError(null);
      setBusy(false);
      el.showModal();
    } else if (!pending && el.open) {
      el.close();
    }
  }, [pending]);

  async function confirm() {
    if (!pending || !choice || busy) return;
    setBusy(true);
    setError(null);
    try {
      const raw = await invoke<unknown>("swap_move", {
        leavingId: choice.leavingId,
        leavingDest: choice.leavingDest,
        enteringId: pending.enteringId,
        enteringTier: pending.enteringTier,
        enteringRank: pending.enteringRank,
        reason: reason.trim() || null,
      });
      const result = SwapResultSchema.parse(raw);
      onItemUpdated(result.leaving);
      onItemUpdated(result.entering);
      close();
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      setError(msg);
      setBusy(false);
    }
  }

  const destOptions = pending ? destOptionsFor(pending.enteringTier) : [];

  return (
    <dialog
      ref={dialogRef}
      className="modal-card modal-card-wide"
      onCancel={(e) => {
        e.preventDefault();
        close();
      }}
      onClose={close}
    >
      <div className="modal-header">
        {pending?.enteringTier ?? "?"} is full
      </div>
      <div className="modal-body">
        <p className="modal-lead">
          To add "{enteringContent}" to {pending?.enteringTier ?? "?"}, one
          existing item must leave. Pick which one.
        </p>
        <div className="swap-choices" role="radiogroup" aria-label="Swap choice">
          {targetActiveItems.map((it) => (
            <div key={it.id} className="swap-choice-row">
              <div className="swap-choice-label">{it.content}</div>
              <div className="swap-choice-dests">
                {destOptions.map((dest) => {
                  const selected =
                    choice?.leavingId === it.id && choice?.leavingDest === dest;
                  return (
                    <label key={dest} className="swap-choice-dest">
                      <input
                        type="radio"
                        name="swap-choice"
                        checked={selected}
                        onChange={() =>
                          setChoice({ leavingId: it.id, leavingDest: dest })
                        }
                      />
                      → {dest}
                    </label>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
        <label className="modal-field">
          <span>Reason (optional)</span>
          <textarea
            className="modal-textarea"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            rows={2}
            disabled={busy}
          />
        </label>
        {error ? <div className="modal-error">{error}</div> : null}
      </div>
      <div className="modal-actions">
        <button type="button" onClick={close} disabled={busy}>
          Cancel
        </button>
        <button
          type="button"
          className="is-primary"
          onClick={confirm}
          disabled={busy || !choice}
        >
          Swap
        </button>
      </div>
    </dialog>
  );
}
