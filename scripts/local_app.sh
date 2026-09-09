#!/bin/sh
# Spec: specs/020-local-development-builds/spec.md
#
# Build, ad-hoc sign, verify and install Butler.app on the machine that ran
# this. Nothing here is published, so nothing here holds a credential: spec 020
# section 3.2 forbids this script from naming one, and spec 017 still governs
# every artifact a stranger might download.
#
# Modes:
#   sh scripts/local_app.sh                    build, sign, verify, install
#   sh scripts/local_app.sh --check-toolchain  the refusals only, then exit 0
#   BUTLER_LOCAL_DRY_RUN=1 sh scripts/local_app.sh   print the plan, touch nothing
#
# Environment:
#   DESTDIR                 install destination (default /Applications)
#   BUTLER_LOCAL_DRY_RUN    non-empty: report the plan and exit
#   BUTLER_LOCAL_FAKE_UNAME override the host for the section 3.2 step 1
#                           refusal, so the verification block can exercise it
#                           on any machine. It can only make this script
#                           refuse, never proceed.

set -eu

TAURI_INSTALL_HINT='cargo install tauri-cli --version "^2" --locked'

# Repository root, so the script works from anywhere.
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
APP_DIR="$ROOT/apps/desktop"
BUNDLE="$ROOT/target/release/bundle/macos/Butler.app"
DEST=${DESTDIR:-/Applications}

# --- section 3.2 step 1: refuse a non-macOS host ------------------------------
# Before the toolchain check, and with a distinct message, because a Linux
# developer needs to know the path does not exist yet rather than be sent to
# install a CLI that will not help them (FR-002).
host=${BUTLER_LOCAL_FAKE_UNAME:-$(uname -s)}
if [ "$host" != "Darwin" ]; then
  echo "local_app.sh: this path is macOS only; the host reports '$host'." >&2
  echo "A Windows local build is deliberately out of scope until a spec can" >&2
  echo "claim it from a machine that can verify it (spec 020 section 6)." >&2
  exit 2
fi

# --- section 3.2 step 2: refuse a missing Tauri CLI ---------------------------
# Named, never silently acquired. Spec 017 D-2 sets the precedent: a missing
# prerequisite is reported with the exact command that supplies it (FR-001).
if ! command -v cargo-tauri >/dev/null 2>&1; then
  echo "local_app.sh: the Tauri CLI is not installed." >&2
  echo "Install it once, then run this again:" >&2
  echo "  $TAURI_INSTALL_HINT" >&2
  exit 3
fi

if [ "${1:-}" = "--check-toolchain" ]; then
  exit 0
fi

if [ -n "${BUTLER_LOCAL_DRY_RUN:-}" ]; then
  echo "local_app.sh: dry run, nothing will be built or installed."
  echo "  build   cargo tauri build --bundles app   (in $APP_DIR)"
  echo "  repair  replace the bundle's executable with the app binary (D-4)"
  echo "  sign    codesign --force --sign - (ad-hoc, no hardened runtime)"
  echo "  verify  codesign --verify --strict, and Signature=adhoc"
  echo "  install $BUNDLE -> $DEST/Butler.app"
  exit 0
fi

# --- section 3.2 step 3: build the app bundle only ----------------------------
# Not `dmg`. A disk image is a transport format and nothing on this path is
# transported; 017 owns everything that is.
echo "==> building (this compiles the workspace in release; first run is slow)"
( cd "$APP_DIR" && cargo tauri build --bundles app )

test -d "$BUNDLE" || {
  echo "local_app.sh: the bundler reported success but $BUNDLE is missing." >&2
  echo "Nothing was installed." >&2
  exit 4
}

# --- section 3.2 step 3b: put the right binary in the bundle -------------------
# `butler-desktop` has two binary targets, the app and spec 011's bindings
# exporter, and the bundler ships the exporter (D-4). Neither `--bin` nor
# `mainBinaryName` changes that: the CLI appends `--bins` after any cargo
# arguments, so both are always built, and the config key renames the copy
# without changing which file is copied.
#
# The replacement is safe precisely because it comes from the build that just
# ran: the CLI compiles with `--features tauri/custom-protocol`, which is what
# makes a release binary serve its embedded assets instead of looking for a dev
# server, and it applies that to every binary it builds. So `target/release/
# butler-desktop` is the app, built correctly, sitting next to the wrong one.
REAL_BIN="$ROOT/target/release/butler-desktop"
test -f "$REAL_BIN" || {
  echo "local_app.sh: $REAL_BIN is missing after a successful build." >&2
  exit 6
}
echo "==> installing the app binary into the bundle (D-4)"
rm -f "$BUNDLE/Contents/MacOS/"*
cp "$REAL_BIN" "$BUNDLE/Contents/MacOS/butler-desktop"
plutil -replace CFBundleExecutable -string butler-desktop "$BUNDLE/Contents/Info.plist"

# Exact, and before signing, which rewrites the bytes. If this ever fails the
# bundle is not Butler and must not be installed.
cmp -s "$REAL_BIN" "$BUNDLE/Contents/MacOS/butler-desktop" || {
  echo "local_app.sh: the bundle's executable is not the app binary." >&2
  exit 7
}

# --- section 3.2 step 4: ad-hoc sign ------------------------------------------
# Nested code first, then the outer bundle. `--deep` would do this in an order
# codesign itself warns against, and it is deprecated (D-2).
#
# This re-signs unconditionally rather than trusting what the bundler left
# behind. An operator whose shell already exports a real signing identity would
# otherwise get a bundle signed with it, and FR-003 requires the artifact of
# this path to stay visibly ad-hoc so it can never be mistaken for a release
# build (D-1).
echo "==> signing ad-hoc"
find "$BUNDLE/Contents" \( -name '*.dylib' -o -name '*.so' -o -name '*.framework' \) -print0 2>/dev/null \
  | xargs -0 -I{} codesign --force --sign - --timestamp=none {} 2>/dev/null || true
codesign --force --sign - --timestamp=none "$BUNDLE"

# --- section 3.2 step 5: verify ------------------------------------------------
# A bundle that cannot launch fails here rather than silently in the Dock.
echo "==> verifying"
codesign --verify --strict "$BUNDLE"
if ! codesign -dv "$BUNDLE" 2>&1 | grep -q 'Signature=adhoc'; then
  echo "local_app.sh: the bundle is not ad-hoc signed. Refusing to install," >&2
  echo "because FR-003 requires a local build to be inspectably local." >&2
  exit 5
fi

# --- section 3.2 step 6: install ----------------------------------------------
echo "==> installing to $DEST/Butler.app"
mkdir -p "$DEST"
rm -rf "$DEST/Butler.app"
cp -R "$BUNDLE" "$DEST/Butler.app"

# --- section 3.2 step 7: report ------------------------------------------------
cat <<REPORT

Installed: $DEST/Butler.app
Signature: ad-hoc. It attests nothing about origin, and that is correct for a
           build made on the machine running it. See docs/threat-model.md for
           what a real signature would defend against.

Next, once, in System Settings > Privacy & Security > Screen Recording:
  add Butler and enable it. The app asks once per launch and then stops.

macOS re-asks after every rebuild. An ad-hoc signature pins the bundle's
cdhash, which changes on each build, so the system sees a new application and
the old entry stays in the list. docs/local-install.md explains the one way to
avoid it.
REPORT
