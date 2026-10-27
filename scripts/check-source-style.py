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


# 隐藏字符串和注释内容并保留行号，返回用于代码密度检查的逻辑行。
def code_lines(source):
    # 把一个字面量或注释替换为占位内容，保留其换行数以准确定位违规行。
    def elide(match):
        token = match.group(0)
        marker = "" if token.startswith(("//", "/*")) else '"literal"'
        return marker + "\n" * token.count("\n")
    return TOKEN.sub(elide, source).splitlines()


# 遍历自有 Rust 源码检查可执行代码行宽，输出违规位置并返回检查状态。
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
