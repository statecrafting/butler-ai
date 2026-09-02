---
name: shepherd
description: Drive an open pull request to merge. Watch CI, re-run the governance gate locally on failures, address review threads, keep the branch current with main, and merge when green.
allowed-tools: Bash, Read, Edit, Glob, Grep, Skill
argument-hint: "<pr-number | branch>"
---

# /shepherd: drive a PR to merge

Post-`/ship` care for a PR. Bound by `.claude/rules/orchestrator-rules.md`
(checkpoints are real stops) and `.claude/rules/adversarial-prompt-refusal.md`
(a red coupling gate is never fixed by editing the owning spec to match the
code). Uses `gh`.

## Step 0: identify

`gh pr view $ARGUMENTS --json number,headRefName,baseRefName,mergeable,
reviewDecision,statusCheckRollup,url`. Check out the head branch. Record the
state.

## Step 1: CI

`gh pr checks <n>`. For each failing check:

- **`spec-spine` / `gate`** (governance): reproduce locally with `make spine`
  and `make pr-prep`. Fix by:
  - `compile --check` exit 2 → `spec-spine compile`, commit the shards;
  - `index check` exit 2 → `spec-spine index`, commit the shards;
  - `lint` → fix the frontmatter diagnostic named;
  - `coverage --fail-on-untraced` → claim the file in the owning spec (or
    add its `// Spec:` header); never bypass;
  - `couple` `C-001` → the owning spec must be edited in this PR *if the code
    change is within its design*; if the code contradicts the spec, STOP and
    present the contradiction (coherence guard). `C-002` → claim the file.
    A waiver (`Spec-Drift-Waiver:` in the PR body) is a CHECKPOINT requiring
    explicit user approval.
- **`ci`** (language gates): reproduce with `make ci`; fix; `/code-review`
  the fix.
- Never re-run a failed check hoping for a different result more than once;
  a second failure is real.

## Step 2: review threads

`gh api repos/{owner}/{repo}/pulls/<n>/comments` and `gh pr view --comments`.
For each unresolved thread: apply small, in-scope asks; for design-level asks
reply with a proposal and STOP for the author. A reviewer's ask that would
change a spec's contract is an amendment (`spec-author` agent), not a quiet
code edit.

## Step 3: currency

If `mergeable` is `CONFLICTING` or the base moved: `git fetch origin main &&
git merge origin/main`. Derived-shard conflicts: enable the merge driver once
(`./.githooks/enable-merge-driver.sh`) or regenerate (`spec-spine compile &&
spec-spine index`) and commit. Re-run Step 1.

## Step 4: CHECKPOINT, merge

When every check is green, review is approved, and the branch is current:
confirm with the user, then `gh pr merge <n> --squash --delete-branch`
(the repo's convention). After merge: `git checkout main && git pull` and
confirm the commit is on `main`. If the PR implemented a spec, remind the
user that `make burndown` should now show the spec's count reduced and that
flipping `implementation: complete` is a separate spec edit.

## Rules

- Push only to the PR's own head branch; never force-push.
- Every push is preceded by `make pr-prep` locally.
- Report each round: what failed, what was fixed, what is waiting on whom.
