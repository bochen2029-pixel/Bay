// One item row. Subscribes to its own item by id — by zustand's
// default strict-equality on the selected value, a change to any other
// item will not re-render this component.
//
// I-08 additions:
//  - overflow menu (edit / mark done / block / delete + date stubs)
//  - inline-edit mode (textarea replaces content on Edit)
//  - visual treatment for blocked (⏸ + reason) and done (strikethrough)
//  - drag listener moves to the grip handle; the whole strip is still
//    the SortableContext's draggable, but listeners live on the grip
//    so menus and inline-edit receive clicks without starting drags.

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import { Item } from "../domain";
import { useStore } from "../store";

export function Strip({ itemId }: { itemId: string }) {
  const item = useStore((s) => s.items[itemId]);
  const isEditing = useStore((s) => s.editingItemId === itemId);
  const setEditingItemId = useStore((s) => s.setEditingItemId);
  const openBlockModal = useStore((s) => s.openBlockModal);
  const onItemUpdated = useStore((s) => s.onItemUpdated);

  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: itemId, disabled: isEditing });

  if (!item) return null;

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.4 : item.state === "done" ? 0.55 : 1,
  } as const;

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={
        "strip" +
        (isDragging ? " is-dragging" : "") +
        (item.state === "done" ? " is-done" : "") +
        (item.state === "blocked" ? " is-blocked" : "")
      }
      data-item-id={item.id}
      data-state={item.state}
      {...attributes}
    >
      <span
        className="strip-handle"
        aria-label="Drag handle"
        {...(isEditing ? {} : listeners)}
      >
        {item.state === "blocked" ? "⏸" : "≡"}
      </span>

      {isEditing ? (
        <StripInlineEdit
          item={item}
          onDone={() => setEditingItemId(null)}
        />
      ) : (
        <span className="strip-content">
          {item.content}
          {item.state === "blocked" && item.blocked_reason ? (
            <span className="strip-blocked-reason">
              {" · "}
              {item.blocked_reason}
            </span>
          ) : null}
        </span>
      )}

      <StripMenu
        item={item}
        onEdit={() => setEditingItemId(item.id)}
        onToggleDone={async () => {
          try {
            const next = item.state === "done" ? "active" : "done";
            const raw = await invoke<unknown>("set_item_state", {
              id: item.id,
              state: next,
              blockedReason: null,
            });
            onItemUpdated(Item.parse(raw));
          } catch (err) {
            console.error("toggle done failed:", err);
          }
        }}
        onToggleBlocked={async () => {
          if (item.state === "blocked") {
            try {
              const raw = await invoke<unknown>("set_item_state", {
                id: item.id,
                state: "active",
                blockedReason: null,
              });
              onItemUpdated(Item.parse(raw));
            } catch (err) {
              console.error("unblock failed:", err);
            }
          } else {
            openBlockModal(item.id);
          }
        }}
        onDelete={async () => {
          try {
            await invoke("delete_item", { id: item.id });
            // Store mutation happens via the item_deleted event listener;
            // the deletedPending toast surfaces there too.
          } catch (err) {
            console.error("delete failed:", err);
          }
        }}
      />
    </div>
  );
}

function StripInlineEdit({
  item,
  onDone,
}: {
  item: Item;
  onDone: () => void;
}) {
  const [draft, setDraft] = useState(item.content);
  const [busy, setBusy] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const onItemUpdated = useStore((s) => s.onItemUpdated);

  useEffect(() => {
    textareaRef.current?.focus();
    textareaRef.current?.select();
  }, []);

  async function commit() {
    const text = draft.trim();
    if (text === item.content.trim() || !text) {
      onDone();
      return;
    }
    if (busy) return;
    setBusy(true);
    try {
      const raw = await invoke<unknown>("edit_item", {
        id: item.id,
        content: text,
      });
      onItemUpdated(Item.parse(raw));
      onDone();
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      if (msg === "NO_OP") onDone();
      else console.error("edit failed:", err);
      setBusy(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void commit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onDone();
    }
  }

  return (
    <textarea
      ref={textareaRef}
      className="strip-edit-textarea"
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onKeyDown={handleKeyDown}
      onBlur={commit}
      rows={1}
      disabled={busy}
    />
  );
}

function StripMenu({
  item,
  onEdit,
  onToggleDone,
  onToggleBlocked,
  onDelete,
}: {
  item: Item;
  onEdit: () => void;
  onToggleDone: () => void;
  onToggleBlocked: () => void;
  onDelete: () => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Click-outside to dismiss.
  useEffect(() => {
    if (!open) return;
    function handleOutside(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleOutside);
    return () => document.removeEventListener("mousedown", handleOutside);
  }, [open]);

  function run(fn: () => void) {
    setOpen(false);
    fn();
  }

  function stub() {
    setOpen(false);
    console.log("date picker lands in I-09");
  }

  return (
    <div className="strip-menu-wrap" ref={ref}>
      <button
        className="strip-menu"
        type="button"
        aria-label="Item menu"
        aria-expanded={open}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
      >
        ⋯
      </button>
      {open ? (
        <div className="strip-menu-popover" role="menu">
          <button type="button" role="menuitem" onClick={() => run(onEdit)}>
            Edit
          </button>
          <button type="button" role="menuitem" onClick={stub}>
            Set start date…
          </button>
          <button type="button" role="menuitem" onClick={stub}>
            Set due date…
          </button>
          <button type="button" role="menuitem" onClick={() => run(onToggleDone)}>
            {item.state === "done" ? "Mark active" : "Mark done"}
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={() => run(onToggleBlocked)}
          >
            {item.state === "blocked" ? "Unblock" : "Mark blocked…"}
          </button>
          <button
            type="button"
            role="menuitem"
            className="is-destructive"
            onClick={() => run(onDelete)}
          >
            Delete
          </button>
        </div>
      ) : null}
    </div>
  );
}
