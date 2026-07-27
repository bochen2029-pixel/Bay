// Strip overflow menu (StripMenu inside Strip.tsx). The empirical
// motivator for this whole render harness was commit 1912e5b: a CSS
// bug (.bay { overflow: hidden }) clipped the strip ⋯ popover when
// it opened near the bottom of a bay. None of the existing checks
// (cargo test, store-logic smoke, pnpm build's tsc) saw that bug.
//
// jsdom can't catch the literal bug — `overflow: hidden` is a
// paint-time concern, no real layout engine in jsdom — but it CAN
// pin the menu's structural contract: when ⋯ is clicked, a popover
// containing the six action items appears in the DOM under the
// strip's menu-wrap; clicking outside or selecting an item closes
// it. Future refactors that accidentally drop the menu, swap roles,
// or change item labels will fail here. A Playwright pass over
// the same surface is the right follow-on for paint-time bugs (see
// tech-debt list in project_bay_state.md).
//
// @dnd-kit/sortable is mocked to inert defaults — useSortable in
// jsdom would otherwise need a DndContext wrapper, and the menu
// behaviour we care about is independent of drag.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Strip } from "./Strip";
import { localDate } from "./TodayLane";
import { useStore } from "../store";
import { Item } from "../domain";

vi.mock("@dnd-kit/sortable", () => ({
  useSortable: () => ({
    attributes: {},
    listeners: {},
    setNodeRef: () => {},
    transform: null,
    transition: undefined,
    isDragging: false,
  }),
}));

vi.mock("@dnd-kit/utilities", () => ({
  CSS: { Transform: { toString: () => "" } },
}));

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function seed(item: Item) {
  useStore.setState({
    items: { [item.id]: item },
    itemsByTier: { inbox: [], A: [item.id], B: [], C: [] },
    settings: {
      hotkey: "Ctrl+Alt+N",
      staleness_inbox_days: 3,
      staleness_a_days: 14,
      staleness_b_days: 21,
      staleness_c_days: null,
      lan_capture_enabled: false,
      lan_capture_port: 47821,
      lan_capture_shared_secret: null,
      llm: {
        base_url: "",
        model: "",
        has_api_key: false,
        timeout_ms: 30000,
      },
      analyze_window_days: 30,
      close_to_tray: true,
    },
    bootstrapped: true,
  });
}

function makeActiveA(): Item {
  return {
    id: "itm-1",
    content: "active A item",
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
  };
}

