//! Final presentation passes shared by client render adapters.

use crate::{
    cloud_gpu::CloudGpu,
    scene_encoder::{self, MainScenePass, SceneDrawStats},
    ShadowGpu,
};
use minimium_world::world::{SectorKey, WorldInstanceId};
use std::collections::HashMap;

/// GPU handles for the UI and output passes of one prepared frame.
pub struct FinalPasses<'a> {
    pub scene_view: &'a wgpu::TextureView,
    pub surface_view: &'a wgpu::TextureView,
    pub ui_pipeline: &'a wgpu::RenderPipeline,
    pub ui_output_pipeline: Option<&'a wgpu::RenderPipeline>,
    pub ui_bind_group: &'a wgpu::BindGroup,
    pub ui_vertex_buffer: &'a wgpu::Buffer,
    pub ui_vertex_count: u32,
    pub output: Option<&'a crate::OutputEncoder>,
}

/// Client-selected shadow caster buffers passed to the renderer encoder.
pub struct ShadowPass<'a> {
    pub gpu: &'a ShadowGpu,
    pub enabled: bool,
    pub instance: WorldInstanceId,
    pub keys: &'a [(SectorKey, u8)],
    pub world_vertices: &'a HashMap<(WorldInstanceId, SectorKey, u8), (wgpu::Buffer, u32)>,
    pub map: Option<(&'a wgpu::Buffer, u32)>,
    pub player: Option<(&'a wgpu::Buffer, u32)>,
}

/// Inputs for encoding all renderer passes for one frame in their stable order.
pub struct FramePasses<'a> {
    pub shadow: ShadowPass<'a>,
    pub scene: MainScenePass<'a>,
    pub clouds: &'a CloudGpu,
    pub final_passes: FinalPasses<'a>,
}

/// Encoded pass draw counts used by the client diagnostics adapter.
pub struct FrameDrawStats {
    pub shadow: u32,
    pub scene: SceneDrawStats,
    pub atmosphere: u32,
    pub clouds: u32,
    pub final_passes: u32,
}

/// Encodes shadow, scene, atmosphere, cloud, overlay and output passes in order.
pub fn encode_frame(encoder: &mut wgpu::CommandEncoder, frame: FramePasses<'_>) -> FrameDrawStats {
    let shadow_casters = frame.shadow.keys.iter().filter_map(|(key, lod)| {
        frame
            .shadow
            .world_vertices
            .get(&(frame.shadow.instance, *key, *lod))
            .map(|(buffer, count)| (buffer, *count))
    });
    let shadow_draws = frame.shadow.gpu.encode_depth_pass(
        encoder,
        shadow_casters
            .chain(frame.shadow.map.into_iter())
            .chain(frame.shadow.player.into_iter()),
        frame.shadow.enabled,
    );
    let scene = scene_encoder::encode_main_scene_pass(encoder, frame.scene);
    let atmosphere = frame.scene.atmosphere.encode(
        encoder,
        frame.scene.color_view,
        frame.scene.camera,
        frame.scene.scene_visible
            && frame.scene.overview_active
            && frame.scene.overview_globe
            && frame.scene.globe.is_some(),
    );
    let clouds = frame.clouds.encode(
        encoder,
        frame.scene.color_view,
        frame.scene.camera,
        frame.scene.scene_visible,
    );
    let final_passes = encode_final_passes(encoder, frame.final_passes);
    FrameDrawStats {
        shadow: shadow_draws,
        scene,
        atmosphere,
        clouds,
        final_passes,
    }
}

/// Encodes overlay, scene-output and surface-overlay passes in their stable order.
pub fn encode_final_passes(encoder: &mut wgpu::CommandEncoder, frame: FinalPasses<'_>) -> u32 {
    let mut draws = 0;
    if frame.ui_vertex_count > 0 && frame.ui_output_pipeline.is_none() {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("minimium-console-ui-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame.scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(frame.ui_pipeline);
        pass.set_bind_group(0, frame.ui_bind_group, &[]);
        pass.set_vertex_buffer(0, frame.ui_vertex_buffer.slice(..));
        pass.draw(0..frame.ui_vertex_count, 0..1);
        draws += 1;
    }
    if let Some(output) = frame.output {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("linear-output-srgb-encode-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame.surface_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(output.pipeline());
        pass.set_bind_group(0, output.bind_group(), &[]);
        pass.draw(0..3, 0..1);
        draws += 1;
    }
    if frame.ui_vertex_count > 0 {
        if let Some(pipeline) = frame.ui_output_pipeline {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("minimium-console-ui-output-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: frame.surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, frame.ui_bind_group, &[]);
            pass.set_vertex_buffer(0, frame.ui_vertex_buffer.slice(..));
            pass.draw(0..frame.ui_vertex_count, 0..1);
            draws += 1;
        }
    }
    draws
}
