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

import type { Settings, UiEvent } from "../ipc/client";

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
  /** The answer so far: every chunk released in `index` order, joined. */
  answer: string;
  /**
   * The released chunks in `index` order (spec 013 §3.3).
   *
   * Kept alongside `answer` because the panel fades each chunk in
   * individually, which needs the seams; `answer` is the same text joined,
   * for everything that only wants to read it.
   */
  chunks: string[];
  /**
   * Whether the chunk carrying `is_last` has been applied (spec 013 §3.3).
   *
   * The caret is on until this is true. Distinct from `phase === "done"`:
   * `AnswerDone` says the provider stopped, while this says the pacer
   * finished releasing what it stopped with, and at a reading pace there are
   * seconds between the two.
   */
  answerComplete: boolean;
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
  /** The current configuration, once the process has sent it (spec 014). */
  settings: Settings | null;
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
    chunks: [],
    answerComplete: false,
    stop: null,
    error: null,
    needsPermission: null,
    needsCredential: null,
    selfTest: null,
    settings: null,
    sentinel: false,
  };
}

const [state, setState] = createStore<RuntimeState>(blank());

export { state };

/**
 * Chunks that arrived before the one in front of them (spec 013 §3.3).
 *
 * Keyed by `index`. Tauri delivers in order, so this is normally empty; the
 * contract does not promise it, and rendering "world hello" once would be
 * worse than the code that prevents it.
 *
 * Outside the store on purpose: nothing renders from it, and a Solid store
 * exists to be subscribed to.
 */
let pending = new Map<number, { text: string; is_last: boolean }>();

/** The `index` the next chunk must carry to be rendered. */
let nextIndex = 0;

/** Forget the buffered chunks. Called wherever the answer resets. */
function resetChunks(): void {
  pending = new Map();
  nextIndex = 0;
}

/**
 * Fold one event into the store (spec 012 §3.4).
 *
 * The `default` arm assigns the event to `never`, which is what actually
 * makes the compiler check every case. Spec 012 first relied on a `switch`
 * with no `default` and a `void` return, which does **not** error on an
 * unhandled variant: TypeScript simply falls through. Adding
 * `UiEvent::SettingsUpdated` typechecked cleanly against a store that ignored
 * it, which is the exact silence the generated union exists to prevent
 * (spec 014 D-3).
 *
 * With the assignment in place a variant added by spec 013 or 010 fails to
 * compile here until it is handled.
 *
 * Spec 013 §3.3 attributes the out-of-order buffer to `PacedAnswer`. It is
 * here instead, because §3.4 gives the overlay exactly one event
 * subscription and forbids a component from opening its own: a component
 * cannot buffer an event it never sees. The behaviour §3.3 requires is
 * unchanged, and in the store it is testable without a DOM (spec 013 D-3).
 */
export function apply(event: UiEvent): void {
  switch (event.type) {
    case "runtime-status":
      setState({ status: event });
      return;
    case "answer-started":
      resetChunks();
      setState({
        request: event.request,
        phase: "streaming",
        answer: "",
        chunks: [],
        answerComplete: false,
        stop: null,
        error: null,
      });
      return;
    case "answer-chunk": {
      // §3.2: a chunk belongs to one inference. An answer superseded
      // mid-stream must not bleed into its replacement, and the request id is
      // what says so. Before `AnswerStarted` there is nothing to compare
      // against, so the first chunk seen adopts its request.
      if (state.request !== null && event.request !== state.request) {
        return;
      }
      pending.set(event.index, { text: event.text, is_last: event.is_last });

      // Release the contiguous run starting at the next expected index.
      // Anything past a gap waits for the chunk that fills it.
      const ready: string[] = [];
      let complete = false;
      for (;;) {
        const next = pending.get(nextIndex);
        if (next === undefined) {
          break;
        }
        pending.delete(nextIndex);
        nextIndex += 1;
        ready.push(next.text);
        complete ||= next.is_last;
      }
      if (ready.length === 0) {
        return;
      }

      const chunks = [...state.chunks, ...ready];
      setState({
        request: event.request,
        chunks,
        answer: chunks.join(" "),
        answerComplete: complete,
      });
      return;
    }
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
    case "settings-updated":
      setState({ settings: event.settings });
      return;
    case "self-test-sentinel":
      // Spec 005 §3.4: the runtime asks for the pattern, captures the screen,
      // and asks for it to go away again. Solid writes the DOM synchronously,
      // so the only wait left on the Rust side is paint.
      setState({ sentinel: event.on });
      return;
    default: {
      // Exhaustiveness. If a new `UiEvent` variant reaches here, `event` is
      // no longer `never` and this assignment fails to compile.
      const unhandled: never = event;
      throw new Error(
        `spec 011 contract drift: unhandled UiEvent ${JSON.stringify(unhandled)}`,
      );
    }
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
  resetChunks();
  setState({
    request: null,
    phase: "idle",
    answer: "",
    chunks: [],
    answerComplete: false,
    stop: null,
    error: null,
  });
}

/** Reset to the initial state. Tests only; the process never does this. */
export function resetForTest(): void {
  resetChunks();
  setState(blank());
}
