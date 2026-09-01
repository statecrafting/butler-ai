# butler-ai spec contract (normative summary)

A one-page operational summary of the bootstrap spec
(`specs/000-butler-bootstrap/spec.md`), for quick reference. The bootstrap spec
and the constitution are authoritative; where this summary is terser, they
govern.

## Inputs (authored truth: markdown only)

- `specs/NNN-slug/spec.md`: one spec per directory; directory name equals `id`;
  `NNN` is a unique three-digit ordinal.
- `standards/spec/`: the constitution, this contract, and the templates.
- `spec-spine.toml`: the compiler configuration (owned by spec 000).

## Outputs (machine truth: compiler-owned JSON, read via `spec-spine` only)

- `.derived/spec-registry/by-spec/<id>.json`: spec-as-source shards (`compile`).
- `.derived/codebase-index/by-spec/<id>.json`, `by-package/<slug>.json`:
  code-as-source shards (`index`).
- `.derived/**/build-meta.json`: wall-clock metadata; gitignored.

Both shard trees are **committed**. `spec-spine compile --check` and
`spec-spine index check` refuse a stale tree in CI.

## Required frontmatter

`id`, `title`, `status` (`draft`/`approved`/`superseded`/`retired`), `created`
(`YYYY-MM-DD`), `summary`, plus, in this corpus, `domain` and `kind` from the
closed taxonomies in `spec-spine.toml` (a missing value is an `L-002`/`L-003`
warning, and CI fails on warnings).

## Extra keys (butler-specific, declared in `frontmatter.extra_known_keys`)

- `platforms`: the OS targets a spec applies to. A list drawn from `windows`,
  `macos`; or the single string `all`. A spec that omits it applies to all.
- `phase`: the integer build phase assigned by `018-implementation-sequencing`.
  A product spec MUST carry the phase that plan assigns it.

## Lifecycle in a specify-first corpus

| status / implementation | meaning | unresolved owned unit |
|---|---|---|
| `draft` | proposed, not yet ratified by a human | `W-001` warning |
| `approved` + `pending` | ratified, not yet built | `W-001` warning |
| `approved` + `in-progress` | being built | `W-001` warning |
| `approved` + `complete` | built and verified | **error** (`I-00x`) |
| `superseded` / `retired` | history | n/a |

`make burndown` lists every `W-001`. A spec flips to `complete` only at zero.

## Typed edges (8; `references` is the only non-owning one)

`establishes`, `extends`, `refines`, `supersedes`, `amends`, `co_authority`,
`constrains`, `references`. `origin` is a bootstrap marker, not an edge.

## Authority units

`file` (bare string shorthand; trailing slash = subtree), `section`
(`{file, anchor}`: a Makefile target, a Markdown heading slug, a `region:`
marker, a workflow `jobs.<name>` / `permissions` / `on` keypath, or a manifest
table path), `symbol` (`{id}`, Rust and TypeScript, resolved by tree-sitter),
`directory` (`{path}`), `crate` (`{id}`, a Cargo or npm package name), `module`
(`{id}`, a `::`-qualified Rust module path).

## Linkage from code to spec (three sources, all required where they apply)

1. Every Cargo package carries `[package.metadata.butler] spec = "NNN-slug"`;
   every npm package carries `"butler": { "spec": "NNN-slug" }`.
2. Every source file opens with a `// Spec: specs/NNN-slug/spec.md` header
   (`<!-- Spec: ... -->` in HTML, `# Spec:` in shell and TOML comments).
3. The owning spec declares the unit on an owning edge.

The manifest floor (1) is a drift safety net and counts as **debt** for
coverage; only (2) or (3) makes a file *specifically claimed*.

## The gate chain

`compile --check` → `index check` → `lint --fail-on-warn` →
`index coverage --fail-on-untraced` → `couple`. The coupling gate refuses a
merge where a claimed unit and its owning spec disagree (`C-001`) or where a
changed source file has no specific owner (`C-002`), unless a scoped
`Spec-Drift-Waiver:` line is present in the PR body. The bypass floor
(`docs/`, `README.md`, lockfiles, `.derived/`, plus `[coupling]
bypass_prefixes`) is exempt.

## Determinism

Pure function of `(config, file contents)` → byte-identical output; the ledger
is diffable and mechanically mergeable; staleness is detected by content-hash
comparison alone.
