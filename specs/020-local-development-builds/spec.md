---
id: "020-local-development-builds"
title: "Local development builds: run and install Butler on your own machine without a signing identity"
status: approved
kind: "tooling"
domain: "distribution"
created: "2026-09-09"
implementation: pending
owner: "butler-ai maintainers"
risk: medium
platforms: ["macos"]
phase: 5
depends_on:
  # The shell is what gets launched, and 013 is the phase 4 leaf: installing a
  # build only means something once the product answers end to end. This is the
  # same gate 017 D-1 records, for the same reason.
  - "004-desktop-shell"
  - "013-output-pacing"
establishes:
  - "scripts/local_app.sh"
  - "docs/local-install.md"
extends:
  # Two new Makefile targets. The file is 002's; a target inside it is a
  # section unit claimed additively, never a second `establishes` on the file.
  - { spec: "002-agentic-harness", unit: { kind: section, file: "Makefile", anchor: "dev" }, nature: additive }
  - { spec: "002-agentic-harness", unit: { kind: section, file: "Makefile", anchor: "app" }, nature: additive }
references:
  - { unit: { kind: file, path: "docs/threat-model.md" }, role: "what an ad-hoc signature does not attest" }
summary: >
  The route from this checkout to a running Butler on the developer's own Mac,
  which no spec claimed: a `make dev` hot-reload loop and a `make app` that
  produces an ad-hoc signed `Butler.app` and installs it. It needs no Developer
  ID, no notarization and no repository secret, because nothing it produces is
  published: the artifact carries no authority in its signature and says so.
  Spec 017 is untouched and still governs anything a stranger downloads. The
  costs an ad-hoc signature imposes, chiefly that macOS re-asks for Screen
  Recording after every rebuild, are written down rather than papered over.
---

# 020: Local development builds

## 1. Purpose

Nineteen specs describe a product, and until this one there was no way to run
it. `make build` compiles a binary; the only path that produced something
launchable was `.github/workflows/release.yml`, which refuses to proceed
without the six Apple and two Windows secrets in 017 D-2. That refusal is
correct for a published artifact: 017 §1 argues that an unsigned build of a
capture-excluding overlay is indistinguishable from malware, and it is right.

It is the wrong gate for the machine the code was written on. A developer
building on their own Mac is not distributing anything, and the trust question
a signature answers ("did this come from who it claims?") has no content when
the answer is "from the checkout in front of you, thirty seconds ago". This
spec claims that route, and only that route.

The separation is structural rather than a matter of care. Nothing here can
produce a release artifact, because nothing here touches the release workflow
or a signing credential, and the bundle it produces carries an ad-hoc signature
whose absence of an authority is visible to `codesign -dv`. Constitution VI
governs the shape: the honest description of a local build is "local build",
and the product should be as truthful about its own provenance as it is about
its capture exclusion.

## 2. Territory

| Unit | Role |
|---|---|
| `scripts/local_app.sh` | build, ad-hoc sign, verify, install |
| `docs/local-install.md` | the procedure, and the macOS behavior it cannot change |
| `Makefile` section `dev` | the hot-reload loop (`extends` 002) |
| `Makefile` section `app` | the installable bundle (`extends` 002) |

**Deliberately not claimed.** `apps/desktop/src-tauri/tauri.conf.json` is 004's,
015 `constrains` it and 017 refines its `bundling-and-updater` aspect; this spec
adds no key to it and reads it only through the Tauri CLI, which is why
`hardenedRuntime` and the bundle targets need no exception here. The Tauri
bundler skips macOS signing entirely when no signing identity is present in the
environment, so `bundle.macOS.hardenedRuntime` is inert on this path and the
script signs afterwards on its own terms. `.github/workflows/release.yml` and
`scripts/bump_version.py` are 017's and are untouched.

## 3. Behavior

### 3.1 `make dev`

- MUST run the Tauri development loop against the overlay's Vite dev server,
  from `apps/desktop`, so the app directory the CLI discovers `src-tauri/` from
  is the one 017 D-10 already established for the build hook.
