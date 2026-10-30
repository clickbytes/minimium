#!/usr/bin/env python3
"""Check selected static contracts for the E005 refactor without running code."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def struct_fields(source: str, name: str) -> list[tuple[str, str]]:
    match = re.search(rf"\bstruct\s+{re.escape(name)}\s*\{{(.*?)\}}\s*;?", source, re.S)
    if not match:
        raise ValueError(f"missing struct {name}")
    fields = []
    raw_fields = []
    start = 0
    depth = 0
    for index, character in enumerate(match.group(1)):
        if character in "[<":
            depth += 1
        elif character in "]>":
            depth -= 1
        elif character == "," and depth == 0:
            raw_fields.append(match.group(1)[start:index])
            start = index + 1
    raw_fields.append(match.group(1)[start:])
    for field in raw_fields:
        field = field.strip()
        if not field:
            continue
        rust = re.fullmatch(r"(?:pub(?:\([^)]*\))?\s+)?(\w+)\s*:\s*(.+)", field, re.S)
        wgsl = re.fullmatch(r"(\w+)\s*:\s*(.+)", field, re.S)
        parsed = rust or wgsl
        if not parsed:
            raise ValueError(f"cannot parse {name} field: {field!r}")
        fields.append((parsed.group(1), re.sub(r"\s+", "", parsed.group(2))))
    return fields


def normalize_gpu_type(type_name: str) -> str:
    compact = re.sub(r"\s+", "", type_name)
    rust_types = {
        "[f32;4]": "vec4<f32>",
        "[u32;4]": "vec4<u32>",
        "[[f32;4];4]": "mat4x4<f32>",
        "[HaloSource;MAX_HALO_BODIES]": "array<HaloSource,8>",
    }
    return rust_types.get(compact, compact)


def main() -> int:
    errors: list[str] = []
    rust_env_path = ROOT / "apps/client/src/renderer/environment_gpu.rs"
    rust_halo_path = ROOT / "apps/client/src/renderer/environment.rs"
    wgsl_path = ROOT / "apps/client/src/renderer/frame.wgsl"
    halo_shader_path = ROOT / "apps/client/src/renderer/celestial_halo.wgsl"
    try:
        rust = rust_env_path.read_text(encoding="utf-8")
        halo_rust = rust_halo_path.read_text(encoding="utf-8")
        wgsl = wgsl_path.read_text(encoding="utf-8")
        halo_shader = halo_shader_path.read_text(encoding="utf-8")
        halo_rust_fields = struct_fields(halo_rust, "HaloSource")
        halo_wgsl_fields = struct_fields(wgsl, "HaloSource")
        halo_rust_fields = [(name, normalize_gpu_type(typ)) for name, typ in halo_rust_fields]
        halo_wgsl_fields = [(name, normalize_gpu_type(typ)) for name, typ in halo_wgsl_fields]
        if halo_rust_fields != halo_wgsl_fields:
            errors.append(f"HaloSource layout differs: Rust={halo_rust_fields}, WGSL={halo_wgsl_fields}")
        rust_env_fields = struct_fields(rust, "EnvironmentUniform")
        wgsl_env_fields = struct_fields(wgsl, "EnvironmentUniform")
        rust_env_fields = [(name, normalize_gpu_type(typ)) for name, typ in rust_env_fields]
        wgsl_env_fields = [(name, normalize_gpu_type(typ)) for name, typ in wgsl_env_fields]
        if rust_env_fields != wgsl_env_fields:
            errors.append(f"EnvironmentUniform layout differs: Rust={rust_env_fields}, WGSL={wgsl_env_fields}")
        if re.search(r"\bbody\s*==\s*[01]\b", halo_shader):
            errors.append("celestial_halo.wgsl contains a body==0/1 special case")
    except (OSError, ValueError) as error:
        errors.append(str(error))

    required_exports = {
        ROOT / "crates/core/src/lib.rs": ("pub mod entity;", "pub mod universe;"),
        ROOT / "crates/platform/src/lib.rs": ("pub mod modules;",),
        ROOT / "crates/platform/src/modules.rs": (
            "pub trait EngineModule", "pub struct ModuleRegistry", "pub enum CommandScope",
        ),
        ROOT / "crates/core/src/entity.rs": (
            "pub struct EntityStore", "pub struct ComponentRegistry", "pub enum ReplicationPolicy",
        ),
        ROOT / "crates/core/src/universe.rs": ("pub struct UniverseCatalog",),
    }
    for path, required in required_exports.items():
        try:
            source = path.read_text(encoding="utf-8")
            for symbol in required:
                if symbol not in source:
                    errors.append(f"{path.relative_to(ROOT).as_posix()}: missing export {symbol}")
        except OSError as error:
            errors.append(str(error))

    central_parser = ROOT / "apps/client/src/client_commands.rs"
    command_modules = (
        "connection.rs", "environment.rs", "gameplay.rs", "interface.rs",
    )
    try:
        central = central_parser.read_text(encoding="utf-8")
        if re.search(r"if\s+\*command\s*==|match\s*\(\*command", central):
            errors.append("client_commands.rs contains command-name-specific parser branches")
        for module in command_modules:
            source = (central_parser.parent / "client_commands" / module).read_text(encoding="utf-8")
            if "const COMMANDS:" not in source or "fn parse(" not in source or "registry.register(COMMANDS)" not in source:
                errors.append(f"client command module lacks a local parser registration: {module}")
    except OSError as error:
        errors.append(str(error))

    if errors:
        print("Static refactor check failed:", file=sys.stderr)
        print("\n".join(f"  - {error}" for error in errors), file=sys.stderr)
        return 1
    print("Static refactor check passed: Rust/WGSL layout, exports, halo dispatch and command parser ownership.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
