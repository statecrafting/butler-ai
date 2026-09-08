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
co_authority:
  - { unit: { kind: section, file: "Makefile", anchor: "build" }, with_specs: ["001-workspace-layout"] }
  - { unit: { kind: section, file: "Makefile", anchor: "test" }, with_specs: ["001-workspace-layout"] }
  - { unit: { kind: section, file: "Makefile", anchor: "lint" }, with_specs: ["001-workspace-layout"] }
references:
  - { unit: { kind: file, path: ".claude/rules/orchestrator-rules.md" }, role: "floor rule (owned by 000)" }
  - { unit: { kind: file, path: ".claude/rules/governed-artifact-reads.md" }, role: "floor rule (owned by 000)" }
  - { unit: { kind: file, path: ".claude/rules/adversarial-prompt-refusal.md" }, role: "floor rule (owned by 000)" }
  - { unit: { kind: file, path: ".claude/rules/derived-artifacts-are-compiler-output.md" }, role: "floor rule (owned by 000)" }
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
  harness surface; the four floor rules stay with spec 000.
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
  paths-scoped context rules. The four floor rules are spec 000's.

## 3. Behavior

### 3.1 Session protocol (`AGENTS.md` § New Sessions)

`/init` MUST read the protocol from `AGENTS.md` and execute it, never
duplicate it. The protocol MUST: load the unconditional floor rules first (the
fourth is paths-scoped and loads on contact); dispatch the
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
| `SessionStart` | startup, resume, clear, compact | establish the binary understands `compile --check` before reading its exit code, then report registry and index freshness; never write |
| `PostToolUse` | `Edit`, `Write` | after a `spec.md` edit, recompile; after any hashed-input edit, `index check` |
| `PreToolUse` | `Bash` | refuse a `git push` that would update `main` (a tag push is not one); on `gh pr create` run `spec-spine couple`, block without a waiver, block if the index is stale or `.derived/` is dirty |
| `Stop` | `*` | report a stale index; never regenerate it |

Hooks MUST read and MUST NOT write, with one exception: the `PostToolUse`
recompile after a `spec.md` edit, where the session is live and can commit the
regenerated shards alongside the edit that made them stale. A hook cannot
commit what it writes, so a writing `SessionStart` hides a stale committed
registry by repairing it as a side effect of reading it, and a writing `Stop`
leaves `.derived/` dirty for a session that has already ended.

Each hook MUST act on the repository the action targets, resolved from the
edited file's path or from the command's explicit `cd` prefix, and MUST NOT
assume the session's project directory: a session with several checkouts open
must never judge one repository by another's state.

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

`spec-spine verify <id>` is the verify runner: it reads the spec's
`## Verification` section, runs every non-comment line inside its `verify:cli`
fences from the repository root in order, and stops at the first non-zero exit.
It reports `passed`, `FAILED at N`, or `not-declared`. `not-declared` exits 0
because it is an honest zero, not a pass, which means a spec with no block is
indistinguishable from one whose checks all succeeded. `--plan` prints the
commands and runs none of them: the safety affordance for the one verb that
executes what the corpus declares, and the way to read a `## Verification`
block this session did not author.

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
- **FR-003.** No hook writes into the repository it observes, except the
  `PostToolUse` recompile after a `spec.md` edit. The `Stop` hook never runs
  `spec-spine index` at all, in any tree state.
- **FR-004.** `make ci` and the CI workflow (spec 003) run the same gate set;
  a local pass implies a CI pass for the governance jobs.
- **FR-005.** `/spec-new` computes the next ordinal from `spec-spine registry
  list --ids-only`, never from `ls`.
- **FR-006.** `/burndown` and `make burndown` derive their list from
  `spec-spine index diagnostics`, the typed read, never from the shard JSON
  and never by grepping `index render`.
- **FR-007.** `spec-spine verify <id>` exits 0 and reports `passed` for every
  spec at `implementation: complete`, and reports `not-declared` for no such
  spec.
- **FR-008.** The `PreToolUse` hook refuses a `git push` that would update
  `main`, whichever branch the session is on. A push that updates no branch is
  not such a push: a tag push (`git push origin v1.2.3`) is allowed from `main`,
  because spec 017's release process cuts one from `main` immediately after the
  release PR merges. The match is anchored on the invocation of the push verb,
  never a substring test over the whole command, so a command that merely
  names the verb in an argument is not refused.

## 5. Acceptance criteria

- **AC-1.** `spec-spine index check --slice governance` exits 0 after any
  harness edit that was followed by `spec-spine index`, and 2 without it.
- **AC-2.** Editing `.claude/settings.json` without editing this spec fails
  `spec-spine couple` with `C-001`.
