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
    """Include activated optional and target-specific normal/build edges."""
    return {
        node["id"]: [
            (
                dependency["pkg"],
                (kind["kind"] or "normal")
                + (f"@{kind['target']}" if kind.get("target") else ""),
            )
            for dependency in node["deps"]
            for kind in dependency["dep_kinds"]
            if kind["kind"] != "dev"
        ]
        for node in metadata["resolve"]["nodes"]
    }


def development_graph(metadata: dict) -> dict:
    """Keep dev-only edges separate from production boundary checks."""
    return {
        node["id"]: [
            (dependency["pkg"], kind.get("target"))
            for dependency in node["deps"]
            for kind in dependency["dep_kinds"]
            if kind["kind"] == "dev"
        ]
        for node in metadata["resolve"]["nodes"]
    }


def forbidden_paths(root: str, targets: set, graph: dict, names: dict) -> list:
    pending = deque([(root, names[root])])
    seen = {root}
    violations = []
    while pending:
        package, path = pending.popleft()
        for dependency, kind in sorted(graph.get(package, [])):
            if dependency in seen:
                continue
            seen.add(dependency)
            dependency_name = names.get(dependency, dependency)
            next_path = f"{path} -[{kind}]-> {dependency_name}"
            if dependency_name in targets:
                violations.append(next_path)
            pending.append((dependency, next_path))
    return violations


def source_boundary_violations(
    repository: Path, package_rules: dict, package_metadata: dict
) -> list[str]:
    """Regex-scan each declared domain/runtime package for forbidden effects."""
    violations = []
    domain_pattern = re.compile(
        r"\buse\s+std\s*::\s*\{(?:(?!;)[\s\S])*?\b(?:fs|process|net|io)\b|"
        r"\b(?:std::fs|std::process|std::net|std::io::(?:Read|Write)|"
        r"File::open|Tcp(?:Listener|Stream)|UdpSocket|Command::new|"
        r"wgpu::|winit::|ash::|glow::)|"
        r"\bminimium_(?:renderer|transport|asset_io|platform|astronomy)::|"
        r"\bunsafe\s+(?:extern\s+\"C\"|extern\s+\"C\")|"
        r'^\s*#\s*\[\s*link\s*\(',
        re.MULTILINE,
    )
    runtime_pattern = re.compile(
        r"\b(?:wgpu|winit|ash|glow)::|\bminimium_(?:renderer|astronomy)::|"
        r"\bunsafe\s+(?:extern\s+\"C\"|extern\s+\"C\")|"
        r'^\s*#\s*\[\s*link\s*\(',
        re.MULTILINE,
    )
    application_pattern = re.compile(
        r"\b(?:wgpu|winit|ash|glow)::|\bminimium_(?:renderer|astronomy)::|"
        r"\bunsafe\s+(?:extern\s+\"C\"|extern\s+\"C\")|"
        r'^\s*#\s*\[\s*link\s*\(',
        re.MULTILINE,
    )
    for name, rule in package_rules.items():
        role = rule.get("role")
        if role not in ("engine-domain", "runtime") and name != "minimium-server":
            continue
        package = package_metadata.get(name)
        if package is None:
            continue
        root = Path(package["manifest_path"]).parent
        if role == "engine-domain":
            forbidden = domain_pattern
        elif role == "runtime":
            forbidden = runtime_pattern
        else:
            forbidden = application_pattern
        for source in sorted(root.rglob("*.rs")):
            if _is_nonproduction_source(source):
                continue
            text = source.read_text(encoding="utf-8")
            for match in forbidden.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                violations.append(
                    f"{source.relative_to(repository).as_posix()}:{line}: source effect conflicts with {role} ownership"
                )
    return violations


def _is_nonproduction_source(source: Path) -> bool:
    """Exclude build output and Rust test modules from production source scans."""
    return any(part in {"target", ".git", "tests"} for part in source.parts) or (
        source.stem == "tests" or source.stem.endswith("_tests")
    )


