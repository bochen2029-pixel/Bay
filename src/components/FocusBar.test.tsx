// FocusBar — the "Now" HUD. Pins the contract that matters: the bar
// exists only while a session is open, it shows the first step (the
// activation-energy handle) in plain sight, and each of the three
// endings passes the right arguments to end_session — including the
// interruption reason, which is the one field the Mirror later
// clusters into the user's personal taxonomy.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { FocusBar } from "./FocusBar";
import { useStore } from "../store";
import { Item, Session } from "../domain";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function makeItem(overrides: Partial<Item> = {}): Item {
  return {
    id: "itm-1",
    content: "write the memo",
    tier: "A",
    rank: "m",
    state: "active",
    blocked_reason: null,
    start_at: null,
    due_at: null,
    recurrence: null,
    first_step: null,
    today_on: null,
    created_at: 100,
    updated_at: 100,
    deleted: false,
    ...overrides,
  };
}

function makeSession(): Session {
  return {
    id: "ses-1",
    item_id: "itm-1",
    started_at: Date.now() - 65_000,
    ended_at: null,
    outcome: null,
    reason: null,
    note: null,
  };
}

function seed(item: Item, session: Session | null) {
  useStore.setState({
    items: { [item.id]: item },
    itemsByTier: { inbox: [], A: [item.id], B: [], C: [] },
    openSession: session,
    bootstrapped: true,
  });
}

describe("FocusBar", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("renders nothing when no session is open", () => {
    seed(makeItem(), null);
    const { container } = render(<FocusBar />);
    expect(container.querySelector(".focus-bar")).toBeNull();
  });

  it("shows the item, its first step, and elapsed time while open", () => {
    seed(makeItem({ first_step: "open contract.pdf" }), makeSession());
    render(<FocusBar />);
    expect(screen.getByText("write the memo")).toBeInTheDocument();
    expect(screen.getByText("→ open contract.pdf")).toBeInTheDocument();
    // 65s elapsed → "1:05" (mm:ss).
    expect(screen.getByText("1:05")).toBeInTheDocument();
  });

  it("Done ends the session with outcome done and no reason", async () => {
    const item = makeItem();
    seed(item, makeSession());
    invokeMock.mockResolvedValueOnce({
      session: { ...makeSession(), ended_at: Date.now(), outcome: "done" },
      item: { ...item, state: "done" },
      spawned: [],
    });
    render(<FocusBar />);
    await userEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(invokeMock).toHaveBeenCalledWith("end_session", {
      outcome: "done",
      reason: null,
      note: null,
    });
  });

  it("Interrupt offers the taxonomy and passes the chosen reason", async () => {
    const item = makeItem();
    seed(item, makeSession());
    invokeMock.mockResolvedValueOnce({
      session: {
        ...makeSession(),
        ended_at: Date.now(),
        outcome: "interrupted",
        reason: "meeting",
      },
      item,
      spawned: [],
    });
    render(<FocusBar />);
    await userEvent.click(screen.getByRole("button", { name: "Interrupt…" }));
    const options = screen.getAllByRole("menuitem").map((b) => b.textContent);
    expect(options).toEqual([
      "meeting",
      "person",
      "self switch",
      "blocked",
      "energy",
    ]);
    await userEvent.click(screen.getByRole("menuitem", { name: "meeting" }));
    expect(invokeMock).toHaveBeenCalledWith("end_session", {
      outcome: "interrupted",
      reason: "meeting",
      note: null,
    });
  });

  it("clears the open session and applies the returned item on end", async () => {
    const item = makeItem();
    seed(item, makeSession());
    invokeMock.mockResolvedValueOnce({
      session: { ...makeSession(), ended_at: Date.now(), outcome: "progress" },
      item: { ...item, updated_at: 200 },
      spawned: [],
    });
    render(<FocusBar />);
    await userEvent.click(screen.getByRole("button", { name: "Pause" }));
    expect(useStore.getState().openSession).toBeNull();
    expect(useStore.getState().items["itm-1"].updated_at).toBe(200);
  });
});
