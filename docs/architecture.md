# butler-ai architecture

The system in one page, with the decisions that shaped it. The specs under
`specs/` are normative; this document is the map. Where they disagree, the
spec wins and this document is wrong.

## 1. What it is

A desktop overlay that reads the user's screen and answers what it sees,
without appearing in anyone else's recording of that screen.

```
             ┌──────────────────────────── the user's display ────────────────────────────┐
             │  other apps (a call, a document, a browser)          ┌───────────────────┐ │
             │                                                      │ Butler overlay    │ │
             │                                                      │ (transparent,     │ │
             │                                                      │  click-through,   │ │
             │                                                      │  capture-excluded)│ │
             │                                                      └───────────────────┘ │
             └───────────────────────────────────────────────────────────────────────────┘
                     │ compositor capture (excludes the overlay)                ▲
                     ▼                                                          │ paced chunks
   ScreenSource ──▶ Frame ──▶ TextRecognizer ──▶ Recognized ──▶ normalize ──▶ ChangeDetector
   (006, xcap)      (RGBA,      (007, Vision /     (lines +       (007)          (008, Levenshtein
                    zeroized)    Windows OCR)      text)                          ratio + stability)
                                                                                      │ Changed
                                                                                      ▼
                                          redact (015) ──▶ InferenceRequest ──▶ Assistant (010, Claude SSE)
                                                                                      │ Chunk::Text
                                                                                      ▼
                                                                          Pacer (013) ──▶ UiEvent::AnswerChunk (011) ──▶ overlay (012)
```

All of the arrows are effects executed by a runtime on behalf of one pure state
machine (009), executed by the runtime host in the desktop crate (019). The
machine decides *when*; the crates decide *how*.

## 2. Crate map and ownership

| Package | Path | Owns | Spec |
|---|---|---|---|
| `butler-core` | `crates/butler-core` | `machine` (reducer), `delta`, `pacing`, `settings`, `redaction`, `ipc` types. No OS, clock, fs, net, `tokio`, `tauri`. | 009 (crate), 008, 013, 014, 015, 011 |
| `butler-capture` | `crates/butler-capture` | `ScreenSource` trait, `XcapSource`, `Frame`, monitors | 006 |
| `butler-ocr` | `crates/butler-ocr` | `TextRecognizer` trait, Apple Vision, Windows OCR, `normalize` | 007 |
| `butler-llm` | `crates/butler-llm` | `Assistant` trait, Claude Messages API client, SSE, prompt, keychain `SecretStore`, `SpendGuard` | 010 |
| `butler-desktop` | `apps/desktop/src-tauri` | Tauri app: window, exclusion + self-test, shortcuts, tray, permissions, runtime, commands/events, settings store, logging, diagnostics | 004 (crate), 005, 009, 011, 014, 016 |
| `@butler-ai/desktop` | `apps/desktop` | SolidJS overlay, generated IPC bindings, paced answer, panels | 012 (package), 011, 013, 014 |

A crate boundary is an ownership boundary. A spec that adds a file inside
another spec's crate does so with an `extends { spec, unit }` edge, so the
indexer shows both owners and the gate requires the right spec to move with
the code.

## 3. The state machine (spec 009)

```mermaid
stateDiagram-v2
    [*] --> Disarmed
    state Armed {
        [*] --> Idle
        Idle --> Capturing: Tick reaches 0 / ForceCapture
        Capturing --> Recognizing: Captured (seq matches)
        Recognizing --> Evaluating: Recognized
        Evaluating --> Idle: Evaluated (Unchanged / Pending)
        Evaluating --> Inferencing: Evaluated (Changed)
        Inferencing --> Rendering: InferenceDone
        Inferencing --> Idle: InferenceFailed (not retryable)
        Rendering --> Idle: ChunkRendered (last)
    }
    Disarmed --> Armed: Arm (exclusion Verified)
    Disarmed --> Degraded: Arm (exclusion not Verified, allow_degraded)
    Armed --> Fault: inference timeout / InferenceFailed (retryable) / CaptureFailed / RecognizeFailed
    Armed --> Disarmed: Disarm / ScreenLocked
    Armed --> Degraded: ExclusionChanged (not Verified)
    Degraded --> Armed: ExclusionChanged (Verified)
    Fault --> Armed: retry_in reaches 0
    Fault --> Disarmed: Disarm
```

