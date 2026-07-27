// Pins the load-bearing UX discipline of SwapModal:
//   1. Radios are UNCHECKED by default (no auto-pick).
//   2. Confirm stays disabled until an explicit choice exists.
//   3. Demoting from A offers B and C as destinations; from B offers C
//      only (no cascade in v1).
//
// Discipline #10 in feedback_bay_discipline.md cites these as the
// reason against reflex-click; without them the user can swap into a
// full A by hitting Enter without ever seeing which item was demoted
// where. RTL is the right layer for this test — the discipline is
// about what the rendered radios look like, not what the reducer
// holds.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { SwapModal } from "./SwapModal";
import { useStore } from "../store";
import { Item, Tier } from "../domain";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => {
    throw new Error("invoke() must not be called in disabled-confirm tests");
  }),
}));

function makeItem(overrides: Partial<Item> & Pick<Item, "id" | "tier">): Item {
  return {
    content: overrides.id,
    rank: "m",
    state: "active",
    blocked_reason: null,
    start_at: null,
    due_at: null,
    recurrence: null,
    created_at: 0,
    updated_at: 0,
    deleted: false,
    ...overrides,
  };
}

function seedFullA(enteringTier: Tier = "A") {
  // Five active A items + one entering item in Inbox. Exactly mirrors
  // the swap-modal opening condition: A is at cap, user just dragged
  // a sixth in.
  const aIds = ["a1", "a2", "a3", "a4", "a5"];
  const items: Record<string, Item> = {};
  for (const id of aIds) {
    items[id] = makeItem({ id, tier: "A", content: `A item ${id}` });
  }
  items.enter1 = makeItem({
    id: "enter1",
    tier: "inbox",
    content: "incoming task",
  });

  useStore.setState({
    items,
    itemsByTier: {
      inbox: ["enter1"],
      A: aIds,
      B: [],
      C: [],
    },
    swapPending: {
      kind: "swap",
      enteringId: "enter1",
      enteringTier,
      enteringRank: "k",
    },
  });
  return aIds;
}

describe("SwapModal", () => {
  beforeEach(() => {
    seedFullA("A");
  });

  afterEach(() => {
    useStore.setState({ swapPending: null });
  });

  it("renders the entering item content in the lead text", () => {
    render(<SwapModal />);
    expect(
      screen.getByText(/incoming task/, { selector: ".modal-lead" }),
    ).toBeInTheDocument();
  });

  it("lists all active items in the target tier as demotion candidates", () => {
    render(<SwapModal />);
    for (const id of ["a1", "a2", "a3", "a4", "a5"]) {
      expect(screen.getByText(`A item ${id}`)).toBeInTheDocument();
    }
  });

  it("renders ALL radio buttons unchecked by default", () => {
    render(<SwapModal />);
    const radios = screen.getAllByRole("radio");
    // 5 candidates × 2 dests (B, C) = 10 radios when entering A.
    expect(radios).toHaveLength(10);
    for (const r of radios) {
      expect(r).not.toBeChecked();
    }
  });

  it("disables the Swap button until an explicit choice is made", () => {
    render(<SwapModal />);
    expect(screen.getByRole("button", { name: "Swap" })).toBeDisabled();
  });

  it("enables Swap once any radio is selected", async () => {
    render(<SwapModal />);
    const radios = screen.getAllByRole("radio");
    await userEvent.click(radios[0]);
    expect(screen.getByRole("button", { name: "Swap" })).toBeEnabled();
  });

  it("offers B and C as destinations when entering A", () => {
    render(<SwapModal />);
    const choiceRows = document.querySelectorAll(".swap-choice-row");
    for (const row of choiceRows) {
      const labels = within(row as HTMLElement).getAllByRole("radio");
      expect(labels).toHaveLength(2);
      expect((row as HTMLElement).textContent).toMatch(/→\s*B/);
      expect((row as HTMLElement).textContent).toMatch(/→\s*C/);
    }
  });

  it("offers only C as destination when entering B (no cascade in v1)", () => {
    // Seed a swap into B instead of A. Move 12 items into B for
    // realism, then swap. We need at least one active B target for
    // the modal to render rows.
    const bIds = Array.from({ length: 12 }, (_, i) => `b${i}`);
    const items: Record<string, Item> = {};
    for (const id of bIds) {
      items[id] = makeItem({ id, tier: "B", content: `B item ${id}` });
    }
    items.enter1 = makeItem({
      id: "enter1",
      tier: "inbox",
      content: "incoming task",
    });
    useStore.setState({
      items,
      itemsByTier: { inbox: ["enter1"], A: [], B: bIds, C: [] },
      swapPending: {
        kind: "swap",
        enteringId: "enter1",
        enteringTier: "B",
        enteringRank: "k",
      },
    });

    render(<SwapModal />);
    const radios = screen.getAllByRole("radio");
    // 12 candidates × 1 dest = 12 radios.
    expect(radios).toHaveLength(12);
    const choiceRows = document.querySelectorAll(".swap-choice-row");
    for (const row of choiceRows) {
      expect((row as HTMLElement).textContent).toMatch(/→\s*C/);
      expect((row as HTMLElement).textContent).not.toMatch(/→\s*B/);
    }
  });

  it("only one radio is checked at a time across all rows (single-pick group)", async () => {
    render(<SwapModal />);
    const radios = screen.getAllByRole("radio");
    await userEvent.click(radios[0]);
    expect(radios.filter((r) => (r as HTMLInputElement).checked)).toHaveLength(1);
    await userEvent.click(radios[5]); // different row + dest
    const checked = radios.filter((r) => (r as HTMLInputElement).checked);
    expect(checked).toHaveLength(1);
    expect(checked[0]).toBe(radios[5]);
  });

  it("excludes blocked items from the demotion candidate list", () => {
    // Blocked items don't count against the cap (CLAUDE.md §1) and so
    // should never appear as demotion candidates — they're already
    // out of the active set.
    useStore.setState((s) => ({
      items: {
        ...s.items,
        a3: { ...s.items.a3, state: "blocked", blocked_reason: "waiting" },
      },
    }));

    render(<SwapModal />);
    expect(screen.queryByText("A item a3")).not.toBeInTheDocument();
    expect(screen.getAllByRole("radio")).toHaveLength(8); // 4 active × 2
  });
});
