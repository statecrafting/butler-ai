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
`/build`) exactly that: which specs to implement in which order, what
can run in parallel, and how a phase is known to be done. It owns no code; its
authority is over the *order* of the other specs (spec-spine spec 018's
spec-scoped `constrains`).

## 2. Phases

| Phase | Specs | Entry | Exit criterion | Parallelism |
|---|---|---|---|---|
| 0 | 000, 002, 003 | none | corpus compiles; harness and CI green on `main` | n/a (done) |
| 1 | 001, 009, 008, 015 | phase 0 | `make ci` green on the CI matrix (003 §3.2); `make burndown` shows zero for 001, 009 and 008, and zero for 015's *own* units (R-009 exempts its forward `constrains` edges) | 009 first (it owns `butler-core`); then 008 and 015 in parallel |
| 2 | 004, 011, 012, 014, 016, 019 | phase 1 | app launches on both platforms to a transparent, click-through overlay showing `Disarmed`; bindings fresh in CI | 004 first; then 011 (the contract and its generated bindings), then 012 (the UI that imports them), then 014; 016 in parallel once 004 is complete; 019 last, since the runtime emits 011's events and 016's traces |
| 3 | 006, 007, 005 | phase 2 | arming runs the self-test and reports `Verified` on both platforms; a static screen yields `Unchanged` cycles | 006 and 007 in parallel; 005 after 006 |
| 4 | 010, 013 | phase 3 | a question on screen produces a paced answer end to end; 015 FR-004/005 pass | 013's pure policy may start with phase 1; wiring after 010 |
| 5 | 017 | phase 4 | `v0.1.0` release with signed artifacts | n/a |

## 3. Rules

- **R-001.** A spec's `phase` value MUST equal the phase this table assigns
  it. `spec-spine lint` does not check this; the `spec-authoring` rule and
  `/code-review` do.
- **R-002.** No phase starts until the previous phase's specs are
  `implementation: complete`, with the exception noted for 013 and the one
  R-009 makes for constraint specs.
- **R-003.** Within a phase, a spec is implemented by one agent on one branch
  named `NNN-slug`; the PR touches that spec's `spec.md` (at minimum flipping
  `implementation` and recording decisions) and its claimed units, and nothing
  else, except regenerated `.derived/` shards and generated bindings.
- **R-004.** Flipping a spec to `complete` requires `make burndown` to report
  zero unresolved units for it and the spec's ACs to be checked off in the PR
  body. R-009 says what "zero unresolved units" means for a constraint spec,
  and R-010 says what "checked off" means for an AC whose evidence a later
  phase delivers.
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

- **R-008.** A spec MUST NOT `depends_on` a constraint spec that `constrains`
  one of its own units. The governing relationship is already carried, in the
  correct direction, by the `constrains` edge and enforced at merge by the
  coupling gate. The inverse edge is a cycle: the constraint spec cannot
  resolve that unit until the constrained spec creates it, and the constrained
  spec cannot start until the constraint spec is complete. See D-2, where
  exactly this deadlocked the whole plan after phase 1.

- **R-009.** A `kind: constraint` spec whose `constrains` edges name units that
  later phases create is **`in-progress` for the duration of those phases, by
  design**, and gates nothing. R-004's "zero unresolved units" is read against
  its *own* units, the ones it `establishes` or `extends`; its forward
  `constrains` edges resolve as each constrained spec lands, and it flips to
  `complete` in the phase of its last one. Such a spec is "delivered" for the
  purposes of a phase exit when its own units are at zero and its assertions
  are written, which is what the phase table's exit column means for it.

- **R-010.** A spec's **manual** acceptance criteria are read against what its
  own phase can observe. A checklist row that depends on a capability a later
  phase delivers is **deferred**: it names the spec it waits on, is marked as
  deferred in the checklist itself, and is signed in the phase that delivers
  that capability. R-004's "the spec's ACs are checked off in the PR body" is
  read against the rows the spec's own phase can observe.

  A deferred row is not an unchecked box, and the distinction MUST be visible
  in the checklist rather than inferred from it: an unchecked box is an
  unknown, a deferred row is a scheduled one. The spec that unblocks a
  deferred row carries its sign-off as part of that spec's own acceptance, so
  the debt is scheduled rather than forgotten.

  This is R-009's treatment applied to the manual half of acceptance, for the
  same reason: an exit criterion naming something only a later phase creates
  is unsatisfiable, and no amount of testing discipline makes it otherwise.
  See D-5.

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

