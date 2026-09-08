#!/usr/bin/env python3
# Spec: specs/017-release-and-distribution/spec.md

"""Keep the product version in lockstep across the three files that carry it.

Spec 017 section 3: `scripts/bump_version.py <x.y.z>` updates `Cargo.toml`
(`[workspace.package] version`), `apps/desktop/package.json`, and
`tauri.conf.json`; `--check` verifies agreement and runs in `make lint`.

Three files rather than one because three build systems each need to be told,
and none of them can read the others. FR-003 is the guard: a release built
from files that disagree ships a binary whose reported version is not the one
on the tag, which makes every later bug report unattributable.

Stdlib only, and edits are line-targeted rather than parse-and-rewrite. A
`json.dump` round trip would reformat the whole file and a TOML writer would
drop the comments, so a version bump would arrive as a hundred-line diff with
the one real change buried in it.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

CARGO = "Cargo.toml"
PACKAGE_JSON = "apps/desktop/package.json"
TAURI_CONF = "apps/desktop/src-tauri/tauri.conf.json"

#: The manifests that inherit the workspace version, and that carry the
#: intra-workspace path dependencies below. Listed so `--check` can assert the
#: inheritance itself: a crate that goes back to pinning its own number
#: reintroduces exactly the drift this script exists to prevent, one level
#: down where nothing would look for it.
MEMBER_MANIFESTS = (
    "crates/butler-capture/Cargo.toml",
    "crates/butler-core/Cargo.toml",
    "crates/butler-llm/Cargo.toml",
    "crates/butler-ocr/Cargo.toml",
    "apps/desktop/src-tauri/Cargo.toml",
)

SEMVER = re.compile(r"^\d+\.\d+\.\d+$")

#: A dependency line naming a sibling crate by path **and** by version, e.g.
#: `butler-core = { path = "../butler-core", version = "0.1.0" }`.
#:
#: The version cannot be dropped: `cargo deny` reads a path dependency without
#: one as a wildcard requirement and spec 001 FR-004 bans wildcards. It cannot
#: be inherited either, because `version.workspace` is not a thing inside a
#: dependency table. So the bump has to write these too, and a bump that
#: misses one leaves the three files in agreement and the workspace refusing
#: to resolve, which is a worse failure than the one FR-003 describes.
PATH_DEP = re.compile(r'(path = "[^"]+", version = )"([^"]*)"')


def read(path: str) -> str:
    return (REPO / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (REPO / path).write_text(text, encoding="utf-8")


def workspace_version(text: str) -> str | None:
    """The `version` under `[workspace.package]`, parsed rather than grepped."""
    try:
        data = tomllib.loads(text)
    except tomllib.TOMLDecodeError as exc:
        raise SystemExit(f"Cargo.toml does not parse: {exc}") from exc
    version = data.get("workspace", {}).get("package", {}).get("version")
    return version if isinstance(version, str) else None


def json_version(text: str, path: str) -> str | None:
    try:
        data = json.loads(text)
    except json.JSONDecodeError as exc:
        raise SystemExit(f"{path} does not parse: {exc}") from exc
    version = data.get("version")
    return version if isinstance(version, str) else None


def versions() -> dict[str, str | None]:
    """What each of the three files currently claims."""
    return {
        CARGO: workspace_version(read(CARGO)),
        PACKAGE_JSON: json_version(read(PACKAGE_JSON), PACKAGE_JSON),
        TAURI_CONF: json_version(read(TAURI_CONF), TAURI_CONF),
    }


def set_toml_version(text: str, version: str) -> str:
    """Rewrite `version` inside `[workspace.package]`, and only there.

    Scoped to the table because `version = ` appears under every dependency in
    this manifest; a file-wide substitution would rewrite the pins.
    """
    lines = text.split("\n")
    in_table = False
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("["):
            in_table = stripped == "[workspace.package]"
            continue
        if in_table and re.match(r"^version\s*=", stripped):
            lines[i] = f'version = "{version}"'
            return "\n".join(lines)
    raise SystemExit(
        f"{CARGO}: no `version` under [workspace.package]. "
        "Spec 017 section 3 requires the workspace to carry the number."
    )


def set_json_version(text: str, version: str, path: str) -> str:
    """Rewrite the top-level `"version"` line, preserving the file's layout."""
    pattern = re.compile(r'^(\s*"version"\s*:\s*)"[^"]*"', re.MULTILINE)
    replaced, count = pattern.subn(rf'\g<1>"{version}"', text, count=1)
    if count != 1:
        raise SystemExit(f"{path}: no top-level \"version\" field to update")
    return replaced


