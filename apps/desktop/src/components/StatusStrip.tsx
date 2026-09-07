// Spec: specs/012-overlay-ui/spec.md

/**
 * The 24 px status strip (spec 012 §3.3).
 *
 * State glyph and word, exclusion glyph, and the degraded banner. It is the
 * only thing on screen when the overlay is idle, so it says what the product
 * is doing in one glance and never overstates it (constitution §VI).
 */

import { Show } from "solid-js";

import { state } from "../state/runtime";

/** The word and glyph for each state name in the contract. */
function stateLabel(name: string): { glyph: string; word: string } {
  switch (name) {
    case "armed":
      return { glyph: "●", word: "Armed" };
    case "degraded":
      return { glyph: "◎", word: "Degraded" };
    case "fault":
      return { glyph: "▲", word: "Retrying" };
    default:
      return { glyph: "○", word: "Disarmed" };
  }
}

/**
 * The exclusion glyph. Five states, not two (constitution §VI): "asked for
 * and not confirmed" is a different thing from "confirmed", and the strip
 * says which.
 */
function exclusionLabel(exclusion: string): string {
  switch (exclusion) {
    case "verified":
      return "hidden from capture";
    case "applied":
      return "exclusion requested, not verified";
    case "compromised":
      return "exclusion failed";
    case "unsupported":
      return "exclusion unsupported here";
    default:
      return "exclusion unknown";
  }
}

export function StatusStrip() {
  return (
    <Show when={state.status} fallback={<div class="plate dim">Starting</div>}>
      {(status) => (
        <div class="plate" style={{ "min-height": "24px" }}>
          <span aria-hidden="true">{stateLabel(status().state).glyph} </span>
          <span>{stateLabel(status().state).word}</span>
          <span class="dim"> {exclusionLabel(status().exclusion)}</span>
          {/*
            FR-003. The text is fixed by §3.3 and the banner cannot be
            dismissed: it is rendered from `status`, which only the runtime
            sets, so `Dismiss` cannot reach it.
          */}
          <Show when={status().state === "degraded"}>
            <div
              role="alert"
              style={{
                background: "var(--butler-danger)",
                color: "var(--butler-danger-ink)",
                "border-radius": "6px",
                padding: "2px 8px",
                "margin-top": "4px",
                "font-weight": "600",
              }}
            >
              VISIBLE TO SCREEN SHARING: {exclusionLabel(status().exclusion)}
            </div>
          </Show>
        </div>
      )}
    </Show>
  );
}
