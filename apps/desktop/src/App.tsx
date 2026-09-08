// Spec: specs/012-overlay-ui/spec.md

/**
 * The overlay's root (spec 012 §3.3).
 *
 * Composition and one lifecycle, nothing else. Every decision belongs to the
 * runtime and reaches here as an event; §1 calls this a thin, honest
 * renderer, and the way to keep it one is to give it nothing to decide.
 *
 * `SettingsPanel` (spec 014) is added to this file by its own spec, through
 * an `extends` edge onto it. `PacedAnswer` (spec 013) is mounted inside
 * `AnswerPanel` rather than here: spec 012 §3.3 gives the answer panel the
 * job of hosting it, and the panel already renders the lifecycle around it.
 */

import { createSignal, onCleanup, onMount, Show } from "solid-js";

import { AnswerPanel } from "./components/AnswerPanel";
import { CredentialPanel } from "./components/CredentialPanel";
import { FatalPanel } from "./components/FatalPanel";
import { NotePrompt } from "./components/NotePrompt";
import { OnboardingPanel } from "./components/OnboardingPanel";
import { Sentinel } from "./components/Sentinel";
import { SettingsPanel } from "./components/SettingsPanel";
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
      {/*
        Spec 014 §3.4. Opened from the tray's "Settings..." item, which spec
        004's `on_menu_event` will route once there is an IPC path for it;
        until then the panel is mounted and closed, so its bindings and its
        typechecking are live rather than dead code.
      */}
      <SettingsPanel open={false} />
      <Sentinel />
    </Show>
  );
}
