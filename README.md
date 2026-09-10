# butler-ai

A private, capture-excluded screen assistant for Windows and macOS.

Butler is a Tauri v2 desktop app whose transparent, always-on-top overlay is
excluded from screen sharing and recording at the compositor level. Every few
seconds it takes a snapshot of one monitor, runs the operating system's own OCR
on it, decides whether the visible text has meaningfully changed, and, only
then, asks an LLM about it and renders the answer in the overlay at a reading
pace. Frames and recognized text never touch disk; only redacted text leaves
the machine, to the one provider you configure.

**Status: built, and runnable on macOS.** Every spec's declared units exist
(`make burndown` reports zero) and `make app` installs a working Butler.app.
Two specs remain open and neither is waiting on code: 017 (signed, notarized
releases) stops at credentials no agent may hold, and 020 waits on a person
confirming the overlay renders.

The repository is a [spec-spine](https://github.com/statecrafting/spec-spine)
corpus: the specification is the authority, the agentic harness builds under
it, and CI refuses code that drifts from its owning spec.

## How this repository works

- `specs/NNN-slug/spec.md` is the product. Each spec declares, in typed
  frontmatter, the crates, files, modules and symbols it owns and its
  relationships to the other specs. `spec-spine` compiles the corpus into a
  hash-verifiable ledger (`.derived/`, committed) and refuses, at PR time, any
  code that drifts from its owning spec or that no spec claims.
- Because the code does not exist yet, every declared unit is a counted
  warning in the index. `make burndown` lists them; that list is the build
  to-do list. `specs/018-implementation-sequencing` is the order.
- `AGENTS.md` and `.claude/` are the harness: a session protocol, skills
  (`/prime`, `/next`, `/build`, `/verify`, `/burndown`, `/ship`, `/shepherd`,
  …), agents, rules, and hooks that keep the derived artifacts fresh and block
  a PR on a red gate.

Start with [`docs/architecture.md`](docs/architecture.md) and
[`docs/threat-model.md`](docs/threat-model.md).

## Getting started

```sh
make setup      # installs the pinned spec-spine, compiles, indexes, verifies the loop
make gate       # the governance gate chain (what CI runs), read-only
make burndown   # what remains to be built, per spec
make refresh    # recompute the committed shard trees, then commit them
```

To run it (macOS), after `cargo install tauri-cli --version "^2" --locked`:

```sh
make dev        # hot-reload loop
make app        # build, ad-hoc sign, install to /Applications
```

No signing identity is needed: `make app` distributes nothing. See
[`docs/local-install.md`](docs/local-install.md), which also explains why macOS
re-asks for Screen Recording after every rebuild. Releases are a different
path entirely (spec 017): signed, notarized, attested, and cut from a tag.

In Claude Code: `/setup`, then `/prime`.

## The specs

| Phase | Spec | Domain |
|---|---|---|
| 0 | 000 bootstrap · 002 agentic harness · 003 governance CI · 018 sequencing | governance |
| 1 | 001 workspace layout · 009 pipeline state machine · 008 change detection · 015 privacy boundary | governance, pipeline, platform |
| 2 | 004 desktop shell · 011 IPC contract · 012 overlay UI · 014 user configuration · 016 diagnostics · 019 runtime host | platform, ui, pipeline |
| 3 | 006 screen capture · 007 text recognition · 005 capture exclusion | pipeline, platform |
| 4 | 010 assistant inference · 013 output pacing | assistant, ui |
| 5 | 017 release and distribution · 020 local development builds | distribution |

## License

AGPL-3.0-only. See [`LICENSE`](LICENSE).
