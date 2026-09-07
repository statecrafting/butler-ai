// Spec: specs/012-overlay-ui/spec.md

/**
 * The overlay's root (spec 012 §3.3).
 *
 * Composition and one lifecycle, nothing else. Every decision belongs to the
 * runtime and reaches here as an event; §1 calls this a thin, honest
 * renderer, and the way to keep it one is to give it nothing to decide.
 *
 * `SettingsPanel` (spec 014) and `PacedAnswer` (spec 013) are added to this
 * file by their own specs, through `extends` edges onto it.
 */

import { createSignal, onCleanup, onMount, Show } from "solid-js";

import { AnswerPanel } from "./components/AnswerPanel";
import { CredentialPanel } from "./components/CredentialPanel";
import { FatalPanel } from "./components/FatalPanel";
import { NotePrompt } from "./components/NotePrompt";
import { OnboardingPanel } from "./components/OnboardingPanel";
import { Sentinel } from "./components/Sentinel";
import { StatusStrip } from "./components/StatusStrip";
import { contractMatches, IPC_CONTRACT_VERSION } from "./ipc/client";
import { connect } from "./state/ipc";

export function App() {
  const [mismatch, setMismatch] = createSignal(false);

  onMount(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    void (async () => {
      // §3.4: one subscription for the whole overlay.
      const stop = await connect();
      if (cancelled) {
        stop();
        return;
      }
      unlisten = stop;
    })();

    // Spec 011 §3.4: compare the major at startup. A failure to reach the
    // process is not a mismatch; it is a process that has not finished
    // starting, and claiming "rebuild required" for that would be a lie in
    // the one panel whose whole job is to be believed.
    void contractMatches()
      .then((matches) => setMismatch(!matches))
      .catch(() => setMismatch(false));

    onCleanup(() => {
      cancelled = true;
      unlisten?.();
    });
  });

  return (
    <Show
      when={!mismatch()}
      fallback={<FatalPanel running={IPC_CONTRACT_VERSION.join(".")} />}
    >
      <StatusStrip />
      <OnboardingPanel />
      <CredentialPanel />
      <AnswerPanel />
      <NotePrompt />
      <Sentinel />
    </Show>
  );
}
