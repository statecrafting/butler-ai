---
id: "012-overlay-ui"
title: "Overlay UI: a transparent SolidJS surface that renders runtime state and answers"
status: approved
kind: "feature"
domain: "ui"
created: "2026-09-01"
implementation: complete
owner: "butler-ai maintainers"
risk: medium
platforms: "all"
phase: 2
depends_on:
  - "001-workspace-layout"
  - "011-ipc-contract"
establishes:
  - { kind: crate, id: "@butler-ai/desktop" }
  - "apps/desktop/package.json"
  - "apps/desktop/index.html"
  - "apps/desktop/vite.config.ts"
  - "apps/desktop/tsconfig.json"
  - "apps/desktop/eslint.config.js"
  - "apps/desktop/src/index.tsx"
  - "apps/desktop/src/App.tsx"
  - "apps/desktop/src/ipc/client.ts"
  - { kind: directory, path: "apps/desktop/src/state/" }
  - { kind: directory, path: "apps/desktop/src/components/" }
  - { kind: directory, path: "apps/desktop/src/styles/" }
  - { kind: directory, path: "apps/desktop/src/test/" }
references:
  - { unit: { kind: file, path: "docs/architecture.md" }, role: "context" }
summary: >
  The overlay's DOM: a SolidJS application built with Vite whose root is fully
  transparent (`rgba(0,0,0,0)` everywhere, no body background, no scrollbars),
  which renders exactly what the runtime tells it through the IPC contract:
  a status strip (disarmed, armed, inferencing, degraded with the
  visible-to-screen-sharing banner), the answer panel that receives paced
  chunks, the onboarding and credential panels, the settings panel, and the
  exclusion self-test sentinel. Pointer events are off unless the interaction
  modifier is held. SolidJS is chosen because the overlay is a passive mirror
  of one external store fed by one event stream, which is Solid's native
  primitive, and because its synchronous DOM commits keep the exclusion
  self-test's "is the sentinel painted" question answerable in one step
  (docs/architecture.md D2).
---

# 012: Overlay UI

## 1. Purpose

The UI is a thin, honest renderer. It holds no pipeline state of its own,
makes no decisions about capture or inference, and never talks to the network.
Its job is to be readable at a glance over any background, to get out of the
way when the user is working, and to say clearly when the app is not doing
what the user might assume (degraded, disarmed, needs a key).

## 2. Territory

The npm package `@butler-ai/desktop` at `apps/desktop/` (excluding
`src-tauri/`, which is the Rust crate of spec 004): the Vite/TypeScript
configuration, the entry files, the IPC client wrapper, and the `state/`,
`components/`, `styles/`, `test/` directories. `src/generated/bindings.ts` is
spec 011's; `components/PacedAnswer.tsx` is spec 013's; `components/
SettingsPanel.tsx` is spec 014's (each added by `extends`).

## 3. Behavior

### 3.1 Stack

- SolidJS 1.x, TypeScript strict, Vite, Vitest with `@solidjs/testing-
  library`, ESLint (flat config) with `@typescript-eslint`,
  `eslint-plugin-solid` (its `recommended` config, at `error`), and a
  `no-restricted-imports` rule that forbids importing `@tauri-apps/api`
  anywhere except `src/ipc/client.ts` and forbids hand-written IPC types.
  `eslint-plugin-solid` is load-bearing, not hygiene: Solid components run
  once, so React idioms (destructured props, a conditional early `return`
  in the component body, a signal read captured into a plain variable at
  component top level) typecheck and render once, then silently stop
  updating. The plugin turns that class of defect into a lint failure.

**Why SolidJS over React (decision D2, `docs/architecture.md`).**

- The overlay holds one store mirroring `RuntimeStatus` plus the answer
  buffer (§3.4), patched by one event subscription. Solid's `createStore` +
  `reconcile` is exactly that primitive; each component subscribes to the
  fields it reads, so a status event does not re-render the answer panel.
  React needs `useSyncExternalStore` or context plus reducer, then
  memoization discipline, to reach the same granularity.
