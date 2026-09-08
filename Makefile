# Spec: specs/002-agentic-harness/spec.md
#
# The command contract the harness skills and CI call (spec 002 §3.6). The
# `build`, `test`, `lint` targets are co-owned with spec 001, which defines
# their content. Every governance target goes through the spec-spine binary;
# nothing here parses .derived/ JSON.

SHELL := /bin/sh
.DEFAULT_GOAL := help

# The spec-spine version CI installs (spec 003 §3.1) and `make setup` installs.
# Bump both here; the workflow reads this file.
SPEC_SPINE_VERSION ?= 0.17.0
SPEC_SPINE ?= $(shell command -v spec-spine 2>/dev/null || echo "$(HOME)/.cargo/bin/spec-spine")
BASE ?= origin/main

HAS_CARGO := $(wildcard Cargo.toml)
HAS_PNPM  := $(wildcard pnpm-workspace.yaml)

.PHONY: help setup spine ci pr-prep burndown coverage spec-new build test lint fmt clean

help:
	@printf '%s\n' \
	  'butler-ai targets:' \
	  '  setup      install spec-spine $(SPEC_SPINE_VERSION), compile, index, verify the loop' \
	  '  spine      compile --check → index check → lint --fail-on-warn → coverage --fail-on-untraced' \
	  '  ci         spine + build + test + lint (what CI runs)' \
	  '  pr-prep    spec-spine index, then couple --base $(BASE) --head HEAD' \
	  '  burndown   unresolved owning units (W-001) per spec' \
	  '  coverage   spec-spine index coverage' \
	  '  spec-new   SLUG=<slug> [DOMAIN=…] [KIND=…] [PHASE=…] make spec-new' \
	  '  build/test/lint   language gates (no-ops until the workspace manifests exist)'

setup:
	@if [ -x "$(SPEC_SPINE)" ] && "$(SPEC_SPINE)" --version 2>/dev/null | grep -q "$(SPEC_SPINE_VERSION)"; then \
	  echo "[setup] spec-spine $(SPEC_SPINE_VERSION) present at $(SPEC_SPINE)"; \
	else \
	  echo "[setup] installing spec-spine $(SPEC_SPINE_VERSION)"; \
	  cargo install spec-spine-cli --version "$(SPEC_SPINE_VERSION)" --locked \
	    || SPEC_SPINE_VERSION="v$(SPEC_SPINE_VERSION)" sh -c 'curl -fsSL https://raw.githubusercontent.com/statecrafting/spec-spine/main/install.sh | sh'; \
	fi
	"$(SPEC_SPINE)" --version
	"$(SPEC_SPINE)" compile
	"$(SPEC_SPINE)" index
	$(MAKE) spine
	@echo "[setup] governed loop verified; run /init"

spine:
	"$(SPEC_SPINE)" compile --check
	"$(SPEC_SPINE)" index check
	"$(SPEC_SPINE)" lint --fail-on-warn
	"$(SPEC_SPINE)" index coverage --fail-on-untraced

ci: spine build test lint

pr-prep:
	"$(SPEC_SPINE)" index
	"$(SPEC_SPINE)" couple --base "$(BASE)" --head HEAD

burndown:
	@"$(SPEC_SPINE)" index check >/dev/null 2>&1 || echo "[burndown] index is STALE; run: spec-spine index"
	@"$(SPEC_SPINE)" index diagnostics
	@if command -v jq >/dev/null 2>&1; then \
	  echo "--- unresolved owning units (W-001) per spec ---"; \
	  "$(SPEC_SPINE)" index diagnostics --json \
	    | jq -r '[.[] | select(.code == "W-001")] | group_by(.specId)[] | "\(length)\t\(.[0].specId)"' \
	    | sort -rn | sed 's/^/  /'; \
	  echo "total unresolved (W-001): $$("$(SPEC_SPINE)" index diagnostics --json | jq '[.[] | select(.code == "W-001")] | length')"; \
	else \
	  echo "total diagnostics (all codes): $$("$(SPEC_SPINE)" index diagnostics | wc -l | tr -d ' '); install jq for the W-001 breakdown"; \
	fi

coverage:
	"$(SPEC_SPINE)" index coverage

# Scaffold specs/NNN-<SLUG>/spec.md with the next ordinal from the registry.
spec-new:
	@test -n "$(SLUG)" || { echo "usage: SLUG=<kebab-slug> [DOMAIN=…] [KIND=…] [PHASE=…] make spec-new"; exit 2; }
	@last=$$("$(SPEC_SPINE)" registry list --ids-only | sed 's/-.*//' | sort -n | tail -1); \
	  next=$$(printf '%03d' $$((10#$$last + 1))); id="$$next-$(SLUG)"; dir="specs/$$id"; \
	  test ! -e "$$dir" || { echo "$$dir exists"; exit 1; }; mkdir -p "$$dir"; \
	  sed -e "s/^id: \"NNN-slug\".*/id: \"$$id\"/" \
	      -e "s/^created: \"YYYY-MM-DD\"/created: \"$$(date -u +%Y-%m-%d)\"/" \
	      -e "s/^domain: \"pipeline\".*/domain: \"$(or $(DOMAIN),pipeline)\"/" \
	      -e "s/^kind: \"feature\".*/kind: \"$(or $(KIND),feature)\"/" \
	      -e "s/^phase: 0.*/phase: $(or $(PHASE),0)/" \
	      -e "s/^# NNN: Title/# $$next: Title/" \
	      standards/spec/templates/spec-template.md > "$$dir/spec.md"; \
	  echo "scaffolded $$dir/spec.md; fill in title, summary, edges, then: spec-spine compile && spec-spine lint --fail-on-warn"

build:
# Spec 001 §3.5: the cargo half activates on a populated workspace (at least
# one crates/*/Cargo.toml), not on the root manifest alone, which cargo cannot
# load while its member entries match nothing.
ifneq ($(wildcard crates/*/Cargo.toml),)
	cargo build --workspace --locked
endif
ifneq ($(HAS_PNPM),)
	pnpm -r build
endif
	@test -n "$(HAS_CARGO)$(HAS_PNPM)" || echo "[build] no workspace manifests yet (spec 001, phase 1); nothing to build"

test:
ifneq ($(wildcard crates/*/Cargo.toml),)
	cargo test --workspace --locked
endif
ifneq ($(HAS_PNPM),)
	pnpm -r test
endif
	@test -n "$(HAS_CARGO)$(HAS_PNPM)" || echo "[test] no workspace manifests yet; nothing to test"

lint:
	@# Spec 017 FR-003: the product version lives in three files and they must
	@# agree. Cheap, has no prerequisites, and catches a release cut from a
	@# half-finished bump, so it runs before the expensive gates.
	python3 scripts/bump_version.py --check
ifneq ($(wildcard crates/*/Cargo.toml),)
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
	cargo deny check
endif
ifneq ($(HAS_PNPM),)
	pnpm -r lint
	pnpm -r typecheck
endif
	@test -n "$(HAS_CARGO)$(HAS_PNPM)" || echo "[lint] no workspace manifests yet; nothing to lint"

fmt:
ifneq ($(HAS_CARGO),)
	cargo fmt --all
endif

clean:
	rm -rf target apps/desktop/dist apps/desktop/node_modules node_modules
