// Pins the I-20 re-org accept flow in AnalyzePanel:
//   - the LLM's proposals render as a selectable diff after analyze
//   - "Apply N changes" sends accept_suggestion with the selected ops
//     (the firewall surface: human accepts, backend writes)
//   - a CAP_EXCEEDED rejection surfaces inline and does NOT close
//   - deselecting every proposal switches the primary to "Mark reviewed"
//     (an observations-only accept, no ops)
//
// invoke + listen are mocked at the module level; the real store is
// seeded with the proposed items so the diff can show their content.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { AnalyzePanel } from "./AnalyzePanel";
import { useStore } from "../store";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

function makeItem(id: string, content: string) {
  return {
    id,
    content,
    tier: "A" as const,
    rank: "m",
    state: "active" as const,
    blocked_reason: null,
    start_at: null,
    due_at: null,
    recurrence: null,
    created_at: 1,
    updated_at: 1,
    deleted: false,
  };
}

const RESULT = {
  suggestion_event_id: 7,
  observations: [],
  proposals: [
    { item_id: "a", action: "move", to_tier: "C", rationale: "stale 14d in A" },
    { item_id: "b", action: "done", to_tier: null, rationale: null },
  ],
  scope: { since_ts: 0, until_ts: 0, event_count: 3, window_days: 30 },
  model: "test-model",
};

describe("AnalyzePanel re-org proposals (I-20)", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    useStore.setState({
      items: { a: makeItem("a", "ship the thing"), b: makeItem("b", "old task") },
    });
  });

  afterEach(() => invokeMock.mockReset());

  async function runAnalyze() {
    await userEvent.click(screen.getByRole("button", { name: "Run analyze" }));
    await waitFor(() =>
      expect(screen.getByText(/Suggested re-org/)).toBeInTheDocument(),
    );
  }

  it("renders proposals as a selectable diff and applies the selection", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "analyze") return Promise.resolve(RESULT);
      if (cmd === "accept_suggestion") return Promise.resolve(undefined);
      return Promise.reject(new Error(`unexpected ${cmd}`));
    });
    const onClose = vi.fn();
    render(<AnalyzePanel open onClose={onClose} />);
    await runAnalyze();

    // Both proposals shown with their item content.
    expect(screen.getByText(/Move .*ship the thing.* → C/)).toBeInTheDocument();
    expect(screen.getByText(/Mark .*old task.* done/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /Apply 2 changes/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("accept_suggestion", {
        suggestionEventId: 7,
        ops: RESULT.proposals,
      }),
    );
    expect(onClose).toHaveBeenCalled();
  });

  it("surfaces a CAP_EXCEEDED rejection inline without closing", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "analyze") return Promise.resolve(RESULT);
      if (cmd === "accept_suggestion") return Promise.reject("CAP_EXCEEDED");
      return Promise.reject(new Error(`unexpected ${cmd}`));
    });
    const onClose = vi.fn();
    render(<AnalyzePanel open onClose={onClose} />);
    await runAnalyze();

    await userEvent.click(screen.getByRole("button", { name: /Apply 2 changes/ }));

    await waitFor(() =>
      expect(screen.getByText(/would exceed an A\/B cap/)).toBeInTheDocument(),
    );
    expect(onClose).not.toHaveBeenCalled();
  });

  it("falls back to Mark reviewed when all proposals are deselected", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "analyze") return Promise.resolve(RESULT);
      if (cmd === "accept_suggestion") return Promise.resolve(undefined);
      return Promise.reject(new Error(`unexpected ${cmd}`));
    });
    render(<AnalyzePanel open onClose={vi.fn()} />);
    await runAnalyze();

    const checkboxes = screen.getAllByRole("checkbox");
    for (const cb of checkboxes) await userEvent.click(cb); // uncheck both

    // Primary becomes "Mark reviewed" (accept with no ops).
    const reviewed = await screen.findByRole("button", { name: "Mark reviewed" });
    await userEvent.click(reviewed);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("accept_suggestion", {
        suggestionEventId: 7,
      }),
    );
  });
});
