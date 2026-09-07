---
id: "000-butler-bootstrap"
title: "Bootstrap spec system for butler-ai (markdown → compiled authority ledger)"
status: approved
kind: "constitutional-bootstrap"
domain: "governance"
created: "2026-09-01"
implementation: complete
owner: "butler-ai maintainers"
risk: low
platforms: "all"
phase: 0
authors:
  - "butler-ai maintainers"
unamendable:
  - "markdown-truth-boundary"
  - "json-truth-boundary"
  - "determinism-requirement"
  - "directory-name-equals-id"
  - "typed-authority-graph"
  - "refusal-rule"
  - "ownership-ratchet"
establishes:
  - "spec-spine.toml"
  - "standards/spec/contract.md"
  - { kind: directory, path: "standards/spec/templates/" }
  - ".claude/rules/orchestrator-rules.md"
  - ".claude/rules/governed-artifact-reads.md"
  - ".claude/rules/adversarial-prompt-refusal.md"
references:
  - { unit: { kind: file, path: "standards/spec/constitution.md" }, role: "tier-2 principles" }
  - { unit: { kind: file, path: "docs/architecture.md" }, role: "context" }
summary: >
  Foundational contract for the butler-ai corpus. Authored truth lives only in
  markdown with YAML frontmatter; machine-consumable truth is spec-spine
  compiled JSON only; every artifact-producing step is a pure, deterministic
  function of (config, file contents); a typed authority graph governs who may
  change what; and, specific to this greenfield repository, no source file may
  ever merge without a spec that specifically claims it. This spec defines what
  a spec IS for butler-ai. It is tier-1: its `unamendable` anchors are
  non-overridable. It owns the compiler configuration, the normative contract,
  the spec templates, and the three floor rules every agent loads first.
---

# 000: Bootstrap spec system for butler-ai

This is the spec that defines what a spec is in this repository. It adopts the
spec-spine bootstrap model (statecrafting/spec-spine, spec 000) and adds one
invariant of its own, the ownership ratchet, because butler-ai is specified
before it is built and can therefore refuse unclaimed code from its first
commit. It sits at the top of the constitutional hierarchy; see
`standards/spec/constitution.md`, which is subordinate to this document.

## 1. The authoring/derived boundary

There are exactly two kinds of truth in this repository.

- **Authored truth** lives only in markdown (`specs/NNN-slug/spec.md`, and the
  documents under `standards/spec/`), with YAML frontmatter blocks permitted
  inside the markdown. Humans and authorized agents write authored truth.
  *(anchor: `markdown-truth-boundary`)*
- **Machine-consumable truth** is emitted only by `spec-spine compile` and
  `spec-spine index`, as JSON, into `.derived/`. No hand-authored JSON is
  authoritative; no consumer may treat hand-edited JSON as truth.
  *(anchor: `json-truth-boundary`)*

Corollary: **typed reads or nothing.** Compiled JSON is read only through the
`spec-spine` binary (`registry`, `index` subcommands). Ad-hoc parsing of
compiled JSON (`jq`/`awk`/`sed`/`python`/hand-rolled readers) is a workflow
violation. `.claude/rules/governed-artifact-reads.md` encodes this for agents.

## 2. Identity: directory name equals id

A spec's directory under `specs/` is named exactly `NNN-slug`, where `NNN` is a
three-digit zero-padded ordinal and `slug` is a kebab-case name. The spec's `id`
frontmatter field MUST equal that directory name. The numeric prefix is unique
across the corpus. New specs take the next unused ordinal (`make spec-new` and
the `/spec-new` skill compute it). *(anchor: `directory-name-equals-id`)*

## 3. Frontmatter grammar

Every `spec.md` begins with a YAML frontmatter block delimited by `---` fences.

**Required keys** (absence is a compile error): `id`, `title`, `status`,
`created`, `summary`.

**Required in this corpus by lint** (absence is a warning, and CI fails on
warnings): `domain` from `[domains] allowed` and `kind` from `[kind] allowed`
in `spec-spine.toml`. The taxonomies are closed on purpose: a spec that cannot
name its domain has not decided which layer it lives in.