- **AC-3.** `make spine` exits 0 on `main`.
- **AC-4.** Every skill file's `allowed-tools` list excludes destructive shell
  verbs; `/ship` and `/shepherd` are the only skills that push.
- **AC-5.** For each `implementation: complete` spec, `spec-spine verify <id>`
  prints `passed` and exits 0; none prints `not-declared`.
- **AC-6.** The `Stop` hook's command contains no `spec-spine index`
  invocation other than `index check`, and the `PreToolUse` hook refuses a
  push to `main`.

## 6. Out of scope

- The four floor rules (spec 000).
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
- **D-4 (2026-09-07, pin bump).** `SPEC_SPINE_VERSION` moves from 0.14.0 to
  0.15.0 (`Makefile`; CI reads it from there). Byte-compatibility was verified
  before the bump, the same way D-2 verified 0.14.0: 0.15.0's `compile
  --check` and `index check` both report fresh against shards written by
  0.14.0, and `make ci` exits 0 with the derived tree unchanged. What the bump
  buys is what D-5 to D-7 spend: `spec-spine verify` (spec-spine spec 049),
  `index diagnostics` and `index check`'s warning counts (050), and the
  read-only kit hooks (046) that D-3 was waiting on.
- **D-5 (2026-09-07, the hooks port D-3 deferred).** D-3 recorded that porting
  the kit's read-only hooks changes what §3.5 requires and is an amendment for
  a human to file, and left the hooks alone. That amendment is filed here, with
  human approval, and §3.5, FR-003 and FR-008 above are the amended text. Three
  writes are removed: `SessionStart`'s bare `compile`, the `PreToolUse` gate's
  `index` (it blocked on the uncommitted output of its own write), and `Stop`'s
  regeneration. All four hooks now resolve the repository the action targets
  rather than `CLAUDE_PROJECT_DIR`, which in a session holding several
  checkouts open judged this repository by a sibling's state. The push gate is
  new: `AGENTS.md` has always said never commit to `main` and nothing enforced
  it. Two project facts are kept over the kit's text: the binary resolution
  falls back to `$HOME/.cargo/bin/spec-spine` after `PATH`, and the
  `PostToolUse` glob list is butler-ai's hashed inputs. That list now genuinely
  equals `[index] extra_hashed_inputs` as §3.5 requires, which the previous
  list did not: it watched `Cargo.toml`, `package.json` and
  `pnpm-workspace.yaml`, none of which are hashed inputs.
- **D-6 (2026-09-07, the verb replaces the script).** `scripts/verify-spec.sh`
  is deleted and its claim removed from §2; `spec-spine verify <id>` (spec-spine
  spec 049) does the same job as a typed, tested read of authored markdown. The
  two were confirmed equivalent before the swap, on this corpus: for spec 004
  the verb's `--plan` lists the same twelve commands in the same order, and a
  real run prints the same `passed (12 command(s))` line the script printed.
  D-1's statement that the script "is the kit's copy and is claimed here" is
  superseded. The kit at the pinned v0.15.0 still ships the script, so
  `.claude/skills/{verify,spec,validate-and-fix}` are taken from the kit's
  `main` instead, where spec-spine spec 051 completed this migration; the
  skill's own text pins the requirement, "spec-spine 0.15.0 or later", which
  D-4 satisfies. The other twelve skills are identical at both points.
- **D-7 (2026-09-07, `--fail-on-unresolved` stays off).** Spec-spine spec 050
  adds an opt-in gate that turns an unresolved unit into exit 1, and its own
  repository turns it on. butler-ai MUST NOT: this corpus is specified before
  it is built, and its 108 `W-001` warnings are the burn-down that
  `specs/018-implementation-sequencing` works through, not a defect. `make
  spine` keeps the four gates it had. What 050 is used for here is reading:
  `make burndown` takes the typed `index diagnostics` instead of grepping
  `index render` (FR-006), and gains a per-spec count for free.
- **D-8 (2026-09-08, pin bump).** `SPEC_SPINE_VERSION` moves from 0.15.0 to
  0.17.0 (`Makefile`; CI reads it from there). Byte-compatibility was verified
  the way D-2 and D-4 verified theirs, and this time it did not hold cleanly,
  which is the point of checking: 0.17.0's `compile --check` and `index check`
  both report fresh against shards written by 0.15.0, but `lint --fail-on-warn`
  exits 1 with 67 `L-008` warnings. `L-008` (spec-spine spec 057) is new and
  names every claimed path that contributes to no content hash. It found a real
  defect here, not a false alarm: spec 000 D-2 has the measurement and the fix,
  and D-3 declares the remainder. What the bump buys beyond that is `[meta]
  required_version` (spec-spine 062, taken up in 000 D-4), the version probe and
  exit-3 usage mapping (063), the anchored push gate (071, D-9 below), and the
  fourth floor rule (068, 000 D-5). `--fail-on-unresolved` stays off: D-7's
  reasoning is about what this corpus is, not about how much of it is built, and
  the burn-down reaching zero does not retire it.
- **D-9 (2026-09-08, two hook defects the kit had already fixed).** The four
  hooks are re-taken from the kit at v0.17.0, carrying D-5's two project facts
  forward unchanged (the `$HOME/.cargo/bin/spec-spine` fallback after `PATH`,
  and the `PostToolUse` glob list, which is butler-ai's hashed inputs and moves
  with them: §3.5 requires the two to be equal, so 000 D-2's additions appear in
  both). Two behaviors change:

  1. `SessionStart` asks whether the binary understands `compile --check`
     before reading its exit code (spec-spine spec 063). An older CLI rejects
     the unknown flag with exit 2, which is the code this tool spends on
     staleness, so the hook reported phantom drift and a session acting on it
     would regenerate and commit shards that were already correct.
  2. The push gate is anchored on the invocation of the push verb and, on
     `main`, refuses only a push that would actually update `main`
     (spec-spine spec 071). The old form was a substring test over the whole
     command, so it refused anything merely *containing* the text, and on
     `main` it refused every push including a tag push. FR-008 above is
     amended: the requirement was already "targets `main`", and the old
     implementation was broader than the requirement it served, but the spec
     was silent on tag pushes and spec 017 cuts a release tag from `main`, so
     the carve-out is written down rather than left to be rediscovered.

## 8. Verification

```verify:cli
# AC-3: the governance gate chain is green.
make spine
# AC-1: the governance slice is fresh (the harness files are hashed inputs).
spec-spine index check --slice governance
# §3.6: every Makefile target the contract names still exists.
sh -c 'for t in setup spine ci pr-prep burndown coverage spec-new build test lint; do grep -qE "^${t}:" Makefile || { echo "missing target: ${t}"; exit 1; }; done'
# §3.7 + AC-5: the verify verb works, and no complete spec is undeclared.
# This spec is excluded from its own list: verifying 002 from inside 002's
# verification block would recurse.
sh -c 'for s in 000-butler-bootstrap 001-workspace-layout 003-governance-ci 008-change-detection 009-pipeline-state-machine; do spec-spine verify "$s" >/dev/null 2>&1 || { echo "verify failed: $s"; exit 1; }; done'
# D-6: the script the verb replaced is gone, and nothing calls it.
sh -c '! test -e scripts/verify-spec.sh'
# FR-003 + AC-6: the Stop hook reads and never regenerates the index.
sh -c 'test "$(jq -r ".hooks.Stop[].hooks[].command" .claude/settings.json | grep -o "index [a-z]*" | sort -u | tr "\n" ",")" = "index check,"'
# FR-008 + AC-6: the push gate exists.
sh -c 'jq -r ".hooks.PreToolUse[].hooks[].command" .claude/settings.json | grep -q "push-gate"'
# FR-008 + D-9: it is anchored on the invocation of the verb, not a substring
# test, and it carves out the tag push spec 017 cuts from `main`. That this
# command can name the verb at all is the regression the anchoring fixed.
sh -c 'jq -r ".hooks.PreToolUse[].hooks[].command" .claude/settings.json | grep -q "A tag push such as"'
sh -c 'jq -r ".hooks.PreToolUse[].hooks[].command" .claude/settings.json | grep -q "Anchored on the command that actually invokes"'
# The positional-argument walk is what distinguishes "would update main" from
# "runs from main": without it the gate refuses a tag push too.
sh -c 'jq -r ".hooks.PreToolUse[].hooks[].command" .claude/settings.json | grep -q "npos"'
# D-9: SessionStart establishes the binary understands the flag before reading
# its exit code (spec-spine spec 063).
sh -c 'jq -r ".hooks.SessionStart[].hooks[].command" .claude/settings.json | grep -q -- "compile --check --help"'
# §3.5: the PostToolUse glob list equals `[index] extra_hashed_inputs`. Both
# are owned (the config by 000, the hook here), so they move in one PR.
sh -c 'jq -r ".hooks.PostToolUse[].hooks[].command" .claude/settings.json | grep -q "githooks"'
# FR-006: burndown reads the typed diagnostics, not a grep over `index render`.
sh -c 'grep -q "index diagnostics" Makefile && ! grep -q "index render | grep" Makefile'
```
