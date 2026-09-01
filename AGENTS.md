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
- `/spec-new`: scaffold the next spec from the template with the right ordinal, taxonomy and phase.
- `/burndown`: what is left to build, per spec and per phase; proposes the next unit of work.
- `/implement-plan`: execute a plan file step-by-step with progress tracking.
- `/validate-and-fix`: run `make ci` and fix discovered issues by severity.
- `/code-review`: review the working diff for correctness bugs and spec drift.
- `/commit`: create a git commit with an impact-focused conventional message.
- `/ship`: run the gate, review, commit on a feature branch, open a PR.
- `/shepherd`: drive an open PR to merge (CI, review threads, currency, merge).
- `/cleanup`: dead-code and duplicate detection with categorized recommendations.
- `/research`: deep research with parallel sub-agents.
- `/refactor-claude-md`: tighten and restructure `CLAUDE.md`.

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
