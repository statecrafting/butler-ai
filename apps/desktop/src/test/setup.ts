// Spec: specs/012-overlay-ui/spec.md

/**
 * Vitest setup (spec 012 §3.1).
 *
 * The Tauri API is stubbed here rather than in each test. There is no Tauri
 * runtime under jsdom, so `invoke` and `listen` would reject; stubbing them
 * once keeps every component test about the component. The stub also gives
 * the tests a handle on the event stream, which is how a `UiEvent` is
 * delivered without a Rust process.
 */

import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

/** Listeners registered through the stubbed `listen`, by channel. */
const listeners = new Map<string, ((message: { payload: unknown }) => void)[]>();

/** Deliver a payload to everything listening on a channel. */
export function emit(channel: string, payload: unknown): void {
  for (const listener of listeners.get(channel) ?? []) {
    listener({ payload });
  }
}

/** Forget every listener. Called between tests. */
export function resetListeners(): void {
  listeners.clear();
}

/** Command invocations recorded by the stub, in order. */
export const invocations: { command: string; args: unknown }[] = [];

vi.mock("@tauri-apps/api/event", () => ({
  listen: (channel: string, handler: (message: { payload: unknown }) => void) => {
    const existing = listeners.get(channel) ?? [];
    existing.push(handler);
    listeners.set(channel, existing);
    return Promise.resolve(() => {
      listeners.set(
        channel,
        (listeners.get(channel) ?? []).filter((h) => h !== handler),
      );
    });
  },
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args: unknown) => {
    invocations.push({ command, args });
    // The version the bindings were generated against, so the startup check
    // in `App` agrees with itself unless a test says otherwise.
    if (command === "get_contract_version") {
      return Promise.resolve([1, 0]);
    }
    return Promise.resolve(null);
  },
  Channel: class {},
}));
