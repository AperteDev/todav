#!/usr/bin/env python3
"""Prepare a release: set the version in Cargo.toml, the app's metainfo and the extension's
metadata.json, move the Unreleased changelog section under the new version, and print its notes
for the GitHub release. The Release workflow runs it; Cargo.lock follows on the next cargo run.

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

    changelog = ROOT / "CHANGELOG.md"
    text = changelog.read_text()
    if f"\n## {version} " in text:
        fail(f"CHANGELOG.md already has a {version} section")
    heading = "## Unreleased\n"
    start = text.find(heading)
    if start == -1:
        fail("CHANGELOG.md has no Unreleased section")
    body_start = start + len(heading)
    nxt = text.find("\n## ", body_start)
    body_end = len(text) if nxt == -1 else nxt + 1
    notes = text[body_start:body_end].strip()
    if not notes:
        fail("The Unreleased section of CHANGELOG.md is empty")
    changelog.write_text(
        f"{text[:body_start]}\n## {version} - {date}\n\n{notes}\n\n{text[body_end:]}"
    )

    edit("Cargo.toml", r'(\[workspace\.package\].*?^version = )"[^"]*"', rf'\g<1>"{version}"')
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
