//! Prepare the same frame/sector contracts consumed by GPU upload, without a device.
//! Compile with: cargo check -p minimium-renderer --no-default-features --example cpu_frame
use minimium_procgen::SectorGeometry;
use minimium_renderer::{
    gpu_frame::{camera_uniform, environment_uniform, EnvironmentUniform, MAX_HALO_BODIES},
    planet_coordinates::PlanetSurfaceScale,
    prepared_sector::PreparedSector,
    render_frame::ClientFrameState,
    transient_lighting::{
        TransientLightContext, TransientLightFrame, TransientLightSample, TransientLightUniform,
    },
    RenderFrame, RenderViewFrame, UiVertex,
};
use minimium_world::world::{MapBox, SectorKey, WorldInstanceId};
use std::{sync::Arc, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let geometry = Arc::new(SectorGeometry {
        boxes: vec![MapBox {
            center: [0.0, 0.5, 0.0],
            size: [1.0; 3],
            color: [0.2, 0.4, 0.6],
            solid: true,
        }],
        triangles: Vec::new(),
        object_triangles: Vec::new(),
    });
    let sector = PreparedSector::new(
        SectorKey::dungeon(0, 0, 0),
        0,
        geometry,
        None,
        None,
        [0.0; 3],
    )?;
    let view = RenderViewFrame {
        game_planet_radius_m: 6_371_000.0,
        camera_eye_world_m: [0.0, 6_371_002.0, 0.0],
        render_origin_m: [0.0, 6_371_000.0, 0.0],
        ..Default::default()
    };
    // A short-lived lamp uses the same source budget without any meteor object.
    let mut lights = TransientLightFrame::new(TransientLightContext {
        camera_eye_world_m: view.camera_eye_world_m,
        render_origin_m: view.render_origin_m,
        game_radius_m: view.game_planet_radius_m,
        surface_scale: PlanetSurfaceScale::UNIT,
    })?;
    let accepted = lights.push(TransientLightSample::new(
        7,
        [4.0, 6_371_004.0, 0.0],
        [1.0, 0.7, 0.3],
        100.0,
        50.0,
        2.0,
        0.1,
    )?);
    let environment = environment_uniform(
        EnvironmentUniform {
            ambient_rgb_exposure: [0.1, 0.1, 0.1, 1.0],
            ..Default::default()
        },
        view,
        true,
        0,
        [Default::default(); MAX_HALO_BODIES],
        Duration::ZERO,
    );
    let mut frame = RenderFrame::new(view, camera_uniform(view), environment, 0)
        .with_transient_lights(lights)
        .with_client_state(ClientFrameState {
            world_instance: WorldInstanceId::new(1).expect("nonzero example world"),
            player_position_m: [0.0, 1.0, 0.0],
            health: 100,
            sectors_ready: true,
        })
        .with_ui_vertices(vec![
            UiVertex {
                position: [0.5, 0.5],
                uv: [0.0; 2],
                color: [1.0; 3],
            };
            3
        ]);
    // A CPU host may inspect prepared bounds, vertex data and output exposure,
    // then hand the unchanged values to a renderer host or retain them offline.
    let environment = frame.presentation_environment(true, true);
    let light_uniform = TransientLightUniform::from_frame(&frame.transient_lights);
    println!(
        "lamp accepted={accepted}, selected={}",
        light_uniform.count[0]
    );
    println!(
        "sector={:?} vertices={} bounds={:?} ui={} exposure={}",
        sector.key,
        sector.shadow.len(),
        sector.bounds,
        frame.ui_vertices().len(),
        environment.ambient_rgb_exposure[3]
    );
    let reusable_overlay = frame.take_ui_vertices();
    println!("reusable overlay capacity={}", reusable_overlay.capacity());
    Ok(())
}
