#!/usr/bin/env python3
"""Check selected Cargo dependency boundaries without running the programs."""

import json
import subprocess
import sys


def main() -> int:
    try:
        output = subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--locked", "--offline", "--format-version", "1"],
            text=True,
        )
        packages = json.loads(output)["packages"]
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError) as error:
        print(f"Cannot inspect Cargo metadata: {error}", file=sys.stderr)
        return 1

    graph = {package["name"]: {dep["name"] for dep in package["dependencies"]}
             for package in packages}
    required = {"minimium-core", "minimium-astronomy", "minimium-procgen", "minimium-sectors", "minimium-assets", "minimium-platform",
                "minimium-protocol", "minimium-telemetry", "minimium-server"}
    missing = required - graph.keys()
    if missing:
        print(f"Missing packages: {', '.join(sorted(missing))}", file=sys.stderr)
        return 1

    forbidden = {
        "minimium-core": {"minimium-astronomy", "minimium-protocol", "minimium-platform", "minimium-assets",
                          "minimium-procgen", "minimium-sectors", "minimium-telemetry"},
        "minimium-assets": {"minimium-astronomy"},
        "minimium-server": {"minimium-astronomy", "wgpu", "winit"},
    }
    violations = []
    for root, targets in forbidden.items():
        seen = set()
        pending = list(graph[root])
        while pending:
            dependency = pending.pop()
            if dependency in seen:
                continue
            seen.add(dependency)
            pending.extend(graph.get(dependency, ()))
        violations.extend(f"{root} reaches {dependency}" for dependency in sorted(seen & targets))
    if violations:
        print("Architecture violations:\n" + "\n".join(violations), file=sys.stderr)
        return 1
    print("Architecture dependency check passed (workspace graph; external transitive dependencies not inspected).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