- The exclusion self-test (005) mounts `Sentinel`, then captures the monitor
  and asserts the pattern is absent. That assertion is only meaningful once
  the sentinel is painted. A Solid signal write mutates the DOM
  synchronously, so the only remaining wait is paint (two
  `requestAnimationFrame`s). React 18 batches and may defer the commit,
  adding `flushSync` and a larger reasoning surface at the one seam where
  DOM timing is coupled to native capture.
- Solid has no dependency arrays and no effect re-run semantics. The class of
  React defect that typechecks and passes shallow tests (stale closures,
  wrong `useEffect` deps, StrictMode double invocation) does not exist here;
  the Solid-specific class that replaces it is caught by `eslint-plugin-solid`.
- Token-rate streaming is not a reason: pacing lives in core (013, D7) and
  the UI receives word-level chunks at reading pace. Bundle size is not a
  reason either; assets load from disk into a resident webview.
- No CSS framework; `styles/` holds a small token sheet (`tokens.css`) and
  component styles. No web fonts (no network; system font stack).
- `package.json` carries `"butler": { "spec": "012-overlay-ui" }`.

### 3.2 Transparency and input

- `index.html` and `styles/base.css` MUST set `html, body, #root {
  background: transparent; margin: 0; overflow: hidden; }` and
  `color-scheme: light dark`. Nothing may paint a full-window background.
- `body` has `pointer-events: none`. Interactive components set
  `pointer-events: auto` on themselves only, and only while
  `state.interactive` is true (mirrors the shell's click-through flag; the
  shell (004) is authoritative, the CSS is belt-and-braces).
- Text is rendered with a subtle backdrop (a rounded translucent plate at
  `rgba(20,20,20,0.72)` in dark, `rgba(250,250,250,0.85)` in light, with
  `backdrop-filter: blur(8px)`) so it is readable on any wallpaper; the plate
  is sized to content, never full-window.
- No animations longer than 150 ms; no continuous animations while idle
  (constant repaints show up in capture-tool CPU graphs and in battery).

### 3.3 Components

| Component | Renders | Source event |
|---|---|---|
| `StatusStrip` | a 24 px strip: state glyph + word, exclusion glyph, the degraded banner when applicable | `RuntimeStatus` |
| `AnswerPanel` | the current answer (hosts `PacedAnswer`, 013), "declined" on refusal, "nothing to add" verbatim | `AnswerStarted/Chunk/Done/Failed` |
| `NotePrompt` | an optional one-line input the user can type into while interactive; sent with the next `AskNow` | local |
| `OnboardingPanel` | why Screen Recording is needed, one button that opens system settings | `NeedsPermission` |
| `CredentialPanel` | provider picker + key input (masked), submits `StoreSecret` | `NeedsCredential` |
| `SettingsPanel` | spec 014 | `SettingsUpdated` |
| `Sentinel` | the full-window test pattern for the exclusion self-test, mounted only during the test | command from the shell |
| `FatalPanel` | contract-version mismatch, only in development | startup check |

The degraded banner text is fixed: "VISIBLE TO SCREEN SHARING" with the reason;
it cannot be dismissed while the state is `Degraded`.

### 3.4 State

`state/runtime.ts` holds a single Solid store mirroring the last
`RuntimeStatus` and the current answer buffer; `state/ipc.ts` subscribes once
to `butler://event` and dispatches by `type`. No component calls IPC directly;
all go through `ipc/client.ts`, which wraps the generated bindings and is the
only file allowed to import `@tauri-apps/api`.

### 3.5 Accessibility

The overlay is `aria-live="polite"` on the answer panel; status changes are
announced. Contrast on the plate meets WCAG AA for the system font at 14 px.

## 4. Functional requirements

- **FR-001.** With the app idle, the computed background of `html`, `body`
  and `#root` is `rgba(0, 0, 0, 0)` (Vitest DOM test).
