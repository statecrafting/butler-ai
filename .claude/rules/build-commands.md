---
paths:
  - "Makefile"
  - "Cargo.toml"
  - "package.json"
  - "pnpm-workspace.yaml"
  - ".github/**"
---

# Build commands (butler-ai)

The `Makefile` is the command contract the skills call (spec 002 §3.6). Do
not rediscover commands by grepping manifests; call the target.

| Target | What it guarantees |
|---|---|
| `make setup` | installs the pinned `spec-spine`, compiles, indexes, verifies the loop |
| `make gate` | `check --fail-on-warn` → `lint --fail-on-warn` → `index coverage --fail-on-untraced` → `couple`; read-only throughout |
| `make refresh` | `spec-spine compile` then `index`: the writing half, for a session that can commit the shards |
| `SPEC=… make verify` | one spec's declared acceptance, through `spec-spine verify` |
| `make spine` | alias for `gate`, the name this contract used before 0.18.0 |
| `make ci` | `gate` + `build` + `test` + `lint` (identical to CI) |
| `make pr-prep` | `make refresh`, then `couple --base $(BASE) --head HEAD` |
| `make burndown` | unresolved owning units (`W-001`) per spec: the build to-do list |
| `make coverage` | `spec-spine index coverage` |
| `SLUG=… make spec-new` | scaffold the next spec directory from the template |
| `make build` / `test` / `lint` | the language gates; no-ops until the root manifests exist |

- `SPEC_SPINE=/path/to/binary make …` overrides the binary.
- Freshness is one verb since spec-spine 0.18.0: `spec-spine check` reads both
  committed shard trees, reports them on separate lines, and returns the more
  severe verdict. Its exit code cannot say which tree moved, so read the lines.
  `compile --check` and `index check` are still the right verbs when exactly one
  tree is in question (`index check --slice governance`, for instance).
- `BASE` is resolved from the repository, never assumed:
  `$SPEC_SPINE_DEFAULT_BRANCH`, then the remote's own HEAD, then `main`. An
  explicit `BASE=` on the command line wins.
- The workflows under `.github/workflows/` are owned by spec 003 (governance
  keypaths as section units) and 017 (release). Editing `on`, `permissions`,
  or a job block requires editing the owning spec in the same PR.
- The CI install step and `make setup` pin the same `SPEC_SPINE_VERSION`.
