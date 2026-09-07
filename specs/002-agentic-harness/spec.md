---
id: "002-agentic-harness"
title: "Agentic engineering harness: skills, agents, rules, hooks, and the session protocol"
status: approved
kind: "tooling"
domain: "governance"
created: "2026-09-01"
implementation: complete
owner: "butler-ai maintainers"
risk: medium
platforms: "all"
phase: 0
depends_on:
  - "000-butler-bootstrap"
establishes:
  - "AGENTS.md"
  - "CLAUDE.md"
  - ".mcp.json"
  - "Makefile"
  - ".claude/settings.json"
  - { kind: directory, path: ".claude/skills/" }
  - { kind: directory, path: ".claude/agents/" }
  - ".claude/rules/spec-authoring.md"
  - ".claude/rules/rust-crates.md"
  - ".claude/rules/overlay-frontend.md"
  - ".claude/rules/build-commands.md"
  - "scripts/verify-spec.sh"
co_authority:
  - { unit: { kind: section, file: "Makefile", anchor: "build" }, with_specs: ["001-workspace-layout"] }
  - { unit: { kind: section, file: "Makefile", anchor: "test" }, with_specs: ["001-workspace-layout"] }
  - { unit: { kind: section, file: "Makefile", anchor: "lint" }, with_specs: ["001-workspace-layout"] }
references:
  - { unit: { kind: file, path: ".claude/rules/orchestrator-rules.md" }, role: "floor rule (owned by 000)" }
  - { unit: { kind: file, path: ".claude/rules/governed-artifact-reads.md" }, role: "floor rule (owned by 000)" }
  - { unit: { kind: file, path: ".claude/rules/adversarial-prompt-refusal.md" }, role: "floor rule (owned by 000)" }
summary: >
  The governed-development loop for agents working in this repository: the
  cross-agent session protocol (AGENTS.md), the Claude Code skills (`/init`,
  `/setup`, `/spec-new`, `/burndown`, `/implement-plan`, `/validate-and-fix`,
  `/code-review`, `/commit`, `/ship`, `/shepherd`, `/cleanup`, `/research`,
  `/refactor-claude-md`), the pipeline agents plus two domain specialists
  (`spec-author`, `tauri-expert`), the paths-scoped context rules, the
  deterministic hooks in `.claude/settings.json`, and the Makefile targets the
  skills call. Adapted from the spec-spine kit (spec-spine spec 029) and
  specialized to butler-ai's stack and its specify-first workflow. Owns the
  harness surface; the three floor rules stay with spec 000.
---

# 002: Agentic engineering harness

## 1. Purpose

butler-ai will be built largely by agents working under the coupling gate. The
harness is what makes that safe: a session protocol that loads the corpus
state before any work, skills that chain the gate verbs into everyday actions,
agents with explicit mutation scopes, rules that auto-load the conventions of
whichever surface an agent is editing, and hooks that recompute derived
artifacts deterministically so a session cannot leave the tree stale.

The harness is vendored from the spec-spine kit and specialized here. It is a
governed surface: every file under it is claimed, hashed into the index, and
gated, because an unreviewed change to a hook or a permission allow-list is a
change to what an agent may do.

## 2. Territory

- `AGENTS.md`: the New Sessions protocol (single source of truth for `/init`)
  plus the agent and command inventory.
- `CLAUDE.md`: project overview and conventions loaded by Claude Code.
- `.mcp.json`: MCP server declarations (empty at bootstrap).
- `Makefile`: the command contract the skills call. `setup`, `spine`, `ci`,
  `pr-prep`, `burndown`, `coverage`, `spec-new` are this spec's; `build`,
  `test`, `lint` are co-owned with spec 001, which defines their content.
- `.claude/settings.json`: permissions and the four hooks.
- `.claude/skills/`: thirteen skills (§3.2).
- `.claude/agents/`: six agents (§3.3).
- `.claude/rules/{spec-authoring,rust-crates,overlay-frontend,build-commands}.md`:
  paths-scoped context rules. The three floor rules are spec 000's.

## 3. Behavior

### 3.1 Session protocol (`AGENTS.md` § New Sessions)

`/init` MUST read the protocol from `AGENTS.md` and execute it, never
duplicate it. The protocol MUST: load the three floor rules first; dispatch the
parallel reads (`CLAUDE.md`, `README.md`, contract, constitution, `spec-spine
compile --check`, `spec-spine index check`, `registry status-report`,
`registry list --ids-only`, the burn-down via `index render`, surface listings,
recent git history); and emit an `## initialized: butler-ai` block with a
`## lifecycle:` and a `## burndown:` sub-section. Freshness gates are reported,
never repaired, by `/init`.

### 3.2 Skills

