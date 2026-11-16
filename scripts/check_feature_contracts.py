#!/usr/bin/env python3
"""Inspect selected Cargo feature graphs and compile their real consumers.

This checks dependency reachability/export usability, not semantic ownership,
runtime behavior or visual acceptance. No example, test or application is run.
"""
from __future__ import annotations

import argparse
import json
import shlex
import subprocess
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def string_list(value: object) -> bool:
    return isinstance(value, list) and all(isinstance(item, str) and item for item in value)


def validate_contracts(declaration: dict, packages: dict, known_names: set[str]) -> list[dict]:
    contracts = declaration.get("feature_contract", [])
    if not isinstance(contracts, list) or not contracts:
        raise ValueError("architecture.toml requires feature_contract declarations")
    names = set()
    for contract in contracts:
        if not isinstance(contract, dict):
            raise ValueError("feature_contract must be a table")
        name, package = contract.get("name"), contract.get("package")
        if not isinstance(name, str) or not name or name in names or package not in packages:
            raise ValueError(f"invalid/duplicate feature contract or package: {name!r}")
        names.add(name)
        if not isinstance(contract.get("no_default_features"), bool):
            raise ValueError(f"{name}: no_default_features must be explicit")
        for key in ("features", "forbidden_dependencies"):
            if not string_list(contract.get(key)):
                raise ValueError(f"{name}: {key} must be a list of names")
        if not contract["forbidden_dependencies"]:
            raise ValueError(f"{name}: forbidden_dependencies cannot be empty")
        unknown = set(contract["forbidden_dependencies"]) - known_names
        if unknown:
            raise ValueError(f"{name}: unknown forbidden dependencies: {sorted(unknown)}")
        if any(feature not in packages[package]["features"] for feature in contract["features"]):
            raise ValueError(f"{name}: unknown requested package feature")
        forbidden = contract.get("forbidden_package_features")
        if not isinstance(forbidden, dict):
            raise ValueError(f"{name}: forbidden_package_features must be a table")
        for target, features in forbidden.items():
            if target not in packages or not string_list(features):
                raise ValueError(f"{name}: invalid feature restriction for {target}")
            if any(feature not in packages[target]["features"] for feature in features):
                raise ValueError(f"{name}: unknown forbidden feature for {target}")
        examples = contract.get("examples", [])
        if "example" in contract:
            if examples:
                raise ValueError(f"{name}: use examples instead of both example and examples")
            examples = [contract["example"]]
        if not string_list(examples):
            raise ValueError(f"{name}: examples must be a list of target names")
        available_examples = {
            target["name"] for target in packages[package]["targets"]
            if "example" in target["kind"]
        }
        missing_examples = sorted(set(examples) - available_examples)
        if missing_examples:
            raise ValueError(f"{name}: missing actual example targets: {missing_examples}")
        allow_dev = contract.get("allowed_dev_dependencies", [])
        if not string_list(allow_dev):
            raise ValueError(f"{name}: allowed_dev_dependencies must be a list")
        unknown_allowlist = set(allow_dev) - known_names
        if unknown_allowlist:
            raise ValueError(f"{name}: unknown allowed dev dependencies: {sorted(unknown_allowlist)}")
        if set(allow_dev) - set(contract["forbidden_dependencies"]):
            raise ValueError(f"{name}: dev exceptions must also be forbidden dependencies")
        if allow_dev and not contract.get("dev_dependency_rationale"):
            raise ValueError(f"{name}: dev dependency exceptions require a rationale")
        direct_dev = {
            dependency["name"] for dependency in packages[package]["dependencies"]
            if dependency.get("kind") == "dev"
        }
        if not set(allow_dev) <= direct_dev:
            raise ValueError(f"{name}: dev exceptions must name direct dev dependencies of {package}")
    return contracts


def selection(contract: dict) -> list[str]:
    args = ["-p", contract["package"], "--locked"]
    if contract["no_default_features"]:
        args.append("--no-default-features")
    if contract["features"]:
        args.extend(["--features", ",".join(contract["features"])])
    return args


