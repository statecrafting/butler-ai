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
| `make spine` | `compile --check` → `index check` → `lint --fail-on-warn` → `coverage --fail-on-untraced` |
| `make ci` | `spine` + `build` + `test` + `lint` (identical to CI) |
| `make pr-prep` | `spec-spine index`, then `couple --base origin/main --head HEAD` |
| `make burndown` | unresolved owning units (`W-001`) per spec: the build to-do list |
| `make coverage` | `spec-spine index coverage` |
| `SLUG=… make spec-new` | scaffold the next spec directory from the template |
| `make build` / `test` / `lint` | the language gates; no-ops until the root manifests exist |

- `SPEC_SPINE=/path/to/binary make …` overrides the binary.
- The workflows under `.github/workflows/` are owned by spec 003 (governance
  keypaths as section units) and 017 (release). Editing `on`, `permissions`,
  or a job block requires editing the owning spec in the same PR.
- The CI install step and `make setup` pin the same `SPEC_SPINE_VERSION`.