def check(expect: str | None = None) -> int:
    """FR-003: fail when any of the three files disagrees.

    `expect` additionally requires them to equal a given version. The release
    workflow passes the tag it was triggered by, which closes the gap FR-003
    leaves open: three files can agree perfectly on a number that is not the
    one on the tag, and the release would then ship `0.1.0` binaries under a
    `v0.2.0` tag with matching provenance attestations, which is worse than
    disagreeing, because everything downstream would look consistent.
    """
    found = versions()
    problems = [p for p, v in found.items() if v is None]
    if problems:
        for path in problems:
            print(f"bump_version: {path}: no version field", file=sys.stderr)
        return 1

    distinct = set(found.values())
    if len(distinct) != 1:
        print("bump_version: the three version fields disagree:", file=sys.stderr)
        for path, version in found.items():
            print(f"  {version}  {path}", file=sys.stderr)
        return 1

    version = distinct.pop()
    if expect is not None and version != expect:
        print(
            f"bump_version: the files say {version!r} but {expect!r} was "
            "expected (the tag and the tree disagree)",
            file=sys.stderr,
        )
        return 1

    # The member crates must inherit rather than pin, or a bump leaves five
    # stale numbers behind where nothing checks them. `tomllib` resolves
    # `version.workspace = true` to `{"workspace": True}`, so a plain string
    # is what a re-pinned crate looks like.
    problems = []
    for path in MEMBER_MANIFESTS:
        text = read(path)
        pinned = tomllib.loads(text).get("package", {}).get("version")
        if isinstance(pinned, str):
            problems.append(
                f"  {path}: [package] version = {pinned!r}, "
                "expected `version.workspace = true`"
            )
        for match in PATH_DEP.finditer(text):
            if match.group(2) != version:
                problems.append(
                    f"  {path}: a path dependency requires "
                    f"{match.group(1)!r}, not {version!r}"
                )

    if problems:
        print(
            "bump_version: the workspace version is not the only number:",
            file=sys.stderr,
        )
        print("\n".join(problems), file=sys.stderr)
        return 1

    print(f"bump_version: {version} everywhere")
    return 0


def bump(version: str) -> int:
    if not SEMVER.match(version):
        raise SystemExit(f"bump_version: {version!r} is not x.y.z")

    write(CARGO, set_toml_version(read(CARGO), version))
    write(PACKAGE_JSON, set_json_version(read(PACKAGE_JSON), version, PACKAGE_JSON))
    write(TAURI_CONF, set_json_version(read(TAURI_CONF), version, TAURI_CONF))
    for path in MEMBER_MANIFESTS:
        write(path, PATH_DEP.sub(rf'\g<1>"{version}"', read(path)))
    print(f"bump_version: set {version} in {CARGO}, {PACKAGE_JSON}, {TAURI_CONF}")
    return check()


