// Spec: specs/012-overlay-ui/spec.md

/**
 * Provider picker and masked key input (spec 012 §3.3).
 *
 * Shown when the process reports `NeedsCredential`. Submits `StoreSecret`,
 * which spec 011 §3.2 requires the Rust side to move into a `Secret` and
 * never log. This file's own duty is smaller and just as strict: the value is
 * held in a local signal, sent once, and cleared, and the input is
 * `type="password"` with autocomplete off so the webview does not offer to
 * remember it.
 */

import { createSignal, Show } from "solid-js";

import { commands } from "../ipc/client";
import { state } from "../state/runtime";

export function CredentialPanel() {
  const [key, setKey] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [failed, setFailed] = createSignal<string | null>(null);

  async function submit(provider: string, event: Event) {
    event.preventDefault();
    setBusy(true);
    setFailed(null);
    const result = await commands.storeSecret(provider, key());
    // Cleared whether or not the store succeeded: a retry retypes it. Keeping
    // it "for convenience" would leave the credential in the webview's heap
    // for the rest of the session.
    setKey("");
    setBusy(false);
    if (result.status === "error") {
      setFailed(result.error);
    }
  }

  return (
    <Show when={state.needsCredential}>
      {(provider) => (
        <form
          class="plate"
          classList={{ interactive: state.interactive }}
          onSubmit={(event) => void submit(provider(), event)}
          aria-label="Provider credential"
        >
          <label for="butler-key">Key for {provider()}</label>
          <input
            id="butler-key"
            type="password"
            autocomplete="off"
            spellcheck={false}
            value={key()}
            onInput={(event) => setKey(event.currentTarget.value)}
          />
          <button type="submit" disabled={busy() || key() === ""}>
            Save to keychain
          </button>
          <Show when={failed()}>
            {(error) => (
              <span class="dim" role="alert">
                {" "}
                Not saved ({error()})
              </span>
            )}
          </Show>
        </form>
      )}
    </Show>
  );
}
