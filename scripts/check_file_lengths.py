#!/usr/bin/env python3
"""Enforce the repository's physical line limit for hand-maintained code."""

from __future__ import annotations

import re
import os
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "docs" / "plan" / "file-length-policy.md"
MAX_LINES = 500

# Keep this list focused on source and executable configuration. Data files and
# generated artifacts are outside this check; see the policy document.
SOURCE_SUFFIXES = {
    ".c", ".cc", ".cpp", ".cxx", ".cs", ".css", ".fs", ".fsx", ".go",
    ".h", ".hh", ".hpp", ".htm", ".html", ".java", ".js", ".jsx", ".kt",
    ".kts", ".lua", ".m", ".mm", ".php", ".pl", ".ps1", ".py", ".rb",
    ".rs", ".scala", ".sc", ".scss", ".sh", ".sql", ".svelte", ".swift",
    ".tf", ".ts", ".tsx", ".vue", ".wgsl", ".xml", ".yaml", ".yml",
}

# Build output and local runtime/cache directories are excluded wherever they
# occur. Ordinary source directories are traversed, including untracked files.
EXCLUDED_DIRS = {
    ".git", ".hg", ".svn", ".venv", "__pycache__", ".pytest_cache",
    ".mypy_cache", ".ruff_cache", ".tox", ".cache", ".local", ".runtime",
    "target", "node_modules", "vendor", "dist", "build", "coverage",
    "runtime", "local-artifacts",
}

EXCEPTION_RE = re.compile(
    r"^<!-- file-length-exception: ([^|]+?)\s*\|\s*(.+?)\s*-->$"
)


def source_files() -> list[Path]:
    """Enumerate source files from disk, so tracked and untracked files count."""
    paths: list[Path] = []
    for directory, subdirectories, filenames in os.walk(ROOT):
        subdirectories[:] = [name for name in subdirectories if name.lower() not in EXCLUDED_DIRS]
        for name in filenames:
            path = Path(directory) / name
            if path.suffix.lower() not in SOURCE_SUFFIXES:
                continue
            # Generated source files are excluded by explicit conventional names.
            if name.lower().endswith((".generated.rs", ".g.rs", ".designer.cs")):
                continue
            paths.append(path)
    return sorted(paths, key=lambda item: item.relative_to(ROOT).as_posix().casefold())


def physical_line_count(path: Path) -> int:
    content = path.read_bytes()
    return content.count(b"\n") + int(bool(content) and not content.endswith(b"\n"))


def registered_exceptions() -> tuple[dict[str, str], list[str]]:
    entries: dict[str, str] = {}
    errors: list[str] = []
    if not POLICY.is_file():
        return entries, [f"missing policy document: {POLICY.relative_to(ROOT)}"]

    for line_number, line in enumerate(POLICY.read_text(encoding="utf-8").splitlines(), 1):
        stripped = line.strip()
        if "file-length-exception:" not in line or not stripped.startswith("<!--"):
            continue
        match = EXCEPTION_RE.fullmatch(stripped)
        if not match:
            errors.append(
                f"{POLICY.relative_to(ROOT).as_posix()}:{line_number}: malformed exception; "
                "use <!-- file-length-exception: path | reason -->"
            )
            continue
        name, reason = match.groups()
        name = name.strip().replace("\\", "/")
        if not reason.strip():
            errors.append(f"{POLICY.relative_to(ROOT).as_posix()}:{line_number}: exception needs a reason")
        elif name in entries:
            errors.append(f"{POLICY.relative_to(ROOT).as_posix()}:{line_number}: duplicate exception for {name}")
        else:
            entries[name] = reason.strip()
    return entries, errors


def main() -> int:
    actual: dict[str, int] = {}
    errors: list[str] = []
    files = source_files()
    included = {path.relative_to(ROOT).as_posix() for path in files}
    for path in files:
        relative = path.relative_to(ROOT).as_posix()
        try:
            count = physical_line_count(path)
        except OSError as exc:
            errors.append(f"{relative}: cannot read file: {exc}")
            continue
        if count > MAX_LINES:
            actual[relative] = count

    exceptions, registry_errors = registered_exceptions()
    errors.extend(registry_errors)
    for relative, count in sorted(actual.items()):
        if relative not in exceptions:
            errors.append(f"{relative}: {count} physical lines (limit {MAX_LINES}); split this file or document a scoped exception")
    for relative in sorted(exceptions):
        if relative not in included:
            errors.append(f"invalid exception for {relative}: path is not an included source file")
        elif relative not in actual:
            errors.append(f"stale exception for {relative}: file is missing or no longer exceeds {MAX_LINES} lines")

    if errors:
        print("File length check failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    print(f"File length check passed: {len(files)} source files, maximum {MAX_LINES} physical lines.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
