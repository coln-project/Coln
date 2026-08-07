#!/usr/bin/env python3
"""Convert integer columns in delimited facts files to Coln entity ids.

Output values match the REPL parser format ``#<commit>:<counter>`` where
``commit`` is a hex-encoded hash (up to 32 bytes, left-padded with zeros when
parsed) and ``counter`` is a decimal u32.

Example::

    python scripts/facts_to_entity_ids.py data/expr_app.csv \\
        --header --columns id,left,right \\
        -o data/expr_app.entities.csv
"""

from __future__ import annotations

import argparse
import sys
from typing import TextIO

# Matches `HASH_SIZE` in `src/commit/hash.rs`.
HASH_SIZE = 32
HASH_HEX_LEN = HASH_SIZE * 2
U32_MAX = 0xFFFF_FFFF
DEFAULT_COMMIT_HASH = "01"


def parse_columns(
    raw: str,
    *,
    one_based: bool,
    header: list[str] | None = None,
) -> set[int]:
    name_to_index = (
        {name.strip(): index for index, name in enumerate(header)}
        if header is not None
        else {}
    )
    columns: set[int] = set()
    for part in raw.split(","):
        part = part.strip()
        if not part:
            continue
        try:
            index = int(part, 10)
        except ValueError:
            if header is None:
                raise ValueError(
                    f"column name {part!r} requires --header"
                ) from None
            try:
                index = name_to_index[part]
            except KeyError as exc:
                known = ", ".join(name_to_index)
                raise ValueError(
                    f"unknown column name {part!r}; known columns: {known}"
                ) from exc
        else:
            if one_based:
                index -= 1
            if index < 0:
                raise ValueError(
                    f"column index must be non-negative, got {part!r}"
                )
        columns.add(index)
    if not columns:
        raise ValueError("at least one column index is required")
    return columns


def normalize_commit_hash(hex_str: str) -> str:
    normalized = hex_str.strip().lower()
    if normalized.startswith("0x"):
        normalized = normalized[2:]
    if not normalized:
        raise ValueError("commit hash must not be empty")
    if len(normalized) > HASH_HEX_LEN:
        raise ValueError(
            f"commit hash must be at most {HASH_HEX_LEN} hex characters "
            f"({HASH_SIZE} bytes), got {len(normalized)}"
        )
    try:
        raw = bytes.fromhex(normalized)
    except ValueError as exc:
        raise ValueError(f"invalid commit hash hex: {hex_str!r}") from exc
    if len(raw) > HASH_SIZE:
        raise ValueError(
            f"commit hash must decode to at most {HASH_SIZE} bytes, "
            f"got {len(raw)}"
        )
    return normalized


def to_entity_id(commit_hex: str, counter: int) -> str:
    if counter < 0 or counter > U32_MAX:
        raise ValueError(f"counter {counter} is outside u32 range (0..{U32_MAX})")
    return f"#{commit_hex}:{counter}"


def convert_line(
    line: str,
    *,
    columns: set[int],
    commit_hex: str,
    input_delimiter: str,
    output_delimiter: str,
) -> str:
    line = line.rstrip("\n\r")
    if not line:
        return line

    fields = line.split(input_delimiter)
    out: list[str] = []
    for index, field in enumerate(fields):
        if index not in columns:
            out.append(field)
            continue

        stripped = field.strip()
        if not stripped:
            raise ValueError(f"column {index} is empty")
        try:
            counter = int(stripped, 10)
        except ValueError as exc:
            raise ValueError(
                f"column {index} value {field!r} is not a decimal integer"
            ) from exc
        out.append(to_entity_id(commit_hex, counter))

    return output_delimiter.join(out)


def rejoin_fields(
    line: str,
    *,
    input_delimiter: str,
    output_delimiter: str,
) -> str:
    line = line.rstrip("\n\r")
    if not line:
        return line
    return output_delimiter.join(line.split(input_delimiter))


def convert_stream(
    input_stream: TextIO,
    output_stream: TextIO,
    *,
    columns: set[int],
    commit_hex: str,
    input_delimiter: str,
    output_delimiter: str,
    header_line: str | None = None,
) -> None:
    if header_line is not None:
        converted_header = rejoin_fields(
            header_line,
            input_delimiter=input_delimiter,
            output_delimiter=output_delimiter,
        )
        output_stream.write(converted_header)
        if not converted_header.endswith("\n"):
            output_stream.write("\n")

    start_line = 2 if header_line is not None else 1
    for line_number, line in enumerate(input_stream, start=start_line):
        try:
            converted = convert_line(
                line,
                columns=columns,
                commit_hex=commit_hex,
                input_delimiter=input_delimiter,
                output_delimiter=output_delimiter,
            )
        except ValueError as exc:
            raise ValueError(f"line {line_number}: {exc}") from exc
        output_stream.write(converted)
        if not converted.endswith("\n"):
            output_stream.write("\n")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Prefix selected tab-separated columns with a dummy commit hash, "
            "producing Coln entity ids (#<commit>:<counter>)."
        )
    )
    parser.add_argument(
        "input",
        nargs="?",
        type=argparse.FileType("r", encoding="utf-8"),
        default=sys.stdin,
        help="input facts file (default: stdin)",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=argparse.FileType("w", encoding="utf-8"),
        default=sys.stdout,
        help="output file (default: stdout)",
    )
    parser.add_argument(
        "-c",
        "--columns",
        required=True,
        help=(
            "comma-separated column indices or names to convert "
            "(names require --header; see --one-based)"
        ),
    )
    parser.add_argument(
        "--header",
        action="store_true",
        help="treat the first input line as a header row (passed through unchanged)",
    )
    parser.add_argument(
        "--one-based",
        action="store_true",
        help="treat column numbers as 1-based (default: 0-based)",
    )
    parser.add_argument(
        "--commit-hash",
        default=DEFAULT_COMMIT_HASH,
        metavar="HEX",
        help=(
            f"commit hash as hex, up to {HASH_SIZE} bytes "
            f"({HASH_HEX_LEN} hex digits; default: 01)"
        ),
    )
    parser.add_argument(
        "-d",
        "--delimiter",
        default="\t",
        help="input field delimiter (default: tab)",
    )
    parser.add_argument(
        "--output-delimiter",
        default=",",
        help="output field delimiter (default: comma)",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        header_line: str | None = None
        header_fields: list[str] | None = None
        if args.header:
            header_line = args.input.readline()
            if header_line:
                header_fields = header_line.rstrip("\n\r").split(args.delimiter)

        columns = parse_columns(
            args.columns,
            one_based=args.one_based,
            header=header_fields,
        )
        commit_hex = normalize_commit_hash(args.commit_hash)
        convert_stream(
            args.input,
            args.output,
            columns=columns,
            commit_hex=commit_hex,
            input_delimiter=args.delimiter,
            output_delimiter=args.output_delimiter,
            header_line=header_line if args.header and header_line else None,
        )
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    finally:
        if args.input is not sys.stdin:
            args.input.close()
        if args.output is not sys.stdout:
            args.output.close()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