Guarantees the reducer gives by construction: a `Captured`/`Recognized`/
`Evaluated` whose `seq` is not the current one is dropped (out-of-order
guard); nothing during `Inferencing` can start a second inference (single
in-flight, no queue); `Disarm` from any state releases every frame and
cancels any inference; every `(state, event)` pair is total.

The transition table in `crates/butler-core/tests/machine.rs` is the source of
truth for this diagram; once the crate exists a test regenerates this section.

## 4. Data flow and the privacy boundary (spec 015)

Spec 015 §3.1 is normative. The table below is copied from it verbatim
(spec 015 AC-2); if the two ever differ, the spec wins and this is the bug.

| Class | Type | Memory | Disk | Network | Logs |
|---|---|---|---|---|---|
| Pixels | `Frame`, `FrameView` | until `ReleaseFrame`, then zeroed | never | never | never (not even dimensions with a timestamp) |
| Recognized text | `Recognized` | until the next commit or disarm | never | never as-is | never |
| Redacted text | `RedactedText` | inside one `InferenceRequest` | never | to the configured provider only | never |
| Answer | `String` in the pacer and UI | until dismiss/disarm | never (v1) | never (except as `prior_answer` to the same provider) | never |
| Secret | `Secret` | during header construction | keychain only | as a header to the provider only | never |
| Settings | `Settings` | always | config dir, `0600` | never | field names only |
| Diagnostics | `DiagnosticsBundle` (016) | on demand | user-chosen path, user-initiated | never | n/a |

The types enforce most of this (`!Serialize`, private fields, single
constructors, zeroize on drop); tests named in the specs enforce the rest.

`redact` (spec 015 §3.2) is the only constructor of `RedactedText`, which is
the only class in the table with a `Network` entry other than "never". That is
the whole outbound surface for screen-derived data.

## 5. Capture exclusion is verified, not assumed (spec 005)

Windows: `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)`. macOS:
`NSWindow.sharingType = .none`. Both are requests to the compositor. On
every arm, the app renders a sentinel pattern in the overlay, captures the
monitor through the same compositor path recording tools use, and asserts the
sentinel is absent. The result (`Verified`, `Compromised`, `Unsupported`) is
shown in the overlay and gates arming; degraded mode is opt-in and carries an
unmistakable banner. The same verification is what keeps the pipeline from
reading its own answers back.

## 6. Decision log

| # | Decision | Alternatives | Why |
|---|---|---|---|
| D1 | Tauri v2 (Rust process + system webview) | Electron; native Swift/WinUI apps | Native window handles for exclusion; one codebase for two platforms; small binary; typed IPC via `tauri-specta`. |
| D2 | SolidJS for the overlay | React | The overlay is a passive mirror of one external store fed by one event stream; Solid's `createStore` + `reconcile` is that primitive, with per-field subscription and no memoization discipline. Synchronous DOM commits keep the exclusion self-test's "is the sentinel painted" question one step (React 18 batches and may defer the commit). No dependency arrays or effect re-runs, the React defect class that typechecks and ships; the Solid-specific class (destructured props, conditional returns, untracked signal reads) is caught by `eslint-plugin-solid` (012 §3.1). Token-rate streaming is not the reason (pacing is in core, D7); bundle size is not either (assets load from disk). |
| D3 | Pure reducer (009) + effect runtime host (019) | Async tasks with shared state; an actor per stage | Exhaustively testable transition table; the outline's ordering guarantees become properties, not hopes. |
| D4 | OS-native OCR (Vision, Windows.Media.Ocr) | Tesseract; PaddleOCR / RapidOCR via ONNX | No bundled model, no GPU dependency of our own, hardware-accelerated, on-device by definition. |
| D5 | Snapshot polling (2.5 s default) | Continuous SCStream / DXGI stream | Text changes on the order of seconds; polling is an order of magnitude cheaper in CPU and battery. |
| D6 | Compare against the last *inferred* text with a stability window (008) | Compare consecutive frames | Slow drift still accumulates into a change; mid-scroll frames never fire. |
| D7 | Pacing in Rust core, UI as passive renderer (013) | Pace in the UI | The machine knows when rendering is complete; the policy is testable without a browser. |
| D8 | Claude Messages API over raw HTTPS + SSE (010) | A community Rust SDK | No official Rust SDK; the wire contract is small and pinned by fixture tests; provider is a trait. |
| D9 | Secrets in the OS keychain (010) | Settings file; env vars | Never on disk in plaintext; no env surface at all. |
| D10 | Specify first, ratchet on from day one (000) | Adopt governance after a prototype | Greenfield: zero debt to burn down; every file is claimed before it exists. |
| D11 | Linux deferred (001) | Ship without exclusion on Linux | No compositor-level exclusion under X11; the product promise would be false there. |
| D12 | One overlay window (004) | Separate settings/onboarding windows | A second window needs its own exclusion and self-test; panels inside the overlay do not. |