- **FR-002.** With `interactive = false`, no element has `pointer-events:
  auto`; with `true`, only the panels listed in §3.3 do.
- **FR-003.** A `RuntimeStatus` with `state = degraded` renders the banner and
  it survives `Dismiss`.
- **FR-004.** `pnpm -r lint` fails on an import of `@tauri-apps/api` outside
  `src/ipc/client.ts`.
- **FR-005.** `pnpm -r build` produces no external network references in
  `dist/` (grep for `http://` and `https://`).
- **FR-006.** `pnpm -r lint` fails on a component that destructures its
  props, returns conditionally from its body, or captures a signal read
  into a plain variable at component top level (`eslint-plugin-solid`
  `solid/no-destructure`, `solid/components-return-once`, `solid/reactivity`
  and the rest of `recommended` at `error`); a fixture under `src/test/`
  proves the rule set is wired.

## 5. Acceptance criteria

- **AC-1.** `pnpm -r test` and `pnpm -r typecheck` pass.
- **AC-2.** A visual checklist in `apps/desktop/README.md` (dark and light
  wallpapers, full-screen app on macOS, 125% and 200% scaling on Windows) is
  signed off before completion.

## 6. Out of scope

- Pacing (013), settings semantics (014), window management (004).
- Theming beyond the light/dark token pair.

## 7. Resolved decisions

- **D-1 (2026-09-07, AC-2 is deferred whole, to 005).** AC-2 asks for a visual
  checklist signed off before completion: dark and light wallpapers, a
  full-screen app on macOS, 125% and 200% scaling on Windows. **Not one row of
  it can be observed today.** The overlay window is created `visible: false`
  and nothing in `butler-desktop` shows it, because spec 004 §3.2 forbids
  showing it before capture exclusion has been applied, and that is spec 005
  in phase 3.

  018 **R-010** is the mechanism, and this is the case it was written for: the
  rows are marked deferred in `apps/desktop/README.md`, each naming 005, and
  they are signed in the phase that makes them observable. Nothing is dropped.

  What survives as an automated check is the half a screenshot would not have
  caught anyway: FR-001 asserts the computed background of `html`, `body` and
  `#root` is `rgba(0, 0, 0, 0)`, and FR-002 asserts the surface is inert by
  default. Those are the properties the product actually rests on. Legibility
  needs an eye, and it gets one in phase 3.

- **D-2 (2026-09-07, promoting "every" Solid rule promoted the disabled ones
  too).** §3.1 says `eslint-plugin-solid`'s recommended config runs "at
  `error`", and the first implementation read that as mapping every key in the
  plugin's rule table to `error`. That turned on `solid/no-proxy-apis`, which
  the plugin ships **off** on purpose: it exists for environments without
  `Proxy`, and it forbids `createStore`, which §3.4 requires.

  The lint failed on `state/runtime.ts`, which is the file §3.4 mandates. The
  fix distinguishes the two cases: a rule the plugin *enables* is promoted to
  `error`, and a rule it explicitly disables stays disabled. "Recommended at
  error" is about severity, not about the rule set.

- **D-3 (2026-09-07, what the fixtures under `src/test/` are for).** FR-004
  and FR-006 both say `pnpm -r lint` must *fail* on something. A rule that is
  configured but never reached is worse than no rule, because it reads as
  protection, and neither requirement is met by the config file containing the
  right words.

  So `src/test/fixtures/` holds two files that violate the rules on purpose,
  `eslint.config.js` ignores that directory so the ordinary lint run stays
  green, and `src/test/lint.test.ts` runs ESLint over them programmatically
  and asserts the violations are reported **by rule id and at severity 2**. A
  third test asserts the other half: that the ordinary run really does ignore
  them, so the two facts cannot drift apart.

  One finding from writing it: the destructured-props fixture is reported as
  `solid/reactivity`, not `solid/no-destructure`. The assertion names what the
  plugin actually emits rather than what the rule list suggests it would.