| Skill | Role | Origin |
|---|---|---|
| `init` | execute the AGENTS.md protocol | kit |
| `setup` | install spec-spine, verify the loop | kit, pinned version |
| `spec-new` | scaffold `specs/NNN-slug/spec.md` from the template with the next ordinal, taxonomy and phase prompts | butler |
| `burndown` | list unresolved owning units (`W-001`) per spec via `spec-spine index render`; propose the next build step from spec 018 | butler |
| `implement-plan` | execute a plan file with checkpoints | kit |
| `validate-and-fix` | run `make ci`, fix by severity | kit, `make ci` |
| `code-review` | correctness + spec drift findings | kit |
| `commit` | conventional commit | kit |
| `ship` | gate → review → commit → PR | kit, `make pr-prep` |
| `shepherd` | drive an open PR to merge: CI, review threads, merge queue | butler |
| `cleanup` | dead code and duplicates | kit, cargo/knip commands |
| `research` | parallel research | kit |
| `refactor-claude-md` | tighten `CLAUDE.md` | kit |

Every skill that reads compiled state MUST do so through `spec-spine`
subcommands. Every skill that mutates MUST stop at the checkpoints
`orchestrator-rules.md` names.

### 3.3 Agents

`architect`, `explorer`, `implementer`, `reviewer` (kit, read-only except
`implementer`), plus:

- `spec-author` (read-write, scoped to `specs/**`): writes and amends specs
  following the template and the edge conventions in spec 000 §4.2; runs
  `spec-spine compile` and `lint --fail-on-warn` after every edit; never edits
  a spec to make the gate pass (the refusal rule).
- `tauri-expert` (read-only): the domain specialist for Tauri v2 (capabilities
  and permissions, IPC commands and events, window APIs, plugins, CSP), the
  Windows and macOS window APIs the exclusion spec uses, and the SolidJS
  overlay. Loads the relevant specs and proposes implementations within their
  constraints.

### 3.4 Rules

Paths-scoped rules auto-load when an agent touches a matching file:

- `spec-authoring.md` (`specs/**`): frontmatter conventions, edge selection,
  lifecycle rules, the burn-down discipline.
- `rust-crates.md` (`crates/**`, `apps/desktop/src-tauri/**`): crate
  boundaries, `unsafe` policy, error types, tracing, the `// Spec:` header.
- `overlay-frontend.md` (`apps/desktop/src/**`): SolidJS conventions, the
  generated IPC bindings, the transparent-root and pointer-events rules.
- `build-commands.md` (`Makefile`, `Cargo.toml`, `package.json`,
  `.github/**`): the command contract and what each target guarantees.

### 3.5 Hooks (`.claude/settings.json`)

| Hook | Matcher | Behavior |
|---|---|---|
| `SessionStart` | startup, resume, clear, compact | recompile the registry; report registry and index freshness |
| `PostToolUse` | `Edit`, `Write` | after a `spec.md` edit, recompile; after any hashed-input edit, `index check` |
| `PreToolUse` | `Bash` matching `gh pr create` | run `spec-spine couple`; block without a waiver; block if `.derived/` is dirty |
| `Stop` | `*` | if the index is stale and no merge/rebase is in progress, `spec-spine index` and report |

The `PostToolUse` glob list MUST equal `[index] extra_hashed_inputs` in
`spec-spine.toml`; a change to one is a change to both (both are owned: the
config by 000, the hook by this spec; the PR must touch both specs).

Permissions MUST allow the read-only git verbs, `spec-spine`, `make`, `cargo`
(except `publish`), `pnpm` (except `publish`), `gh pr`/`gh run` reads, and MUST
deny `cargo publish`, `pnpm publish`, `gh release create/delete`, `gh repo
delete/archive`, and any `rm -rf` outside `target/`, `node_modules/`, `dist/`.

### 3.6 Makefile contract

| Target | Guarantee |
|---|---|
| `setup` | installs the pinned `spec-spine`, compiles, indexes, verifies the loop |
| `spine` | `compile --check` → `index check` → `lint --fail-on-warn` → `coverage --fail-on-untraced` |
| `ci` | `spine` + `build` + `test` + `lint` (the same set CI runs) |
| `pr-prep` | `spec-spine index` then `couple --base origin/main` |
| `burndown` | the `W-001` list per spec |
| `coverage` | `spec-spine index coverage` |
| `spec-new` | `SLUG=... make spec-new` scaffolds the next spec directory |
| `build`, `test`, `lint` | the language gates (content per spec 001) |

`SPEC_SPINE` MAY be overridden (`make SPEC_SPINE=/path/to/binary ...`); the
default is the binary on `PATH`.

### 3.7 Verification blocks

`scripts/verify-spec.sh <id>` is the kit's verify runner: it reads the spec's
`## Verification` section, runs every non-comment line inside its `verify:cli`
fences from the repository root in order, and stops at the first non-zero exit.
It reports `passed`, `FAILED at N`, or `not-declared`. `not-declared` exits 0
because it is an honest zero, not a pass, which means a spec with no block is
indistinguishable from one whose checks all succeeded.

- Every spec MUST carry a `## Verification` section with at least one
  `verify:cli` command **before it flips to `implementation: complete`**. The
  scaffold in `standards/spec/templates/spec-template.md` carries the section,
  so specs created by `make spec-new` start with one.
- Commands MUST be mechanical and host-independent: no absolute paths, no
  interactive prompts, no network beyond what the gate already needs.
- The commands prove the spec's own acceptance criteria. `make spine` alone is
  the corpus-wide floor, not a substitute for a spec-specific check.
