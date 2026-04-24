// Mount-once singleton that bridges Tauri backend events into the
// frontend store. Subscriptions live for the lifetime of the component;
// listeners are only added for events whose Rust emitters already exist.
// I-04 subscribes to `item_created` only; other events join as their
// commands land (item_updated in I-06, item_deleted in I-08, etc.).

import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

import { Item } from "../domain";
import { useStore } from "../store";

export function TauriEventBridge() {
  const onItemCreated = useStore((s) => s.onItemCreated);

  useEffect(() => {
    // `listen` resolves asynchronously; guard against unmount before
    // subscription is ready so we never leak a live listener.
    let unlistenFn: (() => void) | undefined;
    let cancelled = false;

    listen<unknown>("item_created", (event) => {
      try {
        const item = Item.parse(event.payload);
        onItemCreated(item);
      } catch (err) {
        console.error("item_created: payload failed Zod parse", err);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenFn = fn;
    });

    return () => {
      cancelled = true;
      unlistenFn?.();
    };
  }, [onItemCreated]);

  return null;
}
