// Spec: specs/014-user-configuration/spec.md

/**
 * The settings panel (spec 014 §3.4).
 *
 * Sections mirroring `Settings`, every control bound to a patch, `Save`
 * sending exactly one `UpdateSettings`. The panel never validates: validation
 * is pure and lives in `butler-core` (§3.1), and a second copy of the ranges
 * here would be a second thing to keep in step and a second answer when they
 * disagreed. An invalid patch comes back as an error and nothing is written.
 *
 * The credential entry is `CredentialPanel`'s (spec 012 §3.3), not this
 * panel's: a key is not a setting, it goes to the OS keychain and never to
 * the settings file (spec 015 §3.4).
 */

import { createSignal, Show } from "solid-js";

import { commands, DEFAULT_SETTINGS, type SettingsPatch } from "../ipc/client";
import { state } from "../state/runtime";

export function SettingsPanel(props: { readonly open: boolean }) {
  const [patch, setPatch] = createSignal<SettingsPatch>({});
  const [busy, setBusy] = createSignal(false);
  const [failed, setFailed] = createSignal(false);

  /** The value a control should show: the pending edit, else the live one. */
  const current = () => state.settings;

  async function save(event: Event) {
    event.preventDefault();
    setBusy(true);
    setFailed(false);
    const result = await commands.updateSettings(patch());
    setBusy(false);
    if (result.status === "error") {
      setFailed(true);
      return;
    }
    // The store already has the new value from the broadcast; clearing the
    // pending edit is what makes the controls follow it.
    setPatch({});
  }

  /**
   * §3.4's "Reset to defaults", sent as one patch.
   *
   * `DEFAULT_SETTINGS` is generated from `Settings::default()` into the
   * bindings, so this cannot drift from the Rust defaults the way a
   * hand-written copy would.
   */
  async function reset() {
    setBusy(true);
    setFailed(false);
    const result = await commands.updateSettings({
      capture: { interval_ms: DEFAULT_SETTINGS.capture.interval_ms },
      detection: {
        threshold: DEFAULT_SETTINGS.detection.threshold,
        stability_frames: DEFAULT_SETTINGS.detection.stability_frames,
        max_compare_chars: DEFAULT_SETTINGS.detection.max_compare_chars,
      },
      assistant: {
        provider: DEFAULT_SETTINGS.assistant.provider,
        model: DEFAULT_SETTINGS.assistant.model,
        effort: DEFAULT_SETTINGS.assistant.effort,
        answer_style: DEFAULT_SETTINGS.assistant.answer_style,
        max_output_tokens: DEFAULT_SETTINGS.assistant.max_output_tokens,
      },
      pacing: { words_per_minute: DEFAULT_SETTINGS.pacing.words_per_minute },
      privacy: {
        redaction_enabled: DEFAULT_SETTINGS.privacy.redaction_enabled,
        redact_pii: DEFAULT_SETTINGS.privacy.redact_pii,
        allow_degraded_mode: DEFAULT_SETTINGS.privacy.allow_degraded_mode,
        diagnostics_level: DEFAULT_SETTINGS.privacy.diagnostics_level,
        region_only: DEFAULT_SETTINGS.privacy.region_only,
      },
      window: {
        anchor: DEFAULT_SETTINGS.window.anchor,
        width_px: DEFAULT_SETTINGS.window.width_px,
        max_height_px: DEFAULT_SETTINGS.window.max_height_px,
        opacity: DEFAULT_SETTINGS.window.opacity,
      },
      ui: {
        theme: DEFAULT_SETTINGS.ui.theme,
        font_scale: DEFAULT_SETTINGS.ui.font_scale,
      },
    });
    setBusy(false);
    setPatch({});
    if (result.status === "error") {
      setFailed(true);
    }
  }

  return (
    <Show when={props.open && current()}>
      {(settings) => (
        <form
          class="plate"
          classList={{ interactive: state.interactive }}
          onSubmit={(event) => void save(event)}
          aria-label="Settings"
        >
          <fieldset>
            <legend>Capture</legend>
            <label for="butler-interval">Interval (ms)</label>
            <input
              id="butler-interval"
              type="number"
              min="1000"
              max="10000"
              step="100"
              value={patch().capture?.interval_ms ?? settings().capture.interval_ms}
              onInput={(event) =>
                setPatch({
                  ...patch(),
                  capture: {
                    ...patch().capture,
                    interval_ms: Number(event.currentTarget.value),
                  },
                })
              }
            />
          </fieldset>

          <fieldset>
            <legend>Assistant</legend>
            <label for="butler-model">Model</label>
            <input
              id="butler-model"
              type="text"
              value={patch().assistant?.model ?? settings().assistant.model}
              onInput={(event) =>
                setPatch({
                  ...patch(),
                  assistant: {
                    ...patch().assistant,
                    model: event.currentTarget.value,
                  },
                })
              }
            />
          </fieldset>

          <fieldset>
            <legend>Reading pace</legend>
            <label for="butler-wpm">Words per minute</label>
            <input
              id="butler-wpm"
              type="number"
              min="60"
              max="1200"
              step="10"
              value={
                patch().pacing?.words_per_minute ??
                settings().pacing.words_per_minute
              }
              onInput={(event) =>
                setPatch({
                  ...patch(),
                  pacing: { words_per_minute: Number(event.currentTarget.value) },
                })
              }
            />
          </fieldset>

          <fieldset>
            <legend>Privacy</legend>
            <label>
              <input
                type="checkbox"
                checked={
                  patch().privacy?.redaction_enabled ??
                  settings().privacy.redaction_enabled
                }
                onChange={(event) =>
                  setPatch({
                    ...patch(),
                    privacy: {
                      ...patch().privacy,
                      redaction_enabled: event.currentTarget.checked,
                    },
                  })
                }
              />{" "}
              Remove secret-shaped text before sending
            </label>
            <label>
              <input
                type="checkbox"
                checked={
                  patch().privacy?.allow_degraded_mode ??
                  settings().privacy.allow_degraded_mode
                }
                onChange={(event) =>
                  setPatch({
                    ...patch(),
                    privacy: {
                      ...patch().privacy,
                      allow_degraded_mode: event.currentTarget.checked,
                    },
                  })
                }
              />{" "}
              Allow running when the overlay is visible to screen sharing
            </label>
          </fieldset>

          <button type="submit" disabled={busy()}>
            Save
          </button>
          <button type="button" disabled={busy()} onClick={() => void reset()}>
            Reset to defaults
          </button>
          <Show when={failed()}>
            <span class="dim" role="alert">
              {" "}
              Rejected; nothing was changed
            </span>
          </Show>
        </form>
      )}
    </Show>
  );
}
