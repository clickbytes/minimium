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
        example = contract.get("example")
        if example is not None and not any(
            target["name"] == example and "example" in target["kind"]
            for target in packages[package]["targets"]
        ):
            raise ValueError(f"{name}: missing actual example target {example!r}")
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
            command = ["cargo", "tree", *selection(contract), "--edges", "normal,build",
                       "--target", "all", "--prefix", "none", "--format", "{p}|{f}",
                       "--color", "never"]
            output = subprocess.check_output(command, cwd=repository, text=True, encoding="utf-8")
            active: dict[str, set[str]] = {}
            for line in output.splitlines():
                if not line.strip():
                    continue
                package, separator, features = line.partition("|")
                if not separator or " v" not in package:
                    raise ValueError(f"unrecognized cargo tree output: {line!r}")
                name = package.split(" v", 1)[0]
                active.setdefault(name, set()).update(
                    feature for feature in features.removesuffix(" (*)").split(",") if feature
                )
            if contract["package"] not in active:
                raise ValueError(f"{contract['name']}: Cargo omitted the selected root")
            forbidden = set(contract["forbidden_dependencies"]) & active.keys()
            for package in sorted(forbidden):
                errors.append(f"{contract['name']}: production graph reaches {package}")
            for package, features in contract["forbidden_package_features"].items():
                for feature in sorted(set(features) & active.get(package, set())):
                    errors.append(f"{contract['name']}: activates {package}/{feature}")
            if not forbidden and not any(error.startswith(contract["name"] + ":") for error in errors):
                print(f"Feature graph PASS: {contract['name']} ({len(active)} packages; normal/build, all targets)")
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
            if "example" in contract:
                command.extend(["--example", contract["example"]])
            if args.target_dir:
                command.extend(["--target-dir", args.target_dir])
            print("Compile command: " + shlex.join(command), flush=True)
            result = subprocess.run(command, cwd=ROOT, check=False)
            print(f"Consumer {'PASS' if result.returncode == 0 else 'FAIL'}: {contract['name']} (exit {result.returncode})")
            if result.returncode:
                return 1
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        print(f"Cannot check feature consumers: {error}", file=sys.stderr)
        return 1
    print("Feature contracts passed; examples/applications were compiled, never executed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
