---
id: "001-workspace-layout"
title: "Workspace layout: one Cargo workspace, one pnpm workspace, pinned toolchains"
status: approved
kind: "tooling"
domain: "governance"
created: "2026-09-01"
implementation: complete
owner: "butler-ai maintainers"
risk: low
platforms: "all"
phase: 1
depends_on:
  - "000-butler-bootstrap"
establishes:
  - "Cargo.toml"
  - "rust-toolchain.toml"
  - ".nvmrc"
  - "deny.toml"
  - "package.json"
  - "pnpm-workspace.yaml"
co_authority:
  - { unit: { kind: section, file: "Makefile", anchor: "build" }, with_specs: ["002-agentic-harness"] }
  - { unit: { kind: section, file: "Makefile", anchor: "test" }, with_specs: ["002-agentic-harness"] }
  - { unit: { kind: section, file: "Makefile", anchor: "lint" }, with_specs: ["002-agentic-harness"] }
  - { unit: { kind: section, file: "Cargo.toml", anchor: "workspace" }, with_specs: ["004-desktop-shell"] }
references:
  - { unit: { kind: file, path: "docs/architecture.md" }, role: "context" }
summary: >
  Fixes the physical shape of the repository so every later spec can name its
  territory unambiguously: a single root Cargo workspace (`crates/*` plus the
  Tauri app crate under `apps/desktop/src-tauri`), a single pnpm workspace
  (`apps/*`), pinned Rust and Node toolchains, edition 2024, a supply-chain
  policy (`deny.toml`), and the manifest metadata every package must carry so
  spec-spine discovers it and maps it to its owning spec. Owns the root
  manifests and co-owns the Makefile's build/test/lint targets with the harness
  spec. Establishes no product code.
---

# 001: Workspace layout

## 1. Purpose

Every other spec in this corpus claims crates, files and symbols by path. Those
paths are only meaningful if the workspace shape is fixed first, and the
spec-spine indexer only discovers a package if the root manifests declare it.
This spec is the physical contract: where packages live, how they are named,
which toolchains build them, and what metadata each carries.

The layout follows one rule: **a crate boundary is an ownership boundary**. A
crate is owned by exactly one spec (its manifest floor), and its contents are
specifically claimed by that spec or by specs that `extend` it. Pure domain
logic lives in a crate with no operating-system dependency so it can be tested
on any CI runner; platform code lives in crates that are `cfg`-gated per OS.

## 2. Territory

| Path | Role | Owner |
|---|---|---|
| `Cargo.toml` | root workspace manifest, shared `[workspace.dependencies]`, `[workspace.lints]` | this spec (the `[workspace]` table is co-owned with 004, §3.1) |
| `rust-toolchain.toml` | pinned channel, components (`rustfmt`, `clippy`) | this spec |
| `deny.toml` | cargo-deny: licenses, advisories, banned crates, source allowlist | this spec |
| `package.json` | pnpm workspace root (private), `engines`, root scripts | this spec |
| `pnpm-workspace.yaml` | workspace members `apps/*` | this spec |
| `crates/butler-core/` | pure domain (spec 009 owns the crate) | 009 |
| `crates/butler-capture/` | screen capture (006) | 006 |
| `crates/butler-ocr/` | text recognition (007) | 007 |
| `crates/butler-llm/` | inference providers and secrets (010) | 010 |
| `apps/desktop/src-tauri/` | the Tauri app crate `butler-desktop` (004) | 004 |
| `apps/desktop/` (npm `@butler-ai/desktop`) | the SolidJS overlay (012) | 012 |
| `Makefile` targets `build`, `test`, `lint` | the language gates | co-owned with 002 |

The per-crate `Cargo.toml` and `package.json` files belong to the spec that
owns the crate; this spec only prescribes what they MUST contain (§3.3).

## 3. Behavior

### 3.1 Cargo workspace

