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
  - ".claude/rules/derived-artifacts-are-compiler-output.md"
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
  the spec templates, and the four floor rules every agent loads first.
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
- The four floor rules under `.claude/rules/` that encode guardrails 3 and 4
  for agents. Three are unconditional; `derived-artifacts-are-compiler-output.md`
  is paths-scoped to the derived directory and reinforces the other two at the
  moment a shard is actually open. The rest of the harness is spec 002's.
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

## 12. Resolved decisions

- **D-1 (2026-09-07, the floor rules gain their carve-outs).** The three rules
  this spec establishes are unchanged in force and clearer in scope, ported
  from the spec-spine kit at its spec 047. `governed-artifact-reads.md`
  forbade ad-hoc parsing of the derived JSON without saying that parsing a
  `spec-spine` verb's own `--json` output is a typed read; read literally it
  outlawed `registry plan --json`, which the `## New Sessions` protocol in
  `AGENTS.md` has always run, and `make burndown` now runs too (spec 002
  FR-006). `adversarial-prompt-refusal.md` said never to edit the owning spec
  to clear the gate, but under §7.5's ratchet adding a created file to
  `establishes` *is* that edit, so the rule as written was unimplementable and
  left agents to guess; it now names the two edits that are always legitimate
  and states that a waiver is a human instrument an agent never writes on its
  own authority. `orchestrator-rules.md` requires the regenerated shards to be
  committed with the change that made them stale, not merely recomputed.
  Guardrails 3 and 4 are untouched and no `unamendable` anchor is affected:
  this narrows ambiguity, it does not move the boundary.
  `standards/spec/templates/spec-template.md` changes in the same pass, from
  `scripts/verify-spec.sh <id>` to `spec-spine verify <id>` (spec 002 D-6).
- **D-2 (2026-09-08, six globs that matched nothing).** `[index]
  extra_hashed_inputs` carried six entries in the form `dir/**`. In the `glob`
  crate `**` matches a sequence of path *components*, so those patterns
  enumerate directories, and the hasher keeps only entries that are files:
  every one of them matched nothing, silently, while the comment above them
  said the harness and the gates were folded into the staleness hash. Measured
  on this corpus before the fix, not inferred:

  ```console
  $ printf '\n<!-- probe -->\n' >> standards/spec/constitution.md
  $ spec-spine index check      → index is fresh
  $ spec-spine compile --check  → spec-registry is fresh: 20 shard(s)
  ```

  So the constitution, the contract, the spec templates, all three workflows
  and every `.claude/` agent, rule and skill had never contributed to any
  content hash. Fixed to the working `dir/**/*` form and extended to the
  claimed governance files that no glob covered (`CODEOWNERS`,
  `.gitattributes`, `.github/dependabot.yml`, `.githooks/`, `scripts/`,
  `rust-toolchain.toml`, `deny.toml`, `package.json`, `pnpm-workspace.yaml`,
  `.nvmrc`). This restales all 26 shards exactly once. Surfaced by `L-008`
  (spec-spine spec 057) on the 0.17.0 bump; the same defect was found in
  spec-spine's own config and in the shipped default it came from (spec-spine
  spec 069). No `unamendable` anchor is affected: putting claimed governance
  files into the ledger for the first time strengthens `json-truth-boundary`
  and `determinism-requirement` rather than moving either boundary.
- **D-3 (2026-09-08, forty-nine unwitnessed source claims, declared).** A
  `file` unit carries no span, and only span-backing sources are folded into a
  shard hash, so the bare `file` claims on 21 Rust sources under `crates/` and
  28 files under `apps/desktop/` are witnessed by nothing: `index check` will
  not call the index stale for an edit to one. `[lint] unwitnessed_allowed`
  (spec-spine spec 057 §3.5) declares them deliberate rather than turning the
  check off; `index check` keeps reporting the count, so the gap stays visible.
  They are not undefended: `require_ownership` is on, so `couple` refuses a
  changed source file whose owning spec did not change in the same PR, which is
  a stricter check than staleness. What the gap costs is written down: the
  index does not notice the edit, and a corpus attestation would cover a ledger
  that never read those bytes. `**/README.md` is declared for the same reason
  it sits in `[coupling] bypass_prefixes`: per-app prose is documentation, not
  authority, and hashing it would force a full reindex on every prose edit.
  This is butler-ai making the decision spec-spine 057 §3.2 says a corpus
  should make deliberately rather than inherit.
- **D-4 (2026-09-08, the CLI is pinned in the corpus, not only in CI).**
  `[meta] required_version = ">=0.17.0"` (spec-spine spec 062). The binary now
  checks itself on every run and refuses with exit 3, which makes the version
  question answerable before any exit code is interpreted: older CLIs spent
  exit 2 on an unknown flag, the same code this tool spends on staleness, and a
  session told its shards are stale when they are not regenerates artifacts
  that were already correct. It constrains the binary only; `SPEC_SPINE_VERSION`
  in the `Makefile` (spec 002) is what CI and `make setup` install, and the two
  move together.
- **D-5 (2026-09-08, a fourth floor rule).** `.claude/rules/derived-artifacts-are-compiler-output.md`
  joins the three, ported from the spec-spine kit at its spec 068. It is
  paths-scoped to `.derived/**` and says nothing new: hand-editing a shard is a
  workflow violation, ad-hoc parsing of one is forbidden, parsing a verb's
  `--json` output is a typed read. It does not replace
  `governed-artifact-reads.md` and cannot. That rule is unconditional because
  the mistake it prevents is reaching for `jq` *instead of* the subcommand, and
  an agent about to make that mistake may never open a file under `.derived/`,
  so a paths-scoped rule would never load. This one reinforces it at the moment
  somebody actually has a shard open. Guardrail 4 is unchanged in force.

## 13. Verification

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
# D-2: every hashed-input glob ends in a file component. `dir/**` enumerates
# directories and the hasher keeps only files, so such an entry silently
# matches nothing. Read through `config show` (a typed read), not the file.
spec-spine config show --json | python3 -c "import json,sys; g=json.load(sys.stdin)['index']['extra_hashed_inputs']; bad=[p for p in g if p.endswith('/**')]; assert not bad, bad"
spec-spine config show --json | python3 -c "import json,sys; s=json.load(sys.stdin)['index']['slices']; bad=[p for v in s.values() for p in v if p.endswith('/**')]; assert not bad, bad"
# D-2: the governance surface is actually witnessed now. Editing the
# constitution must stale the index; before the fix both gates said fresh.
sh -c 'b="${TMPDIR:-/tmp}/butler-000-probe.bak"; cp standards/spec/constitution.md "$b" && printf "\n<!-- probe -->\n" >> standards/spec/constitution.md; spec-spine index check >/dev/null 2>&1; rc=$?; cp "$b" standards/spec/constitution.md; rm -f "$b"; test "$rc" -ne 0'
# D-3: the unwitnessed remainder is declared, not silenced. The count is still
# reported, and the lint is green at the tier CI gates on.
spec-spine index check
spec-spine lint --fail-on-warn
# D-4: the corpus pins the binary it is governed by.
spec-spine config show --json | python3 -c "import json,sys; v=json.load(sys.stdin)['meta']['required_version']; assert v, v"
# D-5: the fourth floor rule exists and is paths-scoped to the derived tree.
sh -c 'head -4 .claude/rules/derived-artifacts-are-compiler-output.md | grep -q "\.derived/\*\*"'
```