def sample_content_violations(package_rules: dict, package_metadata: dict) -> list[str]:
    """Domains and runtimes must not take content from the sample game directory."""
    violations = []
    forbidden = re.compile(r"games[/\\]sample|minimium[_-]game[_-]sample")
    for name, rule in package_rules.items():
        if rule.get("role") not in ("engine-domain", "runtime"):
            continue
        root = Path(package_metadata[name]["manifest_path"]).parent
        for source in sorted(root.rglob("*.rs")):
            if _is_nonproduction_source(source):
                continue
            text = source.read_text(encoding="utf-8")
            for match in forbidden.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                violations.append(
                    f"{source.relative_to(root).as_posix()}:{line}: engine domain references sample game content ({name})"
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
        dev_graph = development_graph(metadata)
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        print(f"Cannot inspect Cargo metadata: {error}", file=sys.stderr)
        return 1

    repository = Path(__file__).resolve().parents[1]
    try:
        declaration = tomllib.loads((repository / "architecture.toml").read_text(encoding="utf-8"))
        declarations = declaration.get("package", [])
        if not isinstance(declarations, list) or any(not isinstance(entry, dict) for entry in declarations):
            print("architecture.toml package declarations must be an array of tables", file=sys.stderr)
            return 1
        declaration_names = [entry.get("name") for entry in declarations]
        if any(not isinstance(name, str) or not name for name in declaration_names):
            print("architecture.toml contains a package declaration without a valid name", file=sys.stderr)
            return 1
        duplicate_names = sorted(
            name for name in set(declaration_names) if name and declaration_names.count(name) > 1
        )
        if duplicate_names:
            print(f"Duplicate package declarations: {', '.join(duplicate_names)}", file=sys.stderr)
            return 1
        package_rules = {
            entry["name"]: entry for entry in declarations if isinstance(entry.get("name"), str)
        }
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Cannot read architecture.toml: {error}", file=sys.stderr)
        return 1

    declared = set(package_rules)
    missing = workspace.keys() - declared
    unknown = declared - workspace.keys()
    if missing or unknown:
        if missing:
            print(f"Workspace packages without declarations: {', '.join(sorted(missing))}", file=sys.stderr)
        if unknown:
            print(f"Declarations without workspace packages: {', '.join(sorted(unknown))}", file=sys.stderr)
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
            forbidden[package].update(
                name for name, target_rule in package_rules.items()
                if target_rule.get("role") in ("game", "game-content")
            )
        if rule.get("role") == "runtime":
            forbidden.setdefault(package, set()).update(
                name for name, target_rule in package_rules.items()
                if target_rule.get("role") in ("game", "game-content")
            )
        if package in {"minimium-server-runtime", "minimium-server"}:
            forbidden.setdefault(package, set()).update(
                {"minimium-renderer", "wgpu", "winit", "ash", "glow", "minimium-astronomy"}
            )
        if package == "minimium-renderer":
            forbidden.setdefault(package, set()).update(
                {"minimium-transport", "minimium-storage", "minimium-platform", "minimium-client-runtime", "minimium-server-runtime", "winit"}
            )
    violations = list(missing_readmes)
    known_package_names = set(names.values())
    for root, targets in forbidden.items():
        if root not in workspace:
            continue
        for target in sorted(targets - known_package_names):
            violations.append(f"{root}: forbidden dependency declaration names unknown package {target}")
        violations.extend(forbidden_paths(workspace[root], targets, graph, names))
    violations.extend(source_boundary_violations(repository, package_rules, package_metadata))
    violations.extend(sample_content_violations(package_rules, package_metadata))
    if violations:
        print("Architecture violations:\n" + "\n".join(violations), file=sys.stderr)
        return 1
    dev_edge_count = sum(len(edges) for edges in dev_graph.values())
    print("Architecture dependency check passed (all-feature production graph, including target-specific and build edges).")
    print(f"Development dependency graph reported separately: {dev_edge_count} edges; excluded from production checks.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