- `verify:browser` fences are counted and skipped by the runner; only an
  orchestrator with a browser stage drives those.

## 4. Functional requirements

- **FR-001.** `/init` on a clean checkout emits the initialized block with
  lifecycle counts and the burn-down without parsing `.derived/**` directly.
- **FR-002.** The `PreToolUse` hook blocks `gh pr create` when `spec-spine
  couple` exits non-zero and the command's `--body` lacks the waiver keyword.
- **FR-003.** The `Stop` hook never runs `spec-spine index` while
  `.git/MERGE_HEAD`, `rebase-merge`, `rebase-apply`, or `CHERRY_PICK_HEAD`
  exists.
- **FR-004.** `make ci` and the CI workflow (spec 003) run the same gate set;
  a local pass implies a CI pass for the governance jobs.
- **FR-005.** `/spec-new` computes the next ordinal from `spec-spine registry
  list --ids-only`, never from `ls`.
- **FR-006.** `/burndown` derives its list from `spec-spine index render`
  output (the governed projection), never from the shard JSON.
- **FR-007.** `scripts/verify-spec.sh <id>` exits 0 and reports `passed` for
  every spec at `implementation: complete`, and reports `not-declared` for no
  such spec.

## 5. Acceptance criteria

- **AC-1.** `spec-spine index check --slice governance` exits 0 after any
  harness edit that was followed by `spec-spine index`, and 2 without it.
- **AC-2.** Editing `.claude/settings.json` without editing this spec fails
  `spec-spine couple` with `C-001`.
- **AC-3.** `make spine` exits 0 on `main`.
- **AC-4.** Every skill file's `allowed-tools` list excludes destructive shell
  verbs; `/ship` and `/shepherd` are the only skills that push.
- **AC-5.** For each `implementation: complete` spec, `scripts/verify-spec.sh
  <id>` prints `passed` and exits 0; none prints `not-declared`.

## 6. Out of scope

- The three floor rules (spec 000).
- CI workflows, CODEOWNERS, merge driver (spec 003).
- Any product behavior. The harness is how butler-ai is built, not what it is.

## 7. Resolved decisions

- **D-1 (2026-09-06, kit adoption).** The skills under `.claude/skills/`
  are the spec-spine kit's fifteen (spec-spine spec 048), taken byte for
  byte, plus this repository's own `/burndown`. `/spec-new` retires in
  favour of the kit's `/spec`, which derives the ordinal from
  `spec-spine registry list --ids-only`, reads the taxonomy from
  `spec-spine.toml`, and leaves the phase to spec 018 (the `make spec-new`
  scaffold target stays for a hand-run). The three floor rules stay
  spec 000's text: the kit's spec 047 wording (the typed-read rationale
  that makes `make burndown`'s grep over `index render` output explicitly
  legitimate, the two legitimate mid-build edits, the `extends` pointer,
  "a waiver is a human instrument") refines guardrails 3 and 4 and is an
  amendment for a human to file against spec 000, not an edit here. The kit
  moved every project fact out of the skills into `AGENTS.md`, which gains
  the "Working the backlog" section the skills sequence and an
  orchestrator extracts verbatim. `scripts/verify-spec.sh` is the kit's
  copy and is claimed here. §3.2's table now reads as the kit's fifteen
  plus `/burndown`; that is a change to what this spec enumerates and is
  recorded here rather than rewritten in place.
- **D-2 (2026-09-06, pin bump).** `SPEC_SPINE_VERSION` moves from 0.11.0
  to 0.14.0 (`Makefile`; CI reads it from there). The corpus was verified
  byte-compatible first: 0.14.0's `compile --check` and `index check` both
  report fresh against shards written by 0.11.0. What the bump buys:
  `registry plan` (which `/next` wraps), `--json` verdicts on the gate
  verbs, `layout.state_dir`, the `depends_on` cycle refusal, and the
  lifecycle fixes this specify-first corpus lives inside.
- **D-3 (2026-09-06, hooks).** §3.5's `SessionStart` recompiles and
  `Stop` regenerates. The kit's hooks now read and never write
  (spec-spine spec 046: a hook cannot commit what it writes, and a
  writing `Stop` hook stalled an orchestrator for eleven hours on a tree
  it had dirtied). Porting them changes what §3.5 requires and is an
  amendment for a human to file; the hooks are unchanged until then.

## 8. Verification

```verify:cli
# AC-3: the governance gate chain is green.
make spine
# AC-1: the governance slice is fresh (the harness files are hashed inputs).
spec-spine index check --slice governance
# §3.6: every Makefile target the contract names still exists.
sh -c 'for t in setup spine ci pr-prep burndown coverage spec-new build test lint; do grep -qE "^${t}:" Makefile || { echo "missing target: ${t}"; exit 1; }; done'
# §3.7 + AC-5: the verify runner works, and no complete spec is undeclared.
test -x scripts/verify-spec.sh
sh -c 'for s in 000-butler-bootstrap 001-workspace-layout 003-governance-ci 009-pipeline-state-machine; do ./scripts/verify-spec.sh "$s" >/dev/null 2>&1 || exit 1; done'
```
