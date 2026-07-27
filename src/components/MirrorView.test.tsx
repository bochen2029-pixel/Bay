// MirrorView — deterministic feedback surface. The contract worth
// pinning is editorial as much as numeric: the figures render from
// get_mirror_stats without any model in the path, the A-leak sentence
// only accuses when the rate is genuinely high, and an empty log reads
// as calm rather than broken.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";

import { MirrorView } from "./MirrorView";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function stats(overrides: Record<string, unknown> = {}) {
  return {
    window_days: 30,
    generated_at: 1_769_817_600_000,
    wip: { inbox: 2, a: 4, b: 6, c: 20 },
    flow: {
      created: 23,
      completed: 6,
      throughput_per_week: 1.4,
      lead_time_p50_days: 3.2,
      lead_time_p90_days: 11.0,
      littles_law_days: 50.0,
    },
    a_leak: { departures: 10, fast_leaks: 4, rate: 0.4 },
    avoidance: [
      {
        item_id: "itm-9",
        content: "prep the board deck",
        tier: "A",
        days_since_touch: 19,
        sessions: 0,
        has_first_step: false,
      },
    ],
    blocks: [{ reason: "waiting on Marco", count: 3, total_days: 12.5 }],
    sessions: {
      count: 8,
      total_minutes: 310,
      median_minutes: 35,
      done: 4,
      progress: 2,
      interrupted: 2,
      interruptions: [["meeting", 2]],
    },
    today: { planned: 12, finished: 7, expired: 5 },
    receipts: [
      {
        item_id: "itm-1",
        content: "shipped the runbook",
        tier: "A",
        done_at: 1_769_817_600_000,
        days_to_done: 4.5,
        sessions: 3,
        minutes: 95,
      },
    ],
    ...overrides,
  };
}

describe("MirrorView", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("requests the stats for the selected window and renders the figures", async () => {
    invokeMock.mockResolvedValue(stats());
    render(<MirrorView />);
    expect(invokeMock).toHaveBeenCalledWith("get_mirror_stats", {
      windowDays: 30,
    });
    expect(await screen.findByText("23")).toBeInTheDocument(); // created
    expect(screen.getByText("1.4/wk")).toBeInTheDocument(); // throughput
    expect(screen.getByText("3.2d")).toBeInTheDocument(); // lead time p50
    expect(screen.getByText("10")).toBeInTheDocument(); // committed WIP 4+6
  });

  it("names the A-inbox pattern when the leak rate is high", async () => {
    invokeMock.mockResolvedValue(stats());
    render(<MirrorView />);
    expect(
      await screen.findByText(/A is functioning as a second inbox/),
    ).toBeInTheDocument();
  });

  it("stays quiet about the leak when the rate is low", async () => {
    invokeMock.mockResolvedValue(
      stats({ a_leak: { departures: 10, fast_leaks: 1, rate: 0.1 } }),
    );
    render(<MirrorView />);
    await screen.findByText(/1 of 10 departures/);
    expect(
      screen.queryByText(/second inbox/),
    ).not.toBeInTheDocument();
  });

  it("lists un-started committed work as the avoidance report", async () => {
    invokeMock.mockResolvedValue(stats());
    render(<MirrorView />);
    expect(await screen.findByText("prep the board deck")).toBeInTheDocument();
    expect(
      screen.getByText(/19d untouched · no first step/),
    ).toBeInTheDocument();
  });

  it("keeps finished work visible as evidence", async () => {
    invokeMock.mockResolvedValue(stats());
    render(<MirrorView />);
    expect(await screen.findByText("shipped the runbook")).toBeInTheDocument();
    expect(screen.getByText(/3 sessions, 1h 35m/)).toBeInTheDocument();
  });

  it("reads calm on an empty log", async () => {
    invokeMock.mockResolvedValue(
      stats({
        flow: {
          created: 0,
          completed: 0,
          throughput_per_week: 0,
          lead_time_p50_days: null,
          lead_time_p90_days: null,
          littles_law_days: null,
        },
        a_leak: { departures: 0, fast_leaks: 0, rate: 0 },
        avoidance: [],
        blocks: [],
        receipts: [],
      }),
    );
    render(<MirrorView />);
    expect(
      await screen.findByText("No items left A in this window."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Every committed item has recorded attention. That is rare."),
    ).toBeInTheDocument();
    expect(screen.getByText("Nothing blocked in this window.")).toBeInTheDocument();
  });
});
