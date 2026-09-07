// Spec: specs/012-overlay-ui/spec.md

/**
 * Why Screen Recording is needed (spec 012 §3.3, spec 004 §3.5).
 *
 * Shown when the process reports `NeedsPermission`. One button, and it opens
 * System Settings rather than re-prompting: macOS shows its own dialog once
 * per binary, and spec 004 §3.5 forbids loop-prompting after that.
 */

import { Show } from "solid-js";

import { state } from "../state/runtime";

export function OnboardingPanel() {
  return (
    <Show when={state.needsPermission}>
      {(permission) => (
        <div
          class="plate"
          classList={{ interactive: state.interactive }}
          role="dialog"
          aria-label="Screen recording permission"
        >
          <p style={{ margin: "0 0 6px" }}>
            Butler reads the screen on this Mac to answer questions about it.
            macOS requires the Screen Recording permission for that, and it is
            not granted yet.
          </p>
          <p class="dim" style={{ margin: "0 0 6px" }}>
            Nothing is captured while the permission is missing, and the
            pipeline stays disarmed. Required permission: {permission()}.
          </p>
          <a
            href="x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            classList={{ interactive: state.interactive }}
          >
            Open System Settings
          </a>
        </div>
      )}
    </Show>
  );
}
