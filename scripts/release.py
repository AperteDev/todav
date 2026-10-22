#!/usr/bin/env python3
"""Prepare a release: set the version in Cargo.toml and Cargo.lock, the app's metainfo and the
extension's metadata.json, move the Unreleased changelog section under the new version, and print
its notes for the GitHub release. The Release workflow runs it.

    scripts/release.py 0.2.0 > notes.md
"""

import datetime
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    sys.exit(1)


def edit(path: str, pattern: str, replacement: str) -> None:
    """Replace the first match of `pattern` in place, so the file keeps its formatting."""
    file = ROOT / path
    text = file.read_text()
    new, count = re.subn(pattern, replacement, text, count=1, flags=re.M | re.S)
    if count == 0:
        fail(f"{path}: nothing matches {pattern!r}")
    file.write_text(new)


def main() -> None:
    version = sys.argv[1] if len(sys.argv) > 1 else ""
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        fail(f'Expected a version like 1.2.3, got "{version}"')
    date = datetime.date.today().isoformat()

    # Keep a Changelog: `## [Unreleased]` and `## [x.y.z] - date` sections, links at the end.
    changelog = ROOT / "CHANGELOG.md"
    text = changelog.read_text()
    if f"\n## [{version}]" in text:
        fail(f"CHANGELOG.md already has a {version} section")
    heading = "## [Unreleased]\n"
    start = text.find(heading)
    if start == -1:
        fail("CHANGELOG.md has no [Unreleased] section")
    body_start = start + len(heading)
    # The next version's section, else the link definitions, end it.
    ends = [i for i in (text.find("\n## [", body_start), text.find("\n[Unreleased]:")) if i != -1]
    body_end = min(ends) + 1 if ends else len(text)
    notes = text[body_start:body_end].strip()
    if not notes:
        fail("The Unreleased section of CHANGELOG.md is empty")
    # [Unreleased] compares the last version with HEAD, and that comparison becomes the new
    # version's link; before the first release it points at the commits, and the first version
    # links to its tag.
    link = re.compile(r"^\[Unreleased\]: (\S+?)/(?:compare/(\S+)\.\.\.HEAD|commits/HEAD)$", re.M)
    match = link.search(text)
    if not match:
        fail("CHANGELOG.md has no [Unreleased] link")
    repo, previous = match[1], match[2]
    this = (
        f"{repo}/compare/{previous}...v{version}" if previous else f"{repo}/releases/tag/v{version}"
    )
    links = f"[Unreleased]: {repo}/compare/v{version}...HEAD\n[{version}]: {this}"
    rest = link.sub(lambda _: links, text[body_end:], count=1)
    changelog.write_text(f"{text[:body_start]}\n## [{version}] - {date}\n\n{notes}\n\n{rest}")

    edit("Cargo.toml", r'(\[workspace\.package\].*?^version = )"[^"]*"', rf'\g<1>"{version}"')
    # The workspace's own crates, the packages without a `source`, so `--locked` builds still work.
    lock = ROOT / "Cargo.lock"
    lock.write_text(
        re.sub(
            r'^(name = "[^"]+"\nversion = )"[^"]*"$(?!\nsource)',
            rf'\g<1>"{version}"',
            lock.read_text(),
            flags=re.M,
        )
    )
    edit(
        "todav/gnome-extension/metadata.json",
        r'("version-name": )"[^"]*"',
        rf'\g<1>"{version}"',
    )
    # The newest release first; one already listed (the first, written by hand) gets today's date.
    metainfo = "todav/linux/data/io.github.selfref.Todav.metainfo.xml"
    if f'<release version="{version}"' in (ROOT / metainfo).read_text():
        edit(
            metainfo,
            rf'(<release version="{re.escape(version)}" date=)"[^"]*"',
            rf'\g<1>"{date}"',
        )
    else:
        edit(
            metainfo,
            r"^(\s*)<releases>\n",
            rf'\g<0>\g<1>  <release version="{version}" date="{date}"/>\n',
        )

    print(notes)
    print(f"Prepared release {version}", file=sys.stderr)


main()
