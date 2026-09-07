---
id: "NNN-slug"                 # MUST equal the directory name; NNN = unique 3-digit ordinal
title: "Short imperative title"
status: draft                  # draft | approved | superseded | retired
kind: "feature"                # constitutional-bootstrap | feature | tooling | constraint | plan
domain: "pipeline"             # governance | platform | pipeline | assistant | ui | distribution
created: "YYYY-MM-DD"
implementation: pending        # pending | in-progress | complete | n-a | deferred
owner: "name"
risk: medium                   # low | medium | high | critical
platforms: ["windows", "macos"]  # or "all"
phase: 0                       # from specs/018-implementation-sequencing
depends_on: ["NNN-other"]
summary: >
  One short paragraph: what territory this spec claims and why it exists.
# --- typed edges (declare territory + relationships) ---
# establishes:
#   - { kind: crate, id: "butler-core" }
#   - "crates/butler-core/src/thing.rs"          # bare string == { kind: file, path: ... }
#   - { kind: directory, path: "crates/butler-core/src/thing/" }
#   - { kind: module, id: "butler_core::thing" }
#   - { kind: symbol, id: "butler_core::thing::run" }
#   - { kind: section, file: "Makefile", anchor: "ci" }
#   - { kind: section, file: ".github/workflows/ci.yml", anchor: "jobs.build" }
# extends:
#   - { spec: "NNN-predecessor", unit: "crates/x/src/added.rs", nature: additive }
# refines:
#   - { aspect: "error-handling", unit: { kind: symbol, id: "butler_core::run" } }
# supersedes: ["NNN-predecessor"]
# amends: ["NNN-predecessor"]
# co_authority:
#   - { unit: { kind: section, file: "Makefile", anchor: "ci" }, with_specs: ["NNN-other"] }
# constrains:
#   - { flavor: invariant-freeze, unit: "crates/x/src/api.rs", note: "additive only" }
#   - { kind: sequencing-plan, target_specs: ["NNN-a", "NNN-b"], note: "a before b" }
# references:
#   - { unit: { kind: file, path: "docs/architecture.md" }, role: "context" }
# --- lifecycle / amendment (as applicable) ---
# superseded_by: "NNN-successor"     # required when status: superseded
# retirement_rationale: "why"        # required when status: retired
# amends_sections: ["anchor"]
# unamendable: ["anchor"]
---

# NNN: Title

## 1. Purpose

What problem this spec solves and what it owns. Cite the constitution principle
or the predecessor spec it descends from.

## 2. Territory

The units this spec claims authority over (mirrors the frontmatter edges, in
prose), and the units it deliberately does not claim.

## 3. Behavior

What the governed code MUST / SHOULD / MAY do. Name the types, traits and
functions the symbol units will resolve to.

## 4. Functional requirements

- **FR-001.** ...

## 5. Acceptance criteria

- **AC-1.** A mechanically checkable statement (a test name, a command and its
  exit code, an observable).

## 6. Out of scope

What this spec deliberately does not cover, and which spec covers it instead.

## 7. Verification

The commands that prove this spec's acceptance criteria on a merged checkout.
`spec-spine verify <id>` runs every non-comment line of the `verify:cli`
fences below, from the repository root, in order, stopping at the first non-zero
exit. Keep them mechanical and independent of the machine they run on: no
absolute paths, no network beyond what the gate already needs, no interactive
prompts. A spec MUST carry at least one `verify:cli` command before it flips to
`implementation: complete` (spec 002 §3.7).

```verify:cli
# Each line is one command; a failure stops the run.
make spine
```
