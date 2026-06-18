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
import { format } from "date-fns";

import { Item } from "../domain";
import { isStale } from "../staleness";
import { useStore } from "../store";

export function Strip({ itemId }: { itemId: string }) {
  const item = useStore((s) => s.items[itemId]);
  const settings = useStore((s) => s.settings);
  const isEditing = useStore((s) => s.editingItemId === itemId);
  const [dateField, setDateField] = useState<"start" | "due" | null>(null);
  const setEditingItemId = useStore((s) => s.setEditingItemId);
  const openBlockModal = useStore((s) => s.openBlockModal);
  const setSelectedItemId = useStore((s) => s.setSelectedItemId);
  const onItemUpdated = useStore((s) => s.onItemUpdated);
  const isSelected = useStore((s) => s.selectedIds.has(itemId));
  const toggleSelected = useStore((s) => s.toggleSelected);
  const selectRangeTo = useStore((s) => s.selectRangeTo);

  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: itemId, disabled: isEditing || dateField !== null });

  if (!item) return null;

  const now = Date.now();
  const stale = isStale(item, settings, now);
  const overdue =
    item.state === "active" && item.due_at !== null && item.due_at < now;

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
        (item.state === "blocked" ? " is-blocked" : "") +
        (isSelected ? " is-selected" : "")
      }
      data-item-id={item.id}
      data-state={item.state}
      {...attributes}
    >
      <input
        type="checkbox"
        className="strip-select"
        checked={isSelected}
        aria-label={isSelected ? "Deselect item" : "Select item"}
        onClick={(e) => {
          e.stopPropagation();
          // Shift-click extends the selection over the tier's order.
          // Plain click falls through to onChange for a simple toggle.
          if (e.shiftKey) {
            e.preventDefault();
            selectRangeTo(item.tier, item.id);
          }
        }}
        onChange={() => toggleSelected(item.id)}
      />

      <span
        className="strip-handle"
        aria-label="Drag handle"
        {...(isEditing ? {} : listeners)}
      >
        {stale ? "⚠" : item.state === "blocked" ? "⏸" : "≡"}
      </span>

      {isEditing ? (
        <StripInlineEdit
          item={item}
          onDone={() => setEditingItemId(null)}
        />
      ) : (
        <span
          className="strip-content"
          onClick={() => setSelectedItemId(item.id)}
        >
          {item.content}
          {item.state === "blocked" && item.blocked_reason ? (
            <span className="strip-blocked-reason">
              {" · "}
              {item.blocked_reason}
            </span>
          ) : null}
          {item.start_at !== null ? (
            <span className="strip-date-badge strip-start">
              ▸ {format(item.start_at, "MMM d")}
            </span>
          ) : null}
          {item.due_at !== null ? (
            <span
              className={
                "strip-date-badge strip-due" + (overdue ? " is-overdue" : "")
              }
            >
              ● {format(item.due_at, "MMM d")}
            </span>
          ) : null}
        </span>
      )}

      {dateField !== null ? (
        <StripDatePicker
          item={item}
          field={dateField}
          onDone={() => setDateField(null)}
          onUpdated={onItemUpdated}
        />
      ) : null}

      <StripMenu
        item={item}
        onEdit={() => setEditingItemId(item.id)}
        onSetDate={(f) => setDateField(f)}
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
  onSetDate,
  onToggleDone,
  onToggleBlocked,
  onDelete,
}: {
  item: Item;
  onEdit: () => void;
  onSetDate: (field: "start" | "due") => void;
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
          <button
            type="button"
            role="menuitem"
            onClick={() => run(() => onSetDate("start"))}
          >
            Set start date…
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={() => run(() => onSetDate("due"))}
          >
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

function StripDatePicker({
  item,
  field,
  onDone,
  onUpdated,
}: {
  item: Item;
  field: "start" | "due";
  onDone: () => void;
  onUpdated: (item: Item) => void;
}) {
  const existing = field === "start" ? item.start_at : item.due_at;
  const [value, setValue] = useState(existing ? unixMsToDateStr(existing) : "");
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => {
    ref.current?.focus();
  }, []);

  async function commit(next: number | null) {
    if (busy) return;
    setBusy(true);
    try {
      const raw = await invoke<unknown>("set_item_date", {
        id: item.id,
        field,
        value: next,
      });
      onUpdated(Item.parse(raw));
      onDone();
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      if (msg === "NO_OP") onDone();
      else {
        console.error("set_item_date failed:", err);
        setBusy(false);
      }
    }
  }

  return (
    <div
      className="strip-date-picker"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          onDone();
        }
      }}
    >
      <input
        ref={ref}
        type="date"
        value={value}
        onChange={(e) => setValue(e.target.value)}
      />
      <button
        type="button"
        onClick={() => commit(value ? dateStrToUnixMs(value) : null)}
        disabled={busy}
      >
        Save
      </button>
      {existing !== null ? (
        <button type="button" onClick={() => commit(null)} disabled={busy}>
          Clear
        </button>
      ) : null}
      <button type="button" onClick={onDone} disabled={busy}>
        Cancel
      </button>
    </div>
  );
}

function unixMsToDateStr(ms: number): string {
  const d = new Date(ms);
  const y = d.getFullYear();
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const da = String(d.getDate()).padStart(2, "0");
  return `${y}-${mo}-${da}`;
}

function dateStrToUnixMs(s: string): number {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d).getTime();
}
