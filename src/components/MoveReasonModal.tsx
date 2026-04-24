// Cross-tier move confirmation. Opened by Board's onDragEnd when the
// user drags an item to a different tier that isn't at cap (or when
// the item is blocked/done — those never trigger SwapModal per SPEC
// §3.2 guard).
//
// Reason is optional; Confirm commits the move, Cancel closes without
// emitting. @dnd-kit handles the visual revert when no mutation fires.

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { Item } from "../domain";
import { useStore } from "../store";

export function MoveReasonModal() {
  const pending = useStore((s) => s.moveReasonPending);
  const close = useStore((s) => s.closeMoveReason);
  const onItemUpdated = useStore((s) => s.onItemUpdated);
  const itemContent = useStore((s) =>
    pending ? (s.items[pending.activeId]?.content ?? "") : "",
  );
  const fromTier = useStore((s) =>
    pending ? (s.items[pending.activeId]?.tier ?? null) : null,
  );

  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const reasonRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = dialogRef.current;
    if (!el) return;
    if (pending && !el.open) {
      setReason("");
      setError(null);
      setBusy(false);
      el.showModal();
      queueMicrotask(() => reasonRef.current?.focus());
    } else if (!pending && el.open) {
      el.close();
    }
  }, [pending]);

  async function confirm() {
    if (!pending || busy) return;
    setBusy(true);
    setError(null);
    try {
      const raw = await invoke<unknown>("move_item", {
        id: pending.activeId,
        toTier: pending.toTier,
        toRank: pending.toRank,
        reason: reason.trim() || null,
      });
      const item = Item.parse(raw);
      onItemUpdated(item);
      close();
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      if (msg === "NO_OP") {
        close();
        return;
      }
      setError(msg);
      setBusy(false);
    }
  }

  return (
    <dialog
      ref={dialogRef}
      className="modal-card"
      onCancel={(e) => {
        e.preventDefault();
        close();
      }}
      onClose={close}
    >
      <div className="modal-header">
        Moving {fromTier ?? "?"} → {pending?.toTier ?? "?"}
      </div>
      <div className="modal-body">
        <div className="modal-item-preview">"{itemContent}"</div>
        <label className="modal-field">
          <span>Reason (optional)</span>
          <textarea
            ref={reasonRef}
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
          disabled={busy}
        >
          Confirm
        </button>
      </div>
    </dialog>
  );
}
