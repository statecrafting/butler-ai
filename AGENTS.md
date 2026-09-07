# AGENTS.md: butler-ai

This file is the cross-agent session-init protocol authority, read by Claude
Code, Codex CLI, Cursor, and GitHub Copilot via the AAIF/Linux Foundation
AGENTS.md standard. It is the single source for the init protocol: tooling that
runs `/init` reads the `## New Sessions` section to derive its plan.

Governance is provided by `spec-spine` (installed on your `PATH` by
`make setup`). All governed reads of compiled artifacts go through its CLI.
Bootstrap spec: `specs/000-butler-bootstrap/spec.md`. Harness spec (this file's
owner): `specs/002-agentic-harness/spec.md`.

**butler-ai is specified before it is built.** Every product spec declares the
crates, files, modules and symbols it will own; the indexer reports each
not-yet-existing unit as a counted `W-001` warning. That list is the build
to-do list (`make burndown`), and the build order is
`specs/018-implementation-sequencing/spec.md`.

## New Sessions

Run `/init` as the first action of every new session. It reads this section to
derive its execution plan dynamically: any item added here is automatically
picked up on the next init.

> AGENTS.md is loaded implicitly as the protocol source; its contents are the
> protocol, so `/init` does not list AGENTS.md as a parallel identity read in
> Step 1 (avoiding the self-reference loop).

**Init protocol:**

0. **Load rules** (read first): `.claude/rules/orchestrator-rules.md`,
   `.claude/rules/governed-artifact-reads.md`, and
   `.claude/rules/adversarial-prompt-refusal.md`.

1. **Parallel reads.** Dispatch the following simultaneously (nothing here
   mutates the working tree, so there is no required ordering):
   - `CLAUDE.md`: project overview, governance model, conventions
   - `README.md`: what butler-ai is
   - `standards/spec/contract.md`: the short normative spec contract
   - `standards/spec/constitution.md`: durable principles (§III specify-first, §V privacy)
   - `spec-spine compile --check`: registry freshness (non-fatal; see **Registry freshness**)
   - `spec-spine index check`: codebase index staleness (non-fatal)
   - `spec-spine registry status-report --json --nonzero-only`: lifecycle counts
   - `spec-spine registry list --ids-only`: spec inventory (latest-spec detection)
   - `spec-spine registry plan`: the ready set (spec-spine 038): which specs can be worked on now and what blocks the rest; `/next` applies the approval and in-flight rules on top of it
   - `spec-spine index coverage`: which source files no spec specifically claims (exit 2 if the index is stale)
   - `spec-spine index render`: the burn-down (`W-001` lines) and coverage table
   - `ls crates apps/desktop apps/desktop/src-tauri 2>/dev/null`: what has been built so far (absent directories are expected before phase 1 and 2)
   - `ls docs/`: docs surface (`architecture.md`, `threat-model.md`)
   - `git log --oneline -10`: recent history
   - `git diff --stat HEAD~1`: last change summary

2. **Emit** an `## initialized: butler-ai` summary block: a layer overview
   (governance, pipeline crates, desktop app, overlay), recent activity, a
   `## lifecycle:` sub-section from the `status-report` output, a
   `## burndown:` sub-section (unresolved units per spec, the active phase
   from spec 018), and a ready-to-help line naming the next unit of work.

**Read discipline:** the init protocol MUST NOT parse `.derived/**/*.json`
directly (no `python`, `jq`, `awk`, `sed` against compiled artifacts). All
structural and lifecycle data comes from `spec-spine` subcommands; the
burn-down is read from the `index render` projection.

**Staleness surface:** both committed artifacts have their own gate, and neither
is fatal to `/init`: report it in the summary and continue. If `spec-spine index
check` exits non-zero, include "Codebase index: stale, run `spec-spine index`".

**Registry freshness:** `spec-spine compile --check` compiles in memory and
compares against the committed shards **without writing**. Read the exit code:

- **`0` (fresh):** the lifecycle counts reflect the current frontmatter.
- **`2` (stale):** check stderr first. A real staleness report names shards:
  report "Spec registry: stale, run `spec-spine compile` and commit" and say
  the counts come from the stale committed ledger. An older CLI rejects the
  flag with exit 2 too (`error: unexpected argument '--check'`): that is a
  version problem, not drift; run `/setup`.
- **`1` (validation failed):** the corpus is broken. Surface the violations and
  report the counts as unverified.
- **any other non-zero:** treat freshness as unknown, report stderr verbatim,
  continue. Never report "fresh" for a code you did not recognize.

Do **not** substitute a plain `spec-spine compile` here: writing repairs the
tree as a side effect of reading it and hides that the committed copy was
stale. `/init` reports; it does not silently mutate.

**CLI missing:** if `spec-spine --version` fails, run `/setup`. Do NOT fall back
to ad-hoc parsing of `.derived/**/*.json`.

If any file is missing: log "not found" and continue.

## Working the backlog

The governed loop is one spec per session, start to finish, then stop. It is
what `spec-spine registry plan`, the in-flight leniency and the ownership
ratchet exist to serve. Record specs (`000`, `002`, `018`) are never work
orders. The phase graph in spec 018 orders the ready set; take the first
ready spec of the lowest open phase.

1. **Pick the spec.** `spec-spine registry plan` prints the ready set in
   dependency order; `/next` applies the two rules on top of it (a `draft`
   is never offered, an `in-progress` spec is in flight) and names the pick.
   Never guess. If the spec's Territory names an operator prerequisite (a
   signing identity, a platform SDK) that is missing, stop and report
   exactly what is needed instead of mocking around it.
