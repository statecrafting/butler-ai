# butler-ai constitution

Durable principles that govern this corpus. This document is **tier 2**: it is
subordinate to the bootstrap spec (`specs/000-butler-bootstrap/spec.md`), whose
`unamendable` anchors it may not contradict, and it governs all ordinary specs
(`001`+).

**Normative hierarchy (highest wins):**

1. `specs/000-butler-bootstrap/spec.md`: the bootstrap spec. Non-overridable.
2. `standards/spec/constitution.md`: this document.
3. `standards/spec/contract.md`: a normative summary of the bootstrap spec.
4. Ordinary specs (`001`+): feature-level claims within this envelope.

When two specs conflict, resolve in this order, then by the typed authority
graph (`supersedes` transfers authority; `amends` patches in place; `constrains`
asserts an invariant every other owner of the unit must respect).

---

## I. Markdown-only authored truth

Authored truth lives only in markdown with YAML frontmatter. There is no
authoritative hand-authored JSON, YAML, or TOML data file that describes the
system; `spec-spine.toml` configures the compiler and is itself owned by the
bootstrap spec. If a fact governs butler-ai, it is written in a `spec.md` (or a
`standards/` document), never in a derived artifact, a README, or a plan file.
*(Bootstrap anchor: `markdown-truth-boundary`.)*

## II. Compiler-owned JSON machine truth

All machine-consumable truth is emitted by `spec-spine compile` and `spec-spine
index` into `.derived/`, and is read only through the `spec-spine` binary.
Hand-editing a derived artifact is a workflow violation, and ad-hoc parsing of
one (`jq`/`awk`/`sed`/`python`) is equally forbidden: typed reads make schema
drift fail at the deserializer, with a clean error, instead of silently
downstream. *(Bootstrap anchor: `json-truth-boundary`.)*

## III. Specify first, then build

butler-ai is specified before it is built. A capability exists in this
repository first as a spec that names the crates, files, modules and symbols it
will own; the code arrives afterwards, under that claim. The indexer counts
every declared-but-unbuilt unit as a `W-001` warning, and that list is the build
burn-down. A spec moves to `implementation: complete` only when its warning
count is zero, at which point an unresolved unit becomes a hard error. Code that
no spec specifically claims cannot merge: the ownership ratchet is on from the
first commit. *(Bootstrap anchor: `ownership-ratchet`.)*

## IV. Determinism and validation

Every artifact-producing function is a pure function of `(config, file
contents)`; the same inputs produce byte-identical output. Validation is
mechanical: the compiler reports violations, the lint reports conformance
warnings (and CI fails on a warning), the coupling gate reports drift. This
principle extends into the product: the pipeline's core (state machine, change
detection, pacing policy) is specified as pure functions of typed inputs so its
behavior is testable without an operating system.
*(Bootstrap anchor: `determinism-requirement`.)*

## V. The user's screen is the user's data

butler-ai reads the user's display. Everything derived from it (frames, OCR
text, prompts, answers) is the user's data and is handled under the privacy
boundary (`specs/015-privacy-boundary`): held in memory only, never persisted by
default, sent to exactly one user-configured inference endpoint and nowhere
else, redacted of secret-shaped content before it leaves the machine, and never
logged. A spec that needs an exception declares it as a `constrains`-aware edit
to spec 015, in the open, never as a side effect of a feature. This principle
is amendable only by a spec that `amends` 015 and is approved by a human.

## VI. Honest capability, verified at runtime

The overlay's exclusion from screen capture is an operating-system capability
that butler-ai requests, not one it can guarantee. The product verifies the
exclusion at runtime and reports a degraded state truthfully rather than
assuming the platform honoured the request. The threat model
(`docs/threat-model.md`) names what exclusion does and does not defend against.
Specs describe capabilities in terms of what is verified, not what is hoped.

## VII. Legacy as evidence

Code that predates a governing spec is not a violation to be erased; it is
evidence. A spec that claims authority over pre-existing code declares
`origin.retroactive: true`. butler-ai has no such code today; this principle
exists so that a future import (a vendored crate, a prototype) is recorded
honestly rather than laundered through a fresh `establishes` claim.

---

## Amendment

This constitution may be amended by an ordinary spec that `amends`
`000-butler-bootstrap` and is approved, **provided** the amendment does not
contradict a `specs/000` `unamendable` anchor. The bootstrap spec's freeze
surface is the hard boundary; everything else in this document is revisable
through the normal governed flow.
