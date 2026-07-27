// TodayLane — the day's ≤3 commitments. The contracts worth pinning
// are the ones that carry doctrine rather than markup:
//   * the frontend owns the calendar date and rolls the day on mount
//     (the backend has no timezone), and it reads state only AFTER the
//     roll, or the lane would render yesterday's leftovers;
//   * the cap is visible and the picker refuses a 4th selection;
//   * last night's "first move" is offered back in the morning.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { TodayLane, localDate } from "./TodayLane";
import { useStore } from "../store";
import { Item } from "../domain";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function makeItem(id: string, content: string, over: Partial<Item> = {}): Item {
  return {
    id,
    content,
    tier: "A",
    rank: id,
    state: "active",
    blocked_reason: null,
    start_at: null,
    due_at: null,
    recurrence: null,
    first_step: null,
    today_on: null,
    created_at: 1,
    updated_at: 1,
    deleted: false,
    ...over,
  };
}

function seed(items: Item[]) {
  useStore.setState({
    items: Object.fromEntries(items.map((i) => [i.id, i])),
    itemsByTier: {
      inbox: [],
      A: items.filter((i) => i.tier === "A").map((i) => i.id),
      B: items.filter((i) => i.tier === "B").map((i) => i.id),
      C: [],
    },
    openSession: null,
    bootstrapped: true,
  });
}

const OPEN_SESSION = {
  id: "s1",
  item_id: "a",
  started_at: 1_769_817_600_000,
  ended_at: null,
  outcome: null,
  reason: null,
  note: null,
};

/** Route by command name rather than call order: `roll_day` and
 *  `get_day_state` fire asynchronously on mount, so a `...Once` mock
 *  queued right after render is liable to be eaten by one of them. */
function routeInvokes(dayState: Record<string, unknown>) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "roll_day") return Promise.resolve({ expired_ids: [] });
    if (cmd === "get_day_state") return Promise.resolve(dayState);
    if (cmd === "start_session") return Promise.resolve(OPEN_SESSION);
    if (cmd === "open_day") return Promise.resolve([]);
    return Promise.resolve(null);
  });
}

describe("TodayLane", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("rolls the day with the LOCAL date before reading state", async () => {
    seed([]);
    routeInvokes({ today_ids: [], day_opened: false, tomorrow_first: null });
    render(<TodayLane />);

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_day_state", { date: localDate() }));
    const calls = invokeMock.mock.calls.map((c) => c[0]);
    expect(calls[0]).toBe("roll_day");
    expect(invokeMock).toHaveBeenCalledWith("roll_day", { today: localDate() });
    // Ordering is the point: reading state before the roll would show
    // yesterday's membership as if it were today's plan.
    expect(calls.indexOf("roll_day")).toBeLessThan(calls.indexOf("get_day_state"));
  });

  it("localDate is the user's calendar day, not a UTC slice", () => {
    // A late-evening local time can already be tomorrow in UTC; the day
    // boundary must be the user's.
    const d = new Date(2026, 6, 26, 23, 30);
    expect(localDate(d)).toBe("2026-07-26");
    expect(localDate(d)).not.toBe(d.toISOString().slice(0, 10));
  });

  it("shows the active count against the cap and lists the chosen items", async () => {
    seed([makeItem("a", "ship the memo"), makeItem("b", "call Marco"), makeItem("c", "done thing", { state: "done" })]);
    routeInvokes({ today_ids: ["a", "b", "c"], day_opened: true, tomorrow_first: null });
    render(<TodayLane />);

    expect(await screen.findByText("ship the memo")).toBeInTheDocument();
    // The counter is ACTIVE-only: the done item stays visible but does
    // not hold a slot.
    expect(screen.getByText("2 / 3")).toBeInTheDocument();
  });

  it("offers last night's first move when nothing is chosen yet", async () => {
    seed([makeItem("a", "rewrite the intro")]);
    routeInvokes({ today_ids: [], day_opened: false, tomorrow_first: "a" });
    render(<TodayLane />);

    expect(
      await screen.findByText(/Last night you named "rewrite the intro" as the first move/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Plan today…" })).toBeInTheDocument();
  });

  it("the picker stops at three and commits through open_day", async () => {
    seed([
      makeItem("a", "one"),
      makeItem("b", "two"),
      makeItem("c", "three"),
      makeItem("d", "four"),
    ]);
    routeInvokes({ today_ids: [], day_opened: false, tomorrow_first: null });
    render(<TodayLane />);

    await userEvent.click(await screen.findByRole("button", { name: "Plan today…" }));
    const boxes = screen.getAllByRole("checkbox");
    await userEvent.click(boxes[0]);
    await userEvent.click(boxes[1]);
    await userEvent.click(boxes[2]);
    // Fourth is refused by the UI — the cap is visible, not just
    // enforced server-side.
    expect(boxes[3]).toBeDisabled();
    expect(screen.getByText(/Today is full/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Commit to 3" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("open_day", {
        date: localDate(),
        todayIds: ["a", "b", "c"],
      }),
    );
  });

  it("starting from the lane opens the Now slot", async () => {
    seed([makeItem("a", "deep work")]);
    routeInvokes({ today_ids: ["a"], day_opened: true, tomorrow_first: null });
    render(<TodayLane />);

    await userEvent.click(await screen.findByRole("button", { name: "▶ Start" }));
    expect(invokeMock).toHaveBeenCalledWith("start_session", { itemId: "a" });
    await waitFor(() => expect(useStore.getState().openSession?.item_id).toBe("a"));
  });

  it("hides Start while another session is already running", async () => {
    seed([makeItem("a", "deep work")]);
    useStore.setState({
      openSession: {
        id: "s0",
        item_id: "other",
        started_at: Date.now(),
        ended_at: null,
        outcome: null,
        reason: null,
        note: null,
      },
    });
    routeInvokes({ today_ids: ["a"], day_opened: true, tomorrow_first: null });
    render(<TodayLane />);

    expect(await screen.findByText("deep work")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "▶ Start" })).not.toBeInTheDocument();
  });

  it("day-close asks exactly one question and sends the answer", async () => {
    seed([makeItem("a", "tomorrow's opener")]);
    routeInvokes({ today_ids: [], day_opened: true, tomorrow_first: null });
    render(<TodayLane />);

    await userEvent.click(await screen.findByRole("button", { name: "Close day…" }));
    const select = screen.getByRole("combobox");
    await userEvent.selectOptions(select, "a");
    invokeMock.mockResolvedValueOnce(undefined);
    await userEvent.click(screen.getByRole("button", { name: "Close day" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("close_day", {
        date: localDate(),
        tomorrowFirst: "a",
        note: null,
      }),
    );
  });
});
