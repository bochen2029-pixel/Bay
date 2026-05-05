// Single source of truth for frontend state. Shape matches SPEC §1.1
// progressively — only fields consumed by the current increment live
// here; UI state (modals, activeView, etc.) lands when its command or
// view does.
//
// Canonical storage is `items: Record<string, Item>` — flat, id-keyed.
// `itemsByTier` is parallel derived state maintained eagerly on every
// mutation, not recomputed on every render. Strip components subscribe
// to their own item by id, so a mutation to item X never re-renders
// the Strip for item Y.

import { create } from "zustand";
import { Item, Settings, Tier } from "./domain";

export type BackendWarning = {
  kind: string;
  message: string;
};

/** Pending cross-tier drag whose confirmation modal is currently open.
 *  Captured at onDragEnd time so cancelling is a single reducer call
 *  with zero side effects. */
export type MoveReasonPending = {
  kind: "reason";
  activeId: string;
  toTier: Tier;
  toRank: string;
};

/** Pending swap: the user dragged an active item into a full A or B
 *  and the SwapModal is collecting which item to demote + where. */
export type SwapPending = {
  kind: "swap";
  enteringId: string;
  enteringTier: Tier; // A or B (where entering goes)
  enteringRank: string;
};

/** Pending delete whose undo toast is still visible. The snapshot is
 *  kept locally so restore can fire even if the backend projection has
 *  rolled forward. */
export type DeletedPending = {
  snapshot: Item;
  deletedAt: number; // unix ms; drives the 10s toast window
};

/** Per-bay session toggle: whether "earlier done items" (items whose
 *  state was 'done' at bootstrap time) are currently revealed. Default
 *  false. Resets on app launch. */
export type DoneRevealed = Record<Tier, boolean>;

/** Most recent LAN-captured item. Drives the auto-dismissing toast
 *  shown when phone capture succeeds, so the desktop user gets visual
 *  confirmation without switching focus. */
export type LanCaptureFlash = {
  content: string;
  ts: number;
};

type State = {
  items: Record<string, Item>;
  itemsByTier: Record<Tier, string[]>;
  settings: Settings | null;
  bootstrapped: boolean;

  // UI state — additive across increments.
  selectedItemId: string | null;
  quickCaptureOpen: boolean;
  backendWarning: BackendWarning | null;
  moveReasonPending: MoveReasonPending | null;
  swapPending: SwapPending | null;
  editingItemId: string | null;
  blockPending: { itemId: string } | null;
  deletedPending: DeletedPending | null;
  lanCaptureFlash: LanCaptureFlash | null;

  /** Items marked done during this session. Rendered inline alongside
   *  active items per SPEC §10.2 resolution; on next app launch they
   *  become "earlier done items" and hide until the per-bay link
   *  reveals them. */
  sessionDoneIds: Set<string>;
  doneRevealed: DoneRevealed;
};

type Actions = {
  bootstrap: (items: Item[], settings: Settings) => void;
  onItemCreated: (item: Item) => void;
  onItemUpdated: (item: Item) => void;
  onItemDeleted: (id: string) => void;

  setSelectedItemId: (id: string | null) => void;
  openQuickCapture: () => void;
  closeQuickCapture: () => void;

  setBackendWarning: (w: BackendWarning | null) => void;

  openMoveReason: (p: MoveReasonPending) => void;
  closeMoveReason: () => void;
  openSwap: (p: SwapPending) => void;
  closeSwap: () => void;

  setEditingItemId: (id: string | null) => void;
  openBlockModal: (itemId: string) => void;
  closeBlockModal: () => void;

  clearDeletedPending: () => void;
  toggleDoneRevealed: (tier: Tier) => void;

  setLanCaptureFlash: (flash: LanCaptureFlash | null) => void;
};

type Store = State & Actions;

const EMPTY_BY_TIER: Record<Tier, string[]> = {
  inbox: [],
  A: [],
  B: [],
  C: [],
};

const EMPTY_DONE_REVEALED: DoneRevealed = {
  inbox: false,
  A: false,
  B: false,
  C: false,
};

