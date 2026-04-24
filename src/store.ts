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

type State = {
  items: Record<string, Item>;
  itemsByTier: Record<Tier, string[]>;
  settings: Settings | null;
  bootstrapped: boolean;

  // UI state — additive across increments.
  selectedItemId: string | null;
  quickCaptureOpen: boolean;
  backendWarning: BackendWarning | null;
};

type Actions = {
  /** Replace the world from a bootstrap payload (initial load). */
  bootstrap: (items: Item[], settings: Settings) => void;
  /** Idempotent add-to-store. Called both by the create_item invoke
   *  resolution and by the TauriEventBridge's `item_created` listener;
   *  whichever arrives first wins, the other is a no-op. */
  onItemCreated: (item: Item) => void;

  setSelectedItemId: (id: string | null) => void;
  openQuickCapture: () => void;
  closeQuickCapture: () => void;

  setBackendWarning: (w: BackendWarning | null) => void;
};

type Store = State & Actions;

const EMPTY_BY_TIER: Record<Tier, string[]> = {
  inbox: [],
  A: [],
  B: [],
  C: [],
};

export const useStore = create<Store>((set, get) => ({
  items: {},
  itemsByTier: EMPTY_BY_TIER,
  settings: null,
  bootstrapped: false,
  selectedItemId: null,
  quickCaptureOpen: false,
  backendWarning: null,

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
    set({ items: itemsMap, itemsByTier: byTier, settings, bootstrapped: true });
  },

  onItemCreated: (item) => {
    const { items, itemsByTier } = get();
    if (items[item.id]) return; // already present (promise and event both arrived)

    const newItems = { ...items, [item.id]: item };
    const newByTier = { ...itemsByTier };
    newByTier[item.tier] = insertByRank(
      itemsByTier[item.tier],
      item.id,
      newItems,
    );
    set({ items: newItems, itemsByTier: newByTier });
  },

  setSelectedItemId: (id) => set({ selectedItemId: id }),
  openQuickCapture: () => set({ quickCaptureOpen: true }),
  closeQuickCapture: () => set({ quickCaptureOpen: false }),

  setBackendWarning: (w) => set({ backendWarning: w }),
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
