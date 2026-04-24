// Global hotkey target. Single autofocused textarea; commits to Inbox
// (unbounded, so no cap check needed). Uses the native <dialog> for
// modal semantics + focus trap + a11y without pulling in a library.
//
// Keyboard:
//   Enter        → commit
//   Ctrl+Enter   → commit + open inspector on the new item
//   Shift+Enter  → newline (default textarea behavior)
//   Esc          → cancel, close without commit

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { Item } from "../domain";
import { useStore } from "../store";

export function QuickCaptureModal() {
  const open = useStore((s) => s.quickCaptureOpen);
  const close = useStore((s) => s.closeQuickCapture);
  const onItemCreated = useStore((s) => s.onItemCreated);
  const setSelectedItemId = useStore((s) => s.setSelectedItemId);

  const [content, setContent] = useState("");
  const dialogRef = useRef<HTMLDialogElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Drive the native <dialog> open/close state from the store.
  useEffect(() => {
    const el = dialogRef.current;
    if (!el) return;
    if (open && !el.open) {
      setContent("");
      el.showModal();
      // showModal runs focus logic before React paints; schedule an
      // explicit textarea focus so autoFocus doesn't lose the race.
      queueMicrotask(() => textareaRef.current?.focus());
    } else if (!open && el.open) {
      el.close();
    }
  }, [open]);

  async function commit(openInspector: boolean) {
    const text = content.trim();
    if (!text) return;
    try {
      const raw = await invoke<unknown>("create_item", {
        tier: "inbox",
        content: text,
      });
      const item = Item.parse(raw);
      onItemCreated(item);
      if (openInspector) setSelectedItemId(item.id);
      close();
    } catch (err) {
      console.error("quick capture create_item failed:", err);
      // Richer error surface lands with the settings UI in I-11.
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void commit(true);
    } else if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void commit(false);
    } else if (e.key === "Escape") {
      // <dialog>'s onCancel also fires; handle explicitly so the state
      // goes through the store and stays consistent.
      e.preventDefault();
      close();
    }
  }

  return (
    <dialog
      ref={dialogRef}
      className="quick-capture"
      onCancel={(e) => {
        e.preventDefault();
        close();
      }}
      onClose={close}
    >
      <div className="quick-capture-header">Quick capture → Inbox</div>
      <textarea
        ref={textareaRef}
        className="quick-capture-textarea"
        value={content}
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={handleKeyDown}
        rows={4}
        placeholder="What came to mind?"
        autoFocus
      />
      <div className="quick-capture-help">
        Esc cancel · Enter commit · Ctrl+Enter commit &amp; inspect
      </div>
    </dialog>
  );
}
