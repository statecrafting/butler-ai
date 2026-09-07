// Spec: specs/012-overlay-ui/spec.md

/**
 * The answer panel (spec 012 §3.3).
 *
 * `aria-live="polite"` per §3.5, so a screen reader announces an answer
 * without interrupting what the user is doing.
 *
 * Spec 013 will host `PacedAnswer` here and feed `state.answer` from paced
 * chunks. Until then the panel renders the buffer as it is, which is empty:
 * the `AnswerChunk` event is not in the contract yet (spec 011 D-3), so there
 * is nothing to append. The lifecycle around it (started, done, failed) is in
 * the contract and is rendered.
 */

import { Match, Switch } from "solid-js";

import { state } from "../state/runtime";

/** §3.3 fixes these two strings. */
const DECLINED = "declined";
const NOTHING = "nothing to add";

export function AnswerPanel() {
  return (
    <div class="plate" aria-live="polite">
      <Switch fallback={<span class="dim">{NOTHING}</span>}>
        <Match when={state.phase === "failed"}>
          <span class="dim">Could not answer ({state.error})</span>
        </Match>
        <Match when={state.phase === "done" && state.stop === "refusal"}>
          <span class="dim">{DECLINED}</span>
        </Match>
        <Match when={state.phase === "streaming" && state.answer === ""}>
          <span class="dim">Thinking</span>
        </Match>
        <Match when={state.answer !== ""}>
          <span>{state.answer}</span>
        </Match>
      </Switch>
    </div>
  );
}
