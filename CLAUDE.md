# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working in this
repository. Run `/init` first in every session; it executes the protocol in
`AGENTS.md`.

## What this is

butler-ai is a private, capture-excluded screen assistant for Windows and
macOS: a Tauri v2 desktop app whose transparent, always-on-top overlay is
excluded from screen sharing and recording at the compositor level, which
polls the display every few seconds, runs on-device OCR, detects when the
visible text has meaningfully changed, asks an LLM about it, and renders the
answer at a reading pace. The system is **specified before it is built**: the
spec corpus under `specs/` is the product today, and the code arrives phase by
phase under those claims (`specs/018-implementation-sequencing`).

Read `docs/architecture.md` first (crate map, data flow, state machine,
decision log) and `docs/threat-model.md` second (what exclusion does and does
not defend against; the data-handling table).

## Commands

The `Makefile` is the command contract the skills and CI call. Do not
rediscover commands by grepping manifests.

```sh
make setup       # install the pinned spec-spine, compile, index, verify the loop
make spine       # compile --check → index check → lint --fail-on-warn → coverage --fail-on-untraced
make ci          # spine + build + test + lint (what CI runs)
make pr-prep     # spec-spine index, then couple --base origin/main --head HEAD
make burndown    # unresolved owning units (W-001) per spec: the build to-do list
make coverage    # spec-spine index coverage
SLUG=my-feature make spec-new   # scaffold specs/NNN-my-feature/spec.md
make build test lint            # language gates (no-ops until Cargo.toml / pnpm-workspace.yaml exist)
```

Rust: toolchain pinned in `rust-toolchain.toml` (1.92.0, edition 2024); always
`--locked`. Web: pnpm, Node 22. Exit codes of `spec-spine` are a contract:
`0` ok, `1` validation/drift, `2` stale, `3` I/O/config.

## Governance (spec-spine)

- **Authored truth** is markdown under `specs/` and `standards/spec/`; **machine
  truth** is `spec-spine`-emitted JSON under `.derived/`, read only through
  `spec-spine registry` / `index` subcommands, never `jq`/`sed`/`python`.
- Every spec declares **typed edges** (`establishes`, `extends`, `refines`,
  `supersedes`, `amends`, `co_authority`, `constrains`; `references` is
  non-owning) over **units** (`crate`, `directory`, `file`, `module`, `symbol`,
  `section`). Convention: a feature spec claims its crate, its files, and its
  load-bearing symbols; a file inside another spec's crate is claimed with
  `extends { spec, unit }`, never a second `establishes`.
- **Taxonomies are closed**: `domain` ∈ governance | platform | pipeline |
  assistant | ui | distribution; `kind` ∈ constitutional-bootstrap | feature |
  tooling | constraint | plan. `platforms` and `phase` are required on product
  specs (`frontmatter.extra_known_keys`).
- **The gate chain** (`make spine` + `couple`) runs in CI on every PR
  (`.github/workflows/spec-spine.yml`, spec 003): `compile --check`, `index
  check`, `lint --fail-on-warn`, `index coverage --fail-on-untraced`, `couple`.
  `C-001` = owned code changed without its spec; `C-002` = a changed source
  file no spec specifically claims (`require_ownership = true`, on from day
  one, never to be turned off: spec 000 anchor `ownership-ratchet`).
- **`.derived/` is committed** (shard trees; `build-meta.json` is gitignored).
  After any change to a spec, a manifest, or a hashed input (`spec-spine.toml
  [index] extra_hashed_inputs`: standards, workflows, the harness, `AGENTS.md`,
  `CLAUDE.md`, `Makefile`, the Tauri security surface), run `spec-spine compile
  && spec-spine index` and commit the shards. The hooks in
  `.claude/settings.json` do this for you and block `gh pr create` on a red gate.
- **Refusal rule**: never edit a spec to make the gate pass on code that
  contradicts it. Surface the contradiction. Waive with a cited
  `Spec-Drift-Waiver:` line only with explicit human approval.

## Specify-first lifecycle

| status / implementation | unresolved owned unit |
|---|---|
| `draft`, or `approved` + `pending`/`in-progress` | `W-001` warning (counted, never skipped) |
| `approved` + `complete` | hard `I-00x` error, `index check` fails |

Only a human sets `status: approved`. A spec flips to `implementation:
complete` only when `make burndown` shows zero for it. `make burndown` is the
build to-do list; `specs/018-implementation-sequencing` is the order (five
phases; one spec per branch named `NNN-slug`).

## Architecture (target)

```
Cargo.toml (workspace)            package.json + pnpm-workspace.yaml
crates/
  butler-core/     pure domain: machine (reducer), delta, pacing, settings, redaction, ipc types   (specs 009, 008, 013, 014, 015, 011)
  butler-capture/  ScreenSource trait + xcap backend; Frame (no persistence)                       (006)
  butler-ocr/      TextRecognizer trait + Apple Vision / Windows.Media.Ocr; normalize              (007)
  butler-llm/      Assistant trait + Claude Messages API (SSE); prompt; keychain secrets; budget   (010)
apps/desktop/
  src-tauri/       butler-desktop: window, exclusion + self-test, shortcuts, tray, runtime, commands, settings store, logging   (004, 005, 009, 011, 014, 016)
  src/             @butler-ai/desktop: SolidJS overlay; generated IPC bindings; paced answer      (012, 013, 011)
```

Invariants that shape every change (each is a spec's MUST):

- `butler-core` has no OS, clock, fs, net, `tokio` or `tauri` dependency; the
  state machine is a pure `reduce(state, event) -> (state, effects)` (009).
- One overlay window, click-through by default, excluded from capture before it
  is shown, with the exclusion **verified** by a self-test and reported
  honestly; degraded mode is opt-in and bannered (004, 005).
- Frames and recognized text never touch disk or the network as-is; only
  `RedactedText` inside one `InferenceRequest` leaves the process, to one
  configured host; secrets live in the OS keychain; logs carry ids and kinds
  only (015, 016).
- Single in-flight inference; out-of-order frames are dropped by the reducer
  (009). Pacing is a pure policy in core; the UI is a passive renderer (013, 012).
- IPC types are defined once in Rust and generated to TypeScript; the generated
  file is committed and CI checks it is fresh (011).
- `unsafe` is `deny`; FFI blocks carry `#[allow(unsafe_code)]` + `// SAFETY:`
  and exist only in the platform crates (001).

## Working here

- New capability: `/spec-new` first (a spec claims territory before code).
- Implementing a spec: branch `NNN-slug`; touch that spec, its claimed units,
  regenerated `.derived/` and generated bindings, nothing else; `make ci`;
  `/ship`; `/shepherd`.
- Paths-scoped rules load automatically: `.claude/rules/spec-authoring.md`,
  `rust-crates.md`, `overlay-frontend.md`, `build-commands.md`.
- License: AGPL-3.0-only (`LICENSE`).
