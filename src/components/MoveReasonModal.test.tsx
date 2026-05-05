// Pins MoveReasonModal's UX:
//   - reason is OPTIONAL (Confirm enabled with no reason — contrast
//     with BlockModal where it's required)
//   - the header shows from-tier → to-tier so the user can see what
//     they're committing to
//   - Cancel never fires invoke
//   - happy path: invoke succeeds, store update fires, modal closes
//
// The optional-vs-required reason split is doctrine: cross-tier
// move logs the reason for later self-audit, but doesn't block the
// move. Block transitions log the reason as a constraint. Mirroring
// behaviour in tests prevents either side drifting.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { MoveReasonModal } from "./MoveReasonModal";
import { useStore } from "../store";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const seededItem = {
  id: "itm-1",
  content: "important task",
  tier: "A" as const,
  rank: "m",
  state: "active" as const,
  blocked_reason: null,
  start_at: null,
  due_at: null,
  created_at: 100,
  updated_at: 100,
  deleted: false,
};

describe("MoveReasonModal", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    useStore.setState({
      items: { [seededItem.id]: seededItem },
      itemsByTier: { inbox: [], A: [seededItem.id], B: [], C: [] },
      moveReasonPending: {
        kind: "reason",
        activeId: seededItem.id,
        toTier: "C",
        toRank: "z",
      },
    });
  });

  afterEach(() => {
    useStore.setState({ moveReasonPending: null });
  });

  it("renders 'From → To' in the header", () => {
    render(<MoveReasonModal />);
    expect(screen.getByText(/Moving\s+A\s+→\s+C/)).toBeInTheDocument();
  });

  it("renders the item content in the preview", () => {
    render(<MoveReasonModal />);
    expect(screen.getByText(/important task/)).toBeInTheDocument();
  });

  it("Confirm is enabled by default — reason is optional", () => {
    render(<MoveReasonModal />);
    expect(screen.getByRole("button", { name: "Confirm" })).toBeEnabled();
  });

  it("Confirm stays enabled after typing a reason", async () => {
    render(<MoveReasonModal />);
    await userEvent.type(screen.getByRole("textbox"), "demoting due to scope");
    expect(screen.getByRole("button", { name: "Confirm" })).toBeEnabled();
  });

  it("Cancel closes without firing invoke", async () => {
    render(<MoveReasonModal />);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(invokeMock).not.toHaveBeenCalled();
    expect(useStore.getState().moveReasonPending).toBeNull();
  });

  it("happy path: Confirm fires invoke with trimmed reason and closes", async () => {
    const updatedItem = { ...seededItem, tier: "C", updated_at: 200 };
    invokeMock.mockResolvedValueOnce(updatedItem);

    render(<MoveReasonModal />);
    await userEvent.type(screen.getByRole("textbox"), "  scope cut  ");
    await userEvent.click(screen.getByRole("button", { name: "Confirm" }));

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("move_item", {
      id: "itm-1",
      toTier: "C",
      toRank: "z",
      reason: "scope cut", // trimmed
    });
    expect(useStore.getState().moveReasonPending).toBeNull();
  });

  it("empty/whitespace reason serializes as null on the wire", async () => {
    invokeMock.mockResolvedValueOnce({ ...seededItem, tier: "C", updated_at: 200 });
    render(<MoveReasonModal />);
    await userEvent.type(screen.getByRole("textbox"), "   ");
    await userEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(invokeMock).toHaveBeenCalledWith(
      "move_item",
      expect.objectContaining({ reason: null }),
    );
  });

  it("NO_OP from backend closes the modal silently (drag-to-same-rank)", async () => {
    invokeMock.mockRejectedValueOnce("NO_OP");
    render(<MoveReasonModal />);
    await userEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(useStore.getState().moveReasonPending).toBeNull();
  });

  it("non-NO_OP error stays surfaced and keeps the modal open", async () => {
    invokeMock.mockRejectedValueOnce("CAP_EXCEEDED");
    render(<MoveReasonModal />);
    await userEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(screen.getByText(/CAP_EXCEEDED/)).toBeInTheDocument();
    expect(useStore.getState().moveReasonPending).not.toBeNull();
  });
});