- **D-4 (2026-09-06, amendment, approved by the maintainer in session).**
  Phase 1's exit criterion asked for `make burndown` at zero for all four of
  its specs. Spec 015 cannot satisfy that in phase 1 and never could: seven of
  its twelve owned units are forward `constrains` edges naming files that
  phases 2 to 4 create. Its own AC-1 says as much ("once they exist").

  The prose was the smaller half of the problem. `registry plan` schedules on
  `depends_on`, and four specs carried `- "015-privacy-boundary"` as a phase
  gate: 004, 010, 016 and 017. That set is not arbitrary. It is exactly the
  set of specs that own a unit 015 constrains, and each pair is a **cycle**:

  | Spec | Owns, and 015 constrains |
  |---|---|
  | 004 | `tauri.conf.json`, `capabilities/` |
  | 010 | `anthropic.rs`, `secrets.rs` |
  | 016 | `logging.rs` |
  | 017 | `tauri.conf.json` |

  Measured rather than reasoned about: with 015 `in-progress`, `registry plan`
  reported `ready: 1, blocked: 12`, and 004's only blocker was 015. With 015
  flipped to `complete` instead, `spec-spine index check` exited 2 with six
  `I-004` and one `I-007`. Both states are red, and nothing after phase 1 could
  start in either. The plan had deadlocked itself.

  R-008 removes the inverted edges and forbids the shape; R-009 says what a
  constraint spec's `implementation` field actually tracks. Neither weakens the
  privacy boundary: 015's authority over those seven units is carried by the
  `constrains` edges, which point the correct way, are acyclic, and are what
  the coupling gate reads at merge. What changed is that a spec no longer waits
  for a constraint on itself to be satisfied before it may create the thing
  being constrained.

- **D-5 (2026-09-07, R-010, approved by the maintainer in session).** Spec 004
  AC-3 requires a manual checklist covering FR-002 to FR-005, signed on both
  platforms, before the spec flips to `complete`. Most of that checklist
  cannot be observed in phase 2 on either platform:

  | Checklist row | Cannot be observed until |
  |---|---|
  | FR-001, window flags as realized by the OS | 005 |
  | FR-002, click-through | 005 |
  | FR-003's Interact row, the overlay accepting clicks | 005 |
  | FR-004's Windows half, no taskbar button | 005 |
  | FR-005 and §3.5, the permission prompt on arming | 019 |

  Spec 004 §3.2 forbids showing the overlay before capture exclusion has been
  applied. That is spec 005, in phase 3. Arming, which is what would trigger
  the permission flow, is spec 019's runtime. Phase 3 requires phase 2
  complete, which requires 004 complete, which requires those rows signed.
  The plan had deadlocked itself again, in the same shape as D-4 and for the
  same underlying reason: an acceptance criterion that names a capability a
  later phase delivers.

  Measured rather than reasoned about: the overlay is created `visible: false`,
  and no code path in `butler-desktop` calls `show()`, so there is no sequence
  of actions a tester could perform that would make those rows observable. The
  rows that *are* observable in phase 2 (shortcut registration, the absence of
  a Dock tile, the tray menu) were verified on macOS while writing this.

  Two alternatives were rejected. Narrowing 004's AC-3 in place would leave
  the next spec to rediscover the same problem with no rule to lean on; 005
  and 017 both have manual acceptance and would have hit it. Holding 004
  `in-progress` until phase 3 and reworking phase 2's dependency edges would
  leave one spec open across two phases and drain R-002 of its meaning. R-010
  generalizes instead, exactly as R-009 did for the machine half, and leaves
  every acceptance criterion's substance intact: nothing is dropped, the
  deferred rows are scheduled onto the spec that makes them observable.

  **Not resolved here.** §2's phase 2 exit criterion has the same defect one
  level up: it asks that the "app launches on both platforms to a transparent,
  click-through overlay showing `Disarmed`", which is the same visible overlay
  §3.2 forbids until 005 has applied exclusion. R-010 governs a *spec's*
  acceptance criteria, not this plan's own exit column, so it does not reach
  that row. It does not bind until phase 2 ends, which is after 019, and it is
  left for the maintainer rather than folded into this amendment.
