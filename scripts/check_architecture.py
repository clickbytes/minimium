#!/usr/bin/env python3
"""Check selected Cargo dependency boundaries without running the programs."""

import json
import re
import subprocess
import sys
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
    roots = (repository / "crates" / "core", repository / "apps" / "server")
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
        graph = production_graph(metadata)
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        print(f"Cannot inspect Cargo metadata: {error}", file=sys.stderr)
        return 1

    required = {"minimium-core", "minimium-astronomy", "minimium-procgen", "minimium-sectors", "minimium-assets", "minimium-platform",
                "minimium-protocol", "minimium-telemetry", "minimium-server", "minimiumctl"}
    missing = required - workspace.keys()
    if missing:
        print(f"Missing packages: {', '.join(sorted(missing))}", file=sys.stderr)
        return 1

    forbidden = {
        "minimium-core": {"minimium-astronomy", "minimium-protocol", "minimium-platform", "minimium-assets",
                          "minimium-procgen", "minimium-sectors", "minimium-telemetry"},
        "minimium-assets": {"minimium-astronomy"},
    }
    client_only = {"minimium-astronomy", "wgpu", "wgpu-core", "wgpu-hal", "winit"}
    for package in ("minimium-core", "minimium-protocol", "minimium-server", "minimiumctl"):
        forbidden.setdefault(package, set()).update(client_only)
    violations = []
    for root, targets in forbidden.items():
        violations.extend(forbidden_paths(workspace[root], targets, graph, names))
    violations.extend(source_boundary_violations(Path(__file__).resolve().parents[1]))
    if violations:
        print("Architecture violations:\n" + "\n".join(violations), file=sys.stderr)
        return 1
    print("Architecture dependency check passed (full Cargo graph, all features/targets, "
          "transitive normal/build edges; dev edges excluded).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