- `Cargo.toml` at the root MUST be a virtual workspace (`[workspace]`, no root
  `[package]`) with `resolver = "3"`. The complete `members` list is
  `["crates/*", "apps/desktop/src-tauri"]`, and it fills in with the phases:
  cargo treats a glob that matches nothing as a literal path and refuses to
  load a workspace with any member that has no manifest, so a member entry
  MUST land in the same change as the crate it names. At phase 1 the list is
  `["crates/*"]`; spec 004 appends `"apps/desktop/src-tauri"` when it lands
  the app crate (co-authority over the `[workspace]` table, §2). No entry may
  name a path that does not exist on `main`.
- `[workspace.package]` MUST set `edition = "2024"`, `rust-version`, `license =
  "AGPL-3.0-only"`, and `repository`. Every member inherits them (`.workspace =
  true`).
- `[workspace.dependencies]` MUST pin every third-party crate once; members
  reference by `{ workspace = true }`. Versions are exact-pinned in
  `Cargo.lock`, which is committed.
- `[workspace.lints.rust]` MUST set `unsafe_code = "deny"` (not `forbid`: the
  platform crates need `unsafe` for FFI, and each such block MUST carry an
  `#[allow(unsafe_code)]` with a `// SAFETY:` comment at the block, so the
  exception is grep-able). `[workspace.lints.clippy]` MUST set `all = "deny"`
  and `pedantic = "warn"`.
- The toolchain is pinned in `rust-toolchain.toml` (`channel = "1.92.0"`,
  components `rustfmt`, `clippy`; targets `x86_64-pc-windows-msvc`,
  `aarch64-apple-darwin`, `x86_64-apple-darwin`). MSRV equals the pinned
  channel; there is no separate MSRV promise for a desktop application.
- `deny.toml` MUST allow only `MIT`, `Apache-2.0`, `BSD-2-Clause`,
  `BSD-3-Clause`, `ISC`, `Zlib`, `Unicode-3.0`, `MPL-2.0`, `AGPL-3.0`
  (our own) and `Unlicense`, deny `advisories` at `vulnerability`, and restrict
  `sources` to crates.io plus the explicitly listed git dependencies (none at
  bootstrap).

### 3.2 pnpm workspace

- `package.json` at the root MUST be `"private": true`, declare
  `"packageManager": "pnpm@<exact>"`, `"engines": { "node": ">=22 <23" }`, and
  root scripts that delegate to workspace packages (`pnpm -r build`, `pnpm -r
  test`, `pnpm -r lint`, `pnpm -r typecheck`).
- `pnpm-workspace.yaml` MUST list `packages: ["apps/*"]`.
- `pnpm-lock.yaml` is committed; CI installs with `--frozen-lockfile`.
- `.nvmrc` MUST pin the exact Node version, and that version MUST satisfy
  `package.json`'s `engines.node` range. It is the Node counterpart of
  `rust-toolchain.toml`: `engines` states the range a consumer must be inside,
  `.nvmrc` states the one version this repository builds with, so `nvm use`
  selects it without argument and a contributor on a newer major is told
  before `pnpm install` fails rather than after.

### 3.3 Manifest metadata (linkage floor)

Every Cargo package MUST carry:

```toml
[package.metadata.butler]
spec = "NNN-slug"
```

Every npm package MUST carry:

```json
"butler": { "spec": "NNN-slug" }
```

The indexer reads these through `[manifest] metadata_namespace = "butler"`.
This is the drift safety net; it is *not* coverage (spec 000 §7.5). Every
source file additionally opens with a `// Spec: specs/NNN-slug/spec.md` header,
so a file is specifically claimed even before its spec lists it by path.

### 3.4 Naming

- Cargo packages: `butler-<area>` (`butler-core`, `butler-capture`,
  `butler-ocr`, `butler-llm`, `butler-desktop`). Library crate names use the
  underscore form (`butler_core`).
