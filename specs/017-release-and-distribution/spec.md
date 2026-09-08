---
id: "017-release-and-distribution"
title: "Release and distribution: signed, notarized, attested desktop builds with an opt-in updater"
status: approved
kind: "feature"
domain: "distribution"
created: "2026-09-01"
implementation: in-progress
owner: "butler-ai maintainers"
risk: high
platforms: ["windows", "macos"]
phase: 5
depends_on:
  # Phase 5 entry (018 R-002, R-007). 013 is the leaf of phase 4 and
  # transitively requires 010, 011 and 012.
  #
  # 015 is deliberately absent (018 R-008, D-2): it constrains
  # `tauri.conf.json`, which this spec also claims.
  - "003-governance-ci"
  - "004-desktop-shell"
  - "013-output-pacing"
establishes:
  - { kind: section, file: ".github/workflows/release.yml", anchor: "on" }
  - { kind: section, file: ".github/workflows/release.yml", anchor: "permissions" }
  - { kind: section, file: ".github/workflows/release.yml", anchor: "jobs.build" }
  - { kind: section, file: ".github/workflows/release.yml", anchor: "jobs.publish" }
  - "scripts/bump_version.py"
extends:
  # FR-003 checks three version fields against each other, so the bump script
  # writes `[workspace.package] version` in spec 001's root manifest and the
  # version in spec 012's app package.json. The third file is 004's
  # `tauri.conf.json`, already refined below.
  - { spec: "001-workspace-layout", unit: "Cargo.toml", nature: additive }
  - { spec: "012-overlay-ui", unit: "apps/desktop/package.json", nature: additive }
  # D-5: the version moved to `[workspace.package]`, so every member inherits
  # it with `version.workspace = true` and the bump script has one line to
  # write instead of five. Each member's manifest also carries the sibling
  # requirements the script rewrites, which cannot be dropped (spec 001
  # FR-004 bans wildcards and `cargo deny` reads an unversioned path
  # dependency as one).
  - { spec: "004-desktop-shell", unit: "apps/desktop/src-tauri/Cargo.toml", nature: additive }
  - { spec: "006-screen-capture", unit: "crates/butler-capture/Cargo.toml", nature: additive }
  - { spec: "007-text-recognition", unit: "crates/butler-ocr/Cargo.toml", nature: additive }
  - { spec: "009-pipeline-state-machine", unit: "crates/butler-core/Cargo.toml", nature: additive }
  - { spec: "010-assistant-inference", unit: "crates/butler-llm/Cargo.toml", nature: additive }
co_authority:
  # §3: `bump_version.py --check` runs in `make lint`, which specs 001 and 002
  # already share.
  - { unit: { kind: section, file: "Makefile", anchor: "lint" }, with_specs: ["001-workspace-layout", "002-agentic-harness"] }
refines:
  - { aspect: "bundling-and-updater", unit: "apps/desktop/src-tauri/tauri.conf.json" }
references:
  - { unit: { kind: file, path: "docs/threat-model.md" }, role: "supply chain" }
summary: >
  How a tagged commit becomes something a user can install and trust: a
  release workflow that builds on the two platform runners, code-signs
  (Authenticode on Windows; Developer ID plus notarization on macOS), emits
  SHA-256 sidecars and GitHub build-provenance attestations, and publishes a
  GitHub Release; an updater (`tauri-plugin-updater`) that is off by default
  and, when the user enables it, checks a single pinned endpoint over HTTPS
  with signature verification, as an amendment to the privacy boundary
  records. Version lives in three files kept in lockstep by a script.
  Deferred until phase 5; specified now so the shell's bundle config is
  authored with it in mind.
---

# 017: Release and distribution

## 1. Purpose

A capture-excluding overlay is exactly the kind of software that must be
verifiably what it claims to be: unsigned builds of it are indistinguishable
from malware to an endpoint agent, and to a user. Signing, notarization and
provenance are therefore not polish; they are part of the product's
trustworthiness. The workflow is specified with the same section ownership
as CI (003) because it runs with publish credentials.

