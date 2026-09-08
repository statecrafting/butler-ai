// Spec: specs/012-overlay-ui/spec.md

/**
 * The store folds events and decides nothing (spec 012 §3.4).
 *
 * FR-003 lives here too: the degraded banner is a property of `status`, which
 * only the runtime sets, so `Dismiss` cannot reach it.
 */

import { beforeEach, describe, expect, it } from "vitest";

import type { UiEvent } from "../ipc/client";
import { DEFAULT_SETTINGS } from "../ipc/client";
import { apply, dismiss, resetForTest, state } from "../state/runtime";

const degraded: UiEvent = {
  type: "runtime-status",
  state: "degraded",
  seq: 4,
  request: null,
  exclusion: "compromised",
  last_error: null,
  armed_for_ticks: 12,
};

describe("the runtime store", () => {
  beforeEach(() => {
    resetForTest();
  });

  it("mirrors the last status", () => {
    apply(degraded);
    expect(state.status?.state).toBe("degraded");
    expect(state.status?.exclusion).toBe("compromised");
  });

  it("tracks the answer lifecycle", () => {
    apply({ type: "answer-started", request: 7 });
    expect(state.phase).toBe("streaming");
    expect(state.request).toBe(7);

    apply({ type: "answer-done", request: 7, stop: "end-turn" });
    expect(state.phase).toBe("done");
    expect(state.stop).toBe("end-turn");
  });

  it("records a failure as a kind, never a message", () => {
    apply({ type: "answer-failed", request: 7, kind: "network" });
    expect(state.phase).toBe("failed");
    expect(state.error).toBe("network");
  });

  it("records the configuration the process sent (spec 014)", () => {
    apply({ type: "settings-updated", settings: DEFAULT_SETTINGS });
    expect(state.settings?.capture.interval_ms).toBe(
      DEFAULT_SETTINGS.capture.interval_ms,
    );
    // Spec 015 §3.2 and spec 004 §3.2: the two defaults that make this the
    // product the constitution describes.
    expect(state.settings?.privacy.redaction_enabled).toBe(true);
    expect(state.settings?.privacy.allow_degraded_mode).toBe(false);
  });

  it("records what the process is waiting on", () => {
    apply({ type: "needs-permission", permission: "screen-recording" });
    apply({ type: "needs-credential", provider: "anthropic" });
    apply({ type: "self-test-result", verdict: "verified" });
    expect(state.needsPermission).toBe("screen-recording");
    expect(state.needsCredential).toBe("anthropic");
    expect(state.selfTest).toBe("verified");
  });

  /**
   * FR-003. A user must not be able to dismiss their way out of being
   * visible to screen sharing.
   */
  it("keeps the degraded status through a dismiss", () => {
    apply(degraded);
    apply({ type: "answer-started", request: 1 });

    dismiss();

    expect(state.phase).toBe("idle");
    expect(state.answer).toBe("");
    expect(state.status?.state).toBe("degraded");
  });
});
