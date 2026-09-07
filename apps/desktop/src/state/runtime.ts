// Spec: specs/012-overlay-ui/spec.md

/**
 * The overlay's whole state (spec 012 §3.4).
 *
 * One Solid store mirroring the last `RuntimeStatus`, plus the answer buffer.
 * The UI holds no pipeline state of its own and makes no decisions: spec 012
 * §1 calls it a thin, honest renderer, and this file is where that is either
 * true or not. Every field here arrives from an event; nothing is inferred.
 */

import { createStore } from "solid-js/store";

import type { UiEvent } from "../ipc/client";

/** The status event, named once so components do not restate the union. */
export type RuntimeStatus = Extract<UiEvent, { type: "runtime-status" }>;

/** What the answer panel is currently showing. */
export type AnswerPhase = "idle" | "streaming" | "done" | "failed";

/** The store's shape. */
export interface RuntimeState {
  /** The last status the runtime reported, or `null` before the first one. */
  status: RuntimeStatus | null;
  /** Whether the overlay currently accepts clicks (mirrors the shell). */
  interactive: boolean;
  /** The request the answer belongs to. */
  request: number | null;
  /** Where the answer is in its lifecycle. */
  phase: AnswerPhase;
  /** The answer so far. Spec 013 appends paced chunks to this. */
  answer: string;
  /** Why the answer stopped, when it stopped. */
  stop: string | null;
  /** The failure kind, when it failed. */
  error: string | null;
  /** A permission the process is waiting on, if any. */
  needsPermission: string | null;
  /** A provider with no usable credential, if any. */
  needsCredential: string | null;
  /** The most recent self-test verdict, if one has run. */
  selfTest: string | null;
  /** Whether the exclusion sentinel is mounted (spec 005 drives this). */
  sentinel: boolean;
}

function blank(): RuntimeState {
  return {
    status: null,
    interactive: false,
    request: null,
    phase: "idle",
    answer: "",
    stop: null,
    error: null,
    needsPermission: null,
    needsCredential: null,
    selfTest: null,
    sentinel: false,
  };
}

const [state, setState] = createStore<RuntimeState>(blank());

export { state };

/**
 * Fold one event into the store (spec 012 §3.4).
 *
 * `switch` over the tag with **no `default`**, and a return type that makes
 * the compiler check every arm: the generated union is exhaustive, so a
 * variant added by spec 013, 014 or 010 fails to typecheck here until it is
 * handled. That is the point of generating the union rather than writing it,
 * and it is why this does not quietly ignore what it does not recognize.
 */
export function apply(event: UiEvent): void {
  switch (event.type) {
    case "runtime-status":
      setState({ status: event });
      return;
    case "answer-started":
      setState({
        request: event.request,
        phase: "streaming",
        answer: "",
        stop: null,
        error: null,
      });
      return;
    case "answer-done":
      setState({ phase: "done", stop: event.stop });
      return;
    case "answer-failed":
      setState({ phase: "failed", error: event.kind });
      return;
    case "needs-credential":
      setState({ needsCredential: event.provider });
      return;
    case "needs-permission":
      setState({ needsPermission: event.permission });
      return;
    case "self-test-result":
      setState({ selfTest: event.verdict });
      return;
  }
}

/** Mirror the shell's click-through flag (§3.2). The shell is authoritative. */
export function setInteractive(interactive: boolean): void {
  setState({ interactive });
}

/** Mount or unmount the exclusion self-test pattern (spec 005). */
export function setSentinel(sentinel: boolean): void {
  setState({ sentinel });
}

/**
 * Clear the current answer (the `Dismiss` action, spec 004 §3.3).
 *
 * FR-003: this does **not** touch the degraded banner. The banner is a
 * property of `status`, which only the runtime sets, so a user cannot dismiss
 * their way out of being visible to screen sharing.
 */
export function dismiss(): void {
  setState({
    request: null,
    phase: "idle",
    answer: "",
    stop: null,
    error: null,
  });
}

/** Reset to the initial state. Tests only; the process never does this. */
export function resetForTest(): void {
  setState(blank());
}
