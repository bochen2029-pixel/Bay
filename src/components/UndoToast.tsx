// Undo-delete toast. Appears for 10 seconds after a successful
// delete_item; clicking Undo calls restore_item, which the backend
// handles by emitting ITEM_RESTORED and (via the thin wrapper)
// re-emits `item_created` so the frontend's idempotent onItemCreated
// re-inserts the item. SPEC §9 I-08 demo + §10.3 resolution.

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { Item } from "../domain";
import { useStore } from "../store";

const TOAST_WINDOW_MS = 10_000;

export function UndoToast() {
  const pending = useStore((s) => s.deletedPending);
  const clear = useStore((s) => s.clearDeletedPending);
  const onItemCreated = useStore((s) => s.onItemCreated);
  const [busy, setBusy] = useState(false);

  // Auto-dismiss after the 10s window. Re-opens reset the timer
  // because `pending` identity changes on each delete.
  useEffect(() => {
    if (!pending) return;
    const elapsed = Date.now() - pending.deletedAt;
    const remaining = Math.max(0, TOAST_WINDOW_MS - elapsed);
    const t = setTimeout(() => clear(), remaining);
    return () => clearTimeout(t);
  }, [pending, clear]);

  if (!pending) return null;

  async function handleUndo() {
    if (!pending || busy) return;
    setBusy(true);
    try {
      const raw = await invoke<unknown>("restore_item", {
        id: pending.snapshot.id,
      });
      // Backend also emits `item_created` as the wire signal for
      // restore; idempotent onItemCreated means this path and the
      // event path each leave the store in the right state regardless
      // of order.
      onItemCreated(Item.parse(raw));
      clear();
    } catch (err) {
      console.error("undo delete failed:", err);
      setBusy(false);
    }
  }

  return (
    <div className="undo-toast" role="status" aria-live="polite">
      <span>Deleted "{pending.snapshot.content.slice(0, 40)}".</span>
      <button
        type="button"
        className="undo-toast-action"
        onClick={handleUndo}
        disabled={busy}
      >
        Undo
      </button>
    </div>
  );
}
