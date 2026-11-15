#!/usr/bin/env python3
"""Check the repository-local AGENTS/Skills structure without running tests."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REQUIRED_AGENTS = (
    "AGENTS.md",
    "crates/AGENTS.md",
    "crates/client-runtime/AGENTS.md",
    "crates/server-runtime/AGENTS.md",
    "crates/renderer/AGENTS.md",
    "apps/client/AGENTS.md",
    "games/sample/AGENTS.md",
)
SKILLS = (
    "minimium-boundary-review",
    "minimium-safe-refactor",
    "minimium-new-system",
    "minimium-verify",
    "minimium-world-consistency",
    "minimium-rendering",
    "minimium-command-surface",
)
SECTIONS = (
    "## When to use",
    "## When NOT to use",
    "## Workflow",
    "## Example",
    "## Completion and evidence",
)
NAME = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*\Z")
MARKDOWN_LINK = re.compile(r"\[[^\]\n]+\]\(([^)\n]+)\)")
EXTERNAL = re.compile(r"^[a-z][a-z0-9+.-]*:", re.IGNORECASE)


def read(relative: str, errors: list[str]) -> str | None:
    path = ROOT / relative
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        errors.append(f"{relative}: missing or unreadable ({exc})")
        return None


def metadata(relative: str, source: str, errors: list[str]) -> str | None:
    lines = source.splitlines()
    if not lines or lines[0].strip() != "---":
        errors.append(f"{relative}: missing first frontmatter ---")
        return None
    try:
        end = lines.index("---", 1)
    except ValueError:
        errors.append(f"{relative}: missing closing frontmatter ---")
        return None
    if end > 30:
        errors.append(f"{relative}: frontmatter too long (max 30 lines)")
        return None
    parsed: dict[str, str] = {}
    for number, line in enumerate(lines[1:end], 2):
        if not line.strip():
            continue
        if ":" not in line:
            errors.append(f"{relative}:{number}: expected simple key: value")
            continue
        key, value = line.split(":", 1)
        key, value = key.strip(), value.strip()
        if not key or key in parsed:
            errors.append(f"{relative}:{number}: duplicate/empty metadata key")
        else:
            parsed[key] = value.strip("'\"")
    name, description = parsed.get("name", ""), parsed.get("description", "")
    if not name or not NAME.fullmatch(name):
        errors.append(f"{relative}: invalid lowercase kebab-case name: {name!r}")
    if not description or len(description) > 1024:
        errors.append(f"{relative}: description must contain 1..1024 chars")
    if "\n" in description or description in {"TODO", "A skill"}:
        errors.append(f"{relative}: description is not an actionable discovery trigger")
    return name or None


def local_links(relative: str, source: str, errors: list[str]) -> int:
    count = 0
    for matched in MARKDOWN_LINK.finditer(source):
        raw = matched.group(1).strip().split(" ", 1)[0].strip("<>")
        if not raw or raw.startswith("#") or EXTERNAL.match(raw):
            continue
        target = raw.split("#", 1)[0].split("?", 1)[0]
        if not target:
            continue
        destination = (ROOT / relative).parent.joinpath(target).resolve()
        if not destination.is_relative_to(ROOT):
            errors.append(f"{relative}: local link escapes repository: {raw}")
        elif not destination.exists():
            errors.append(f"{relative}: broken local link: {raw}")
        else:
            count += 1
    return count


def main() -> int:
    errors: list[str] = []
    for path in REQUIRED_AGENTS:
        source = read(path, errors)
        if source is not None and not source.lstrip().startswith("# "):
            errors.append(f"{path}: missing Markdown title")
    root = read("AGENTS.md", [])
    if root is not None:
        for section in ("## Tests", "## File size", "## Domain ownership"):
            if section not in root:
                errors.append(f"AGENTS.md: missing preserved policy heading {section}")

    base = ROOT / ".agents" / "skills"
    if not base.is_dir():
        errors.append(".agents/skills: missing skills directory")
        found: set[str] = set()
    else:
        found = {path.name for path in base.iterdir() if path.is_dir()}
    required = set(SKILLS)
    for missing in sorted(required - found):
        errors.append(f".agents/skills/{missing}: missing skill directory")
    for unexpected in sorted(found - required):
        errors.append(f".agents/skills/{unexpected}: undeclared skill; update routing/checker before adding")

    seen: set[str] = set()
    links = 0
    for skill in SKILLS:
        relative = f".agents/skills/{skill}/SKILL.md"
        source = read(relative, errors)
        if source is None:
            continue
        name = metadata(relative, source, errors)
        if name != skill:
            errors.append(f"{relative}: name must equal directory {skill!r}")
        if name in seen:
            errors.append(f"{relative}: duplicate skill name {name!r}")
        if name is not None:
            seen.add(name)
        for section in SECTIONS:
            if section not in source.splitlines():
                errors.append(f"{relative}: required heading missing: {section}")
        links += local_links(relative, source, errors)

    workflow = read("docs/agent-workflow.md", errors)
    if workflow is not None:
        for skill in SKILLS:
            if f".agents/skills/{skill}/SKILL.md" not in workflow:
                errors.append(f"docs/agent-workflow.md: missing skill route {skill}")
        for agent in REQUIRED_AGENTS:
            if agent not in workflow:
                errors.append(f"docs/agent-workflow.md: missing AGENTS link {agent}")
        links += local_links("docs/agent-workflow.md", workflow, errors)

    adr = read("docs/adr/0014-agent-engineering-governance.md", errors)
    if adr is not None and not adr.startswith("# ADR 0014:"):
        errors.append("docs/adr/0014-agent-engineering-governance.md: incorrect ADR title")
    architecture = read("docs/architecture.md", errors)
    if architecture is not None and "agent-workflow.md" not in architecture:
        errors.append("docs/architecture.md: missing agent workflow reference")

    if errors:
        print("Agent contract check FAILED:", file=sys.stderr)
        for issue in errors:
            print(f"  - {issue}", file=sys.stderr)
        return 1
    print(
        f"Agent contract check PASSED: {len(REQUIRED_AGENTS)} AGENTS files, "
        f"{len(SKILLS)} named Skills, {links} local Markdown links verified."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
