#!/usr/bin/env python3
"""Extract one version's section from CHANGELOG.md for a GitHub release.

Usage: python3 tools/release-notes.py v1.0.0
Prints the body of the `## [1.0.0]` section (without the heading),
or the whole file's Unreleased section if the tag is not found.
Exit 1 when neither exists.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: release-notes.py vX.Y.Z", file=sys.stderr)
        return 2
    version = sys.argv[1].lstrip("v")
    text = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    pattern = re.compile(
        r"^## \[" + re.escape(version) + r"\][^\n]*\n(.*?)(?=^## \[|\Z)",
        re.M | re.S,
    )
    m = pattern.search(text)
    if m is None:
        print(f"CHANGELOG.md has no section for [{version}]", file=sys.stderr)
        return 1
    print(m.group(1).strip())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