Add a row when a spec is amended for a design reason (spec 018 R-006).

## 7. Reference hardware (for the performance FRs)

To be fixed in phase 3: one Apple Silicon laptop (M-series, 16 GB) and one
Windows laptop (recent x86-64, integrated GPU, 16 GB), named here by model
when chosen. Performance numbers in specs 006, 007, 008 are measured on them.

### 7.1 Change detection (spec 008 AC-2)

`cargo bench -p butler-core --bench delta`, criterion, `bench` profile
(optimized), Apple M1 Max, macOS 26.5.1. Two 6000-character inputs, which is
the worst case `max_compare_chars` admits:

| Benchmark | p50 |
|---|---|
| `evaluate_6000_vs_6000_dissimilar` | **35.5 ms** |
| `evaluate_6000_identical` | **35.3 ms** |

Identical and dissimilar inputs cost the same: the Levenshtein DP fills the
whole `n x m` matrix either way, so the ratio it computes does not affect the
work done to compute it.

**This contradicts spec 008 §3.2**, which estimates "single-digit
milliseconds" for the same 6000 x 6000 comparison. The estimate treated the
36 M matrix cells as byte operations; a DP cell is a comparison plus a
three-way minimum over `usize`, so ~1 ns per cell is the honest figure and
~36 ms is the result. The gap is arithmetic in the estimate, not a defect in
the implementation. Recorded here rather than resolved: amending §3.2 is a
human decision (see spec 008 D-2).

Two consequences worth carrying forward:

- A single `evaluate` can run the DP **twice** (once against the committed
  base, once against the previous candidate for the stability check), so the
  worst case per call is ~70 ms.
- Cost is quadratic in `max_compare_chars`, so halving the window quarters the
  cost. Reaching single-digit milliseconds by that route needs a window near
  3000 characters, which is a spec 014 default, not an implementation choice.

None of this threatens the pipeline: spec 009 evaluates on a capture cycle of
seconds and the runtime calls the detector off the UI thread.

## 8. Settings defaults (spec 014)

Generated from `Settings::default()` by a test and diffed (spec 014 AC-2), the
way §3's diagram is for spec 009. Never hand-edited: run
`cargo test -p butler-core --test settings -- --ignored regenerate` after
changing a default.

<!-- generated: settings-defaults -->
| Setting | Default |
|---|---|
| `assistant.answer_style` | `"short"` |
| `assistant.budget.daily_usd` | `2.0` |
| `assistant.budget.max_input_tokens` | `8000` |
| `assistant.budget.monthly_usd` | `20.0` |
| `assistant.effort` | `"medium"` |
| `assistant.endpoint_override` | none |
| `assistant.max_output_tokens` | `1024` |
| `assistant.model` | `"claude-opus-5"` |
| `assistant.provider` | `"anthropic"` |
| `capture.interval_ms` | `2500` |
| `capture.monitor.kind` | `"primary"` |
| `capture.region` | none |
| `detection.max_compare_chars` | `6000` |
| `detection.stability_frames` | `2` |
| `detection.threshold` | `0.85` |
| `pacing.words_per_minute` | `300` |
| `privacy.allow_degraded_mode` | `false` |
| `privacy.diagnostics_level` | `"minimal"` |
| `privacy.redact_pii` | `false` |
| `privacy.redaction_enabled` | `true` |
| `privacy.region_only` | `false` |
| `schema` | `1` |
| `shortcuts.arm_disarm` | `"CmdOrCtrl+Shift+B"` |
| `shortcuts.ask_now` | `"CmdOrCtrl+Shift+Enter"` |
| `shortcuts.interact` | `"CmdOrCtrl+Shift+Space"` |
| `shortcuts.toggle_visibility` | `"CmdOrCtrl+Shift+H"` |
| `ui.font_scale` | `1.0` |
| `ui.theme` | `"system"` |
| `window.anchor` | `"top-right"` |
| `window.max_height_px` | `600` |
| `window.opacity` | `0.92` |
| `window.width_px` | `420` |
<!-- generated: settings-defaults -->
