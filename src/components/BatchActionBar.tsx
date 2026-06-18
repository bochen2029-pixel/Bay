// I-19 batch operations. A bar that appears above the board whenever one
// or more items are multi-selected (via the per-strip checkboxes, with
// shift-click range selection). Each action routes through an atomic
// backend command (batch_set_state / batch_delete) — no new event types:
// each item in the batch emits its own ITEM_STATE_CHANGED / ITEM_DELETED
// sharing one ts, so the whole batch is one undoable action (Ctrl+Z).

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { useStore } from "../store";

export function BatchActionBar() {
  const selectedIds = useStore((s) => s.selectedIds);
  const clearSelected = useStore((s) => s.clearSelected);
  const clearDeletedPending = useStore((s) => s.clearDeletedPending);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const count = selectedIds.size;

  // Reset transient state whenever the selection size changes.
  useEffect(() => {
    setError(null);
    setConfirmingDelete(false);
  }, [count]);

  // Escape clears the selection (unless the user is typing).
  useEffect(() => {
    if (count === 0) return;
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing =
        !!t &&
        (t.tagName === "INPUT" ||
          t.tagName === "TEXTAREA" ||
          t.isContentEditable);
      if (e.key === "Escape" && !typing) clearSelected();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [count, clearSelected]);

  if (count === 0) return null;

  const ids = Array.from(selectedIds);

  const setState = async (state: "done" | "active") => {
    setBusy(true);
    setError(null);
    try {
      await invoke("batch_set_state", { ids, state, blockedReason: null });
      clearSelected();
    } catch (e) {
      const msg = String(e);
      setError(
        msg.includes("CAP_EXCEEDED")
          ? "Can't activate — would exceed an A/B cap. Demote or free a slot first."
          : `Batch action failed: ${msg}`,
      );
    } finally {
      setBusy(false);
    }
  };

  const doDelete = async () => {
    setBusy(true);
    setError(null);
    try {
      await invoke("batch_delete", { ids });
      clearSelected();
      // The per-item delete toast would only restore one of the batch;
      // the whole batch is undoable via Ctrl+Z, so suppress that toast.
      setTimeout(() => clearDeletedPending(), 60);
    } catch (e) {
      setError(`Batch delete failed: ${String(e)}`);
    } finally {
      setBusy(false);
      setConfirmingDelete(false);
    }
  };

  return (
    <div className="batch-bar" role="toolbar" aria-label="Batch actions">
      <span className="batch-bar-count">{count} selected</span>
      <div className="batch-bar-actions">
        <button
          className="batch-bar-button"
          disabled={busy}
          onClick={() => setState("done")}
        >
          Mark done
        </button>
        <button
          className="batch-bar-button"
          disabled={busy}
          onClick={() => setState("active")}
        >
          Mark active
        </button>
        {confirmingDelete ? (
          <button
            className="batch-bar-button is-destructive"
            disabled={busy}
            onClick={doDelete}
          >
            Confirm delete {count}?
          </button>
        ) : (
          <button
            className="batch-bar-button is-destructive"
            disabled={busy}
            onClick={() => setConfirmingDelete(true)}
          >
            Delete
          </button>
        )}
      </div>
      <button
        className="batch-bar-clear"
        disabled={busy}
        onClick={() => clearSelected()}
      >
        Clear
      </button>
      {error ? <span className="batch-bar-error">{error}</span> : null}
    </div>
  );
}
