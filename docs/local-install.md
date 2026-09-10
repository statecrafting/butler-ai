# Running Butler on your own Mac

<!-- Spec: specs/020-local-development-builds/spec.md -->

This is the local path. It needs no signing identity, no notarization and no
repository secret, because it distributes nothing. Anything that leaves this
machine is [spec 017](../specs/017-release-and-distribution/spec.md)'s, and that
path deliberately refuses to produce an artifact it cannot sign.

macOS only for now. The Windows equivalent is out of scope until a spec can
claim it from a machine that can verify it (spec 020 section 6).

## Once

```sh
make setup                                       # spec-spine and the governed loop
cargo install tauri-cli --version "^2" --locked  # the bundler
pnpm install --frozen-lockfile
```

You need the pinned Rust toolchain (`rust-toolchain.toml`) and Node 22
(`.nvmrc`). Xcode is not required; Command Line Tools is enough.

## The two commands

```sh
make dev     # hot-reload loop: Vite dev server plus the Rust runtime
make app     # build, ad-hoc sign, verify, install to /Applications
```

`make dev` is the loop you want while changing the overlay. `make app` is what
you want when you are testing Butler as an application: launched from
Spotlight, holding its own permissions, behaving the way it will for a user.

`DESTDIR=~/Applications make app` installs somewhere else.

Both refuse rather than guess. A missing Tauri CLI is reported with the exact
command that supplies it; a non-macOS host is reported as such. Neither
installs a toolchain on your behalf.

## Give it Screen Recording

Butler reads the screen, so macOS requires an explicit grant. The app asks once
per launch and then stops, deliberately: an app that re-prompts trains you to
dismiss the dialog.

**System Settings > Privacy & Security > Screen Recording**, add Butler, enable
it, and relaunch.

It never asks for Accessibility. Butler does not synthesize input, and
requesting a permission the product does not use is exactly the pattern a
privacy-first tool should not have.

## The rebuild re-prompt, and how to stop it

**macOS will ask again after every rebuild.** This is the sharpest edge on the
local path and it is worth understanding rather than fighting.

An ad-hoc signature has no certificate behind it. The requirement macOS derives
for the bundle therefore pins its `cdhash`, which is a hash of the code itself,
and every rebuild changes it. The permission system compares the new bundle
against the recorded requirement, finds no match, and correctly concludes it is
looking at a different application. The old entry stays in the list, and they
accumulate.

Nothing in the build can avoid this. What avoids it is a signature backed by a
certificate that does not change between builds, and you can make one locally:

1. **Keychain Access > Certificate Assistant > Create a Certificate.** Name it
   `Butler Local`, Identity Type `Self Signed Root`, Certificate Type
   `Code Signing`. Let it default everything else.
2. After each `make app`, re-sign the installed copy:

   ```sh
   codesign --force --sign "Butler Local" /Applications/Butler.app
   ```

The grant then survives rebuilds, because the requirement pins the certificate
rather than the code. This is deliberately not automated: it puts a new root
certificate in your keychain, which is a trust decision, and a script should
not make one for you.

Note the ordering. `make app` verifies its own output is ad-hoc before
installing, so the re-sign happens to the installed copy afterwards. That check
exists so a local build can never quietly acquire a real identity and start
looking like a release.

## Gatekeeper and quarantine

A bundle built on the machine that runs it is never quarantined, so `make app`
produces something that just launches. No override, no right-click, no
"Open Anyway".

The override only matters if you carry the bundle to a second machine, where it
arrives with the quarantine attribute and `spctl --assess` rejects it. Either:

```sh
xattr -dr com.apple.quarantine /Applications/Butler.app
```

or launch it once, let macOS refuse, then **System Settings > Privacy &
Security > Open Anyway**.

Do this only for a bundle you built yourself. The whole argument of spec 017
section 1 is that a capture-excluding overlay from an unknown source is
indistinguishable from malware, and clearing quarantine is precisely the step
that discards that protection.

## What this build is not

`codesign -dv /Applications/Butler.app` reports `Signature=adhoc` and no
authority. That is not a defect to be worked around, it is the honest state of
a build made thirty seconds ago on the machine in front of you. It attests
nothing about origin, and none of the supply-chain defenses in
[the threat model](threat-model.md) apply to it.

A release is a different artifact produced by a different path: signed with a
Developer ID, notarized, stapled, hashed, attested, and published from a tag.
Spec 017 owns all of it and stops, correctly, at credentials no agent may hold.
