---
id: "020-local-development-builds"
title: "Local development builds: run and install Butler on your own machine without a signing identity"
status: approved
kind: "tooling"
domain: "distribution"
created: "2026-09-09"
implementation: in-progress
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

## 7. Resolved decisions

- **D-1 (2026-09-09, the bundler is not trusted to have left it unsigned).**
  §3.2 step 4 says "ad-hoc sign". The first reading was "sign what the bundler
  did not", which is wrong on a machine whose shell already exports a real
  signing identity: the Tauri bundler would sign with it and the script would
  add nothing, producing a locally built bundle carrying a Developer ID.

  Nothing about that is dangerous, and it is still not what this path may
  produce. FR-003 exists so a local artifact is inspectably local, and an
  artifact that is sometimes ad-hoc and sometimes not, depending on an
  environment variable nobody remembers exporting, is exactly the ambiguity it
  forbids. The script therefore re-signs unconditionally and verifies
  `Signature=adhoc` before it installs anything.

  §3.2's prohibition on naming a credential is what rules out the obvious
  alternative, unsetting the identity variable before the build: the script
  cannot name it. Re-signing afterwards reaches the same guarantee without
  knowing what the variable is called.

- **D-2 (2026-09-09, nested code before the bundle, and not with `--deep`).**
  Signing the outer bundle in one pass is what the deprecated deep flag does,
  and it visits nested code in an order `codesign` itself warns about. The
  script signs dylibs and frameworks first, then the bundle, which is the order
  Apple documents and the only one that produces a bundle passing
  `--verify --strict`.

- **D-3 (2026-09-09, an assertion that a comment could satisfy).** The first
  version of §8's check for the deprecated flag was a bare `grep` over the
  whole script, and it failed on the first run against a script that does not
  use the flag: the comment in D-2 above, written into the source to explain
  the choice, satisfied the grep.

  That is 017 D-10's defect in a different file. An assertion about what code
  *does* must read only what executes, so the four behavioral checks strip
  comments first, and the signing check anchors on a line that begins with the
  command rather than on the flag appearing anywhere.

  The credential check deliberately does **not** strip comments. §3.2 forbids
  the script from setting, reading *or naming* a release credential, so a
  secret name written in a comment is a violation and the assertion covering it
  is correct to read the whole file.

- **D-4 (2026-09-09, the bundle shipped the wrong binary).** The first bundle
  this script produced installed cleanly, signed cleanly, verified cleanly, and
  had `Contents/MacOS/export-bindings` as its executable: spec 011's bindings
  exporter, which writes a TypeScript file and exits. Butler was never in it.

  `butler-desktop` declares two binary targets, `src/main.rs` (the app,
  10,769,232 bytes) and `src/bin/export-bindings.rs` (the exporter, 3,117,696
  bytes). The bundler ships the second.

  **Two obvious levers do not work, and each fails quietly.** Passing
  `-- --bin butler-desktop` produces `cargo build --bin butler-desktop --bins
  --features tauri/custom-protocol --release`: the CLI appends `--bins` after
  the caller's arguments, so both binaries are built anyway and the selection
  is unchanged. Setting `mainBinaryName` renames the copy without changing
  which file is copied, which is the worse of the two failures: the bundle then
  contains `Contents/MacOS/butler-desktop` holding the exporter's bytes, and
  every name-based check passes on it. That is how this defect survived its
  first fix here; it was caught by a content marker, not by a name.

  So the script replaces the bundle's executable with `target/release/
  butler-desktop` after bundling and rewrites `CFBundleExecutable`. The
  replacement is sound because it comes from the same build: the CLI compiles
  every binary with `--features tauri/custom-protocol`, the feature that makes
  a release binary serve embedded assets rather than look for a dev server. The
  script then compares the two files byte for byte, before signing rewrites
  them, and refuses to install on a mismatch.

  This is a workaround in this spec's territory, not a fix. The fix is to stop
  the exporter being an auto-discovered binary, with `required-features` on a
  `[[bin]]` table in 004's manifest or by moving it out of `src/bin/`. Both are
  other specs' territory and would change how 011's bindings are generated, so
  neither is made here.

  **This is not only this spec's problem, and it is not fixed here.**
  `.github/workflows/release.yml` lines 140 and 165 run `cargo tauri build`
  with no binary named, and `tauri.conf.json` sets no `mainBinaryName`. A
  release cut today would sign, notarize, staple, hash and attest an installer
  whose application is the bindings exporter, and every check in 017 would pass
  on it, exactly as 017 D-10 describes for a bundle whose webview renders
  nothing. The fix belongs to 004 (a `mainBinaryName` key in its config) or to
  017 (the flag in its workflow); both are other specs' requirements and
  changing one mid-build is not this spec's to make. It is reported instead.

- **D-5 (2026-09-09, what is verified, and the one row that waits for a
  person).** Everything §8 asserts runs and passes, and the path was exercised
  end to end on macOS 15 (arm64, Command Line Tools, no signing identity in the
  keychain): `make app` built, repaired, signed, verified and installed;
  `codesign --verify --strict` accepts the bundle; `codesign -dv` reports
  `Signature=adhoc` with no team identifier; the installed executable is
  10,724,768 bytes, carries none of the exporter's markers, and
  `CFBundleExecutable` names it. Launched from `/Applications`, it runs, does
  not crash, and creates its settings directory and 016's log file.

  **AC-1's last clause is not signed.** "Launches to the overlay" needs an eye
  on the overlay, and this one cannot be automated here for a reason that is
  the product working correctly: spec 005 excludes the window from the
  compositor's frame buffer, so a screenshot of a healthy Butler and a
  screenshot of a Butler that renders nothing are the same image. R-010 governs
  this shape, and here the row waits on no later spec, only on an operator
  looking at their own screen. The spec stays `implementation: in-progress`
  until that signature, as 017 D-9 stays open for its own handed-over rows.

## 8. Verification

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
# §3.2 and D-3: these four read only the lines that execute. A comment saying
# why the deprecated flag is not used satisfied an earlier bare grep, which is
# 017 D-10's defect in a different file.
#
# The bundle target is `app` alone. A `dmg` here would be a transport format
# for something this spec does not transport.
sh -c 'sed "s/#.*//" scripts/local_app.sh | grep -q -- "cargo tauri build --bundles app"'
sh -c '! sed "s/#.*//" scripts/local_app.sh | grep -q -- "--bundles dmg"'
# D-4: the crate has two binaries and the bundler ships spec 011's exporter.
# The repair must survive a later edit, and so must the byte comparison that
# proves it worked: a name-based check passes on a renamed exporter, which is
# how the first attempt at this fix went undetected.
sh -c 'sed "s/#.*//" scripts/local_app.sh | grep -qE "cp .*target/release/butler-desktop|cp \"\\\$REAL_BIN\""'
sh -c 'sed "s/#.*//" scripts/local_app.sh | grep -q "CFBundleExecutable"'
sh -c 'sed "s/#.*//" scripts/local_app.sh | grep -q "cmp -s"'
# Step 4: ad-hoc, anchored on the command rather than the flag, and never via
# the deprecated deep flag, which signs nested code in an order codesign itself
# warns about (D-2).
sh -c 'sed "s/#.*//" scripts/local_app.sh | grep -qE "^[[:space:]]*codesign --force --sign - "'
sh -c '! sed "s/#.*//" scripts/local_app.sh | grep -q -- "--deep"'
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
