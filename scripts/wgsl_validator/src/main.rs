//! Static shader compilation only: does not create an adapter or run the game.
#[path = "../../../apps/client/src/renderer/shader_source.rs"]
mod source;

fn main() {
    let variants = [
        ("world", source::world()),
        ("globe", source::globe()),
        ("sky", source::sky()),
        ("sun", source::sun()),
        ("stars", source::stars()),
        ("moon", source::moon()),
        ("moon_sphere", source::moon_sphere()),
        ("celestial_halo", source::celestial_halo()),
        ("ellipsoids", source::clouds(false)),
        ("aggregates", source::clouds(true)),
        ("composite", source::cloud_composite()),
        ("atmosphere", source::atmosphere()),
        ("fog", source::weather_fog()),
        ("rain", source::rain()),
        ("bolt", source::bolt()),
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
        if matches!(name, "ellipsoids" | "aggregates") {
            cloud_contract(&module, &info);
        }
        println!("{name}: valid");
    }
}

fn cloud_contract(module: &naga::Module, info: &naga::valid::ModuleInfo) {
    for (_, ty) in module.types.iter() {
        let expected = match ty.name.as_deref() {
            Some("CloudInstanceGpu") => 176,
            Some("InstanceUniform") => 80,
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
                (0, 0..=1) | (1, 0) | (3, 0..=1) => true,
                (1, 1..=4) | (2, 0) => entry.stage == naga::ShaderStage::Fragment,
                _ => false,
            };
            assert!(
                visible,
                "binding {:?} is unavailable in {:?}",
                binding, entry.stage
            );
        }
        if entry.name == "fs_cloud" {
            let result = entry.function.result.as_ref().unwrap();
            let naga::TypeInner::Struct { members, .. } = &module.types[result.ty].inner else {
                panic!("expected attachments");
            };
            assert_eq!(members.len(), 4);
            for (index, member) in members.iter().enumerate() {
                assert!(
                    matches!(member.binding, Some(naga::Binding::Location { location, .. }) if location == index as u32)
                );
            }
        }
    }
}
