// Spec: specs/012-overlay-ui/spec.md

/**
 * FR-001: with the app idle, `html`, `body` and `#root` compute to a fully
 * transparent background.
 *
 * This is the single property the whole product rests on visually. The window
 * behind the DOM is transparent (spec 004 §3.2) and the compositor shows the
 * user's screen through it; one opaque rule here turns the overlay into a
 * rectangle covering their work.
 */

import { beforeEach, describe, expect, it } from "vitest";

import "../styles/base.css";

/** jsdom normalizes `transparent` to this. */
const TRANSPARENT = "rgba(0, 0, 0, 0)";

describe("FR-001: the overlay paints no background", () => {
  beforeEach(() => {
    document.body.innerHTML = '<div id="root"></div>';
  });

  it("leaves html, body and #root fully transparent", () => {
    const root = document.getElementById("root");
    expect(root).not.toBeNull();

    for (const element of [
      document.documentElement,
      document.body,
      root as Element,
    ]) {
      const background = getComputedStyle(element).backgroundColor;
      expect(
        background === "" || background === TRANSPARENT || background === "transparent",
      ).toBe(true);
    }
  });

  it("hides overflow, so nothing can produce a scrollbar over the screen", () => {
    for (const element of [document.documentElement, document.body]) {
      expect(getComputedStyle(element).overflow).toBe("hidden");
    }
  });

  it("leaves the surface inert until a component opts in (FR-002)", () => {
    expect(getComputedStyle(document.body).pointerEvents).toBe("none");
  });
});