- `status` ∈ { `draft`, `approved`, `superseded`, `retired` }.
  - `superseded` requires `superseded_by` resolving to an existing id.
  - `retired` requires `retirement_rationale`.
- `created` is an ISO date (`YYYY-MM-DD`).

**Optional descriptive keys**: `authors`, `owner`, `risk`
(`low`/`medium`/`high`/`critical`), `depends_on`, `code_aliases`,
`feature_branch`, `implementation` (`pending`/`in-progress`/`complete`/`n-a`/
`deferred`).

**Declared extra keys** (`frontmatter.extra_known_keys`): `platforms` (a list
from `windows`, `macos`, or the string `all`) and `phase` (an integer assigned
by `018-implementation-sequencing`). They carry any YAML value through the
compiler and are read by the harness, never by the gate.

**Freeze surface**: `unamendable` is a list of anchors that no amendment may
alter. The list in *this* frontmatter is the authoritative freeze surface.

**Unknown keys** overflow into `extra_frontmatter` (scalars and string-lists
only). Promote a key by adding it to `extra_known_keys` in a change that also
edits this spec (it owns `spec-spine.toml`).

## 4. The typed authority graph

A spec declares, in frontmatter, **typed edges** to the rest of the corpus and
the **authority units** it owns. Authority over any unit is *derived by walking
the graph*, never declared directly. *(anchor: `typed-authority-graph`)*

### 4.1 Edges: eight types, seven ownership-bearing

| Edge | Ownership? | Meaning in butler-ai |
|---|---|---|
| `establishes` | yes | first brings a unit into being (a crate, a module, a file) |
| `extends` | yes | adds surface to a predecessor's unit (a new event on the IPC contract) |
| `refines` | yes | tightens behavior on a named aspect (exclusion refines the window) |
| `supersedes` | yes | replaces a predecessor; inherits its current authority |
| `amends` | yes | patches a predecessor in place; co-authority over its `spec.md` |
| `co_authority` | yes | shares a named section (a Makefile target, a workflow job) |
| `constrains` | yes | asserts an invariant others must respect (the privacy boundary) |
| `references` | **no** | points without claiming authority (the gate ignores it) |

`origin` is a bootstrap marker, **not** an edge.

### 4.2 Authority units

Ownership resolves at six granularities: `file` (bare string shorthand;
trailing slash denotes the subtree), `section` (`{file, anchor}`: a Makefile
target, a Markdown heading slug, a `region:` marker, a workflow keypath such as
`jobs.govern` or `permissions`, or a manifest table path such as
`package.metadata.butler`), `symbol` (`{id}`, Rust and TypeScript, resolved by
the indexer), `directory` (`{path}`), `crate` (`{id}`, a Cargo package name or
an npm package name), and `module` (`{id}`, a `::`-qualified Rust module path).

The convention in this corpus: a feature spec claims its **crate** (the floor),
plus the **files** it establishes, plus the **symbols** that carry its
load-bearing contracts (a reducer, a trait, a policy function). The symbol
claims are what let two specs share a file without sharing authority over every
line of it.

### 4.3 Resolution and amends-awareness

"Who currently owns unit X" is a derived query over the graph, computed by the
indexer, not a runtime guess. A `supersedes` edge transfers a predecessor's
current authority; `establishes` records historical origin. An `amends` edge
grants the amender co-authority over the predecessor's `spec.md`, but only
expands an already-firing owner set; it never silently enrolls a new owner.

## 5. The compiled artifacts

- **Registry** (`.derived/spec-registry/by-spec/<id>.json`, spec-as-source): for
  each spec, its status, relationships, claimed units, and a validation report.
- **Codebase index** (`.derived/codebase-index/by-spec/<id>.json`,
  `by-package/<slug>.json`, code-as-source): for each path/section/symbol,
  which spec(s) currently claim it, plus a content hash for staleness.

They are inverses. The coupling gate joins them at PR time and refuses the
merge if they disagree. Both shard trees are committed; `build-meta.json` is
not.

## 6. Determinism

