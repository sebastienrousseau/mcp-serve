#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Hold every function and file to the complexity ceilings.

The ceilings are those of ~/Code/AGENTS.md section 0: per function,
cyclomatic 10, cognitive 15, Halstead difficulty 30 and 60 lines; per
file, 500 lines. Measured with rust-code-analysis-cli over every Rust
source the crate tracks (src, tests, examples). A function's figures
include the closures nested in it.

There is no baseline: the crate starts at zero offenders and a new one
fails the check.
"""

import json
import pathlib
import subprocess
import sys

FUNCTION = {"cyclomatic": 10, "cognitive": 15, "difficulty": 30, "lines": 60}
FILE_LINES = 500
ROOTS = ["src", "tests", "examples"]


def measure(path):
    out = subprocess.run(
        ["rust-code-analysis-cli", "-m", "-O", "json", "-p", str(path)],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(out)


def figures(space):
    m = space["metrics"]
    return {
        "cyclomatic": m["cyclomatic"]["sum"],
        "cognitive": m["cognitive"]["sum"],
        "difficulty": m["halstead"].get("difficulty") or 0.0,
        "lines": m["loc"]["sloc"],
    }


def functions(space):
    if space["kind"] == "function" and space["name"] != "<anonymous>":
        yield space
    for child in space.get("spaces", []):
        yield from functions(child)


def offences(path):
    unit = measure(path)
    lines = len(path.read_text(encoding="utf-8").splitlines())
    if lines > FILE_LINES:
        yield f"{path}: {lines} lines (ceiling {FILE_LINES})"
    for fn in functions(unit):
        for metric, value in figures(fn).items():
            if value > FUNCTION[metric]:
                yield (
                    f"{path}:{fn['start_line']} {fn['name']}: {metric} "
                    f"{value:.1f} (ceiling {FUNCTION[metric]})"
                )


def main():
    root = pathlib.Path(__file__).resolve().parent.parent
    paths = sorted(p for r in ROOTS for p in (root / r).rglob("*.rs"))
    found = [o for p in paths for o in offences(p.relative_to(root))]
    for line in found:
        print(line)
    print(f"{len(paths)} files checked, {len(found)} over a ceiling")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
