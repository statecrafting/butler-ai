---
id: "017-release-and-distribution"
title: "Release and distribution: signed, notarized, attested desktop builds with an opt-in updater"
status: approved
kind: "feature"
domain: "distribution"
created: "2026-09-01"
implementation: pending
owner: "butler-ai maintainers"
risk: high
platforms: ["windows", "macos"]
phase: 5
depends_on:
  # Phase 5 entry (018 R-002, R-007). 013 is the leaf of phase 4 and
  # transitively requires 010, 011 and 012.
  - "003-governance-ci"
  - "004-desktop-shell"
  - "013-output-pacing"
  - "015-privacy-boundary"
establishes:
  - { kind: section, file: ".github/workflows/release.yml", anchor: "on" }
  - { kind: section, file: ".github/workflows/release.yml", anchor: "permissions" }
  - { kind: section, file: ".github/workflows/release.yml", anchor: "jobs.build" }
  - { kind: section, file: ".github/workflows/release.yml", anchor: "jobs.publish" }
  - "scripts/bump_version.py"
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
