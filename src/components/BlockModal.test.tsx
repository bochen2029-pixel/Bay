// First render-level test in the suite. Pins the load-bearing
// "reason required" UX discipline for BlockModal: the Block button
// stays disabled until the user types a non-whitespace reason.
//
// This test pins behaviour the existing test stack cannot see: cargo
// tests cover backend logic, scripts/test-store-logic.mjs covers pure
// frontend reducers, and `pnpm build` catches TS errors — none of
// them rendered the component before this.
//
// Pattern for future RTL tests in this codebase:
//   - Mock @tauri-apps/api/core.invoke at the top of the file so
//     confirm() never tries to dispatch into a real IPC channel.
//   - Reset the Zustand store via setState() in beforeEach. The
//     store is a singleton; leaks between tests cause the most
//     painful order-dependent failures.
//   - Query by role + accessible name. Avoid getByText for buttons.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { BlockModal } from "./BlockModal";
import { useStore } from "../store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => {
    throw new Error(
      "invoke() must not be called in BlockModal disabled-button tests",
    );
  }),
}));

const seededItem = {
  id: "itm-1",
  content: "do the thing",
  tier: "A" as const,
  rank: "m",
  state: "active" as const,
  blocked_reason: null,
  start_at: null,
  due_at: null,
  recurrence: null,
  created_at: 0,
  updated_at: 0,
  deleted: false,
};

describe("BlockModal", () => {
  beforeEach(() => {
    useStore.setState({
      items: { [seededItem.id]: seededItem },
      itemsByTier: { inbox: [], A: [seededItem.id], B: [], C: [] },
      blockPending: { itemId: seededItem.id },
    });
  });

  afterEach(() => {
    useStore.setState({ blockPending: null });
  });

  it("renders the item content in the preview", () => {
    render(<BlockModal />);
    expect(screen.getByText(/do the thing/)).toBeInTheDocument();
  });

  it("Block button is disabled when no reason has been entered", () => {
    render(<BlockModal />);
    expect(screen.getByRole("button", { name: "Block" })).toBeDisabled();
  });

  it("Block button stays disabled for whitespace-only reasons", async () => {
    render(<BlockModal />);
    const ta = screen.getByRole("textbox");
    await userEvent.type(ta, "   \t  ");
    expect(screen.getByRole("button", { name: "Block" })).toBeDisabled();
  });

  it("Block button enables after a real reason is typed", async () => {
    render(<BlockModal />);
    const ta = screen.getByRole("textbox");
    await userEvent.type(ta, "waiting on Y");
    expect(screen.getByRole("button", { name: "Block" })).toBeEnabled();
  });

  it("Block button re-disables when reason is cleared back to empty", async () => {
    render(<BlockModal />);
    const ta = screen.getByRole("textbox");
    await userEvent.type(ta, "wait");
    expect(screen.getByRole("button", { name: "Block" })).toBeEnabled();
    await userEvent.clear(ta);
    expect(screen.getByRole("button", { name: "Block" })).toBeDisabled();
  });
});
