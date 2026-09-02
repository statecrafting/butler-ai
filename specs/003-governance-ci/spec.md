---
id: "003-governance-ci"
title: "Governance CI: the gate chain on every PR, code ownership, and derived-artifact merge hygiene"
status: approved
kind: "tooling"
domain: "governance"
created: "2026-09-01"
implementation: complete
owner: "butler-ai maintainers"
risk: high
platforms: "all"
phase: 0
depends_on:
  - "000-butler-bootstrap"
  - "002-agentic-harness"
establishes:
  - { kind: section, file: ".github/workflows/spec-spine.yml", anchor: "on" }
  - { kind: section, file: ".github/workflows/spec-spine.yml", anchor: "permissions" }
  - { kind: section, file: ".github/workflows/spec-spine.yml", anchor: "jobs.govern" }
  - { kind: section, file: ".github/workflows/spec-spine.yml", anchor: "jobs.gate" }
  - { kind: section, file: ".github/workflows/ci.yml", anchor: "on" }
  - { kind: section, file: ".github/workflows/ci.yml", anchor: "permissions" }
  - { kind: section, file: ".github/workflows/ci.yml", anchor: "jobs.rust" }
  - { kind: section, file: ".github/workflows/ci.yml", anchor: "jobs.web" }
  - ".github/dependabot.yml"
  - "CODEOWNERS"
  - ".gitattributes"
  - { kind: directory, path: ".githooks/" }
summary: >
  Makes the governance model enforceable: a `spec-spine` workflow that runs the
  full gate chain (`compile --check`, `index check`, `lint --fail-on-warn`,
  `index coverage --fail-on-untraced`, `couple` against the PR's frozen base
  and head SHAs with the PR-body waiver) on every pull request; a `ci` workflow
  for the language gates that activates itself when the workspace lands; a
  single `gate` status check for branch protection; CODEOWNERS for the
  load-bearing paths; LF normalization and the opt-in merge driver for the
  committed shard trees; Dependabot for actions, cargo and npm. Owns the
  security-relevant workflow keys (`on`, `permissions`, the jobs) as section
  units so a token-scope or trigger change forces this spec's review.
---

# 003: Governance CI

## 1. Purpose

The coupling gate is only a gate if it runs where merges are decided. This
spec wires the chain into GitHub Actions, freezes the parts of the workflows
that decide *what runs with which token* under section ownership, and adds the
repository hygiene the committed derived artifacts need (LF on checkout, a
merge driver for the rare same-shard conflict, code owners on the corpus).

## 2. Territory

Workflow files live under `.github/`, which the bypass floor exempts; this
spec claims the security-relevant keypaths explicitly (spec-spine spec 022), so
the claim overrides the floor for exactly those blocks:

- `.github/workflows/spec-spine.yml`: `on`, `permissions`, `jobs.govern`,
  `jobs.gate`.
- `.github/workflows/ci.yml`: `on`, `permissions`, `jobs.rust`, `jobs.web`.

Whole files: `.github/dependabot.yml`, `CODEOWNERS`, `.gitattributes`, and the
`.githooks/` directory (the merge driver and its enabler, the opt-in
pre-commit staleness check).

## 3. Behavior

### 3.1 `spec-spine.yml` (the governance gate)

- Triggers on `pull_request` and `push` to `main`, plus `merge_group` (inert
  until a merge queue is enabled).
- `permissions: contents: read` at the workflow level. Nothing in this workflow
  needs write access.
- `jobs.govern` MUST, in order, on `ubuntu-latest` with `fetch-depth: 0`:
  1. install the pinned `spec-spine` (the version in `Makefile`'s
     `SPEC_SPINE_VERSION`), verifying the release checksum;
  2. `spec-spine compile --check` (validation + registry freshness; exit 1
     invalid, 2 stale);
  3. `spec-spine index check` (exit 2 stale);
  4. `spec-spine lint --fail-on-warn`;
  5. `spec-spine index coverage --fail-on-untraced` (spec 000 §7.5);
  6. on `pull_request` only: `spec-spine couple --base <base.sha> --head
     <head.sha> --pr-body <file>` with the PR body written to a file. Both
     endpoints are the event's frozen SHAs, never the merge ref.
  Nothing in the job writes to `.derived/`; a writing `compile` before
  `--check` would make the check pass unconditionally.
