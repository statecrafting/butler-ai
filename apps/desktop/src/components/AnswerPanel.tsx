// Spec: specs/012-overlay-ui/spec.md

/**
 * The answer panel (spec 012 §3.3).
 *
 * `aria-live="polite"` per §3.5, so a screen reader announces an answer
 * without interrupting what the user is doing.
 *
 * Spec 013 hosts `PacedAnswer` here: the answer text itself is rendered chunk
 * by chunk at a reading pace, and this panel is the lifecycle around it
 * (started, done, failed, refused).
 */

import { Match, Switch } from "solid-js";

import { PacedAnswer } from "./PacedAnswer";
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
        <Match when={state.phase === "streaming" && state.chunks.length === 0}>
          <span class="dim">Thinking</span>
        </Match>
        <Match when={state.chunks.length > 0}>
          <PacedAnswer />
        </Match>
      </Switch>
    </div>
  );
}
