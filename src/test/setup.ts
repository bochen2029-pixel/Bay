// Vitest setup. Loaded once per test file before any user code runs.
//
// Two responsibilities:
//   1. Register @testing-library/jest-dom's custom matchers
//      (toBeInTheDocument, toBeDisabled, toHaveTextContent, etc.).
//   2. Polyfill the parts of the DOM that jsdom does not implement,
//      specifically <dialog>'s showModal/close/cancel APIs. Our modal
//      components call these directly (not via a portal hack), so
//      tests would otherwise throw "showModal is not a function".
//
// Global Zustand store reset between tests is each test file's job —
// the store is a singleton and leaking state across tests is the
// most common source of order-dependent failures. Use
// `useStore.setState(initialStateSnapshot, true)` in beforeEach.

import "@testing-library/jest-dom/vitest";

import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

// jsdom (as of v29) does not implement HTMLDialogElement's modal API.
// The polyfill below mirrors the behaviour our components rely on:
// showModal()/show() set `open=true`; close() sets `open=false` and
// fires a `close` event; pressing Escape on an open dialog fires
// `cancel` and (unless prevented) `close`.
if (typeof HTMLDialogElement !== "undefined") {
  const proto = HTMLDialogElement.prototype as HTMLDialogElement & {
    showModal?: () => void;
    show?: () => void;
    close?: (returnValue?: string) => void;
  };
  if (typeof proto.showModal !== "function") {
    proto.showModal = function showModal(this: HTMLDialogElement) {
      this.setAttribute("open", "");
      // jsdom doesn't expose `open` as a real property; mirror it.
      Object.defineProperty(this, "open", {
        configurable: true,
        get: () => this.hasAttribute("open"),
      });
    };
  }
  if (typeof proto.show !== "function") {
    proto.show = function show(this: HTMLDialogElement) {
      this.setAttribute("open", "");
    };
  }
  if (typeof proto.close !== "function") {
    proto.close = function close(this: HTMLDialogElement, returnValue?: string) {
      this.removeAttribute("open");
      if (returnValue !== undefined) {
        (this as unknown as { returnValue: string }).returnValue = returnValue;
      }
      this.dispatchEvent(new Event("close"));
    };
  }
}

// React Testing Library cleans up rendered trees automatically after
// each test, which prevents one test's DOM from leaking into the
// next.
afterEach(() => {
  cleanup();
});
