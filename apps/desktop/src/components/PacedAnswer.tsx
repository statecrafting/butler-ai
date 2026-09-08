// Spec: specs/013-output-pacing/spec.md

/**
 * The paced answer (spec 013 §3.3).
 *
 * A passive renderer, and deliberately a dumb one. Every question about
 * *when* a word appears was answered in `butler_core::pacing` before the
 * chunk was emitted, so this file schedules nothing and has nothing to get
 * out of step with the state machine. §1 is explicit that the policy lives in
 * core so the machine knows when rendering is complete; a component that also
 * had an opinion about timing would make that claim false.
 *
 * Spec 013 §8 greps this file for the browser timer functions. Naming one
 * here would make that check pass or fail on this comment rather than on the
 * code (spec 016 D-2).
 *
 * What is left is presentation: each chunk fades in over 120 ms, and a caret
 * sits after the last one until the answer is complete.
 *
 * The `index`-order buffer §3.3 describes is in `state/runtime.ts`. Spec 012
 * §3.4 gives the overlay one event subscription and forbids a component from
 * opening its own, so a component cannot buffer an event it never sees
 * (spec 013 D-3).
 */

import { For, Show } from "solid-js";

import { state } from "../state/runtime";

export function PacedAnswer() {
  return (
    <span class="paced">
      <For each={state.chunks}>
        {(chunk, index) => (
          <>
            {/*
              A space between chunks, not inside them: a release carries whole
              words with no trailing space (spec 013 §3.1), so the seam is the
              renderer's to draw. HTML collapses it against any whitespace an
              unpaced chunk brought with it.
            */}
            <Show when={index() > 0}>{" "}</Show>
            <span class="chunk">{chunk}</span>
          </>
        )}
      </For>
      {/*
        §3.3: the caret is on until the chunk carrying `is_last` lands. Not
        until `AnswerDone`, which says the provider stopped talking, and at a
        reading pace arrives long before the reader has caught up.
      */}
      <Show when={!state.answerComplete}>
        <span class="caret" aria-hidden="true" />
      </Show>
    </span>
  );
}
