//! Finite-distance layer, then foreground-cloud transmission and final resolve.
use crate::scene_encoder::MainScenePass;

pub(crate) fn encode(encoder: &mut wgpu::CommandEncoder, frame: MainScenePass<'_>) -> u32 {
    if !frame.scene_visible
        || !frame.milky_way.any_emission()
        || frame.terrain_gpu.uniform.debug[0] != 0
        || frame.atmosphere.debug != 0
    {
        return 0;
    }
    let Some(depth) = frame.milky_way.occlusion.as_ref() else {
        return 0;
    };
    let Some(layer) = &frame.milky_way.layer else {
        return 0;
    };
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bolid-dust-after-atmosphere"),
        color_attachments: &[
            attachment(&layer.color, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)),
            attachment(
                &layer.distance,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            ),
        ],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    let mut draws = 0;
    if frame.milky_way.active() {
        pass.set_pipeline(&frame.environment_pipelines.bolid_emission);
        pass.set_bind_group(0, frame.camera, &[]);
        pass.set_bind_group(1, &frame.milky_way.bind_group, &[]);
        pass.set_bind_group(2, depth, &[]);
        pass.draw(0..3, 0..1);
        draws += 1;
    }
    draws += frame.milky_way.meteors.draw(&mut pass, frame.camera, depth);
    drop(pass);
    // Dust uses premultiplied scene/display color; emission must stay linear
    // until cloud transmission is applied. Clear dust even without a train.
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bolid dust scattering layer"),
        color_attachments: &[
            attachment(&layer.dust, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)),
            attachment(&layer.distance, wgpu::LoadOp::Load),
        ],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    if frame.milky_way.has_dust() {
        pass.set_pipeline(&frame.environment_pipelines.bolid_dust);
        pass.set_bind_group(0, frame.camera, &[]);
        pass.set_bind_group(1, &frame.milky_way.bind_group, &[]);
        pass.set_bind_group(2, depth, &[]);
        pass.draw(0..3, 0..1);
        draws += 1;
    }
    draws
        + frame
            .milky_way
            .meteors
            .draw_dust(&mut pass, frame.camera, depth)
}

pub(crate) fn resolve(
    encoder: &mut wgpu::CommandEncoder,
    frame: MainScenePass<'_>,
    clouds: &crate::CloudGpu,
) -> u32 {
    if !frame.scene_visible
        || !frame.milky_way.any_emission()
        || frame.terrain_gpu.uniform.debug[0] != 0
        || frame.atmosphere.debug != 0
    {
        return 0;
    }
    let Some(layer) = &frame.milky_way.layer else {
        return 0;
    };
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("foreground clouds to bolid"),
            color_attachments: &[
                attachment(&layer.transmission, wgpu::LoadOp::Clear(wgpu::Color::WHITE)),
                attachment(
                    &layer.foreground,
                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                ),
            ],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if clouds.count > 0 {
            pass.set_pipeline(&clouds.bolid_mask_pipeline);
            pass.set_bind_group(0, frame.camera, &[]);
            pass.set_bind_group(1, &clouds.group, &[]);
            pass.set_bind_group(2, &layer.distance_group, &[]);
            pass.set_vertex_buffer(0, clouds.faces.slice(..));
            pass.draw(0..6, 0..clouds.count);
        }
    }
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("finite bolid layer resolve"),
        color_attachments: &[attachment(frame.color_view, wgpu::LoadOp::Load)],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(&frame.environment_pipelines.bolid_composite);
    pass.set_bind_group(0, &layer.composite_group, &[]);
    pass.draw(0..3, 0..1);
    1 + u32::from(clouds.count > 0)
}

fn attachment(
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load,
            store: wgpu::StoreOp::Store,
        },
    })
}
