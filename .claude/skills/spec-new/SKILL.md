---
name: spec-new
description: Scaffold the next spec directory (specs/NNN-slug/spec.md) from the template with the correct ordinal, taxonomy, phase and lifecycle defaults, then compile and lint it.
allowed-tools: Bash(spec-spine:*), Bash(make spec-new*), Bash(ls:*), Read, Write, Edit
argument-hint: "<slug> [domain] [kind] [phase]"
---

# /spec-new: scaffold a spec

Creates `specs/NNN-slug/spec.md` where `NNN` is the next unused ordinal. Bound
by `.claude/rules/spec-authoring.md`.

## Steps

1. **Ordinal.** `spec-spine registry list --ids-only`; take the highest `NNN`
   and add one. Never derive it from `ls specs/` (a stale checkout would
   collide).
2. **Arguments.** `$ARGUMENTS` = `<slug> [domain] [kind] [phase]`. Domain and
   kind MUST be values from `spec-spine.toml` `[domains]`/`[kind]`; ask if
   absent. `phase` defaults to the last phase in
   `specs/018-implementation-sequencing/spec.md` §2 plus one, and the user is
   told that 018 must be amended to add the spec to a phase.
3. **Scaffold.** `SLUG=<slug> make spec-new` (which copies the template and
   fills `id`, `created`, `domain`, `kind`, `phase`), or do the same by hand
   if `make` is unavailable.
4. **Fill in** `title`, `summary`, the edges (see the rule: `establishes` for
   own territory, `extends` for files inside another spec's crate), and the
   six body sections. Keep `status: draft`, `implementation: pending`.
5. **Verify.**
   ```sh
   spec-spine compile && spec-spine lint --fail-on-warn && spec-spine index && spec-spine index check
   ```
6. **Report** the id, the units claimed, and the `W-001` delta
   (`make burndown`).

CHECKPOINT: a new spec is a new claim of territory; present the frontmatter
to the user before writing the body if the edges touch another spec's crate.