Every artifact-producing function is a pure function of `(config, file
contents)`: the same committed inputs MUST produce byte-identical output. No
ambient clock or environment reads enter an artifact; the sole exception is the
wall-clock `builtAt` in `build-meta.json`, excluded from every check.
*(anchor: `determinism-requirement`)*

The staleness hash folds every input that can change a resolved artifact: the
corpus, the manifests, the files named in `[index] extra_hashed_inputs` (the
harness, the workflows, the Tauri security surface), and every source file
backing a resolved symbol or section span.

## 7. The guardrails

1. A **deterministic compiler** mints the registry (§5, §6).
2. A **coupling gate** at PR time refuses code/spec drift (`C-001`).
3. **Typed reads or nothing** for compiled JSON (§1 corollary).
4. A **refusal rule** at prompt time stops an agent from "resolving" a coupling
   failure by quietly editing the contract to match the code it just wrote. The
   agent MUST surface the contradiction and let a human (or an agent with
   explicit authority) decide. *(anchor: `refusal-rule`)*
5. The **ownership ratchet**: `[coupling] require_ownership = true` makes a
   changed source file with no specific owning spec a `C-002` refusal, and CI
   runs `spec-spine index coverage --fail-on-untraced` so the whole tree stays
   at 100% specifically claimed. A manifest floor alone never counts as a
   claim. This flag MUST NOT be turned off and the CI step MUST NOT be removed;
   a spec that needs an unclaimed file claims it instead.
   *(anchor: `ownership-ratchet`)*

The coupling gate is the PR-time defense; the refusal rule is the prompt-time
defense; the ratchet closes the third hole, code that arrives claimed by nobody.

## 8. Lifecycle in a specify-first corpus

Every product spec is authored before its code. The indexer reports each
owning unit that does not yet resolve as a counted `W-001` warning when the
owning spec is `draft` or `implementation: pending`/`in-progress`, and as a hard
error once the spec is `approved` + `complete`. The rules:

- A spec MAY be `approved` while `implementation: pending`: approval ratifies
  the design, not the code.
- A spec MUST NOT be marked `implementation: complete` while any of its owning
  units is unresolved (`make burndown` reports zero for that spec).
- `make burndown` is the authoritative build to-do list; nothing else is.

## 9. Territory of this spec

- `spec-spine.toml`: the compiler configuration. Any change to a taxonomy, a
  hashed input, a slice, the bypass list, or the ratchet is a change to this
  spec.
- `standards/spec/contract.md` and `standards/spec/templates/`: the normative
  summary and the authoring templates.
- The three floor rules under `.claude/rules/` that encode guardrails 3 and 4
  for agents. The rest of the harness is spec 002's.
- `standards/spec/constitution.md` is referenced, not owned: the bypass floor
  leaves it editable through the amendment path in its own §Amendment.

## 10. Bootstrap order

1. This spec is authored by hand, before any code exists.
2. `spec-spine` (an installed dependency, never vendored) compiles it.
3. Specs `001`+ declare the system; the indexer's `W-001` list is the plan.
4. Code lands under those claims, phase by phase (spec 018), each phase
   flipping its specs to `complete` at zero warnings.

## 11. Status

This is a `constitutional-bootstrap` spec. Its `unamendable` anchors are
frozen: no amendment may alter the authoring/derived boundary, the identity
rule, the typed-authority-graph principle, the determinism requirement, the
refusal rule, or the ownership ratchet. Amendments may add surface elsewhere.

## 12. Verification

The bootstrap spec's own claims are the compiler's contract: the authoring and
derived boundary (§1), determinism (§6), and the guardrails (§7). Each is
mechanically checkable from a clean checkout.

```verify:cli
# §1 + §5: the committed shards are exactly what the corpus compiles to.
# `--check` compiles in memory and compares without writing, so a pass proves
# the ledger is a pure function of the authored markdown (§6 determinism).
spec-spine compile --check
spec-spine index check
# §3: every spec's frontmatter satisfies the grammar and the closed taxonomies.
spec-spine lint --fail-on-warn
# §7.5: the ownership ratchet, on since the first commit.
spec-spine index coverage --fail-on-untraced
# §1: authored truth is markdown only. No hand-authored JSON under specs/.
sh -c '! find specs -name "*.json" | grep -q .'
```