export const useStore = create<Store>((set, get) => ({
  items: {},
  itemsByTier: EMPTY_BY_TIER,
  settings: null,
  bootstrapped: false,
  selectedItemId: null,
  quickCaptureOpen: false,
  backendWarning: null,
  moveReasonPending: null,
  swapPending: null,
  editingItemId: null,
  blockPending: null,
  deletedPending: null,
  lanCaptureFlash: null,
  sessionDoneIds: new Set<string>(),
  doneRevealed: EMPTY_DONE_REVEALED,

  bootstrap: (items, settings) => {
    const itemsMap: Record<string, Item> = {};
    const byTier: Record<Tier, string[]> = {
      inbox: [],
      A: [],
      B: [],
      C: [],
    };
    for (const item of items) {
      itemsMap[item.id] = item;
      byTier[item.tier].push(item.id);
    }
    for (const t of Object.keys(byTier) as Tier[]) {
      byTier[t].sort((a, b) => compareRank(itemsMap[a].rank, itemsMap[b].rank));
    }
    // Bootstrap establishes the "prior session" baseline: any items
    // already in 'done' state are "earlier done items" and hide by
    // default. sessionDoneIds starts empty.
    set({
      items: itemsMap,
      itemsByTier: byTier,
      settings,
      bootstrapped: true,
      sessionDoneIds: new Set<string>(),
      doneRevealed: EMPTY_DONE_REVEALED,
    });
  },

  onItemCreated: (item) => {
    const { items, itemsByTier } = get();
    if (items[item.id]) return; // already present

    const newItems = { ...items, [item.id]: item };
    const newByTier = { ...itemsByTier };
    newByTier[item.tier] = insertByRank(
      itemsByTier[item.tier],
      item.id,
      newItems,
    );
    set({ items: newItems, itemsByTier: newByTier });
  },

  onItemUpdated: (item) => {
    const { items, itemsByTier, sessionDoneIds } = get();
    const existing = items[item.id];
    if (!existing) return; // unknown id

    // Idempotent: repeated deliveries (invoke-resolve + event) match on
    // updated_at and bail.
    if (existing.updated_at === item.updated_at) return;

    const newItems = { ...items, [item.id]: item };
    const newByTier = { ...itemsByTier };
    newByTier[existing.tier] = itemsByTier[existing.tier].filter(
      (id) => id !== item.id,
    );
    newByTier[item.tier] = insertByRank(
      newByTier[item.tier],
      item.id,
      newItems,
    );

    // Track session-done: if this update is the first time we see this
    // item in `done`, remember it so it stays visible for the rest of
    // the session.
    let newSessionDone = sessionDoneIds;
    if (
      item.state === "done" &&
      existing.state !== "done" &&
      !sessionDoneIds.has(item.id)
    ) {
      newSessionDone = new Set(sessionDoneIds);
      newSessionDone.add(item.id);
    }

    set({
      items: newItems,
      itemsByTier: newByTier,
      sessionDoneIds: newSessionDone,
    });
  },

  onItemDeleted: (id) => {
    const { items, itemsByTier, sessionDoneIds } = get();
    const existing = items[id];
    if (!existing) return;

    const newItems = { ...items };
    delete newItems[id];
    const newByTier = { ...itemsByTier };
    newByTier[existing.tier] = itemsByTier[existing.tier].filter(
      (x) => x !== id,
    );
    let newSessionDone = sessionDoneIds;
    if (sessionDoneIds.has(id)) {
      newSessionDone = new Set(sessionDoneIds);
      newSessionDone.delete(id);
    }

    set({
      items: newItems,
      itemsByTier: newByTier,
      sessionDoneIds: newSessionDone,
      deletedPending: { snapshot: existing, deletedAt: Date.now() },
    });
  },

  setSelectedItemId: (id) => set({ selectedItemId: id }),
  openQuickCapture: () => set({ quickCaptureOpen: true }),
  closeQuickCapture: () => set({ quickCaptureOpen: false }),

  setBackendWarning: (w) => set({ backendWarning: w }),

  openMoveReason: (p) => set({ moveReasonPending: p }),
  closeMoveReason: () => set({ moveReasonPending: null }),
  openSwap: (p) => set({ swapPending: p }),
  closeSwap: () => set({ swapPending: null }),

  setEditingItemId: (id) => set({ editingItemId: id }),
  openBlockModal: (itemId) => set({ blockPending: { itemId } }),
  closeBlockModal: () => set({ blockPending: null }),

  clearDeletedPending: () => set({ deletedPending: null }),
  toggleDoneRevealed: (tier) =>
    set((s) => ({
      doneRevealed: { ...s.doneRevealed, [tier]: !s.doneRevealed[tier] },
    })),

  setLanCaptureFlash: (flash) => set({ lanCaptureFlash: flash }),
}));

function compareRank(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

function insertByRank(
  ids: string[],
  newId: string,
  itemsMap: Record<string, Item>,
): string[] {
  const newRank = itemsMap[newId].rank;
  const out: string[] = [];
  let inserted = false;
  for (const id of ids) {
    if (!inserted && compareRank(itemsMap[id].rank, newRank) > 0) {
      out.push(newId);
      inserted = true;
    }
    out.push(id);
  }
  if (!inserted) out.push(newId);
  return out;
}
