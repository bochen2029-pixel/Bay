// Block-with-reason modal. Reason is required: Confirm is disabled
// until the textarea has non-whitespace content. No default reason;
// the user must describe the blocker explicitly (SPEC §3.1 guard).

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { Item } from "../domain";
import { useStore } from "../store";

export function BlockModal() {
  const pending = useStore((s) => s.blockPending);
  const close = useStore((s) => s.closeBlockModal);
  const onItemUpdated = useStore((s) => s.onItemUpdated);
  const itemContent = useStore((s) =>
    pending ? (s.items[pending.itemId]?.content ?? "") : "",
  );

  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = dialogRef.current;
    if (!el) return;
    if (pending && !el.open) {
      setReason("");
      setError(null);
      setBusy(false);
      el.showModal();
      queueMicrotask(() => textareaRef.current?.focus());
    } else if (!pending && el.open) {
      el.close();
    }
  }, [pending]);

  const trimmedReason = reason.trim();
  const canSubmit = !busy && trimmedReason.length > 0 && !!pending;

  async function confirm() {
    if (!canSubmit || !pending) return;
    setBusy(true);
    setError(null);
    try {
      const raw = await invoke<unknown>("set_item_state", {
        id: pending.itemId,
        state: "blocked",
        blockedReason: trimmedReason,
      });
      onItemUpdated(Item.parse(raw));
      close();
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      setError(msg);
      setBusy(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void confirm();
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
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
      <div className="modal-header">Mark blocked</div>
      <div className="modal-body">
        <div className="modal-item-preview">"{itemContent}"</div>
        <label className="modal-field">
          <span>What's blocking this item? (required)</span>
          <textarea
            ref={textareaRef}
            className="modal-textarea"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            onKeyDown={handleKeyDown}
            rows={3}
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
          disabled={!canSubmit}
        >
          Block
        </button>
      </div>
    </dialog>
  );
}
