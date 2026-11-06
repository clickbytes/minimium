//! Encodes the main scene pass from client-prepared values and renderer resources.
use crate::{
    atmosphere_gpu::AtmosphereGpu, celestial_halo::CelestialHaloGpu,
    environment_pipelines::EnvironmentPipelines, milky_way_gpu::MilkyWayGpu, moon_disk::MoonGpu,
    moon_sphere::MoonSphereGpu, overview_gpu::OverviewGpuBuffers, star_gpu::StarGpu,
};
use minimium_world::world::{SectorKey, WorldInstanceId};
use std::collections::HashMap;

/// Prepared visibility decisions from the environment adapter.
#[derive(Clone, Copy, Default)]
pub struct EnvironmentDrawFlags {
    pub overview_sky: bool,
    pub overview_stars: bool,
    pub overview_sun: bool,
    pub overview_moon: bool,
    pub surface_sky: bool,
    pub surface_stars: bool,
    pub surface_sun: bool,
    pub surface_moon: bool,
}

/// Renderer handles and scene selections required by the main world pass.
#[derive(Clone, Copy)]
pub struct MainScenePass<'a> {
    pub color_view: &'a wgpu::TextureView,
    pub depth_view: &'a wgpu::TextureView,
    pub clear_color: wgpu::Color,
    pub camera: &'a wgpu::BindGroup,
    pub world_pipeline: &'a wgpu::RenderPipeline,
    pub terrain_gpu: &'a crate::terrain_gpu::TerrainGpu,
    pub terrain_catalog:
        &'a HashMap<(WorldInstanceId, SectorKey, u8), crate::world_geometry_gpu::TerrainBuffers>,
    pub shadow_empty: &'a wgpu::BindGroup,
    pub shadow_sampling: &'a wgpu::BindGroup,
    pub environment_pipelines: &'a EnvironmentPipelines,
    pub milky_way: &'a MilkyWayGpu,
    pub stars: &'a StarGpu,
    pub celestial_halo: &'a CelestialHaloGpu,
    pub moon: &'a MoonGpu,
    pub moon_sphere: &'a MoonSphereGpu,
    pub atmosphere: &'a AtmosphereGpu,
    pub overview_buffers: &'a OverviewGpuBuffers,
    pub world_vertices: &'a wgpu::Buffer,
    pub world_vertex_count: u32,
    pub remote_vertices: &'a wgpu::Buffer,
    pub remote_vertex_count: u32,
    pub world_vertex_catalog: &'a HashMap<(WorldInstanceId, SectorKey, u8), (wgpu::Buffer, u32)>,
    pub instance: WorldInstanceId,
    pub sector_hints: &'a [(SectorKey, u8)],
    pub scene_visible: bool,
    pub overview_active: bool,
    pub overview_planet: bool,
    pub uploaded_stars_match: bool,
    pub halo_count: u32,
    pub environment: EnvironmentDrawFlags,
}

/// Counts produced by the main scene pass for client telemetry.
#[derive(Clone, Copy, Default)]
pub struct SceneDrawStats {
    pub draws: u32,
    pub sky: u32,
    pub stars: u32,
    pub halos: u32,
    pub sun: u32,
    pub moon: u32,
    pub overview_marker: u32,
}

/// Encodes sky, celestial bodies, overview geometry, world sectors and remote players.
pub fn encode_main_scene_pass(
    encoder: &mut wgpu::CommandEncoder,
    frame: MainScenePass<'_>,
) -> SceneDrawStats {
    let mut stats = SceneDrawStats::default();
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("main-pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: frame.color_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(frame.clear_color),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: frame.depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_bind_group(0, frame.camera, &[]);
    if !frame.scene_visible {
        bind_waiting_world(
            &mut pass,
            frame.world_pipeline,
            frame.shadow_empty,
            frame.shadow_sampling,
        );
    } else if frame.overview_active {
        bind_world(
            &mut pass,
            frame.world_pipeline,
            frame.shadow_empty,
            frame.shadow_sampling,
        );
        encode_overview_environment(&mut pass, frame, &mut stats);
        if frame.overview_planet {
            encode_terrain(&mut pass, frame, &mut stats);
        }
        bind_world(
            &mut pass,
            frame.world_pipeline,
            frame.shadow_empty,
            frame.shadow_sampling,
        );
        if let Some((buffer, count)) = frame.overview_buffers.world() {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..*count, 0..1);
            stats.draws += 1;
        }
        if let Some((buffer, count)) = frame.overview_buffers.marker() {
            pass.set_pipeline(&frame.environment_pipelines.marker);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..*count, 0..1);
            stats.draws += 1;
            stats.overview_marker += 1;
        }
    } else {
        encode_surface_environment(&mut pass, frame, &mut stats);
        bind_world(
            &mut pass,
            frame.world_pipeline,
            frame.shadow_empty,
            frame.shadow_sampling,
        );
        encode_terrain(&mut pass, frame, &mut stats);
    }
    if !frame.overview_active && frame.remote_vertex_count > 0 {
        bind_world(
            &mut pass,
            frame.world_pipeline,
            frame.shadow_empty,
            frame.shadow_sampling,
        );
        pass.set_vertex_buffer(0, frame.remote_vertices.slice(..));
        pass.draw(0..frame.remote_vertex_count, 0..1);
        stats.draws += 1;
    }
    stats
}