- MUST refuse, with a non-zero exit and the exact install command, when the
  Tauri CLI is absent. It MUST NOT install a toolchain on the developer's
  behalf: 017 D-2 sets the precedent that a missing prerequisite is named, not
  guessed at or silently acquired.

### 3.2 `make app`

`make app` MUST delegate to `scripts/local_app.sh`, which performs, in order,
stopping at the first failure:

1. **Refuse a non-macOS host.** `platforms: ["macos"]`, and §6 says why the
   Windows half is not written here.
2. **Refuse a missing Tauri CLI**, with the same message `make dev` uses.
3. **Build** the `app` bundle only. Not `dmg`: a disk image is a transport
   format, and nothing on this path is transported.
4. **Ad-hoc sign** the bundle (`codesign --sign -`), signing nested code before
   the outer bundle rather than relying on the deprecated `--deep`.
5. **Verify** the signature (`codesign --verify --strict`), so a bundle that
   cannot launch fails here rather than in the Dock.
6. **Install** to `/Applications/Butler.app` by default, replacing any previous
   local build; `DESTDIR` overrides the destination.
7. **Report** the signature's nature and the permission step the user must take
   next.

- The script MUST NOT set, read or name any credential from 017 D-2
  (`APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`,
  `APPLE_TEAM_ID`, `APPLE_CERTIFICATE_PASSWORD`, `WINDOWS_CERTIFICATE`,
  `WINDOWS_CERTIFICATE_PASSWORD`). A local build has no business holding one,
  and a script that reads them is one environment variable away from becoming
  an unaudited release path.
- The script MUST NOT write `tauri.conf.json` or any file under `.github/`.
- Neither target may be a prerequisite of `build`, `test`, `lint`, `gate` or
  `ci`. Installing an application is not a gate, and CI has no display.

### 3.3 What an ad-hoc signature costs, stated plainly

An ad-hoc signature has no certificate, so the designated requirement macOS
derives for the bundle pins its `cdhash`. Every rebuild changes that hash.
Three consequences follow, and `docs/local-install.md` MUST record all three
rather than let the developer discover them as bugs:

- **Screen Recording is re-asked after a rebuild.** TCC matches the new bundle
  against the recorded requirement, does not match, and treats it as a new
  application. The old entry accumulates in System Settings. This is macOS
  behavior, not a defect in the product, and no amount of scripting removes it
  without a stable signing certificate.
- **Gatekeeper.** A bundle built on the machine that runs it is never
  quarantined, so it launches with no prompt. The override matters only for a
  bundle carried to a second machine, where `spctl --assess` will reject it;
  the document names both the `xattr` removal and the System Settings route.
- **The signature attests nothing about origin.** `docs/threat-model.md`
  describes what Developer ID and notarization defend against; none of it
  applies here. A local build is trusted because the developer built it, and
  for no other reason.

## 4. Functional requirements

- **FR-001.** With the Tauri CLI absent, `scripts/local_app.sh` exits non-zero,
  names the install command, and creates no bundle.
- **FR-002.** On a host that is not macOS, `scripts/local_app.sh` exits
  non-zero with a message distinct from FR-001's.
- **FR-003.** The installed bundle passes `codesign --verify --strict` and
  reports an ad-hoc signature (`Signature=adhoc`), so a build that silently
  acquired a real identity is visible as a difference.
- **FR-004.** No file in this spec's territory names a credential from 017 D-2,
  writes `tauri.conf.json`, or writes under `.github/`.
- **FR-005.** `make ci` and `make gate` do not invoke `dev` or `app`.

## 5. Acceptance criteria

- **AC-1.** `make app` on macOS with the toolchain present produces
  `Butler.app`, installs it, and the installed bundle launches to the overlay.
- **AC-2.** `docs/local-install.md` names the Screen Recording grant, the
  rebuild re-prompt of §3.3, and the Gatekeeper override.
