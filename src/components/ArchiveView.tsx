// Archive view: lists soft-deleted items, most-recently-deleted
// first, with a Restore button per row. v1 already records every
// delete as ITEM_DELETED + soft-delete flag in the projection;
// before this view, the only way to restore was the 10-second
// undo-toast or rebuilding the projection from the event log.
//
// The view re-fetches list_archived_items on mount and after every
// successful restore, so the cap-recheck path on restore (an item
// restored into a now-full A/B fails with CAP_EXCEEDED) shows the
// user an up-to-date list. Backend errors surface inline next to
// the offending row.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { format } from "date-fns";

import { Item, Tier } from "../domain";
import { useStore } from "../store";

const TIER_LABELS: Record<Tier, string> = {
  inbox: "Inbox",
  A: "A",
  B: "B",
  C: "C",
};

export function ArchiveView() {
  const onItemCreated = useStore((s) => s.onItemCreated);
  const [items, setItems] = useState<Item[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [restoring, setRestoring] = useState<string | null>(null);
  const [rowError, setRowError] = useState<Record<string, string>>({});

  const refetch = useCallback(async () => {
    try {
      const raw = await invoke<unknown[]>("list_archived_items");
      const parsed = raw.map((r) => Item.parse(r));
      setItems(parsed);
      setError(null);
    } catch (err) {
      setError(typeof err === "string" ? err : String(err));
      setItems([]);
    }
  }, []);

  useEffect(() => {
    void refetch();
  }, [refetch]);

  async function handleRestore(item: Item) {
    if (restoring) return;
    setRestoring(item.id);
    setRowError((m) => {
      const next = { ...m };
      delete next[item.id];
      return next;
    });
    try {
      const raw = await invoke<unknown>("restore_item", { id: item.id });
      // Backend emits item_created on restore; the bridge will pick
      // it up. Drive the local store too in case the user is mid-
      // navigation and the listener hasn't fired yet.
      onItemCreated(Item.parse(raw));
      // Refetch so the just-restored row leaves the archive list
      // and any concurrent state is reflected.
      await refetch();
    } catch (err) {
      const msg = typeof err === "string" ? err : String(err);
      setRowError((m) => ({ ...m, [item.id]: msg }));
    } finally {
      setRestoring(null);
    }
  }

  if (items === null) {
    return (
      <div className="archive">
        <div className="archive-empty">Loading…</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="archive">
        <h2 className="archive-title">Archive</h2>
        <div className="archive-error">{error}</div>
      </div>
    );
  }

  return (
    <div className="archive">
      <header className="archive-header">
        <h2 className="archive-title">Archive</h2>
        <span className="archive-count">
          {items.length} {items.length === 1 ? "item" : "items"}
        </span>
      </header>
      {items.length === 0 ? (
        <div className="archive-empty">
          Nothing archived. Items deleted from the board land here.
        </div>
      ) : (
        <ul className="archive-list">
          {items.map((it) => (
            <li key={it.id} className="archive-row">
              <span
                className={`archive-tier-badge archive-tier-${it.tier}`}
                aria-label={`from ${TIER_LABELS[it.tier]}`}
              >
                {TIER_LABELS[it.tier]}
              </span>
              <span className="archive-content">{it.content}</span>
              <span className="archive-meta" title={new Date(it.updated_at).toISOString()}>
                deleted {format(it.updated_at, "MMM d, yyyy")}
              </span>
              <button
                type="button"
                className="archive-restore"
                onClick={() => void handleRestore(it)}
                disabled={restoring === it.id}
              >
                {restoring === it.id ? "Restoring…" : "Restore"}
              </button>
              {rowError[it.id] ? (
                <span className="archive-row-error" role="alert">
                  {rowError[it.id]}
                </span>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