def graph_violations(
    repository: Path, declaration: dict, packages: dict, known_names: set[str]
) -> list[str]:
    """Use independent Cargo selections; all-features metadata cannot prove CPU variants."""
    errors = []
    try:
        contracts = validate_contracts(declaration, packages, known_names)
        for contract in contracts:
            def active_packages(edges: str) -> dict[str, set[str]]:
                command = ["cargo", "tree", *selection(contract), "--edges", edges,
                           "--target", "all", "--prefix", "none", "--format", "{p}|{f}",
                           "--color", "never"]
                output = subprocess.check_output(command, cwd=repository, text=True, encoding="utf-8")
                result: dict[str, set[str]] = {}
                for line in output.splitlines():
                    if not line.strip():
                        continue
                    package, separator, features = line.partition("|")
                    if not separator or " v" not in package:
                        raise ValueError(f"unrecognized cargo tree output: {line!r}")
                    name = package.split(" v", 1)[0]
                    result.setdefault(name, set()).update(
                        feature for feature in features.removesuffix(" (*)").split(",") if feature
                    )
                return result

            production = active_packages("normal,build")
            has_example = bool(contract.get("examples") or contract.get("example"))
            edges = "normal,build,dev" if has_example else "normal,build"
            active = active_packages(edges) if has_example else production
            if contract["package"] not in active or contract["package"] not in production:
                raise ValueError(f"{contract['name']}: Cargo omitted the selected root")
            dev_exceptions = set(contract.get("allowed_dev_dependencies", []))
            forbidden = set(contract["forbidden_dependencies"]) & production.keys()
            if has_example:
                forbidden |= (set(contract["forbidden_dependencies"]) - dev_exceptions) & active.keys()
                bad_exceptions = dev_exceptions & production.keys()
                for package in sorted(bad_exceptions):
                    errors.append(f"{contract['name']}: declared dev exception {package} is also in normal/build graph")
            for package in sorted(forbidden):
                errors.append(f"{contract['name']}: production graph reaches {package}")
            for package, features in contract["forbidden_package_features"].items():
                for feature in sorted(set(features) & active.get(package, set())):
                    errors.append(f"{contract['name']}: activates {package}/{feature}")
            if not forbidden and not any(error.startswith(contract["name"] + ":") for error in errors):
                if has_example and dev_exceptions & active.keys():
                    print(
                        f"Feature graph PASS: {contract['name']} ({len(active)} packages; "
                        f"{edges}, all targets; declared dev edge(s): "
                        f"{', '.join(sorted(dev_exceptions & active.keys()))})"
                    )
                else:
                    print(f"Feature graph PASS: {contract['name']} ({len(active)} packages; {edges}, all targets)")
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        errors.append(f"Cannot inspect feature contracts: {error}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--graph-only", action="store_true", help="Skip consumer compilation explicitly")
    parser.add_argument("--target-dir", help="Reuse a chosen Cargo build cache")
    args = parser.parse_args()
    try:
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--locked", "--all-features", "--format-version", "1"],
            cwd=ROOT, text=True, encoding="utf-8",
        ))
        packages = {package["name"]: package for package in metadata["packages"]
                    if package["id"] in metadata["workspace_members"]}
        known_names = {package["name"] for package in metadata["packages"]}
        declaration = tomllib.loads((ROOT / "architecture.toml").read_text(encoding="utf-8"))
        errors = graph_violations(ROOT, declaration, packages, known_names)
        if errors:
            print("\n".join(errors), file=sys.stderr)
            return 1
        if args.graph_only:
            print("Consumer compilation NOT RUN (--graph-only).")
            return 0
        for contract in validate_contracts(declaration, packages, known_names):
            command = ["cargo", "check", *selection(contract)]
            examples = contract.get("examples", [])
            if "example" in contract:
                examples = [contract["example"]]
            targets = examples or [None]
            for example in targets:
                target_command = list(command)
                if example:
                    target_command.extend(["--example", example])
                if args.target_dir:
                    target_command.extend(["--target-dir", args.target_dir])
                label = f"{contract['name']}::{example}" if example else contract["name"]
                print("Compile command: " + shlex.join(target_command), flush=True)
                result = subprocess.run(target_command, cwd=ROOT, check=False)
                print(f"Consumer {'PASS' if result.returncode == 0 else 'FAIL'}: {label} (exit {result.returncode})")
                if result.returncode:
                    return 1
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        print(f"Cannot check feature consumers: {error}", file=sys.stderr)
        return 1
    print("Feature contracts passed; examples/applications were compiled, never executed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
