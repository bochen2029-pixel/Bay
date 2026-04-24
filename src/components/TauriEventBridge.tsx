// Mount-once singleton that bridges Tauri backend events into the
// frontend store. Subscriptions live for the lifetime of the component;
// listeners are only added for events whose Rust emitters already exist.
// I-04 wired `item_created`; I-05 adds `quick_capture_requested` and
// `backend_warning`. Others join as their commands land.

import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";

import { Item } from "../domain";
import { BackendWarning, useStore } from "../store";

const BackendWarningSchema: z.ZodType<BackendWarning> = z.object({
  kind: z.string(),
  message: z.string(),
});

const DeletedIdSchema = z.object({ id: z.string() });

export function TauriEventBridge() {
  const onItemCreated = useStore((s) => s.onItemCreated);
  const onItemUpdated = useStore((s) => s.onItemUpdated);
  const onItemDeleted = useStore((s) => s.onItemDeleted);
  const openQuickCapture = useStore((s) => s.openQuickCapture);
  const setBackendWarning = useStore((s) => s.setBackendWarning);

  useEffect(() => {
    // `listen` resolves asynchronously; guard against unmount before
    // subscriptions are ready so we never leak live listeners.
    const unlisteners: Array<() => void> = [];
    let cancelled = false;

    function track(promise: Promise<() => void>) {
      promise.then((fn) => {
        if (cancelled) fn();
        else unlisteners.push(fn);
      });
    }

    track(
      listen<unknown>("item_created", (event) => {
        try {
          const item = Item.parse(event.payload);
          onItemCreated(item);
        } catch (err) {
          console.error("item_created: payload failed Zod parse", err);
        }
      }),
    );

    track(
      listen<unknown>("item_updated", (event) => {
        try {
          const item = Item.parse(event.payload);
          onItemUpdated(item);
        } catch (err) {
          console.error("item_updated: payload failed Zod parse", err);
        }
      }),
    );

    track(
      listen<unknown>("item_deleted", (event) => {
        try {
          const { id } = DeletedIdSchema.parse(event.payload);
          onItemDeleted(id);
        } catch (err) {
          console.error("item_deleted: payload failed Zod parse", err);
        }
      }),
    );

    track(
      listen<unknown>("quick_capture_requested", () => {
        openQuickCapture();
      }),
    );

    track(
      listen<unknown>("backend_warning", (event) => {
        try {
          const w = BackendWarningSchema.parse(event.payload);
          setBackendWarning(w);
        } catch (err) {
          console.error("backend_warning: payload failed Zod parse", err);
        }
      }),
    );

    return () => {
      cancelled = true;
      for (const fn of unlisteners) fn();
    };
  }, [
    onItemCreated,
    onItemUpdated,
    onItemDeleted,
    openQuickCapture,
    setBackendWarning,
  ]);

  return null;
}
