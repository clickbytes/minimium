#!/usr/bin/env python3
"""Check real ownership and Rust/WGSL layout boundaries without running code."""

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
        "[f32;2]": "vec2<f32>",
        "[f32;3]": "vec3<f32>",
        "[f32;4]": "vec4<f32>",
        "[u32;4]": "vec4<u32>",
        "[[f32;4];4]": "mat4x4<f32>",
        "[HaloSource;MAX_HALO_BODIES]": "array<HaloSource,8>",
    }
    return rust_types.get(compact, compact)


def main() -> int:
    errors: list[str] = []
    rust_env_path = ROOT / "crates/renderer/src/gpu_frame.rs"
    rust_halo_path = ROOT / "crates/renderer/src/gpu_frame.rs"
    wgsl_path = ROOT / "crates/renderer/src/shaders/frame.wgsl"
    halo_shader_path = ROOT / "crates/renderer/src/shaders/celestial_halo.wgsl"
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
        camera_rust = (ROOT / "crates/renderer/src/gpu_frame.rs").read_text(encoding="utf-8")
        rust_camera = [(name, normalize_gpu_type(typ)) for name, typ in struct_fields(camera_rust, "CameraUniform")]
        shader_camera = struct_fields(wgsl, "CameraUniform")
        if rust_camera != shader_camera:
            errors.append("CameraUniform layout differs between Rust and WGSL")
        atmosphere = (ROOT / "crates/renderer/src/atmosphere_uniform.rs").read_text(encoding="utf-8")
        shader = (ROOT / "crates/renderer/src/shaders/atmosphere_frame.wgsl").read_text(encoding="utf-8")
        cpu = [(name, normalize_gpu_type(typ)) for name, typ in struct_fields(atmosphere, "AtmosphereUniform")]
        if cpu != struct_fields(shader, "AtmosphereUniform"):
            errors.append("AtmosphereUniform layout differs between Rust and WGSL")
        clouds = (ROOT / "crates/renderer/src/cloud_uniform.rs").read_text(encoding="utf-8")
        cloud_shader = (ROOT / "crates/renderer/src/shaders/clouds.wgsl").read_text(encoding="utf-8")
        for name in ("CloudUniform", "CloudRecord"):
            cpu = [(field, normalize_gpu_type(typ)) for field, typ in struct_fields(clouds, name)]
            if cpu != struct_fields(cloud_shader, name):
                errors.append(f"{name} layout differs between Rust and WGSL")
        if re.search(r"\bbody\s*==\s*[01]\b", halo_shader):
            errors.append("celestial_halo.wgsl contains a body==0/1 special case")
    except (OSError, ValueError) as error:
        errors.append(str(error))

    ownership = {
        "ClientGameState": ROOT / "apps/client/src/application/game.rs",
        "WorldTransitionController": ROOT / "apps/client/src/application/transitions.rs",
        "AppPaths": ROOT / "crates/platform/src/app_paths.rs",
        "RenderFrame": ROOT / "crates/renderer/src/render_frame.rs",
        "GeometryPublicationStamp": ROOT / "crates/renderer/src/world_geometry_gpu.rs",
    }
    declarations = re.compile(r"\b(?:struct|enum|trait|type)\s+{}\b")
    rust_sources = sorted(ROOT.rglob("*.rs"))
    rust_sources = [path for path in rust_sources if "target" not in path.parts and ".git" not in path.parts]
    for symbol, owner in ownership.items():
        try:
            owner_source = owner.read_text(encoding="utf-8")
        except OSError as error:
            errors.append(f"missing owner for {symbol}: {error}")
            continue
        if not re.search(declarations.pattern.format(re.escape(symbol)), owner_source):
            errors.append(f"{owner.relative_to(ROOT).as_posix()} does not define {symbol}")
        for source_path in rust_sources:
            if source_path == owner:
                continue
            source = source_path.read_text(encoding="utf-8")
            if re.search(declarations.pattern.format(re.escape(symbol)), source):
                errors.append(f"{symbol} has a second definition in {source_path.relative_to(ROOT).as_posix()}")

    renderer_shaders = ROOT / "crates/renderer/src/shaders"
    client_renderer = ROOT / "apps/client/src/renderer"
    if not (renderer_shaders / "frame.wgsl").is_file() or not (renderer_shaders / "shadow_sampling.wgsl").is_file():
        errors.append("renderer-owned WGSL sources are missing from the renderer crate")
    if list(client_renderer.glob("*.wgsl")):
        errors.append("client application still owns renderer WGSL source files")
    frame_source = (ROOT / "crates/renderer/src/render_frame.rs").read_text(encoding="utf-8")
    if re.search(r"\bwgpu::|\b(?:Buffer|Texture|Device|Queue)\b", frame_source):
        errors.append("RenderFrame contains GPU handles instead of CPU frame values")

    old_paths = (
        ROOT / "apps/client/src/renderer/client_game.rs",
        ROOT / "crates/procgen/src/earth.rs",
    )
    for path in old_paths:
        if path.exists():
            errors.append(f"replaced implementation remains at {path.relative_to(ROOT).as_posix()}")
    for path in (ROOT / "crates/renderer/src").rglob("*.rs"):
        source = path.read_text(encoding="utf-8")
        if re.search(r"\bTerrainView::legacy\b|\bpub\s+fn\s+render_patches(?:_cached|_cancellable)?\s*\(", source):
            errors.append(f"legacy terrain presentation entry point remains in {path.relative_to(ROOT).as_posix()}")

    if errors:
        print("Static refactor check failed:", file=sys.stderr)
        print("\n".join(f"  - {error}" for error in errors), file=sys.stderr)
        return 1
    print("Static refactor check passed: Rust/WGSL layouts, symbol ownership and replaced-path boundaries.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
