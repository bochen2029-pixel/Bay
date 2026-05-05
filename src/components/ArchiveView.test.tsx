// Pins ArchiveView's UX:
//   - empty state when nothing has been deleted
//   - rows render with tier badge + content + delete-date
//   - Restore fires invoke('restore_item') with the right id and
//     then refetches the list (so the just-restored row disappears)
//   - Restore error surfaces inline next to the offending row
//     without dropping the whole view
//
// The Tauri invoke surface is mocked at the module level. Each test
// configures the mock to return the responses in the order it
// expects them to be called (list_archived_items first, then
// possibly restore_item, then list_archived_items again).

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { ArchiveView } from "./ArchiveView";
import { useStore } from "../store";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function makeArchivedItem(id: string, content: string, deletedAt: number) {
  return {
    id,
    content,
    tier: "A" as const,
    rank: "m",
    state: "active" as const,
    blocked_reason: null,
    start_at: null,
    due_at: null,
    created_at: deletedAt - 1_000,
    updated_at: deletedAt,
    deleted: true,
  };
}

describe("ArchiveView", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    useStore.setState({ items: {}, itemsByTier: { inbox: [], A: [], B: [], C: [] } });
  });

  afterEach(() => {
    invokeMock.mockReset();
  });

  it("renders Loading initially while the list is in flight", async () => {
    // Resolve never on this run to keep the loading state visible.
    invokeMock.mockImplementationOnce(() => new Promise(() => {}));
    render(<ArchiveView />);
    expect(screen.getByText(/Loading…/)).toBeInTheDocument();
  });

  it("renders the empty state when nothing has been deleted", async () => {
    invokeMock.mockResolvedValueOnce([]);
    render(<ArchiveView />);
    await waitFor(() =>
      expect(screen.getByText(/Nothing archived/)).toBeInTheDocument(),
    );
    expect(screen.getByText(/Archive/)).toBeInTheDocument();
    expect(screen.getByText("0 items")).toBeInTheDocument();
  });

  it("renders rows for each archived item", async () => {
    const items = [
      makeArchivedItem("itm-2", "second deleted", 2_000_000_000_000),
      makeArchivedItem("itm-1", "first deleted", 1_000_000_000_000),
    ];
    invokeMock.mockResolvedValueOnce(items);
    render(<ArchiveView />);

    await waitFor(() =>
      expect(screen.getByText("second deleted")).toBeInTheDocument(),
    );
    expect(screen.getByText("first deleted")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Restore" })).toHaveLength(2);
    expect(screen.getByText(/^2 items$/)).toBeInTheDocument();
  });

  it("Restore fires invoke('restore_item') and refetches the list", async () => {
    const item = makeArchivedItem("itm-1", "to restore", 1_000_000_000_000);
    invokeMock.mockResolvedValueOnce([item]); // initial list
    invokeMock.mockResolvedValueOnce({ ...item, deleted: false }); // restore_item return
    invokeMock.mockResolvedValueOnce([]); // refetch returns empty

    render(<ArchiveView />);
    await waitFor(() =>
      expect(screen.getByText("to restore")).toBeInTheDocument(),
    );

    await userEvent.click(screen.getByRole("button", { name: "Restore" }));

    // restore_item was invoked with the right id.
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("restore_item", { id: "itm-1" });
    });

    // Refetched: row no longer present.
    await waitFor(() =>
      expect(screen.queryByText("to restore")).not.toBeInTheDocument(),
    );
    expect(screen.getByText(/Nothing archived/)).toBeInTheDocument();

    // Three calls total: list, restore, list.
    const calls = invokeMock.mock.calls.map((c) => c[0]);
    expect(calls).toEqual([
      "list_archived_items",
      "restore_item",
      "list_archived_items",
    ]);
  });

  it("surfaces a restore error inline, keeping the row", async () => {
    const item = makeArchivedItem("itm-1", "blocked by cap", 1_000_000_000_000);
    invokeMock.mockResolvedValueOnce([item]);
    invokeMock.mockRejectedValueOnce("CAP_EXCEEDED");

    render(<ArchiveView />);
    await waitFor(() =>
      expect(screen.getByText("blocked by cap")).toBeInTheDocument(),
    );

    await userEvent.click(screen.getByRole("button", { name: "Restore" }));

    await waitFor(() =>
      expect(screen.getByText(/CAP_EXCEEDED/)).toBeInTheDocument(),
    );
    // Row still shown so the user can see what failed.
    expect(screen.getByText("blocked by cap")).toBeInTheDocument();
  });

  it("renders the toplevel error when the listing call itself fails", async () => {
    invokeMock.mockRejectedValueOnce("DB_LOCKED");
    render(<ArchiveView />);
    await waitFor(() =>
      expect(screen.getByText(/DB_LOCKED/)).toBeInTheDocument(),
    );
  });
});