fn encode_terrain<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    frame: MainScenePass<'a>,
    stats: &mut SceneDrawStats,
) {
    bind_world(
        pass,
        frame.world_pipeline,
        frame.shadow_empty,
        frame.shadow_sampling,
    );
    if frame.sector_hints.is_empty() {
        pass.set_vertex_buffer(0, frame.world_vertices.slice(..));
        pass.draw(0..frame.world_vertex_count, 0..1);
        stats.draws += 1;
    } else {
        for (key, lod) in frame.sector_hints {
            if let Some(buffers) = frame.terrain_catalog.get(&(frame.instance, *key, *lod)) {
                pass.set_pipeline(&frame.terrain_gpu.pipeline);
                pass.set_bind_group(1, &frame.terrain_gpu.bind_group, &[]);
                pass.set_bind_group(2, frame.shadow_sampling, &[]);
                pass.set_vertex_buffer(0, buffers.surface.slice(..));
                pass.draw(0..buffers.surface_count, 0..1);
                stats.draws += 1;
                bind_world(
                    pass,
                    frame.world_pipeline,
                    frame.shadow_empty,
                    frame.shadow_sampling,
                );
                if buffers.object_count > 0 {
                    pass.set_vertex_buffer(0, buffers.objects.slice(..));
                    pass.draw(0..buffers.object_count, 0..1);
                    stats.draws += 1;
                }
                continue;
            }
            if let Some((buffer, vertex_count)) =
                frame
                    .world_vertex_catalog
                    .get(&(frame.instance, *key, *lod))
            {
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*vertex_count, 0..1);
                stats.draws += 1;
            }
        }
        // Ground/objects populate depth first, so inland sea facets do not shade.
        // Fully submerged ground was already rejected during CPU preparation.
        pass.set_pipeline(&frame.terrain_gpu.ocean_pipeline);
        pass.set_bind_group(1, &frame.terrain_gpu.bind_group, &[]);
        pass.set_bind_group(2, frame.shadow_sampling, &[]);
        for (key, lod) in frame.sector_hints {
            if let Some(buffers) = frame.terrain_catalog.get(&(frame.instance, *key, *lod)) {
                if buffers.ocean_count > 0 {
                    pass.set_vertex_buffer(0, buffers.ocean.slice(..));
                    pass.draw(0..buffers.ocean_count, 0..1);
                    stats.draws += 1;
                }
            }
        }
    }
}

fn encode_overview_environment(
    pass: &mut wgpu::RenderPass<'_>,
    frame: MainScenePass<'_>,
    stats: &mut SceneDrawStats,
) {
    if frame.environment.overview_sky {
        pass.set_pipeline(&frame.environment_pipelines.sky);
        pass.set_bind_group(1, &frame.milky_way.bind_group, &[]);
        pass.draw(0..3, 0..1);
        stats.draws += 1;
        stats.sky += 1;
    }
    if frame.environment.overview_stars && frame.uploaded_stars_match {
        pass.set_pipeline(&frame.stars.pipeline);
        pass.set_vertex_buffer(0, frame.stars.buffer.slice(..));
        pass.draw(0..6, 0..crate::stars::STAR_GPU_RECORD_COUNT as u32);
        stats.draws += 1;
        stats.stars += 1;
    }
    if frame.environment.overview_sun || frame.environment.overview_moon {
        pass.set_pipeline(&frame.celestial_halo.pipeline);
        pass.draw(0..6, 0..frame.halo_count.min(8));
        stats.draws += 1;
        stats.halos += 1;
    }
    if frame.environment.overview_sun {
        pass.set_pipeline(&frame.environment_pipelines.sun);
        pass.draw(0..6, 0..1);
        stats.draws += 1;
        stats.sun += 1;
    }
    if frame.environment.overview_moon {
        pass.set_pipeline(&frame.moon_sphere.pipeline);
        pass.draw(0..6, 0..1);
        stats.draws += 1;
        stats.moon += 1;
    }
}

fn encode_surface_environment(
    pass: &mut wgpu::RenderPass<'_>,
    frame: MainScenePass<'_>,
    stats: &mut SceneDrawStats,
) {
    if !frame.environment.surface_sky {
        return;
    }
    pass.set_pipeline(&frame.environment_pipelines.sky);
    pass.set_bind_group(1, &frame.milky_way.bind_group, &[]);
    pass.draw(0..3, 0..1);
    stats.draws += 1;
    stats.sky += 1;
    if frame.environment.surface_stars && frame.uploaded_stars_match {
        pass.set_pipeline(&frame.stars.pipeline);
        pass.set_vertex_buffer(0, frame.stars.buffer.slice(..));
        pass.draw(0..6, 0..crate::stars::STAR_GPU_RECORD_COUNT as u32);
        stats.draws += 1;
        stats.stars += 1;
    }
    pass.set_pipeline(&frame.celestial_halo.pipeline);
    pass.draw(0..6, 0..frame.halo_count.min(8));
    stats.draws += 1;
    stats.halos += 1;
    if frame.environment.surface_sun {
        pass.set_pipeline(&frame.environment_pipelines.sun);
        pass.draw(0..6, 0..1);
        stats.draws += 1;
        stats.sun += 1;
    }
    if frame.environment.surface_moon {
        pass.set_pipeline(&frame.moon.pipeline);
        pass.draw(0..6, 0..1);
        stats.draws += 1;
        stats.moon += 1;
    }
}

fn bind_world<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a wgpu::RenderPipeline,
    empty: &'a wgpu::BindGroup,
    sampling: &'a wgpu::BindGroup,
) {
    pass.set_pipeline(pipeline);
    pass.set_bind_group(1, empty, &[]);
    pass.set_bind_group(2, sampling, &[]);
}

fn bind_waiting_world<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a wgpu::RenderPipeline,
    empty: &'a wgpu::BindGroup,
    sampling: &'a wgpu::BindGroup,
) {
    bind_world(pass, pipeline, empty, sampling);
}
