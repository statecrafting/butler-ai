// Spec: specs/012-overlay-ui/spec.md

/**
 * The exclusion self-test pattern (spec 012 §3.3, spec 005).
 *
 * Mounted only while the self-test runs. The test paints this, captures the
 * monitor, and asserts the pattern is **absent** from the capture: if it
 * shows up, the overlay is not excluded and the runtime goes `Degraded`.
 *
 * Two properties matter and both are deliberate. It is full-window, which is
 * the one exception to §3.2's "never full-window" rule and exists because a
 * capture has to be searched for it. And it is opaque and high-contrast,
 * because a translucent pattern over an arbitrary wallpaper is not reliably
 * findable in a downscaled screenshot.
 *
 * Solid writes to the DOM synchronously, so once the signal is set the only
 * remaining wait before capturing is paint, two `requestAnimationFrame`s
 * (docs/architecture.md D2). That is why this component is Solid's rather
 * than React's.
 */

import { Show } from "solid-js";

import { state } from "../state/runtime";

/** The pattern's colours, fixed so the native side can look for them. */
export const SENTINEL_COLORS = ["#ff00ff", "#00ff00"] as const;

/** The data attribute the self-test uses to confirm the mount. */
export const SENTINEL_TEST_ID = "butler-sentinel";

export function Sentinel() {
  return (
    <Show when={state.sentinel}>
      <div
        data-testid={SENTINEL_TEST_ID}
        aria-hidden="true"
        style={{
          position: "fixed",
          inset: "0",
          "pointer-events": "none",
          background: `repeating-linear-gradient(45deg, ${SENTINEL_COLORS[0]} 0 24px, ${SENTINEL_COLORS[1]} 24px 48px)`,
        }}
      />
    </Show>
  );
}
