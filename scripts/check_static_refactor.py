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

    required_exports = {
        ROOT / "crates/core/src/lib.rs": ("pub mod identity;", "pub mod ids;"),
        ROOT / "crates/world/src/lib.rs": ("pub mod entity;", "pub mod world;"),
        ROOT / "crates/simulation/src/lib.rs": (
            "pub use simulation::{",
            "step_character_mm",
            "CombatSettings",
        ),
        ROOT / "crates/commands/src/lib.rs": ("pub use registry::{CommandRegistry, CommandSpec};",),
        ROOT / "crates/server-runtime/src/lib.rs": ("pub fn run(args: &[String])",),
        ROOT / "crates/renderer/src/lib.rs": (
            "pub mod gpu_frame;",
            "pub use render_frame::RenderFrame;",
            "pub mod gpu_bindings;",
            "pub mod geometry;",
            "pub mod geometry_gpu;",
            "pub mod globe_vertex;",
            "pub mod globe_detail;",
            "pub mod globe_gpu;",
            "pub mod render_frame;",
            "pub mod frame_encoder;",
            "pub mod scene_encoder;",
            "pub mod surface_frame;",
            "pub mod world_geometry_gpu;",
            "pub mod scene_encoder;",
            "pub mod overview_gpu;",
            "pub mod cloud_face;",
            "pub mod cloud_pipeline;",
            "pub mod cloud_targets;",
            "pub mod cloud_voxels;",
            "pub mod cloud_mesh;",
            "pub mod cloud_uniform;",
            "pub mod cloud_gpu;",
            "pub mod ui_vertex;",
            "pub mod ui_pipeline;",
            "pub mod atmosphere;",
            "pub mod atmosphere_table;",
            "pub mod atmosphere_pipeline;",
            "pub mod atmosphere_uniform;",
            "pub mod atmosphere_gpu;",
            "pub mod environment_pipelines;",
            "pub mod world_pipeline;",
            "pub mod output;",
            "pub mod stars;",
            "pub mod star_gpu;",
            "pub mod shadow_target;",
            "pub mod shadow_settings;",
            "pub mod shadow_geometry;",
            "pub mod shadow_uniform;",
            "pub mod shadow_gpu;",
            "pub mod planet_visibility;",
            "pub mod profiles;",
            "pub mod lighting;",
            "pub mod moon_lighting;",
            "pub mod shader_source;",
            "pub use celestial_halo::CelestialHaloGpu;",
            "pub use moon_disk::MoonGpu;",
            "pub use moon_sphere::MoonSphereGpu;",
        ),
        ROOT / "games/sample/src/lib.rs": ("DEFAULT_COMBAT_CONFIG", "demo_universe_catalog"),
        ROOT / "crates/client-runtime/src/lib.rs": (
            "pub mod environment;",
            "ClientSession, DisconnectReason",
            "pub use prediction::{ClientPrediction, PredictedStep};",
            "pub use snapshots::{RemotePlayerTrack, SnapshotReconciler};",
        ),
        ROOT / "crates/procgen/src/lib.rs": ("pub use geography::{EarthSampleError, PreparedGeography};",),
    }
    for path, required in required_exports.items():
        try:
            source = path.read_text(encoding="utf-8")
            for symbol in required:
                if symbol not in source:
                    errors.append(f"{path.relative_to(ROOT).as_posix()}: missing export {symbol}")
        except OSError as error:
            errors.append(str(error))

    renderer_shaders = ROOT / "crates/renderer/src/shaders"
    client_renderer = ROOT / "apps/client/src/renderer"
    if not (renderer_shaders / "frame.wgsl").is_file():
        errors.append("renderer frame shader is missing from the renderer crate")
    if list(client_renderer.glob("*.wgsl")):
        errors.append("client application still owns renderer WGSL source files")
    for launcher in ("import-gebco-lite.ps1", "import-gebco.ps1"):
        if not (ROOT / launcher).is_file():
            errors.append(f"repository-root geography launcher is missing: {launcher}")
    if not (ROOT / "tools/geography/scripts/import_gebco_lite.py").is_file():
        errors.append("compact geography importer is not under tools/geography")

    if (ROOT / "crates/procgen/src/earth.rs").exists():
        errors.append("procgen still contains the legacy global GEBCO package loader")
    server_manifest = ROOT / "apps/server/Cargo.toml"
    try:
        server_dependencies = server_manifest.read_text(encoding="utf-8")
        if "minimium-server-runtime" not in server_dependencies:
            errors.append("server application does not depend on minimium-server-runtime")
        if "minimium-protocol" in server_dependencies or "minimium-storage" in server_dependencies:
            errors.append("server application still directly depends on runtime domains")
    except OSError as error:
        errors.append(str(error))
    client_manifest = ROOT / "apps/client/Cargo.toml"
    try:
        if "minimium-client-runtime" not in client_manifest.read_text(encoding="utf-8"):
            errors.append("client application does not depend on minimium-client-runtime")
    except OSError as error:
        errors.append(str(error))

    central_parser = ROOT / "apps/client/src/client_commands.rs"
    command_modules = (
        "connection.rs", "environment.rs", "gameplay.rs", "interface.rs", "weather.rs",
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
