---
id: "018-implementation-sequencing"
title: "Implementation sequencing: the phased build order for agentic development"
status: approved
kind: "plan"
domain: "governance"
created: "2026-09-01"
implementation: n-a
owner: "butler-ai maintainers"
risk: low
platforms: "all"
phase: 0
depends_on:
  - "000-butler-bootstrap"
constrains:
  - kind: sequencing-plan
    target_specs: ["001-workspace-layout", "009-pipeline-state-machine", "008-change-detection", "015-privacy-boundary"]
    note: "Phase 1: workspace and the pure core. Everything here is platform-free and builds on any host, Linux included; the Rust CI matrix itself is Windows and macOS (003 §3.2)."
  - kind: sequencing-plan
    target_specs: ["004-desktop-shell", "011-ipc-contract", "012-overlay-ui", "014-user-configuration", "016-diagnostics-and-logging", "019-runtime-host"]
    note: "Phase 2: the shell, the IPC seam, the overlay skeleton, settings, logging, and the runtime host that executes phase 1's reducer. Requires phase 1 complete."
  - kind: sequencing-plan
    target_specs: ["006-screen-capture", "007-text-recognition", "005-capture-exclusion"]
    note: "Phase 3: the platform pipeline crates and the exclusion self-test. Requires phase 2 complete; 005 after 006, because its self-test captures through `ScreenSource`."
  - kind: sequencing-plan
    target_specs: ["010-assistant-inference", "013-output-pacing"]
    note: "Phase 4: inference and pacing. Requires phases 1 to 3; the first end-to-end answer is this phase's exit criterion."
  - kind: sequencing-plan
    target_specs: ["017-release-and-distribution"]
    note: "Phase 5: signed, attested releases. Requires phase 4."
references:
  - { unit: { kind: file, path: "docs/architecture.md" }, role: "context" }
summary: >
  The build order for butler-ai as a sequencing plan over the product specs,
  expressed with spec-scoped `constrains` edges so the plan is part of the
  authority graph rather than a README. Five phases, each with an entry
  condition, an exit criterion (the phase's specs at zero `W-001` and flipped
  to `implementation: complete`), and the parallelism an orchestrator may use
  within it. The `phase` frontmatter key on every product spec mirrors this
  plan, and so does its `depends_on` (R-007); all three MUST agree.
---

# 018: Implementation sequencing

## 1. Purpose

Agents build fastest when the next unit of work is unambiguous and its
verification is mechanical. This plan gives an orchestrator (a human, or
`/implement-plan`) exactly that: which specs to implement in which order, what
can run in parallel, and how a phase is known to be done. It owns no code; its
authority is over the *order* of the other specs (spec-spine spec 018's
spec-scoped `constrains`).

## 2. Phases

| Phase | Specs | Entry | Exit criterion | Parallelism |
|---|---|---|---|---|
| 0 | 000, 002, 003 | none | corpus compiles; harness and CI green on `main` | n/a (done) |
| 1 | 001, 009, 008, 015 | phase 0 | `make ci` green on the CI matrix (003 §3.2); `make burndown` shows zero for these four | 009 first (it owns `butler-core`); then 008 and 015 in parallel |
| 2 | 004, 011, 012, 014, 016, 019 | phase 1 | app launches on both platforms to a transparent, click-through overlay showing `Disarmed`; bindings fresh in CI | 004 first; then 011 (the contract and its generated bindings), then 012 (the UI that imports them), then 014; 016 in parallel once 004 is complete; 019 last, since the runtime emits 011's events and 016's traces |
| 3 | 006, 007, 005 | phase 2 | arming runs the self-test and reports `Verified` on both platforms; a static screen yields `Unchanged` cycles | 006 and 007 in parallel; 005 after 006 |
| 4 | 010, 013 | phase 3 | a question on screen produces a paced answer end to end; 015 FR-004/005 pass | 013's pure policy may start with phase 1; wiring after 010 |
| 5 | 017 | phase 4 | `v0.1.0` release with signed artifacts | n/a |

## 3. Rules

- **R-001.** A spec's `phase` value MUST equal the phase this table assigns
  it. `spec-spine lint` does not check this; the `spec-authoring` rule and
  `/code-review` do.
- **R-002.** No phase starts until the previous phase's specs are
  `implementation: complete`, with the exception noted for 013.
- **R-003.** Within a phase, a spec is implemented by one agent on one branch
  named `NNN-slug`; the PR touches that spec's `spec.md` (at minimum flipping
  `implementation` and recording decisions) and its claimed units, and nothing
  else, except regenerated `.derived/` shards and generated bindings.
- **R-004.** Flipping a spec to `complete` requires `make burndown` to report
  zero unresolved units for it and the spec's ACs to be checked off in the PR
  body.
- **R-005.** A design change discovered during implementation is a spec
  amendment first (the refusal rule): the agent stops, proposes the amendment,
  and continues only after it is approved.
- **R-006.** Each phase ends with `/code-review` over the whole phase and an
  update to `docs/architecture.md`'s decision log.
- **R-007.** A spec's `depends_on` MUST entail this table's order. R-002 is
  normative for a human reading the plan; `depends_on` is the only thing an
  orchestrator schedules on, so the two must agree the way `phase` and this
  table must (R-001). The mechanical form is: the entry spec of each phase
  depends on the *leaf* specs of the previous phase, which transitively cover
  the rest of it. An edge that exists only as a phase gate MUST carry a
  frontmatter comment saying so, so it is never mistaken for a compile-time
  dependency and never deleted as unused.

## 4. Out of scope

- Estimates. The plan orders work; it does not schedule it.
- Post-v0.1 features (window-scoped capture, history, local models); each
  arrives as a new spec and a new row here by amendment.

## 5. Resolved decisions

- **D-1 (2026-09-02).** Phase 2 gained 019, the runtime host. The executor
  that runs the reducer's effects was spec 009's, which put two phase 2 units
  inside a phase 1 spec and made this plan unsatisfiable in the graph an
  orchestrator schedules on. 019 D-1 records the alternatives that were
  rejected.
- **D-2 (2026-09-02).** R-007 was added, and the entry spec of every phase
  after 1 (004, 006, 010, 017) gained the gate edges it names; 008 lost a
  conceptual edge to 007 that had put a phase 1 spec behind a phase 3 spec
  (008 D-1). Before this, the plan lived only in `constrains` and in prose:
  an orchestrator reading `depends_on` alone saw 004 and 009 become ready
  together after 001, and started the phase 2 shell against a workspace with
  no crates in it. The alternative considered was teaching the orchestrator
  to read a `sequencing-plan` constraint. That remains the better general
  answer and is not foreclosed; R-007 is what makes this corpus buildable by
  one that does not yet do it.
- **D-3 (2026-09-02).** Phase 1's exit criterion no longer claims "Linux CI".
  Spec 003 §3.2 runs `jobs.rust` on `windows-latest` and `macos-latest` only,
  so there is no Linux runner to be green. The platform independence phase 1
  cares about is enforced by 009 AC-3 (nothing OS-specific in
  `cargo tree -p butler-core`), which any runner checks. An ubuntu job scoped
  to `-p butler-core` would enforce it directly; that is a 003 amendment, and
  it is deliberately not folded into this one.
