//! Static shader compilation only: does not create an adapter or run the game.
#[path = "../../../crates/renderer/src/shader_source.rs"]
mod source;

fn main() {
    let variants = [
        (
            "geometry_rebase",
            include_str!("../../../crates/renderer/src/shaders/geometry_rebase.wgsl").to_owned(),
        ),
        (
            "shadow",
            include_str!("../../../crates/renderer/src/shaders/shadow.wgsl").to_owned(),
        ),
        ("clouds", source::clouds()),
        ("meteors", source::meteors()),
        ("cloud_composite", source::cloud_composite()),
        ("world", source::world()),
        ("terrain", source::terrain()),
        ("ocean", source::ocean()),
        ("sky", source::sky()),
        ("bolid_composite", source::bolid_composite()),
        ("sun", source::sun()),
        ("stars", source::stars()),
        ("moon", source::moon()),
        ("moon_sphere", source::moon_sphere()),
        ("celestial_halo", source::celestial_halo()),
        ("atmosphere", source::atmosphere()),
        ("atmosphere_sky_lut", source::atmosphere_compute("sky")),
        (
            "atmosphere_aerial_lut",
            source::atmosphere_compute("aerial"),
        ),
        ("atmosphere_multi_lut", source::atmosphere_compute("multi")),
        ("marker", source::marker()),
        ("ui", source::ui()),
        ("encode_output", source::encode_output()),
        ("ui_output_srgb", source::ui_output(true)),
        ("ui_output_linear", source::ui_output(false)),
    ];
    for (name, code) in variants {
        let module = naga::front::wgsl::parse_str(&code)
            .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(&code)));
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(&code)));
        if matches!(
            name,
            "atmosphere" | "atmosphere_sky_lut" | "atmosphere_aerial_lut" | "atmosphere_multi_lut"
        ) {
            atmosphere_contract(&module, &info);
        }
        if name == "clouds" {
            cloud_contract(&module, &info);
        }
        if matches!(name, "terrain" | "ocean") {
            terrain_contract(&module, &info);
        }
        transient_contract(&module, &info);
        println!("{name}: valid");
    }
}

fn terrain_contract(module: &naga::Module, info: &naga::valid::ModuleInfo) {
    for (_, ty) in module.types.iter() {
        if ty.name.as_deref() == Some("TerrainSettings") {
            let naga::TypeInner::Struct { members, span } = &ty.inner else {
                panic!("expected terrain uniform");
            };
            assert_eq!(*span, 240);
            assert_eq!(members.len(), 10);
            for (i, member) in members.iter().enumerate() {
                assert_eq!(member.offset, i as u32 * 16);
            }
        }
    }
    for (index, entry) in module.entry_points.iter().enumerate() {
        for (handle, global) in module.global_variables.iter() {
            if info.get_entry_point(index)[handle].is_empty() {
                continue;
            }
            let Some(binding) = global.binding else {
                continue;
            };
            let visible = match (binding.group, binding.binding) {
                (0, 0..=2) => true,
                (0, 3..=11) => entry.stage != naga::ShaderStage::Vertex,
                (1, 4) => true,
                (1, 0..=3) | (1, 5..=6) | (2, 0..=2) => entry.stage == naga::ShaderStage::Fragment,
                _ => false,
            };
            assert!(
                visible,
                "terrain binding {binding:?} unavailable in {:?}",
                entry.stage
            );
        }
    }
}

fn cloud_contract(module: &naga::Module, info: &naga::valid::ModuleInfo) {
    for (_, ty) in module.types.iter() {
        let (expected_span, offsets): (u32, &[u32]) = match ty.name.as_deref() {
            Some("CloudUniform") => (320, &[0, 64, 128, 192, 208, 224, 240, 256]),
            Some("CloudRecord") => (96, &[0, 16, 32, 48, 64, 80]),
            _ => continue,
        };
        let naga::TypeInner::Struct { members, span } = &ty.inner else {
            panic!("expected cloud struct");
        };
        assert_eq!(*span, expected_span);
        assert_eq!(
            members.iter().map(|m| m.offset).collect::<Vec<_>>(),
            offsets
        );
    }
    for (index, entry) in module.entry_points.iter().enumerate() {
        for (handle, global) in module.global_variables.iter() {
            if info.get_entry_point(index)[handle].is_empty() {
                continue;
            }
            let Some(binding) = global.binding else {
                continue;
            };
            let visible = match (binding.group, binding.binding) {
                (0, 0..=2) | (1, 0..=1) => true,
                (0, 3..=11) => entry.stage != naga::ShaderStage::Vertex,
                (1, 2) => entry.stage == naga::ShaderStage::Fragment,
                (2, 0) => {
                    matches!(entry.name.as_str(), "fs_bolid_mask" | "fs_inside_mask")
                        && entry.stage == naga::ShaderStage::Fragment
                }
                _ => false,
            };
            assert!(
                visible,
                "cloud binding {binding:?} unavailable in {:?}",
                entry.stage
            );
        }
    }
}

fn atmosphere_contract(module: &naga::Module, info: &naga::valid::ModuleInfo) {
    for (_, ty) in module.types.iter() {
        let expected = match ty.name.as_deref() {
            Some("AtmosphereUniform") => 256,
            _ => continue,
        };
        let naga::TypeInner::Struct { members, span } = &ty.inner else {
            panic!("expected struct");
        };
        assert_eq!(*span, expected);
        for (index, member) in members.iter().enumerate() {
            assert_eq!(member.offset, index as u32 * 16);
        }
    }
    for (index, entry) in module.entry_points.iter().enumerate() {
        for (handle, global) in module.global_variables.iter() {
            if info.get_entry_point(index)[handle].is_empty() {
                continue;
            }
            let Some(binding) = global.binding else {
                continue;
            };
            let visible = match (binding.group, binding.binding) {
                (0, 0..=2) => true,
                (0, 3..=11) => entry.stage != naga::ShaderStage::Vertex,
                (1, 0..=1) | (2, 0) => entry.stage != naga::ShaderStage::Vertex,
                (1, 2) => entry.stage == naga::ShaderStage::Fragment,
                _ => false,
            };
            assert!(
                visible,
                "binding {:?} is unavailable in {:?}",
                binding, entry.stage
            );
        }
    }
}

fn transient_contract(module: &naga::Module, info: &naga::valid::ModuleInfo) {
    for (_, ty) in module.types.iter() {
        let (size, offsets): (u32, &[u32]) = match ty.name.as_deref() {
            Some("TransientLight") => (48, &[0, 16, 32]),
            Some("TransientLightUniform") => (240, &[0, 16, 32, 48]),
            _ => continue,
        };
        let naga::TypeInner::Struct { members, span } = &ty.inner else {
            panic!("expected transient uniform");
        };
        assert_eq!(*span, size);
        assert_eq!(
            members
                .iter()
                .map(|member| member.offset)
                .collect::<Vec<_>>(),
            offsets
        );
    }
    for (index, entry) in module.entry_points.iter().enumerate() {
        for (handle, global) in module.global_variables.iter() {
            if global.name.as_deref() != Some("transient_lights") {
                continue;
            }
            assert_eq!(
                global.binding,
                Some(naga::ResourceBinding {
                    group: 0,
                    binding: 11
                })
            );
            assert_eq!(global.space, naga::AddressSpace::Uniform);
            if !info.get_entry_point(index)[handle].is_empty() {
                assert_eq!(entry.stage, naga::ShaderStage::Fragment);
            }
        }
    }
}