- **AC-3.** Both refusals of FR-001 and FR-002 are exercised by the
  verification block with distinct messages.

## 6. Out of scope

- **Windows local builds.** The equivalent is an unsigned NSIS bundle and the
  SmartScreen "More info, Run anyway" route. It is not written here because it
  cannot be verified from the machine this spec was authored on, and unverified
  prose in a spec is the failure mode 017 D-2 spent a page warning about. A
  later spec claims it, with `platforms: ["windows"]`.
- **Distribution of any kind.** Releases are 017's, entirely. Nothing here
  produces a `dmg`, a checksum, an attestation or a release.
- **A stable local signing identity.** A self-signed certificate in the login
  keychain would make the Screen Recording grant survive rebuilds, which is the
  sharpest edge in §3.3. It needs a keychain trust decision made by a human at
  an interactive prompt, so it is documented as an option in
  `docs/local-install.md` and automated by nothing.
- **The updater.** Absent, per 017 D-4, and this path never checks for one.

## 7. Verification

```verify:cli
# FR-001: a missing Tauri CLI is named, not guessed at, and nothing is built.
# PATH is emptied of cargo so the check cannot pass because the CLI happens to
# be installed on the machine running it.
sh -c 'out=$(PATH=/usr/bin:/bin BUTLER_LOCAL_DRY_RUN=1 sh scripts/local_app.sh 2>&1); test $? -ne 0 && printf "%s" "$out" | grep -q "cargo install tauri-cli"'
# FR-002: a non-macOS host is refused, with a different message from FR-001's.
sh -c 'out=$(BUTLER_LOCAL_FAKE_UNAME=Linux sh scripts/local_app.sh 2>&1); test $? -ne 0 && printf "%s" "$out" | grep -qi "macos"'
sh -c '! BUTLER_LOCAL_FAKE_UNAME=Linux sh scripts/local_app.sh 2>&1 | grep -q "cargo install tauri-cli"'
# FR-004: no release credential is named anywhere in this spec's territory, and
# the script writes neither 004's config nor 017's workflow.
sh -c '! grep -qE "APPLE_(CERTIFICATE|SIGNING_IDENTITY|ID|PASSWORD|TEAM_ID)|WINDOWS_CERTIFICATE" scripts/local_app.sh docs/local-install.md'
sh -c '! grep -qE ">[[:space:]]*(apps/desktop/src-tauri/tauri\.conf\.json|\.github/)" scripts/local_app.sh'
# FR-005: installing an application is not a gate. `ci` and `gate` must not
# reach `app` or `dev`.
sh -c '! awk "/^ci:/{print}" Makefile | grep -qE "\b(app|dev)\b"'
sh -c '! awk "/^gate:/,/^\$/" Makefile | grep -qE "make (app|dev)"'
# §3.2: the bundle target is `app` alone. A `dmg` here would be a transport
# format for something this spec does not transport.
grep -q -- "--bundles app" scripts/local_app.sh
sh -c '! grep -q -- "--bundles dmg" scripts/local_app.sh'
# §3.2 step 4: ad-hoc, and not via the deprecated `--deep`, which signs nested
# code in an order codesign itself warns about.
grep -q -- "--sign -" scripts/local_app.sh
sh -c '! grep -q -- "--deep" scripts/local_app.sh'
# AC-2: the three costs of §3.3 are each named in the document.
grep -qi "Screen Recording" docs/local-install.md
grep -qi "rebuild" docs/local-install.md
grep -qi "quarantine" docs/local-install.md
# §3.1 and §3.2: both targets exist and `app` delegates to the script.
sh -c 'awk "/^app:/,/^\$/" Makefile | grep -q "scripts/local_app.sh"'
sh -c 'awk "/^dev:/,/^\$/" Makefile | grep -q "tauri"'
# Territory: every unit this spec claims resolves.
sh -c 'spec-spine index render | grep "W-001" | grep -q "020-local-development-builds" && exit 1 || exit 0'
```
