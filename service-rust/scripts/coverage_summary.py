#!/usr/bin/env python3
"""Summarize LLVM production line coverage without counting test source lines.

LLVM also counts inline unit-test modules in src/*.rs. Rustfmt places their
top-level closing brace at column zero, allowing us to exclude just those
modules while retaining any production code before or after them. The original
LCOV/JSON/HTML reports remain available for auditing and detailed inspection.
"""

import json
from pathlib import Path
import sys


def inline_test_lines(source):
    lines = source.read_text().splitlines()
    excluded = set()
    for index, line in enumerate(lines):
        if line != "#[cfg(test)]":
            continue
        opening = index + 1
        while opening < len(lines) and lines[opening].startswith("#["):
            opening += 1
        if opening >= len(lines) or lines[opening] != "mod tests {":
            continue
        closing = next(
            (end for end in range(opening + 1, len(lines)) if lines[end] == "}"),
            None,
        )
        if closing is None:
            raise ValueError(f"Unclosed inline test module in {source}")
        excluded.update(range(index + 1, closing + 2))
    return excluded


def summarize(lcov):
    root = Path(__file__).resolve().parents[1]
    files = []
    source = None
    counts = {}
    for line in lcov.read_text().splitlines():
        if line.startswith("SF:"):
            source = Path(line[3:]).resolve()
            counts = {}
        elif line.startswith("DA:"):
            number, count, *_ = line[3:].split(",")
            counts[int(number)] = int(count)
        elif line == "end_of_record" and source is not None:
            if not source.is_relative_to(root / "src"):
                continue
            excluded = inline_test_lines(source)
            production = {number: count for number, count in counts.items() if number not in excluded}
            covered = sum(count > 0 for count in production.values())
            files.append({
                "file": str(source.relative_to(root)),
                "covered": covered,
                "lines": len(production),
                "percent": round(100 * covered / len(production), 2) if production else 100.0,
                "uncoveredLines": [number for number, count in production.items() if count == 0],
            })
    if not files:
        raise ValueError("No production Rust source files were found in the LCOV report")
    covered = sum(file["covered"] for file in files)
    total = sum(file["lines"] for file in files)
    return {
        "scope": "src/*.rs executable lines; excludes integration tests and inline unit-test modules",
        "covered": covered,
        "lines": total,
        "percent": round(100 * covered / total, 2),
        "files": sorted(files, key=lambda file: file["file"]),
    }


def main():
    if len(sys.argv) != 3:
        raise SystemExit("Usage: coverage_summary.py <lcov.info> <summary.json>")
    summary = summarize(Path(sys.argv[1]))
    Path(sys.argv[2]).write_text(json.dumps(summary, indent=2) + "\n")
    for file in summary["files"]:
        print(f'{file["file"]:28} {file["covered"]:4}/{file["lines"]:<4} {file["percent"]:6.2f}%')
    print(f'Production Rust lines: {summary["covered"]}/{summary["lines"]} ({summary["percent"]:.2f}%)')


if __name__ == "__main__":
    main()
