#!/usr/bin/env python3
"""Collapse ``perf script`` output into folded stacks.

Reads stdin, writes ``root;callee;...;leaf count`` lines sorted by descending
count. This is the usual input for flamegraph tools and a compact form for AI
profile analysis.

Example::

    perf script -i perf.data --demangle | python scripts/fold_perf_script.py \\
        > profile.folded
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter

# Leaf-first stack frame from `perf script`, e.g.:
#   55abcd foo::bar+0x10 (/path/to/bin)
#   ffffffff8d82bfd0 [unknown] ([unknown])
_FRAME_RE = re.compile(
    r"^\s+(?:[0-9a-fA-F]+\s+)?"  # optional address
    r"(.+?)"  # symbol
    r"(?:\+0x[0-9a-fA-F]+)?"  # optional offset
    r"(?:\s+\(([^)]*)\))?\s*$"  # optional (dso)
)

# Event header: `comm pid[/tid] timestamp: period event:`
_HEADER_RE = re.compile(
    r"^(\S+(?:\s+\S+)*?)\s+(\d+)(?:/(\d+))?\s+([\d.]+):\s+(\d+)\s+(\S+):\s*$"
)


def clean_symbol(sym: str) -> str:
    sym = sym.strip()
    if sym.startswith("["):
        return sym
    # Drop trailing " (inlined)" that some perf builds append outside the dso.
    if sym.endswith(" (inlined)"):
        sym = sym[: -len(" (inlined)")]
    return sym


def parse_frame(line: str, *, skip_unknown: bool) -> str | None:
    m = _FRAME_RE.match(line)
    if m is None:
        return None
    sym = clean_symbol(m.group(1))
    if skip_unknown and (sym == "[unknown]" or sym.startswith("0x")):
        return None
    return sym


def fold_perf_script(
    lines: list[str],
    *,
    skip_unknown: bool,
) -> Counter[str]:
    counts: Counter[str] = Counter()
    stack: list[str] = []
    weight = 1

    def flush() -> None:
        nonlocal stack, weight
        if stack:
            # perf script is leaf-first; folded stacks are root-first.
            counts[";".join(reversed(stack))] += weight
        stack = []
        weight = 1

    for raw in lines:
        line = raw.rstrip("\n")
        if not line or line.startswith("#"):
            flush()
            continue
        if line[0].isspace():
            frame = parse_frame(line, skip_unknown=skip_unknown)
            if frame is not None:
                stack.append(frame)
            continue

        flush()
        header = _HEADER_RE.match(line)
        if header is not None:
            weight = int(header.group(5))
        else:
            # Still treat as a new sample boundary.
            weight = 1

    flush()
    return counts


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "-o",
        "--output",
        type=argparse.FileType("w"),
        default=sys.stdout,
        help="output path (default: stdout)",
    )
    parser.add_argument(
        "--keep-unknown",
        action="store_true",
        help="keep [unknown] / bare-address frames",
    )
    parser.add_argument(
        "--min-count",
        type=int,
        default=1,
        help="omit stacks with total weight below this (default: 1)",
    )
    args = parser.parse_args(argv)

    counts = fold_perf_script(
        sys.stdin.readlines(),
        skip_unknown=not args.keep_unknown,
    )
    for stack, count in counts.most_common():
        if count < args.min_count:
            break
        print(f"{stack} {count}", file=args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
