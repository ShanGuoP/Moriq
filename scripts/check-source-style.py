#!/usr/bin/env python3
"""Reject packed Rust statements that rustfmt may leave unchanged.

String literals and comments do not count towards the 180-column code budget.
The gate intentionally ignores vendor sources and generated build artifacts.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
TOKEN = re.compile(
    r'//[^\n]*|/\*.*?\*/|(?:br|r)(?P<hashes>\#*)".*?"(?P=hashes)'
    r'|"(?:[^"\\]|\\.)*"|\'(?:[^\'\\]|\\.)\'',
    re.DOTALL,
)


def code_lines(source):
    def elide(match):
        token = match.group(0)
        marker = "" if token.startswith(("//", "/*")) else '"literal"'
        return marker + "\n" * token.count("\n")
    return TOKEN.sub(elide, source).splitlines()


def main():
    failures = []
    paths = sorted((ROOT / "crates").rglob("*.rs")) + sorted((ROOT / "apps").rglob("*.rs"))
    for path in paths:
        if "target" in path.relative_to(ROOT).parts:
            continue
        for line, code in enumerate(code_lines(path.read_text(encoding="utf-8")), 1):
            if len(code.rstrip()) > 180:
                failures.append(f"{path.relative_to(ROOT)}:{line}: {len(code.rstrip())} code columns")
    if failures:
        print("Packed Rust statements (split expressions before running rustfmt):")
        print("\n".join(failures))
        return 1
    print(f"Source style passed ({len(paths)} Rust files).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