- **D-4 (2026-09-07, `process.cwd()` and Vite's `/@fs/` rewriting).**
  `lint.test.ts` first derived the package root from `import.meta.url`. Under
  Vitest that resolves to `/@fs/Users/...`, Vite's internal form, which ESLint
  cannot resolve, and every fixture lookup failed with "No files matching".
  Vitest runs with the package directory as the working directory, so
  `process.cwd()` is both correct and shorter. Recorded because the symptom
  ("file not found" for a file that plainly exists) points nowhere near the
  cause.

- **D-5 (2026-09-07, what this spec does not render yet).** Three things §3.3
  lists are absent, each owned by a spec that has not landed, and each named
  in §2 as arriving by `extends`:

  | Absent | Owner |
  |---|---|
  | `PacedAnswer` inside `AnswerPanel` | 013 |
  | `SettingsPanel` | 014 |
  | The `SettingsUpdated` case in the store | 014 |

  `AnswerPanel` renders the buffer directly meanwhile, and the buffer stays
  empty: `AnswerChunk` is not in the contract yet (spec 011 D-3), so there is
  nothing to append. The lifecycle around it (`AnswerStarted`, `AnswerDone`,
  `AnswerFailed`) **is** in the contract and is rendered, including the
  `refusal` case §3.3 fixes as "declined".

  `state/runtime.ts`'s `switch` has no `default` arm and returns `void`, so
  each of those variants will fail to typecheck until its spec handles it.
  That is deliberate: the generated union is the reason to generate it.

## 8. Verification

```verify:cli
# AC-1. Typecheck, the DOM tests behind FR-001 to FR-003, and the lint tests
# behind FR-004 and FR-006.
pnpm --filter @butler-ai/desktop typecheck
pnpm --filter @butler-ai/desktop test
# FR-004 and FR-006: the ordinary lint run is clean, which is only meaningful
# alongside the fixture tests above proving the rules fire at all.
pnpm --filter @butler-ai/desktop lint
# FR-005: the build carries no external network reference. Built first,
# because `dist/` is gitignored and may not exist.
pnpm --filter @butler-ai/desktop build
sh -c '! grep -rIqE "https?://" apps/desktop/dist/'
# Section 3.2: nothing paints a full-window background, in the one file that
# renders before the bundle loads.
grep -q "background: transparent" apps/desktop/index.html
# Section 3.4: exactly one file in the product *imports* the Tauri API.
# Matching `from "@tauri-apps/api` rather than the bare name, and excluding
# generated/ and test/: a Vitest `vi.mock("@tauri-apps/api/event")` and a doc
# comment naming the rule are not imports, and counting them made this check
# fail on correct code. ESLint enforces the real rule; this is the second
# opinion, and it has to be about the same property.
sh -c 'test "$(grep -rlE "from \"@tauri-apps/api" apps/desktop/src --include="*.ts" --include="*.tsx" | grep -v "^apps/desktop/src/generated/" | grep -v "^apps/desktop/src/test/" | wc -l | tr -d " ")" -eq 1'
grep -q 'from "@tauri-apps/api' apps/desktop/src/ipc/client.ts
# D-2: `solid/no-proxy-apis` must stay disabled, or `createStore` is forbidden
# and section 3.4 becomes unimplementable. The real check is the lint run
# above: it passes over this file, which uses `createStore`. Asserting the use
# is what makes that run meaningful, rather than green because the construct
# was quietly dropped. Grepping the config for the rule name would only match
# the comment that explains why it is off.
grep -q "createStore" apps/desktop/src/state/runtime.ts
sh -c '! grep -qE "\"solid/no-proxy-apis\"[[:space:]]*:" apps/desktop/eslint.config.js'
# AC-2, read through 018 R-010: the visual rows are marked deferred, naming
# the spec that makes them observable.
grep -q "spec 012 AC-2" apps/desktop/README.md
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "012-overlay-ui" && exit 1 || exit 0'
```
