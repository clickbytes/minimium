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
        ("exposure_meter", source::exposure_meter()),
        ("ui_output_srgb", source::ui_output(true)),
        ("ui_output_linear", source::ui_output(false)),
        (
            "held_tool",
            include_str!("../../../crates/renderer/src/shaders/held_tool.wgsl").to_owned(),
        ),
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
        if matches!(
            name,
            "terrain"
                | "ocean"
                | "world"
                | "exposure_meter"
                | "atmosphere_sky_lut"
                | "atmosphere_aerial_lut"
        ) {
            lunar_bindings(&module, &info);
        }
        if name == "clouds" {
            cloud_contract(&module, &info);
        }
        if name == "geometry_rebase" {
            rebase_contract(&module);
        }
        if matches!(name, "exposure_meter" | "encode_output") {
            exposure_contract(&module);
        }
        if name == "sky" {
            bolid_contract(&module, &info);
        }
        if matches!(name, "terrain" | "ocean") {
            terrain_contract(&module, &info);
        }
        transient_contract(&module, &info);
        println!("{name}: valid");
    }
}

fn lunar_bindings(module: &naga::Module, info: &naga::valid::ModuleInfo) {
    for (group, binding, name) in [
        (0, 12, "sky_irradiance"),
        (2, 0, "shadow_depth"),
        (2, 2, "shadow"),
        (2, 4, "landscape_shadow"),
        (2, 5, "secondary_moon"),
        (2, 6, "secondary_landscape"),
    ] {
        let (handle, var) = module
            .global_variables
            .iter()
            .find(|(_, v)| v.name.as_deref() == Some(name))
            .expect("lunar lighting binding");
        assert_eq!(var.binding, Some(naga::ResourceBinding { group, binding }));
        match &module.types[var.ty].inner {
            naga::TypeInner::Image {
                dim: naga::ImageDimension::D2,
                arrayed,
                class,
                ..
            } => {
                assert_eq!(*arrayed, name == "shadow_depth");
                assert_eq!(
                    matches!(class, naga::ImageClass::Depth { multi: false }),
                    name == "shadow_depth"
                );
            }
            naga::TypeInner::Struct { span, .. } => {
                assert_eq!(var.space, naga::AddressSpace::Uniform);
                assert_eq!(*span, 128);
            }
            _ => panic!("unexpected lunar lighting binding type {name}"),
        }
        assert!(module
            .entry_points
            .iter()
            .enumerate()
            .any(|(i, entry)| matches!(
                entry.stage,
                naga::ShaderStage::Compute | naga::ShaderStage::Fragment
            ) && !info.get_entry_point(i)[handle].is_empty()));
    }
}

fn exposure_contract(module: &naga::Module) {
    for (name, size, offsets) in [
        (
            "OutputParams",
            48,
            &[0, 4, 8, 12, 16, 20, 24, 28, 32, 36][..],
        ),
        ("EyeExposure", 32, &[0, 4, 8, 12, 16][..]),
    ] {
        let (_, ty) = module
            .types
            .iter()
            .find(|(_, ty)| ty.name.as_deref() == Some(name))
            .expect("output exposure buffer type");
        let naga::TypeInner::Struct { members, span } = &ty.inner else {
            panic!("expected {name} struct");
        };
        assert_eq!(*span, size);
        assert_eq!(
            members.iter().map(|m| m.offset).collect::<Vec<_>>(),
            offsets
        );
    }
}

fn rebase_contract(module: &naga::Module) {
    let (_, parameters) = module
        .types
        .iter()
        .find(|(_, ty)| ty.name.as_deref() == Some("Parameters"))
        .expect("geometry rebase parameters");
    let naga::TypeInner::Struct { members, span } = &parameters.inner else {
        panic!("expected geometry rebase uniform struct");
    };
    // Rust uploads eight words: delta, stride, triangle width, normal offset, padding.
    assert_eq!(*span, 32);
    assert_eq!(
        members
            .iter()
            .map(|member| member.offset)
            .collect::<Vec<_>>(),
        [0, 12, 16, 20, 24]
    );
}

fn bolid_contract(module: &naga::Module, info: &naga::valid::ModuleInfo) {
    for (_, ty) in module.types.iter() {
        let expected = match ty.name.as_deref() {
            Some("BolidGpuEvent") => Some((
                1824,
                vec![0, 16, 32, 48, 64, 80, 96, 112, 416, 736, 1056, 1440],
            )),
            Some("BolidGpuFrame") => Some((14608, vec![0, 14592])),
            _ => None,
        };
        if let Some((size, offsets)) = expected {
            let naga::TypeInner::Struct { members, span } = &ty.inner else {
                panic!("expected bolid uniform struct");
            };
            assert_eq!(*span, size);
            assert_eq!(
                members.iter().map(|m| m.offset).collect::<Vec<_>>(),
                offsets
            );
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
                (0, 0..=2) | (1, 2) => true,
                (0, 3..=10) | (1, 0..=1) | (2, 0) => entry.stage == naga::ShaderStage::Fragment,
                _ => false,
            };
            assert!(
                visible,
                "sky binding {binding:?} unavailable in {:?}",
                entry.stage
            );
        }
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
                (0, 3..=12) => entry.stage != naga::ShaderStage::Vertex,
                (1, 4) => true,
                (1, 0..=3) | (1, 5..=6) | (2, 0..=2 | 4..=6) => {
                    entry.stage == naga::ShaderStage::Fragment
                }
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
                (0, 3..=12) => entry.stage != naga::ShaderStage::Vertex,
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
            Some("AtmosphereUniform") => 272,
            _ => continue,
        };
        let naga::TypeInner::Struct { members, span } = &ty.inner else {
            panic!("expected struct");
        };
        assert_eq!(*span, expected);
        assert_eq!(
            members
                .iter()
                .map(|member| member.offset)
                .collect::<Vec<_>>(),
            [0, 16, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 256]
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
                (0, 0..=2) => true,
                (0, 3..=12) => entry.stage != naga::ShaderStage::Vertex,
                (1, 0..=1) => entry.stage != naga::ShaderStage::Vertex,
                (2, 0..=2 | 4..=6) => entry.stage == naga::ShaderStage::Compute,
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
            Some("TransientLightUniform") => (2288, &[0, 16, 32, 48, 240]),
            Some("CloudLightOccluder") => (64, &[0, 16, 32, 48]),
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
                assert!(matches!(
                    entry.stage,
                    naga::ShaderStage::Vertex | naga::ShaderStage::Fragment
                ));
            }
        }
    }
}