2. **Branch and flip.** `/build <id>` sequences steps 2 to 6. Work on a
   feature branch named `NNN-slug`. Flip the spec to `implementation:
   in-progress`, run `spec-spine compile` and `spec-spine index`, and commit
   the flip with the regenerated derived shards before writing code. Never
   commit to `main`.
3. **Re-read the spec in full before coding.** If the design is imprecise,
   record the choice as a dated decision entry in the spec. If the design is
   *wrong*, stop and report the contradiction: never edit a spec afterwards
   to ratify what the code happened to do
   (`.claude/rules/adversarial-prompt-refusal.md`).
4. **Implement within the territory.** Every file you add is claimed by the
   spec you are implementing, in the same change (`C-002` refuses an
   unclaimed source file; a `// Spec:` header is the other claim). Touching
   a unit another spec owns requires an `extends` edge on that spec's unit.
   Never edit `.derived/` by hand.
5. **Run the gate before every commit.** `make ci` (`make spine`: `compile
   --check`, `index check`, `lint --fail-on-warn`, `coverage
   --fail-on-untraced`; then `build`, `test`, `lint`) and `make pr-prep`
   (`spec-spine index`, then `couple --base origin/main`). All must exit 0.
   Commit the regenerated shards with the code they describe.
6. **Satisfy the spec's acceptance criteria verbatim.** `/verify <id>` runs
   the spec's `## Verification` block through `scripts/verify-spec.sh`. If a
   criterion cannot be satisfied, keep `implementation: in-progress`, add a
   dated note to the spec saying exactly what remains, and report it. Flip to
   `implementation: complete` only at zero `W-001` for the spec
   (`/burndown`) and with acceptance holding; recompile and commit.
7. **Ship.** `/ship`: gate, review, a conventional commit naming the spec id,
   push the feature branch, open the PR. A `Spec-Drift-Waiver:` line needs
   explicit human approval; a driven session never self-approves one.
   `/shepherd` watches the checks, remediates through the gate, merges, and
   confirms on disk. Then stop: the next session takes the next spec.

## Available Agents

Agents live in `.claude/agents/`. Four pipeline agents handle the
plan/explore/implement/review cycle, and two specialists know this project:

- `architect`: plans and decomposes tasks, validates approaches against specs. Read-only.
- `explorer`: searches the codebase, traces dependencies, gathers context. Read-only.
- `implementer`: executes focused changes from an existing plan. Minimal diffs.
- `reviewer`: post-change review for bugs, correctness, performance, spec compliance. Read-only.
- `spec-author`: writes and amends specs under `specs/**` only; keeps the registry fresh; never approves or completes a spec on its own.
- `tauri-expert`: read-only specialist for Tauri v2, the Windows/macOS window and capture APIs, OCR bindings, and the SolidJS overlay.

## Available Commands

Skills live in `.claude/skills/`:

- `/init`: initialize a session (this protocol).
- `/setup`: one-time contributor setup; installs the pinned spec-spine and verifies the governed loop.
- `/next`: the next ready spec from `registry plan`, minus drafts, with in-flight specs and honest blockers. Read-only.
- `/build <id>`: one spec start to finish per "Working the backlog".
- `/verify <id>`: run a spec's `verify:cli` blocks locally through `scripts/verify-spec.sh`.
- `/spec`: author the next spec from the template at the next free ordinal, born `draft`; taxonomy from `spec-spine.toml`, phase from spec 018.
- `/burndown`: what is left to build, per spec and per phase; proposes the next unit of work (this repository's own).
- `/implement-plan`: execute a cross-cutting plan file step by step with checkpoints.
- `/validate-and-fix`: run `make ci` and fix discovered issues by severity.
- `/code-review`: review the working diff for correctness bugs, spec drift, and illegitimate mid-build spec edits.
- `/commit`: create a git commit with an impact-focused conventional message, spec ordinal as scope.
- `/ship`: run the gate, review, commit on a feature branch, open a PR.
- `/shepherd`: watch the PR's checks by head sha, remediate through the gate, merge, confirm on disk.
- `/cleanup`: dead-code and duplicate detection with ownership-aware recommendations.
- `/research`: deep research with parallel sub-agents.
- `/refactor-claude-md`: tighten and restructure `CLAUDE.md`.

The fifteen (all but `/burndown`) are the spec-spine kit's, byte for byte
(spec-spine spec 048). The project layer the skills read lives in this file
(the pin in `Makefile`, `make ci` and `make pr-prep` as the gate, the default
branch) and in the path-scoped rules; do not edit a skill to add a project
fact, add it here.

## Conventions

- Items added to the "New Sessions" init protocol are auto-loaded on the next init.
- Orchestrated workflows read compiled artifacts (`.derived/**`) through
  `spec-spine` subcommands, never via ad-hoc parsers (see
  `.claude/rules/governed-artifact-reads.md`).
- Every substantive change is bound to a spec; owned paths and their owning
  `spec.md` move together (`spec-spine couple` enforces this at PR time), and no
  source file merges without a spec that specifically claims it
  (`[coupling] require_ownership`, spec 000 §7.5).
- Implementation follows `specs/018-implementation-sequencing`: one spec per
  branch named `NNN-slug`; a spec flips to `implementation: complete` only at
  zero `W-001`.
- The Makefile is the command contract (`make setup | spine | ci | pr-prep |
  burndown | coverage | spec-new | build | test | lint`).
