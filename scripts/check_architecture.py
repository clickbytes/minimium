#!/usr/bin/env python3
"""Check selected Cargo dependency boundaries without running the programs."""

import json
import re
import subprocess
import sys
import tomllib
from collections import deque
from pathlib import Path


def production_graph(metadata: dict) -> dict:
    """Keep normal/build edges, including external and target-specific crates."""
    return {
        node["id"]: [
            (dependency["pkg"], kind["kind"] or "normal")
            for dependency in node["deps"]
            for kind in dependency["dep_kinds"]
            if kind["kind"] in (None, "build")
        ]
        for node in metadata["resolve"]["nodes"]
    }


def forbidden_paths(root: str, targets: set, graph: dict, names: dict) -> list:
    pending = deque([(root, names[root])])
    seen = {root}
    violations = []
    while pending:
        package, path = pending.popleft()
        for dependency, kind in sorted(graph[package]):
            if dependency in seen:
                continue
            seen.add(dependency)
            next_path = f"{path} -[{kind}]-> {names[dependency]}"
            if names[dependency] in targets:
                violations.append(next_path)
            pending.append((dependency, next_path))
    return violations


def source_boundary_violations(repository: Path) -> list[str]:
    """Catch domain FFI/GPU references that Cargo package edges cannot express."""
    violations = []
    roots = (
        repository / "crates" / "core",
        repository / "crates" / "server-runtime",
        repository / "crates" / "client-runtime",
        repository / "apps" / "server",
    )
    forbidden = re.compile(
        r'\b(?:wgpu|winit|ash|glow)::|\bminimium_astronomy::|'
        r'\bunsafe\s+(?:extern\s+"C"|extern\s+"C")|'
        r'^\s*#\s*\[\s*link\s*\(',
        re.MULTILINE,
    )
    for root in roots:
        for source in sorted(root.rglob("*.rs")):
            if "target" in source.parts:
                continue
            text = source.read_text(encoding="utf-8")
            for match in forbidden.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                violations.append(f"{source.relative_to(repository).as_posix()}:{line}: forbidden FFI/GPU/astronomy source reference")
    for source in (repository / "crates/environment/src/weather").rglob("*.rs"):
        if re.search(r"\b(?:wgpu|winit|renderer|camera)::", source.read_text(encoding="utf-8")):
            violations.append(f"{source.relative_to(repository)}: weather model depends on rendering")
    return violations


def sample_content_violations(package_rules: dict, package_metadata: dict) -> list[str]:
    """Engine domains must not take content from the sample game directory."""
    violations = []
    forbidden = re.compile(r"games[/\\]sample|minimium-game-sample")
    for name, rule in package_rules.items():
        if rule.get("role") != "engine-domain":
            continue
        root = Path(package_metadata[name]["manifest_path"]).parent
        for source in sorted(root.rglob("*.rs")):
            text = source.read_text(encoding="utf-8")
            for match in forbidden.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                violations.append(
                    f"{source.relative_to(root).as_posix()}:{line}: engine domain references sample game content ({name})"
                )
    return violations


def filesystem_parser_violations(repository: Path) -> list[str]:
    """Format parsers consume supplied bytes; file access belongs to adapters."""
    violations = []
    forbidden = re.compile(r"std::fs|std::io::Read|File::open|canonicalize\(")
    root = repository / "crates" / "assets" / "src"
    for source in sorted(root.rglob("*.rs")):
        text = source.read_text(encoding="utf-8")
        for match in forbidden.finditer(text):
            line = text.count("\n", 0, match.start()) + 1
            violations.append(
                f"{source.relative_to(repository).as_posix()}:{line}: format parser performs filesystem access"
            )
    return violations


def main() -> int:
    try:
        output = subprocess.check_output(
            ["cargo", "metadata", "--locked", "--all-features", "--format-version", "1"],
            text=True,
        )
        metadata = json.loads(output)
        names = {package["id"]: package["name"] for package in metadata["packages"]}
        workspace = {names[package]: package for package in metadata["workspace_members"]}
        package_metadata = {
            package["name"]: package
            for package in metadata["packages"]
            if package["id"] in metadata["workspace_members"]
        }
        graph = production_graph(metadata)
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        print(f"Cannot inspect Cargo metadata: {error}", file=sys.stderr)
        return 1

    repository = Path(__file__).resolve().parents[1]
    try:
        declaration = tomllib.loads((repository / "architecture.toml").read_text(encoding="utf-8"))
        package_rules = {entry["name"]: entry for entry in declaration["package"]}
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Cannot read architecture.toml: {error}", file=sys.stderr)
        return 1

    required = set(package_rules)
    missing = required - workspace.keys()
    if missing:
        print(f"Missing packages: {', '.join(sorted(missing))}", file=sys.stderr)
        return 1

    missing_readmes = []
    required_sections = {
        "Purpose / Non-goals",
        "Public API",
        "Dependencies",
        "Data flow",
        "Ownership and execution",
        "Invariants and errors",
        "Commands",
        "Change guide",
        "Verification",
    }
    for name, rule in package_rules.items():
        manifest = Path(package_metadata[name]["manifest_path"])
        readme = manifest.parent / "README.md"
        if not readme.is_file():
            missing_readmes.append(f"{name}: missing {readme.relative_to(repository).as_posix()}")
            continue
        headings = {
            line[3:].strip()
            for line in readme.read_text(encoding="utf-8").splitlines()
            if line.startswith("## ")
        }
        for section in sorted(required_sections - headings):
            missing_readmes.append(
                f"{name}: {readme.relative_to(repository).as_posix()} missing '## {section}'"
            )

    tool_readme = repository / "tools/geography/README.md"
    if not tool_readme.is_file():
        missing_readmes.append("tools/geography: missing README.md")
    else:
        headings = {
            line[3:].strip()
            for line in tool_readme.read_text(encoding="utf-8").splitlines()
            if line.startswith("## ")
        }
        for section in sorted(required_sections - headings):
            missing_readmes.append(f"tools/geography: README.md missing '## {section}'")

    forbidden = {
        package: set(rule.get("forbidden_dependencies", []))
        for package, rule in package_rules.items()
    }
    application_packages = {
        name
        for name, rule in package_rules.items()
        if rule.get("role") in ("application", "runtime")
    }
    for package, rule in package_rules.items():
        if rule.get("role") == "engine-domain":
            forbidden.setdefault(package, set()).update(application_packages)
    violations = list(missing_readmes)
    for root, targets in forbidden.items():
        violations.extend(forbidden_paths(workspace[root], targets, graph, names))
    violations.extend(source_boundary_violations(repository))
    violations.extend(sample_content_violations(package_rules, package_metadata))
    violations.extend(filesystem_parser_violations(repository))
    if violations:
        print("Architecture violations:\n" + "\n".join(violations), file=sys.stderr)
        return 1
    print("Architecture dependency check passed (full Cargo graph, all features/targets, "
          "transitive normal/build edges; dev edges excluded).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