- npm packages: `@butler-ai/<area>` (`@butler-ai/desktop`).
- Binary: `butler` (the Tauri app's product name is "Butler").

### 3.5 Build targets (co-owned with spec 002)

The Makefile targets `build`, `test`, `lint` are the language gates the harness
skills call. Their content is this spec's; their existence and names are spec
002's. They MUST:

- `build`: `cargo build --workspace --locked` and `pnpm -r build`. Each half is
  guarded so the target is a no-op until there is something to build: the
  cargo half by a *populated* workspace (`Cargo.toml` present and
  `crates/*/Cargo.toml` matching at least one manifest; the root manifest
  alone is not loadable, §3.1), the pnpm half by `pnpm-workspace.yaml`. Spec
  003 §3.2 guards the CI jobs on the same two predicates.
- `test`: `cargo test --workspace --locked` and `pnpm -r test`.
- `lint`: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  --locked -- -D warnings`, `cargo deny check`, `pnpm -r lint`, `pnpm -r
  typecheck`.

## 4. Functional requirements

- **FR-001.** `spec-spine index` discovers exactly five Cargo packages and one
  npm package once the workspace lands, each with a `spec` metadata value that
  resolves to an existing spec id.
- **FR-002.** Once the workspace is populated (spec 009 lands the first
  crate), `cargo build --workspace --locked` succeeds on Windows and macOS
  runners with the pinned toolchain and no network beyond crates.io.
- **FR-003.** `cargo deny check` passes with the license allowlist in §3.1.
- **FR-004.** No member declares a dependency version outside
  `[workspace.dependencies]` (a CI grep, `scripts/check-workspace-deps.sh`, is
  spec 003's to add if drift appears; not required at bootstrap).
- **FR-005.** `unsafe` appears only in `butler-capture`, `butler-ocr`, and
  `butler-desktop`, each occurrence under `#[allow(unsafe_code)]` with a
  `// SAFETY:` comment.

## 5. Acceptance criteria

- **AC-1.** `spec-spine index coverage` reports every discovered package with
  a floor spec and `0 unclaimed`.
- **AC-2.** `make build test lint` exits 0 on a clean checkout on both target
  platforms: as no-ops before the first crate lands, as the real gates after.
- **AC-3.** `rg -n "unsafe" crates/butler-core` returns nothing.
- **AC-4.** `cargo metadata --format-version 1 | jq` is NOT used anywhere in
  the harness for governance reads (spec 000 §1); `spec-spine registry` and
  `index` are.

## 6. Out of scope

- The contents of any crate (the owning feature specs).
- CI workflow definitions (spec 003).
- Release packaging, signing, and updater configuration (spec 017).
- Linux as a build target (see `docs/threat-model.md` §Platforms; there is no
  compositor-level capture exclusion under X11 and Wayland portals differ, so
  Linux is deferred until an exclusion mechanism with the same guarantee
  exists).

## 7. Resolved decisions

- **D-1 (2026-09-02).** The first driven build of this spec (PR #3) landed the
  root manifests alone, and both Rust CI jobs failed at `cargo build` with
  "failed to load manifest for workspace member `crates/*`": cargo 1.92 falls
  back to the literal path when a `members` glob matches nothing, and refuses
  any member without a manifest. Three resolutions were on the table: merge
  red and let 009 heal it (it cannot: the literal `apps/desktop/src-tauri`
  entry stays unloadable until phase 2), bootstrap skeleton crates under a
  drift waiver (they belong to 009 and 004), or make the members list and the
  gate guards phase-aware. The third is what §3.1, §3.5 and spec 003 §3.2 now
  say: a member entry lands with its crate, and the language gates activate on
  a populated workspace rather than on the presence of `Cargo.toml`. This spec
  is therefore complete when its five manifests exist and the gates are honest
  no-ops; FR-002 becomes checkable when 009 lands.

## 7. Verification

```verify:cli
# AC-1: every discovered package has a floor spec and nothing is unclaimed.
spec-spine index coverage --fail-on-untraced
# AC-2: the language gates exit 0 (no-ops before the first crate, real after).
make build
make test
make lint
# AC-3: the pure core carries no `unsafe`.
sh -c '! grep -rn "unsafe" crates/butler-core'
# §3.1/§3.2: the pinned toolchains are declared where the spec says.
grep -q 'channel = "1.92.0"' rust-toolchain.toml
grep -q '"node": ">=22 <23"' package.json
grep -q 'packages:' pnpm-workspace.yaml
# §3.2: .nvmrc pins one exact version, and it satisfies engines.node.
sh -c 'v=$(tr -d "[:space:]" < .nvmrc); echo "$v" | grep -qE "^22\\.[0-9]+\\.[0-9]+$"'
```
