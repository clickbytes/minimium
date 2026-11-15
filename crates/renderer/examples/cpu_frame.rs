//! Prepare the same frame/sector contracts consumed by GPU upload, without a device.
//! Compile with: cargo check -p minimium-renderer --no-default-features --example cpu_frame
use minimium_procgen::SectorGeometry;
use minimium_renderer::{
    gpu_frame::{camera_uniform, EnvironmentUniform},
    prepared_sector::PreparedSector,
    render_frame::ClientFrameState,
    RenderFrame, RenderViewFrame, UiVertex,
};
use minimium_world::world::{MapBox, SectorKey, WorldInstanceId};
use std::sync::Arc;

fn main() {
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
    );
    let view = RenderViewFrame::default();
    let environment = EnvironmentUniform {
        ambient_rgb_exposure: [0.1, 0.1, 0.1, 1.0],
        ..Default::default()
    };
    let mut frame = RenderFrame::new(view, camera_uniform(view), environment, 0)
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
}
