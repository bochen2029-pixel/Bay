// Three-way conjunction that distinguishes "cross-tier drag needs a
// SwapModal" from "cross-tier drag only needs MoveReasonModal". Extracted
// into a pure helper so onDragEnd stays a thin dispatcher and the
// Node-side smoke test can import-and-verify the same predicate.
//
// All three conditions must hold:
//   1. The target is A or B (Inbox and C are uncapped)
//   2. The target's active count is already at cap
//   3. The dragged item is itself `state === 'active'` (blocked/done
//      items don't count toward caps so their arrival doesn't push
//      the target over)
//
// Miss any one and needsSwap returns false — the caller falls back to
// MoveReasonModal (simple cross-tier move). See SPEC §3.2 and
// CLAUDE.md §Design philosophy #1.

import { A_CAP, B_CAP, Item, Tier } from "./domain";

export function needsSwap(
  item: Item,
  targetTier: Tier,
  targetActiveCount: number,
): boolean {
  if (targetTier !== "A" && targetTier !== "B") return false;
  if (item.state !== "active") return false;
  const cap = targetTier === "A" ? A_CAP : B_CAP;
  return targetActiveCount >= cap;
}