## 2. Territory

The security-relevant keypaths of `.github/workflows/release.yml`; the
version bump script; the `bundle` and `plugins.updater` sections of
`tauri.conf.json`, refined here on the `bundling-and-updater` aspect (the file
is 004's).

## 3. Behavior

- **Trigger**: `on.push.tags: ["v*"]` only; `permissions: contents: write,
  id-token: write, attestations: write` at the job level, read elsewhere.
- **`jobs.build`** (matrix `windows-latest`, `macos-latest`): `pnpm install
  --frozen-lockfile`, `cargo tauri build` with the platform targets from
  spec 001; sign (Windows: Authenticode via a certificate in a secret,
  timestamped; macOS: Developer ID Application, hardened runtime, notarize
  with `notarytool`, staple); produce `.msi`/`.exe` and `.dmg`; write
  `.sha256` sidecars; `actions/attest-build-provenance` on every artifact.
- **`jobs.publish`**: create the GitHub Release from the tag with the
  artifacts, sidecars and the updater manifest (`latest.json`, signed with
  the Tauri updater key held in a secret).
- **Version**: `scripts/bump_version.py <x.y.z>` updates `Cargo.toml`
  (`[workspace.package] version`), `apps/desktop/package.json`, and
  `tauri.conf.json`; `--check` verifies agreement and runs in `make lint`.
- **Updater**: disabled in `tauri.conf.json` by default; settings expose
  `updates.check_enabled` (default off). When on, the plugin checks the pinned
  release endpoint at most once per day, verifies the manifest signature with
  the embedded public key, and installs only after the user confirms. The
  endpoint is added to 015's single-destination rule by amendment when this
  spec is implemented.
- **SBOM**: `cargo cyclonedx` and `pnpm sbom` outputs attached to the release.

## 4. Functional requirements

- **FR-001.** Every artifact on a release has a sidecar whose hash matches
  and an attestation that `gh attestation verify` accepts.
- **FR-002.** macOS builds pass `spctl --assess --type execute` on a clean
  machine; Windows builds show the publisher name in SmartScreen.
- **FR-003.** `bump_version.py --check` fails when any of the three files
  disagrees.
- **FR-004.** With updates off, the app makes no request to the release
  endpoint (015 FR-005 extended to this host).

## 5. Acceptance criteria

- **AC-1.** A `v0.1.0` tag produces a release with both platform artifacts.
- **AC-2.** The threat model's supply-chain section names the signing keys'
  custody and rotation procedure.

## 6. Out of scope

- App stores (Microsoft Store, Mac App Store): sandboxing conflicts with the
  window flags the shell needs; revisit if the flags become permissible.
- Linux packaging (spec 001 §6).

## 7. Resolved decisions

- **D-1 (2026-09-02).** `depends_on` gained 013, the phase 4 leaf, as the 018
  R-002 gate: a release cannot be cut before the product it ships works end
  to end, and the orchestrator schedules on `depends_on` alone.

- **D-2 (2026-09-07, this spec stops at a person, and here is the list).**
  Everything in this spec that a repository can hold is built. Everything that
  needs a signing identity is not, because a signing identity is not something
  an agent may create, hold, or stand in for. The spec stays
  `implementation: in-progress` for that reason and not because a unit is
  missing: `make burndown` shows zero.

  **What an operator must supply**, as GitHub Actions secrets on this
  repository:

  | Secret | What it is |
  |---|---|
  | `APPLE_CERTIFICATE` | the Developer ID Application certificate and key, as a base64 `.p12` |
  | `APPLE_CERTIFICATE_PASSWORD` | the password for that `.p12` |
  | `APPLE_SIGNING_IDENTITY` | the identity's common name, e.g. `Developer ID Application: NAME (TEAMID)` |
  | `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | the notarization credential (`APPLE_PASSWORD` is an app-specific password, never the account password) |
  | `WINDOWS_CERTIFICATE` | the Authenticode certificate and key, as a base64 `.pfx` |
  | `WINDOWS_CERTIFICATE_PASSWORD` | the password for that `.pfx` |

  Until they exist, `jobs.build` fails on its first substantive step with a
  message naming each missing one. That is deliberate: §1 says an unsigned
  build of this product is indistinguishable from malware, so refusing is
  correct and producing an unsigned artifact anyway is not.

  **No step in this workflow has ever run.** There is no tag and there are no
  secrets, so the first tag exercises every step for the first time. The three
  worth watching, in order of likelihood:

  1. The Windows signing step (D-7's certificate fork).
  2. `cargo tauri build` on a runner: nothing has yet built a bundle on either
     platform, only a binary.
  3. The artifact glob in `Collect artifacts`, which assumes the bundler's
     `target/release/bundle/**` layout. It fails loudly on an empty directory
     rather than publishing nothing, which is the failure mode that matters.

  What *is* verified: every action reference resolves at its pinned tag and
  none is a Docker container action on a non-Linux runner
  (`scripts/check-action-runners.sh`); every input name used exists on the
  action that receives it; `bump_version.py` is tested with both controls;
  `cargo cyclonedx --top-level` and `pnpm licenses list --json` were run in
  this repository before being written into a step; and every key added to
  `tauri.conf.json` was checked against `https://schema.tauri.app/config/2`
  and then compiled, since `tauri-build` reads the config at build time.

- **D-3 (2026-09-07, `pnpm sbom` is not a pnpm command).** §3 says "`cargo
  cyclonedx` and `pnpm sbom` outputs attached to the release". `pnpm sbom`
  prints `undefined` and exits 0, which is worse than failing: a step built on
  it would attach an empty file and report success.

  The Rust half is real and is what matters, because the compiled attack
  surface is Rust: `cargo cyclonedx --top-level` emits one CycloneDX 1.3
  document for the shipped binary's graph. The JavaScript half is
  `pnpm licenses list --json`, which is an inventory rather than CycloneDX;
  the overlay's dependencies are build-time only and none of them ship inside
  the binary, so an inventory is the honest artefact rather than a CycloneDX
  document implying more than pnpm can tell us.

- **D-4 (2026-09-07, the updater is not configured, and that is the default
  §3 asks for).** §3 says the updater is "disabled in `tauri.conf.json` by
  default". It is absent, which is disabled, and it stays absent because
  enabling it is four things an agent cannot do alone:

  - a signing keypair whose private half the operator must hold and never
    put on a runner (see the threat model's custody table: this key decides
    what every installed copy downloads next);
  - a pinned release endpoint, which is a **second network destination**, and
    spec 015's `constrains` edge on `tauri.conf.json` allows one. §3 itself
    says the endpoint "is added to 015's single-destination rule by
    amendment"; amending 015 needs explicit human approval and an agent never
    writes one on its own authority (`.claude/rules/adversarial-prompt-refusal.md`);
  - `tauri-plugin-updater` in the manifest and a capability grant for it;
  - `updates.check_enabled` in `Settings`, which is dead configuration while
    the three above are missing, and a settings toggle that does nothing is a
    lie in the one panel whose job is to be believed.

  **Owed**, as one unit and in that order. This is the same shape as spec 016
  D-4, where a `dialog` grant was declined for the same reason.

- **D-5 (2026-09-07, the version moved to the workspace, and the sibling
  requirements that could not follow).** §3 names `[workspace.package]
  version`, which did not exist: all five members pinned `version = "0.1.0"`
  inline. They now inherit with `version.workspace = true`, so the bump has
  one line to write and five stale numbers cannot be left behind.

  The intra-workspace **dependency** requirements could not move the same way.
  `version.workspace` is not a thing inside a dependency table, and the
  requirement cannot simply be dropped: spec 001 FR-004 sets `[bans] wildcards
  = "deny"` and `cargo deny` reads a path dependency without a version as a
  wildcard. Removing them made `cargo deny` the thing that would fail, and
  leaving them made a bump to `0.2.0` fail to resolve, with the three version
  fields still agreeing.

  So `bump_version.py` rewrites them too, and `--check` asserts they equal the
  workspace version. That failure is worse than the one FR-003 describes and
  the spec does not mention it: three files agreeing while the workspace
  refuses to load looks like a build problem, not a versioning one. The
  self-test has a control for it.

- **D-6 (2026-09-07, `--expect`, because agreeing on the wrong number is
  still wrong).** FR-003 checks the three files against each other and nothing
  against the tag. Three files can agree perfectly on `0.1.0` under a `v0.2.0`
  tag, and the release would then ship `0.1.0` binaries with provenance
  attestations that match, so every downstream check would pass. That is a
  worse outcome than disagreement, which at least fails.

  `--check --expect X.Y.Z` closes it and `jobs.build` passes
  `${GITHUB_REF_NAME#v}` as its first substantive step, before any credential
  is touched.

- **D-7 (2026-09-07, the Windows certificate decides the mechanism).** The
  workflow implements the OV path: import the `.pfx` into the user store, read
  the thumbprint back, and pass it to the bundler through `--config`, which
  drives `signtool` with the timestamp URL and digest algorithm from
  `tauri.conf.json`.

  An **EV or cloud-HSM certificate cannot be imported that way**: the private
  key never leaves the token, and signing goes through a vendor tool. That is
  `bundle.windows.signCommand`, a different shape entirely. Which applies is a
  property of the certificate the operator buys, so the fork is recorded here
  rather than guessed at. macOS has no equivalent fork: Developer ID is the
  only option and the Tauri bundler drives signing, notarization and stapling
  from the six environment variables in D-2.

- **D-8 (2026-09-07, the publish credential is not in the build job).** §3
  puts `contents: write, id-token: write, attestations: write` "at the job
  level". They are, but split: `jobs.build` takes `id-token` and
  `attestations` and keeps `contents: read`, and only `jobs.publish` takes
  `contents: write`.

  The build job is the one that runs a compiler over third-party code with the
  signing certificates in its environment. Giving it the ability to write
  releases as well would mean a single compromised build dependency could sign
  *and* publish. Splitting them means it can do neither on its own. This is
  narrower than §3 asks for, in the direction §3's own "read elsewhere" is
  reaching.

- **D-9 (2026-09-07, what cannot be observed without a real release).** AC-1
  ("a `v0.1.0` tag produces a release with both platform artifacts"), FR-001
  (an attestation `gh attestation verify` accepts), FR-002 (`spctl --assess`
  and SmartScreen) and FR-004 (no request to the release endpoint) all require
  artifacts that do not exist and cannot exist without D-2's credentials.

  018 R-010 reads a spec's manual criteria against what its own phase can
  observe, and says a deferred row names the spec it waits on. These rows wait
  on **no spec**: nothing later in the corpus makes them observable, only an
  operator action outside the repository. So they are not deferred to a phase,
  they are handed over, and this spec stays `in-progress` until the operator
  runs the first tag. FR-004 additionally waits on D-4, since there is no
  endpoint to not-call yet.

  AC-2 and FR-003 are met and verified in §8.

- **D-10 (2026-09-07, the release would have shipped an empty window).**
  `tauri.conf.json` had no `build.beforeBuildCommand`, so `cargo tauri build`
  packages `frontendDist` exactly as it finds it. What it would have found is
  spec 004 D-3's placeholder `dist/`, written by `build.rs` so the Rust crate
  could compile before the overlay package existed.

  The result would have been a signed, notarized, attested installer whose
  webview renders nothing, with every check in this spec passing: the
  artifacts exist, the hashes match, the attestations verify. Nothing in the
  pipeline looks at what is inside the bundle.

  The hook is `pnpm build`, and it is in `tauri.conf.json` rather than as a
  workflow step so that a developer running `cargo tauri build` locally gets
  the same guarantee. Both build steps run in `apps/desktop`, which is the
  Tauri app directory: the CLI finds `src-tauri/` from there and the hook runs
  there. §8 asserts the hook through the parser.

  Found by re-reading the workflow rather than by any check, which is the
  point of D-2's list: no step in it has run.

## 8. Verification

Every command here runs today. The steps that cannot run without a signing
identity are D-2's, and D-9 says which criteria they carry.

```verify:cli
# FR-003 with all four controls, against a copy of the real tree so the shapes
# it exercises are the shapes that ship: the bump moves the three files and
# the sibling requirements, and `--check` catches a file left behind, a stale
# sibling requirement, and a tag the tree does not carry (D-5, D-6).
python3 scripts/bump_version.py --self-test
# FR-003 as `make lint` runs it, and the wiring that makes it run at all.
python3 scripts/bump_version.py --check
grep -q "bump_version.py --check" Makefile
# D-6: the release checks the tag against the tree, and rejects a tag it does
# not carry. The second command must fail, which is what makes the first mean
# something.
python3 scripts/bump_version.py --check --expect 0.1.0
sh -c '! python3 scripts/bump_version.py --check --expect 9.9.9 >/dev/null 2>&1'
# Section 3: tags only, and the default permission is read (D-8 splits the
# rest across the two jobs).
grep -q 'tags: \["v\*"\]' .github/workflows/release.yml
# Spec 003 D-1: no Docker container action on the Windows/macOS matrix. This
# also fetches every action's metadata, so a reference that does not resolve
# at its pinned tag is reported.
./scripts/check-action-runners.sh
# Section 1: an unsigned artifact is never published. The build refuses before
# it starts when a signing secret is absent, rather than building something it
# cannot sign.
grep -q "cannot sign a" .github/workflows/release.yml
# D-8: the job holding the signing certificates cannot also publish.
sh -c 'awk "/^  build:/,/^  publish:/" .github/workflows/release.yml | grep -q "contents: read"'
# Section 3's bundle section: the installers a release publishes, and the
# `active` flag without which the bundler emits nothing. Asserted through the
# parser rather than by grep, so a key in a comment cannot satisfy it.
python3 -c "import json; c = json.load(open('apps/desktop/src-tauri/tauri.conf.json')); assert c['bundle']['active'] is True; assert sorted(c['bundle']['targets']) == ['dmg', 'msi', 'nsis'], c['bundle']['targets']"
# D-10: the bundler packages `frontendDist` as it finds it, and what it would
# find without a build hook is spec 004 D-3's placeholder. Every other check
# in this spec passes on an installer whose webview renders nothing, because
# nothing else looks inside the bundle.
python3 -c "import json; c = json.load(open('apps/desktop/src-tauri/tauri.conf.json')); assert c['build']['beforeBuildCommand'] == 'pnpm build', c['build']"
# D-4: the updater is absent, which is section 3's default. Adding it needs a
# keypair and an amendment to spec 015's single-destination rule.
sh -c '! grep -q "updater" apps/desktop/src-tauri/tauri.conf.json'
# Spec 015's freeze on this file survived the bundle section.
python3 -c "import json; c = json.load(open('apps/desktop/src-tauri/tauri.conf.json')); assert c['app']['security']['csp'].startswith(\"default-src 'self'\"); assert c['app']['security']['assetProtocol']['enable'] is False"
# The config compiles: `tauri-build` reads it at build time, so a key that
# does not exist fails here rather than on the first tag.
cargo build -p butler-desktop --locked
# AC-2: the threat model names the custody and rotation procedure.
grep -q "Signing-key custody and rotation" docs/threat-model.md
grep -q "One named custodian per key" docs/threat-model.md
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "017-release-and-distribution" && exit 1 || exit 0'
```