describe("Strip — overflow menu", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(() => {
    useStore.setState({ editingItemId: null });
  });

  it("renders the ⋯ button by default with the menu closed", () => {
    seed(makeActiveA());
    render(<Strip itemId="itm-1" />);
    const trigger = screen.getByRole("button", { name: "Item menu" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("opens the popover with the eleven menu items on ⋯ click", async () => {
    // Eleven for an active, non-recurring item off Today. "Stop
    // repeating" appears only with a recurrence rule (I-21), and the
    // Today entry is hidden for a done item that isn't on Today.
    seed(makeActiveA());
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));

    const menu = screen.getByRole("menu");
    expect(menu).toBeInTheDocument();
    const items = screen.getAllByRole("menuitem");
    expect(items).toHaveLength(11);
    expect(items.map((b) => b.textContent)).toEqual([
      "Edit",
      "Set first step…",
      "Add to Today",
      "Set start date…",
      "Set due date…",
      "Mark done",
      "Mark blocked…",
      "Repeat daily",
      "Repeat weekly",
      "Repeat monthly",
      "Delete",
    ]);
  });

  it("Add to Today sends the LOCAL date, and flips to Remove once on Today", async () => {
    // `add_to_today` was registered, specced, and unreachable: the only
    // way onto Today was the day-open picker, so "put this one on
    // today" — the natural gesture while looking at the board — had no
    // affordance.
    const item = makeActiveA();
    seed(item);
    invokeMock.mockResolvedValueOnce({
      ...item,
      today_on: localDate(),
      updated_at: item.updated_at + 1,
    });
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Add to Today" }));

    expect(invokeMock).toHaveBeenCalledWith("add_to_today", {
      id: "itm-1",
      date: localDate(),
    });
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    expect(
      screen.getByRole("menuitem", { name: "Remove from Today" }),
    ).toBeInTheDocument();
  });

  it("surfaces TODAY_FULL inline instead of failing silently", async () => {
    seed(makeActiveA());
    invokeMock.mockRejectedValueOnce("TODAY_FULL");
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Add to Today" }));

    expect(
      await screen.findByText(/Today is full/),
    ).toBeInTheDocument();
  });

  it("first step is settable, and the item shows it once set", async () => {
    // Regression: `set_first_step` shipped with the field rendered in
    // three places (strip, Today lane, focus bar) and the Mirror even
    // reporting "no first step" — but no way to SET one. The command was
    // unreachable from the app.
    const item = makeActiveA();
    seed(item);
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Set first step…" }),
    );

    const input = screen.getByRole("textbox", { name: "First step" });
    await userEvent.type(input, "open contract.pdf");
    // `updated_at` must advance: the store's onItemUpdated is idempotent
    // on it, so a reply carrying the old timestamp is correctly ignored.
    invokeMock.mockResolvedValueOnce({
      ...item,
      first_step: "open contract.pdf",
      updated_at: item.updated_at + 1,
    });
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(invokeMock).toHaveBeenCalledWith("set_first_step", {
      id: "itm-1",
      step: "open contract.pdf",
    });
    expect(await screen.findByText(/open contract\.pdf/)).toBeInTheDocument();
  });

  it("an item with a first step offers to change or clear it", async () => {
    seed({ ...makeActiveA(), first_step: "dial Marco" });
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Change first step…" }),
    );
    expect(screen.getByRole("textbox", { name: "First step" })).toHaveValue(
      "dial Marco",
    );
    expect(screen.getByRole("button", { name: "Clear" })).toBeInTheDocument();
  });

  it("recurring item shows the active rule checked plus Stop repeating (I-21)", async () => {
    seed({ ...makeActiveA(), recurrence: "FREQ=WEEKLY" });
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    expect(
      screen.getByRole("menuitem", { name: "✓ Repeat weekly" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: "Stop repeating" }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("menuitem")).toHaveLength(12);
  });

  it("Repeat weekly fires invoke('set_item_recurrence') with the rule", async () => {
    const item = makeActiveA();
    seed(item);
    invokeMock.mockResolvedValueOnce({ ...item, recurrence: "FREQ=WEEKLY" });
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Repeat weekly" }),
    );
    expect(invokeMock).toHaveBeenCalledWith("set_item_recurrence", {
      id: "itm-1",
      rule: "FREQ=WEEKLY",
    });
  });

  it("the popover lives inside the strip-menu-wrap (the structural fix from 1912e5b)", async () => {
    seed(makeActiveA());
    const { container } = render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));

    // The popover must be a child of the menu-wrap, not portaled
    // elsewhere. The 1912e5b fix kept it as a child but relaxed bay
    // overflow; a future refactor that portals the popover into a
    // different ancestor would invalidate the visual contract this
    // test is pinning. If you portal it on purpose, update this
    // assertion to match the new ancestor.
    const wrap = container.querySelector(".strip-menu-wrap");
    expect(wrap).not.toBeNull();
    const popover = container.querySelector(".strip-menu-popover");
    expect(popover).not.toBeNull();
    expect(wrap?.contains(popover!)).toBe(true);
  });

  it("toggles 'Mark done' ↔ 'Mark active' based on item state", async () => {
    const doneItem: Item = { ...makeActiveA(), state: "done" };
    seed(doneItem);
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    expect(
      screen.getByRole("menuitem", { name: "Mark active" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("menuitem", { name: "Mark done" }),
    ).not.toBeInTheDocument();
  });

  it("toggles 'Mark blocked…' ↔ 'Unblock' based on item state", async () => {
    const blocked: Item = {
      ...makeActiveA(),
      state: "blocked",
      blocked_reason: "waiting",
    };
    seed(blocked);
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    expect(screen.getByRole("menuitem", { name: "Unblock" })).toBeInTheDocument();
    expect(
      screen.queryByRole("menuitem", { name: "Mark blocked…" }),
    ).not.toBeInTheDocument();
  });

  it("clicking a menu item closes the popover", async () => {
    seed(makeActiveA());
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("menuitem", { name: "Edit" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("clicking outside the strip closes the popover", async () => {
    seed(makeActiveA());
    render(
      <div>
        <Strip itemId="itm-1" />
        <div data-testid="outside">click target</div>
      </div>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    await userEvent.click(screen.getByTestId("outside"));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("Delete fires invoke('delete_item') with the item id", async () => {
    seed(makeActiveA());
    invokeMock.mockResolvedValueOnce(undefined);
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));

    expect(invokeMock).toHaveBeenCalledWith("delete_item", { id: "itm-1" });
  });
});
