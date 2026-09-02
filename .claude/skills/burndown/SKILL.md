---
name: burndown
description: Report what remains to be built, per spec and per phase, from the indexer's unresolved-unit warnings (W-001), and propose the next unit of work from the sequencing plan.
allowed-tools: Bash(spec-spine:*), Bash(make burndown*), Bash(make coverage*), Read, Grep
argument-hint: "[spec-id | phase N]"
---

# /burndown: what is left to build

butler-ai is specified before it is built. Every owning unit a spec declares
that does not yet exist is a counted `W-001` warning in the codebase index
(spec-spine spec 025). That list is the build to-do list; nothing else is.

## Steps

1. **Freshness.** `spec-spine index check`. If stale, say so and run
   `spec-spine index` (the derived shards must be committed with the change
   that made them stale).
2. **The list.** `make burndown` (which runs `spec-spine index render` and
   keeps the `W-001` lines). Read it through the CLI projection only, never
   from `.derived/**/*.json` (`.claude/rules/governed-artifact-reads.md`).
3. **Group** by spec id, then by unit kind (crate, directory, file, module,
   symbol, section). Count per spec.
4. **Phase view.** Read `specs/018-implementation-sequencing/spec.md` §2.
   For each phase list its specs with their counts; mark a phase *done* when
   every spec in it is `implementation: complete` (from `spec-spine registry
   show <id>`), *active* when it is the lowest phase with a non-zero count,
   *blocked* otherwise.
5. **Coverage.** `make coverage`: report claimed/floor-only/unclaimed. Any
   floor-only or unclaimed file is a ratchet violation waiting to happen;
   name it.
6. **Next step.** From the active phase, per its parallelism note, name the
   spec(s) an implementer should take next, the branch name (`NNN-slug`), and
   the exit criterion.

If `$ARGUMENTS` names a spec, restrict steps 2 to 3 to it and print its FR/AC
list for the implementer.

## Output

```
## burndown: butler-ai
Index: fresh | stale (regenerated)
Total unresolved: N units across M specs

### Phase 1 (active): 001, 009, 008, 015
- 009-pipeline-state-machine: 9 (crate 1, file 4, module 1, symbol 4)
- ...
### Phase 2 (blocked by phase 1): ...

Coverage: X/Y claimed, F floor-only, U unclaimed

Next: implement 009-pipeline-state-machine on branch `009-pipeline-state-machine`
      (owns butler-core; 008 and 015 can start once its crate exists).
```