def self_test() -> int:
    """FR-003 with both controls, against a copy of the real tree.

    A `--check` that passes proves nothing on its own: it would also pass if
    the comparison were `True`. So this bumps a copy, asserts all three files
    moved, then breaks one of them and asserts `--check` reports it. Run
    against a copy because the point is to exercise the real files' shapes,
    not a fixture that happens to match today.
    """
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "repo"
        root.mkdir()
        (root / "scripts").mkdir()
        shutil.copy2(REPO / "scripts" / "bump_version.py", root / "scripts")
        for path in (CARGO, PACKAGE_JSON, TAURI_CONF, *MEMBER_MANIFESTS):
            dest = root / path
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / path, dest)

        script = str(root / "scripts" / "bump_version.py")

        def run(*args: str) -> subprocess.CompletedProcess[str]:
            return subprocess.run(
                [sys.executable, script, *args],
                capture_output=True,
                text=True,
                check=False,
            )

        started = run("--check")
        if started.returncode != 0:
            print(
                "self-test: the tree does not start in agreement:\n"
                + started.stderr,
                file=sys.stderr,
            )
            return 1

        bumped = run("9.8.7")
        if bumped.returncode != 0:
            print("self-test: the bump failed:\n" + bumped.stderr, file=sys.stderr)
            return 1
        for path in (CARGO, PACKAGE_JSON, TAURI_CONF):
            if "9.8.7" not in (root / path).read_text(encoding="utf-8"):
                print(f"self-test: {path} was not bumped", file=sys.stderr)
                return 1
        for path in MEMBER_MANIFESTS:
            text = (root / path).read_text(encoding="utf-8")
            stale = [m.group(2) for m in PATH_DEP.finditer(text)]
            if any(v != "9.8.7" for v in stale):
                print(
                    f"self-test: {path} still requires {stale} from a sibling",
                    file=sys.stderr,
                )
                return 1

        # The tag control: agreement on the wrong number is still wrong.
        if run("--check", "--expect", "9.8.7").returncode != 0:
            print("self-test: --expect rejected the version it was given", file=sys.stderr)
            return 1
        if run("--check", "--expect", "9.9.9").returncode == 0:
            print(
                "self-test: --check passed with a tag the files do not carry",
                file=sys.stderr,
            )
            return 1

        # The negative control: one file left behind must be caught.
        conf = root / TAURI_CONF
        conf.write_text(
            conf.read_text(encoding="utf-8").replace('"9.8.7"', '"9.8.6"', 1),
            encoding="utf-8",
        )
        drifted = run("--check")
        if drifted.returncode == 0:
            print(
                "self-test: --check passed on files that disagree, so FR-003 "
                "is not being enforced",
                file=sys.stderr,
            )
            return 1
        conf.write_text(
            conf.read_text(encoding="utf-8").replace('"9.8.6"', '"9.8.7"', 1),
            encoding="utf-8",
        )

        # The second negative control: a sibling requirement left behind. This
        # is the failure the three-file reading of FR-003 does not cover, and
        # it is worse, because the version fields still agree while `cargo`
        # refuses to resolve the workspace.
        target = root / MEMBER_MANIFESTS[-1]
        text = target.read_text(encoding="utf-8")
        broken, count = PATH_DEP.subn(r'\g<1>"9.8.6"', text, count=1)
        if count != 1:
            print(
                f"self-test: {MEMBER_MANIFESTS[-1]} has no path dependency to "
                "break, so this control proves nothing",
                file=sys.stderr,
            )
            return 1
        target.write_text(broken, encoding="utf-8")
        if run("--check").returncode == 0:
            print(
                "self-test: --check passed on a stale sibling requirement",
                file=sys.stderr,
            )
            return 1

    print("bump_version: self-test passed (bump, and drift is caught)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Set or verify the product version across the three files "
        "that carry it (spec 017 section 3).",
    )
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("version", nargs="?", help="the new version, as x.y.z")
    group.add_argument(
        "--check",
        action="store_true",
        help="verify the three files agree; runs in `make lint` (FR-003)",
    )
    group.add_argument(
        "--self-test",
        action="store_true",
        help="prove --check catches drift, against a copy of the real tree",
    )
    parser.add_argument(
        "--expect",
        metavar="X.Y.Z",
        help="with --check, also require the files to say this version "
        "(the release workflow passes the tag it was triggered by)",
    )
    args = parser.parse_args()

    if args.expect and not args.check:
        parser.error("--expect is only meaningful with --check")

    if args.check:
        return check(args.expect)
    if args.self_test:
        return self_test()
    return bump(args.version)


if __name__ == "__main__":
    raise SystemExit(main())