- `jobs.gate` is the single aggregate status (`needs: [govern]` here; the
  `ci.yml` jobs are aggregated in their own workflow) that branch protection
  requires. A skipped job counts as a pass; a failed or cancelled job fails it.

### 3.2 `ci.yml` (the language gates)

- Same triggers and read-only permissions.
- `jobs.rust` runs on a matrix of `windows-latest` and `macos-latest`: `cargo
  build --workspace --locked`, `cargo test --workspace --locked`, `cargo clippy
  --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check`,
  `cargo deny check`. Every step is guarded by a populated workspace, spec 001
  §3.5's predicate (`Cargo.toml` present and `crates/*/Cargo.toml` matching at
  least one manifest), exported by a `Detect workspace` step as
  `steps.ws.outputs.present`. Presence of `Cargo.toml` alone is not enough:
  cargo refuses to load a workspace whose member entries match nothing, so the
  job is green until spec 009 lands the first crate and real thereafter.
- `jobs.web` runs on `ubuntu-latest`: `pnpm install --frozen-lockfile`, `pnpm
  -r typecheck`, `pnpm -r lint`, `pnpm -r test`, `pnpm -r build`, guarded by
  `hashFiles('pnpm-workspace.yaml') != ''`.
- A `gate` job aggregates both, as in §3.1.

### 3.3 Repository hygiene

- `.gitattributes` MUST force `text eol=lf` on everything text (the content
  hash normalizes CRLF, but LF on checkout keeps diffs clean on Windows) and
  assign `merge=spec-spine-derived-regen` to the three shard globs.
- `.githooks/merge-derived-index.sh` and `enable-merge-driver.sh` are the
  spec-spine spec 020 driver, verbatim except for the binary lookup; opt-in per
  clone. `.githooks/pre-commit` refuses a commit when `spec-spine index check`
  reports stale; opt-in via `git config core.hooksPath .githooks`.
- `CODEOWNERS` MUST name a reviewer for `/specs/`, `/standards/`,
  `/spec-spine.toml`, `/.claude/`, `/.github/`, `/.githooks/`, `/AGENTS.md`,
  `/CLAUDE.md`, and `/apps/desktop/src-tauri/capabilities/` (the Tauri grants).
- `dependabot.yml` covers `github-actions`, `cargo`, and `npm` weekly, grouped.
  Version-pin-only PRs self-waive the gate (`auto_waive_dependency_only`).

## 4. Functional requirements

- **FR-001.** A PR that edits `.github/workflows/spec-spine.yml` under
  `permissions:` without editing this spec fails `couple` with `C-001`.
- **FR-002.** A PR that edits a `spec.md` without recompiling fails
  `jobs.govern` at `compile --check` with exit 2.
- **FR-003.** A PR that adds a source file inside a discovered package without
  a specific claim fails `jobs.govern` at `coverage --fail-on-untraced` and, if
  the file changed, at `couple` with `C-002`.
- **FR-004.** `jobs.govern` completes in under three minutes on a cold runner
  (the prebuilt binary, no Rust toolchain).
- **FR-005.** The `spec-spine` binary in CI is the same version `make setup`
  installs; a mismatch is a change to both this spec and spec 002.

## 5. Acceptance criteria

- **AC-1.** `spec-spine index render` lists this spec with eight resolved
  section units and four resolved file/directory units.
- **AC-2.** Branch protection on `main` requires exactly two checks: `gate`
  from each workflow.
- **AC-3.** After `./.githooks/enable-merge-driver.sh`, `git check-attr merge
  .derived/spec-registry/by-spec/000-butler-bootstrap.json` prints
  `spec-spine-derived-regen`.

## 6. Out of scope

- Release workflows, signing, notarization, attestations (spec 017).
- AI PR review automation (a later tooling spec; it needs write permissions
  and a secret, which this workflow deliberately does not have).
