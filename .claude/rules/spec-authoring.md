---
paths:
  - "specs/**"
  - "standards/**"
---

# Spec authoring (butler-ai)

Read `standards/spec/contract.md` first; `specs/000-butler-bootstrap/spec.md`
is tier 1 and its `unamendable` anchors cannot be contradicted.

## Frontmatter

- `id` equals the directory name; the next ordinal comes from
  `spec-spine registry list --ids-only`, never from `ls`.
- `domain` and `kind` are required (closed taxonomies in `spec-spine.toml`);
  `platforms` and `phase` are required on product specs; `phase` MUST match
  `specs/018-implementation-sequencing`.
- Start every new spec as `status: draft`, `implementation: pending`. Only a
  human flips `status` to `approved`. Only a PR whose `make burndown` shows
  zero for the spec flips `implementation` to `complete`.

## Choosing edges

- `establishes`: units this spec brings into being in territory it owns (its
  own crate, its own files).
- `extends { spec, unit }`: a file or symbol this spec adds **inside another
  spec's crate** (e.g. `delta.rs` inside 009's `butler-core`). Never
  `establishes` a path under a crate you do not own.
- `refines { aspect, unit }`: tightening behavior of a unit another spec owns
  (name the aspect; it is a review key, not decoration).
- `co_authority`: only for `section` units (a Makefile target, a workflow
  job) that two specs legitimately share.
- `constrains { flavor: invariant-freeze, unit, note }`: an invariant on a
  unit another spec owns; the note is normative. `constrains { kind:
  sequencing-plan, target_specs }` for ordering plans (spec 018 only).
- `references`: context only; the gate ignores it. Point at `docs/`.
- Claim the **crate**, the **files**, and the **load-bearing symbols** (a
  reducer, a trait, a policy function). Symbol ids are `crate::module::Item`
  for top-level items only; put such items directly in the module file.

## Body

Follow the template sections: Purpose, Territory, Behavior (MUST/SHOULD/MAY,
naming the types the symbol units resolve to), Functional requirements
(`FR-`), Acceptance criteria (`AC-`, mechanically checkable), Out of scope.

## After every edit

```sh
spec-spine compile && spec-spine lint --fail-on-warn && spec-spine index && spec-spine index check
```

Commit the regenerated `.derived/` shards with the spec. Never edit a spec to
make `spec-spine couple` pass on code that contradicts it
(`adversarial-prompt-refusal.md`).
