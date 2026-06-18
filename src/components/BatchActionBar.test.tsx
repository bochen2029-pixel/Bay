// Pins BatchActionBar's UX (I-19):
//   - renders nothing when no items are selected
//   - shows the count + the three actions when a selection exists
//   - "Mark done" / "Mark active" call batch_set_state with the right
//     args and clear the selection on success
//   - "Mark active" surfaces a CAP_EXCEEDED error and KEEPS the selection
//   - "Delete" is a two-step confirm before batch_delete fires
//
// The Tauri invoke surface is mocked at the module level; the real store
// is seeded with a selection via setState.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { BatchActionBar } from "./BatchActionBar";
import { useStore } from "../store";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function seedSelection(ids: string[]) {
  useStore.setState({
    selectedIds: new Set(ids),
    lastSelectedId: ids[ids.length - 1] ?? null,
  });
}

describe("BatchActionBar", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    useStore.setState({
      selectedIds: new Set<string>(),
      lastSelectedId: null,
      deletedPending: null,
    });
  });

  afterEach(() => {
    invokeMock.mockReset();
  });

  it("renders nothing when no items are selected", () => {
    const { container } = render(<BatchActionBar />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the count and the three actions when items are selected", () => {
    seedSelection(["a", "b", "c"]);
    render(<BatchActionBar />);
    expect(screen.getByText("3 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mark done" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mark active" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
  });

  it("Mark done batches the state change and clears the selection", async () => {
    seedSelection(["a", "b"]);
    invokeMock.mockResolvedValueOnce({ affected_ids: ["a", "b"] });
    render(<BatchActionBar />);

    await userEvent.click(screen.getByRole("button", { name: "Mark done" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("batch_set_state", {
        ids: ["a", "b"],
        state: "done",
        blockedReason: null,
      }),
    );
    // Selection cleared on success → the bar disappears.
    await waitFor(() =>
      expect(screen.queryByText(/selected/)).not.toBeInTheDocument(),
    );
    expect(useStore.getState().selectedIds.size).toBe(0);
  });

  it("Mark active surfaces a cap error and keeps the selection", async () => {
    seedSelection(["a", "b"]);
    invokeMock.mockRejectedValueOnce("CAP_EXCEEDED");
    render(<BatchActionBar />);

    await userEvent.click(screen.getByRole("button", { name: "Mark active" }));

    await waitFor(() =>
      expect(screen.getByText(/exceed an A\/B cap/)).toBeInTheDocument(),
    );
    // Selection retained so the user can demote and retry.
    expect(useStore.getState().selectedIds.size).toBe(2);
  });

  it("Delete requires a second confirming click before batch_delete fires", async () => {
    seedSelection(["a", "b"]);
    invokeMock.mockResolvedValueOnce({ affected_ids: ["a", "b"] });
    render(<BatchActionBar />);

    // First click arms the confirm; nothing is invoked yet.
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(invokeMock).not.toHaveBeenCalled();
    const confirm = screen.getByRole("button", { name: /Confirm delete 2/ });

    // Second click executes the batch delete.
    await userEvent.click(confirm);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("batch_delete", {
        ids: ["a", "b"],
      }),
    );
    await waitFor(() =>
      expect(screen.queryByText(/selected/)).not.toBeInTheDocument(),
    );
  });
});
