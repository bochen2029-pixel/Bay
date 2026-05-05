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

  it("opens the popover with the six menu items on ⋯ click", async () => {
    seed(makeActiveA());
    render(<Strip itemId="itm-1" />);
    await userEvent.click(screen.getByRole("button", { name: "Item menu" }));

    const menu = screen.getByRole("menu");
    expect(menu).toBeInTheDocument();
    const items = screen.getAllByRole("menuitem");
    expect(items).toHaveLength(6);
    expect(items.map((b) => b.textContent)).toEqual([
      "Edit",
      "Set start date…",
      "Set due date…",
      "Mark done",
      "Mark blocked…",
      "Delete",
    ]);
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
