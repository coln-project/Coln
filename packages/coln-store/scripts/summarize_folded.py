#!/usr/bin/env python3
"""Summarize a folded stack profile into a compact, AI-readable report.

Raw ``profile.folded`` files are usually too large and noisy to paste into a
model. This script emits:

1. Totals and unique-stack counts
2. Top functions by self time (leaf samples)
3. Top functions by total time (any frame on the stack)
4. Crate / module rollup
5. Top abbreviated call paths (interesting frames only)

Example::

    python scripts/summarize_folded.py profile.folded -o profile.summary.txt
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from pathlib import Path

_LLVM_SUFFIX_RE = re.compile(r"\.llvm\.\d+$")
_GENERIC_RE = re.compile(r"<[^<>]*>")
_MANGLED_RE = re.compile(r"^_Z")
_PATH_RE = re.compile(r"^/")
_MAX_FRAME_LEN = 100

# Keep these frames when abbreviating stacks; drop hashbrown / core noise.
_INTERESTING_PREFIXES = (
    "coln_store::",
    "hexane::",
    "copy_from_csv",
    "execute_sql",
    "insert_packed",
    "apply_commit",
    "apply_staged",
    "HashMapper",
    "TableIndex",
    "IdColumn",
    "decode_txn",
)


def parse_folded_line(line: str) -> tuple[list[str], int] | None:
    line = line.strip()
    if not line or line.startswith("#"):
        return None
    # Count is the last whitespace-separated field; stack may contain spaces
    # only inside demangled names rarely — FlameGraph form is "stack count".
    try:
        stack_s, count_s = line.rsplit(None, 1)
        count = int(count_s)
    except ValueError:
        return None
    frames = [f for f in stack_s.split(";") if f]
    if not frames or count <= 0:
        return None
    return frames, count


def _simplify_trait_impl(frame: str) -> str | None:
    """Rewrite ``<Type as Trait>::method`` into a short stable name."""
    if not frame.startswith("<") or " as " not in frame:
        return None
    end = frame.find(">::")
    if end < 0:
        return None
    inner = frame[1:end]
    method = frame[end + 3 :]
    if " as " not in inner:
        return None
    typ, trait = inner.rsplit(" as ", 1)
    method_base = method.split("<", 1)[0]
    for candidate in (typ, trait):
        if "coln_store::" in candidate or candidate.startswith("coln_store"):
            return f"{candidate.split('<', 1)[0]}::{method_base}"
        if "hexane::" in candidate or candidate.startswith("hexane"):
            return f"{candidate.split('<', 1)[0]}::{method_base}"
    typ_short = typ.split("<", 1)[0]
    if "::" in typ_short:
        typ_short = typ_short.rsplit("::", 1)[-1]
    return f"{typ_short}::{method_base}"


def _strip_generics(frame: str) -> str:
    """Strip type arguments but keep path structure (``foo::Bar<>::baz`` → ``foo::Bar::baz``)."""
    prev = None
    while prev != frame:
        prev = frame
        frame = _GENERIC_RE.sub("", frame)
    return frame.replace("<>", "")


def simplify_frame(frame: str, *, strip_generics: bool) -> str:
    frame = _LLVM_SUFFIX_RE.sub("", frame)
    if _PATH_RE.match(frame):
        return frame.rsplit("/", 1)[-1]
    if _MANGLED_RE.match(frame):
        return frame[:48] + "…" if len(frame) > 48 else frame

    impl = _simplify_trait_impl(frame)
    if impl is not None:
        frame = impl
    elif strip_generics and "<" in frame:
        frame = _strip_generics(frame)

    if len(frame) > _MAX_FRAME_LEN:
        frame = frame[: _MAX_FRAME_LEN - 1] + "…"
    return frame


def crate_of(frame: str) -> str:
    if "coln_store::" in frame or frame.startswith("coln_store"):
        return "coln_store"
    if "hexane::" in frame or frame.startswith("hexane"):
        return "hexane"
    if frame.startswith(("std::", "core::", "alloc::")):
        return "std/core/alloc"
    if "hashbrown" in frame:
        return "hashbrown"
    if frame.startswith("_Z") or ".llvm." in frame:
        return "mangled/other"
    if "::" in frame:
        head = frame.split("::", 1)[0]
        if head.isidentifier():
            return head
    return "other"


def is_interesting(frame: str) -> bool:
    if frame.startswith(_INTERESTING_PREFIXES):
        return True
    return any(p in frame for p in _INTERESTING_PREFIXES)


def abbreviate_stack(frames: list[str], *, strip_generics: bool, max_frames: int) -> str:
    interesting = [
        simplify_frame(f, strip_generics=strip_generics)
        for f in frames
        if is_interesting(f)
    ]
    # Deduplicate adjacent repeats after simplification.
    deduped: list[str] = []
    for f in interesting:
        if not deduped or deduped[-1] != f:
            deduped.append(f)
    if len(deduped) > max_frames:
        head = max_frames // 2
        tail = max_frames - head
        deduped = deduped[:head] + ["…"] + deduped[-tail:]
    return " -> ".join(deduped) if deduped else "(no interesting frames)"


def pct(part: int, whole: int) -> str:
    if whole <= 0:
        return "0.0%"
    return f"{100.0 * part / whole:5.1f}%"


def summarize(
    stacks: list[tuple[list[str], int]],
    *,
    top_n: int,
    strip_generics: bool,
    max_path_frames: int,
) -> str:
    total = sum(c for _, c in stacks)
    self_counts: Counter[str] = Counter()
    total_counts: Counter[str] = Counter()
    crate_self: Counter[str] = Counter()
    crate_total: Counter[str] = Counter()
    path_counts: Counter[str] = Counter()

    for frames, count in stacks:
        simplified = [simplify_frame(f, strip_generics=strip_generics) for f in frames]
        leaf = simplified[-1]
        self_counts[leaf] += count
        crate_self[crate_of(frames[-1])] += count

        seen_frames: set[str] = set()
        seen_crates: set[str] = set()
        for raw, simp in zip(frames, simplified):
            # Attribute a sample once per unique frame name on the stack.
            if simp not in seen_frames:
                total_counts[simp] += count
                seen_frames.add(simp)
            # Attribute once per crate touched by the stack.
            cr = crate_of(raw)
            if cr not in seen_crates:
                crate_total[cr] += count
                seen_crates.add(cr)

        path_counts[
            abbreviate_stack(
                frames, strip_generics=strip_generics, max_frames=max_path_frames
            )
        ] += count

    lines: list[str] = []
    lines.append("# Profile Summary")
    lines.append("")
    lines.append(f"total_weight: {total}")
    lines.append(f"unique_stacks: {len(stacks)}")
    lines.append(f"unique_self_frames: {len(self_counts)}")
    lines.append(f"unique_total_frames: {len(total_counts)}")
    lines.append("")

    lines.append("## Self Time (Leaf Frames)")
    lines.append("percent  weight            frame")
    for frame, weight in self_counts.most_common(top_n):
        lines.append(f"{pct(weight, total)}  {weight:<16}  {frame}")
    lines.append("")

    lines.append("## Total Time (Any Frame on Stack)")
    lines.append("percent  weight            frame")
    for frame, weight in total_counts.most_common(top_n):
        lines.append(f"{pct(weight, total)}  {weight:<16}  {frame}")
    lines.append("")

    lines.append("## Crate Self Time")
    lines.append("percent  weight            crate")
    for crate, weight in crate_self.most_common():
        lines.append(f"{pct(weight, total)}  {weight:<16}  {crate}")
    lines.append("")

    lines.append("## Crate Total Time (Stack Touches Crate)")
    lines.append("percent  weight            crate")
    for crate, weight in crate_total.most_common():
        lines.append(f"{pct(weight, total)}  {weight:<16}  {crate}")
    lines.append("")

    lines.append("## Top Abbreviated Paths")
    lines.append("Interesting frames only (coln_store / hexane / key entrypoints).")
    lines.append("percent  weight            path")
    for path, weight in path_counts.most_common(top_n):
        lines.append(f"{pct(weight, total)}  {weight:<16}  {path}")
    lines.append("")

    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "folded",
        nargs="?",
        type=Path,
        default=Path("profile.folded"),
        help="folded stacks input (default: profile.folded)",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=None,
        help="write report here (default: stdout)",
    )
    parser.add_argument(
        "-n",
        "--top",
        type=int,
        default=30,
        help="rows per top-N section (default: 30)",
    )
    parser.add_argument(
        "--keep-generics",
        action="store_true",
        help="do not strip <...> from frame names",
    )
    parser.add_argument(
        "--max-path-frames",
        type=int,
        default=12,
        help="max frames kept in abbreviated paths (default: 12)",
    )
    args = parser.parse_args(argv)

    text = args.folded.read_text(encoding="utf-8", errors="replace")
    stacks: list[tuple[list[str], int]] = []
    for line in text.splitlines():
        parsed = parse_folded_line(line)
        if parsed is not None:
            stacks.append(parsed)
    if not stacks:
        print(f"no stacks parsed from {args.folded}", file=sys.stderr)
        return 1

    report = summarize(
        stacks,
        top_n=args.top,
        strip_generics=not args.keep_generics,
        max_path_frames=args.max_path_frames,
    )
    if args.output is None:
        sys.stdout.write(report)
    else:
        args.output.write_text(report, encoding="utf-8")
        print(f"wrote {args.output}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
