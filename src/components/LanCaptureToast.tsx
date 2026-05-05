// Phone-capture confirmation toast. Backend's LAN capture server
// emits `lan_capture_received` with the new Item; TauriEventBridge
// catches that and pushes the content into store.lanCaptureFlash.
// This component subscribes to that field and renders a transient
// banner so the desktop user sees that the phone-side submission
// landed without having to switch view to Inbox.
//
// Auto-dismisses after TOAST_WINDOW_MS. Resets the timer on every
// new capture (the `flash.ts` identity changes per capture).

import { useEffect } from "react";

import { useStore } from "../store";

const TOAST_WINDOW_MS = 3500;
const MAX_PREVIEW_CHARS = 60;

export function LanCaptureToast() {
  const flash = useStore((s) => s.lanCaptureFlash);
  const clear = useStore((s) => s.setLanCaptureFlash);

  useEffect(() => {
    if (!flash) return;
    const t = setTimeout(() => clear(null), TOAST_WINDOW_MS);
    return () => clearTimeout(t);
  }, [flash, clear]);

  if (!flash) return null;

  const preview =
    flash.content.length > MAX_PREVIEW_CHARS
      ? flash.content.slice(0, MAX_PREVIEW_CHARS) + "…"
      : flash.content;

  return (
    <div className="lan-capture-toast" role="status" aria-live="polite">
      <span className="lan-capture-toast-label">Captured to Inbox</span>
      <span className="lan-capture-toast-content">"{preview}"</span>
    </div>
  );
}
