// Staleness determination per tier. Thresholds come from settings
// (v1.4 per-tier config: Inbox 3d, A 14d, B 21d, C null = disabled).
// An item is stale when (now - updated_at) exceeds its tier's
// threshold. updated_at is bumped by every projection mutation, so it
// accurately reflects "days since last event touched this item".
// SPEC §10.7, CLAUDE.md §Interaction rules / Staleness.

import { Item, Settings, Tier } from "./domain";

const MS_PER_DAY = 24 * 60 * 60 * 1000;

export function thresholdDaysForTier(
  tier: Tier,
  settings: Settings,
): number | null {
  switch (tier) {
    case "inbox":
      return settings.staleness_inbox_days;
    case "A":
      return settings.staleness_a_days;
    case "B":
      return settings.staleness_b_days;
    case "C":
      return settings.staleness_c_days;
  }
}

export function isStale(
  item: Item,
  settings: Settings | null,
  now: number,
): boolean {
  if (!settings) return false;
  if (item.state !== "active") return false; // only active items go stale
  const days = thresholdDaysForTier(item.tier, settings);
  if (days === null) return false;
  return now - item.updated_at > days * MS_PER_DAY;
}
