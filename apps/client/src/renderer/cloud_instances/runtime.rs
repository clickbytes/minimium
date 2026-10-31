use super::{build::Built, shadow, CloudInstances, Request, View};
use glam::DVec3;
use crate::environment::weather::wind::EARTH_RADIUS;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

// A local FPP cut is never reused after a planetary teleport. The physical
// field and distant globe coverage remain available during asynchronous rebuild.
fn local_cut_tolerance(radius: f64) -> f64 {
    (radius / EARTH_RADIUS * 20_000.0).max(1.0)
}

impl CloudInstances {
    pub(in crate::renderer) fn invalidate_view(&mut self) {
        self.serial = self.serial.wrapping_add(1);
        self.ready = None;
        self.requested = None;
        self.draw_count = 0;
        self.lod_history.clear();
        self.last_request = Instant::now() - Duration::from_secs(1);
    }

    pub(in crate::renderer) fn update(&mut self, queue: &wgpu::Queue, view: View) {
        self.active_limit = view.settings.instances;
        self.active_aggregate_limit = view.settings.aggregates;
        let Some(atlas) = self.atlas.clone() else {
            return;
        };
        if view.radius <= 0.0 {
            // Interior/unavailable worlds must never accept an in-flight Earth cut
            // if the player later re-enters a visually similar planet.
            if self.requested.is_some() || self.published.is_some() || self.ready.is_some() {
                self.serial = self.serial.wrapping_add(1);
            }
            self.clear(queue);
            self.last_seconds = None;
            return;
        }
        let seconds = view.utc_ms as f64 / 1000.0;
        // Cadence adapts to physical-field preparation, never graphics quality/FPS.
        let desired = (view.world_rate * (self.build_seconds + 0.25) * 2.0).max(5.0);
        let duration = [5.0, 10.0, 30.0, 60.0, 120.0, 300.0]
            .into_iter()
            .find(|&d| d >= desired)
            .unwrap_or(300.0);
        let mut request = Request {
            world: view.world,
            previous_lod: self.lod_history.clone(),
            atlas,
            eye: DVec3::from_array(view.eye),
            radius: view.radius,
            pixels_per_radian: view.pixels_per_radian.max(1.0),
            start_seconds: (seconds / duration).floor() * duration,
            duration,
            clock_revision: view.clock_revision,
            blend: view.blend,
            seed: view.seed,
            settings: view.settings,
            serial: self.serial,
        };
        let reversed = self.last_seconds.is_some_and(|previous| seconds < previous);
        self.last_seconds = Some(seconds);
        if reversed
            || self
                .published
                .as_ref()
                .is_some_and(|old| !old.same_world(&request))
        {
            self.clear(queue);
            self.serial += 1;
            request.serial = self.serial;
        }
        if let Some(built) = self.ready.take() {
            self.receive(queue, &request, seconds, built);
        }
        loop {
            match self.receiver.try_recv() {
                Ok(built) => self.receive(queue, &request, seconds, built),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.failed = true;
                    break;
                }
            }
        }
        self.morph = self.published.as_ref().map_or(0.0, |old| {
            ((seconds - old.start_seconds) / old.duration).clamp(0.0, 1.0) as f32
        });
        // Never draw a stale near-field after teleport/large view relocation.
        let local_fresh = self.published.as_ref().is_some_and(|old| {
            old.same_world(&request)
                && old.eye.distance(request.eye) <= local_cut_tolerance(view.radius)
        });
        self.draw_count = if local_fresh && self.count <= view.settings.instances {
            self.count
        } else {
            0
        };
        let lead =
            ((self.build_seconds + 0.2) * view.world_rate.max(1.0)).clamp(0.3, duration * 0.9);
        if self.published.as_ref().is_some_and(|old| {
            !old.needs_update(&request) && seconds >= old.start_seconds + old.duration - lead
        }) {
            request.start_seconds += duration;
        }
        if !self.failed
            && self.last_request.elapsed() >= Duration::from_millis(250)
            && self
                .requested
                .as_ref()
                .is_none_or(|old| old.needs_update(&request))
            && self.sender.try_send(request.clone()).is_ok()
        {
            self.requested = Some(request);
            self.last_request = Instant::now();
        }
    }

    fn receive(&mut self, queue: &wgpu::Queue, request: &Request, seconds: f64, built: Built) {
        if !built.request.same_world(request)
            || built.request.eye.distance(request.eye) > local_cut_tolerance(request.radius)
            || built.request.serial != self.serial
            || !built.request.settings.same_representation(request.settings)
        {
            return;
        }
        if seconds < built.request.start_seconds {
            self.ready = Some(built);
            return;
        }
        if self.published.as_ref().is_some_and(|old| {
            built.request.start_seconds < old.start_seconds
                || built.request.atlas.revision < old.atlas.revision
        }) {
            return;
        }
        assert!(built.instances.len() <= request.settings.instances as usize);
        if !built.instances.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&built.instances));
        }
        self.count = built.instances.len() as u32;
        self.lod_history = built.depths;
        self.far.upload(queue, &built.far);
        queue.write_texture(
            self.shadows.as_image_copy(),
            &built.shadows,
            shadow_layout(),
            self.shadows.size(),
        );
        self.region = built.request.atlas.region;
        self.local_region = built.region;
        self.origin = built.request.eye;
        if built.diagnostics.physical_seconds > 0.01 {
            self.build_seconds = built
                .diagnostics
                .physical_seconds
                .max(self.build_seconds * 0.8);
        }
        self.diagnostics = built.diagnostics;
        self.frame = Some(built.frame);
        self.published = Some(built.request);
        self.transition_start = Instant::now();
    }

    fn clear(&mut self, queue: &wgpu::Queue) {
        if self.published.is_some() {
            queue.write_texture(
                self.shadows.as_image_copy(),
                &vec![0; shadow::BYTES as usize],
                shadow_layout(),
                self.shadows.size(),
            );
        }
        self.count = 0;
        self.draw_count = 0;
        self.lod_history.clear();
        self.far.index_count = 0;
        self.frame = None;
        self.requested = None;
        self.published = None;
        self.ready = None;
        self.morph = 0.0;
    }
}

fn shadow_layout() -> wgpu::TexelCopyBufferLayout {
    wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(shadow::SIZE * 4),
        rows_per_image: Some(shadow::SIZE),
    }
}
