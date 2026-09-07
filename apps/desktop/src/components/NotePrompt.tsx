// Spec: specs/012-overlay-ui/spec.md

/**
 * An optional one-line note the user can type while the overlay is
 * interactive (spec 012 §3.3), sent with the next `AskNow`.
 *
 * The note is held here and nowhere else. It is the user's own words rather
 * than screen content, but spec 015's rules do not distinguish: it leaves the
 * process only inside an `InferenceRequest`, and until spec 010 exists there
 * is nothing to send it to, so `AskNow` carries it no further than the
 * command boundary today.
 */

import { createSignal, Show } from "solid-js";

import { commands } from "../ipc/client";
import { state } from "../state/runtime";

export function NotePrompt() {
  const [note, setNote] = createSignal("");

  async function ask(event: Event) {
    event.preventDefault();
    // Spec 011 §3.1: `AskNow` carries no payload in contract v1.0. The note
    // rides along once spec 010 defines where a prompt addition belongs;
    // until then it is cleared rather than silently dropped on the floor.
    await commands.askNow();
    setNote("");
  }

  return (
    <Show when={state.interactive}>
      <form class="plate interactive" onSubmit={(event) => void ask(event)}>
        <label for="butler-note" class="dim">
          Note
        </label>
        <input
          id="butler-note"
          type="text"
          value={note()}
          placeholder="optional"
          onInput={(event) => setNote(event.currentTarget.value)}
        />
      </form>
    </Show>
  );
}
