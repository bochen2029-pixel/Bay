// Pins the LAN-capture toast: appears when a phone submission lands,
// auto-dismisses after the visibility window, and truncates long
// content so the toast never grows unbounded.
//
// The wiring path lives in TauriEventBridge — backend emits
// `lan_capture_received` → bridge calls setLanCaptureFlash. This test
// drives setLanCaptureFlash directly to keep the unit narrow; the
// listener-side wiring is exercised end-to-end at runtime.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, act } from "@testing-library/react";

import { LanCaptureToast } from "./LanCaptureToast";
import { useStore } from "../store";

describe("LanCaptureToast", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useStore.setState({ lanCaptureFlash: null });
  });

  afterEach(() => {
    vi.useRealTimers();
    useStore.setState({ lanCaptureFlash: null });
  });

  it("renders nothing when no capture has fired", () => {
    const { container } = render(<LanCaptureToast />);
    expect(container.firstChild).toBeNull();
  });

  it("renders the captured content when a flash is set", () => {
    useStore.setState({
      lanCaptureFlash: { content: "phone capture", ts: Date.now() },
    });
    render(<LanCaptureToast />);
    expect(screen.getByText(/Captured to Inbox/)).toBeInTheDocument();
    expect(screen.getByText(/phone capture/)).toBeInTheDocument();
  });

  it("auto-dismisses after the visibility window", () => {
    useStore.setState({
      lanCaptureFlash: { content: "phone capture", ts: Date.now() },
    });
    render(<LanCaptureToast />);
    expect(screen.getByText(/Captured to Inbox/)).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(4000);
    });

    expect(screen.queryByText(/Captured to Inbox/)).not.toBeInTheDocument();
    expect(useStore.getState().lanCaptureFlash).toBeNull();
  });

  it("truncates content longer than the preview limit", () => {
    const long = "x".repeat(500);
    useStore.setState({
      lanCaptureFlash: { content: long, ts: Date.now() },
    });
    render(<LanCaptureToast />);
    // Content should be truncated to 60 chars + ellipsis (per the
    // MAX_PREVIEW_CHARS constant in LanCaptureToast.tsx).
    const node = screen.getByText(/^"x{1,80}…"$/);
    expect(node.textContent).toMatch(/^"x{60}…"$/);
  });

  it("a fresh capture resets the timer", () => {
    useStore.setState({
      lanCaptureFlash: { content: "first", ts: 1 },
    });
    const { rerender } = render(<LanCaptureToast />);

    act(() => {
      vi.advanceTimersByTime(2000);
    });
    // First capture's timer is at 2000ms — still visible.
    expect(screen.getByText(/first/)).toBeInTheDocument();

    // Second capture comes in (ts changes; useEffect re-runs and
    // resets the dismiss timer).
    act(() => {
      useStore.setState({
        lanCaptureFlash: { content: "second", ts: 2 },
      });
    });
    rerender(<LanCaptureToast />);
    expect(screen.getByText(/second/)).toBeInTheDocument();

    // Advance another 2000ms (first capture would have been dismissed
    // by now; second's timer started at 2000 and is still under
    // window).
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(screen.getByText(/second/)).toBeInTheDocument();
  });
});
