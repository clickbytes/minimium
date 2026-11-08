use super::{
    GpuResources,
    shadow_map_cache::{MAX_MAP_VERTEX_BYTES, PreparedMapCasters},
};
use minimium_renderer::{ShadowSettings, geometry::Vertex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShadowUploadStatus {
    Ready,
    Pending,
    StaleContext,
    Failed,
}

impl GpuResources {
    pub(super) fn set_shadow_settings(
        &mut self,
        settings: ShadowSettings,
    ) -> Result<(), &'static str> {
        let previous_bytes = self.shadow_gpu.gpu_bytes();
        self.shadow_gpu.reconfigure(&self.device, settings)?;
        let current_bytes = self.shadow_gpu.gpu_bytes();
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(previous_bytes)
            .saturating_add(current_bytes);
        if !settings.enabled {
            self.release_shadow_map_casters();
            self.fixed_gpu_bytes = self
                .fixed_gpu_bytes
                .saturating_sub(self.shadow_gpu.release_player_casters());
            self.shadow_remote_complete = false;
            self.shadow_remote_stamp = None;
            self.shadow_remote_dirty = true;
        }
        Ok(())
    }

    pub(super) fn update_remote_shadow_vertices(
        &mut self,
        vertices: &[Vertex],
        stamp: Option<minimium_renderer::world_geometry_gpu::GeometryPublicationStamp>,
    ) {
        self.shadow_publish_ready = false;
        if !self.shadow_gpu.settings.enabled {
            self.shadow_gpu.clear_player_casters();
            self.shadow_remote_complete = false;
            self.shadow_remote_stamp = None;
            self.shadow_remote_dirty = true;
            return;
        }
        self.shadow_remote_complete = false;
        self.shadow_remote_stamp = None;
        self.shadow_remote_dirty = false;
        let size = (super::MAX_VISIBLE_PLAYERS * 36 * std::mem::size_of::<Vertex>()) as u64;
        match self
            .shadow_gpu
            .upload_player_casters(&self.device, &self.queue, vertices, size)
        {
            Ok(allocated_bytes) => {
                self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(allocated_bytes);
                self.shadow_remote_complete = stamp.is_some();
                self.shadow_remote_stamp = stamp;
            }
            Err(()) => {
                self.shadow_remote_complete = false;
                self.shadow_remote_stamp = None;
                self.shadow_remote_dirty = true;
            }
        }
    }

    pub(super) fn upload_prepared_shadow_map(
        &mut self,
        prepared: &PreparedMapCasters,
    ) -> ShadowUploadStatus {
        let key = prepared.key;
        let publication_matches = self
            .world_geometry
            .publication()
            .is_some_and(|publication| publication.stamp == key.stamp);
        let version_matches = self.world_geometry.versions().get(&key.instance)
            == Some(&(key.generation, key.content_revision));
        if !publication_matches || !version_matches {
            return ShadowUploadStatus::StaleContext;
        }
        if self.shadow_map_key == Some(key) {
            return ShadowUploadStatus::Ready;
        }
        if std::mem::size_of_val(prepared.vertices.as_ref()) as u64 > MAX_MAP_VERTEX_BYTES {
            return ShadowUploadStatus::Failed;
        }
        match self.shadow_gpu.upload_map_casters(
            &self.device,
            &self.queue,
            prepared.vertices.as_ref(),
            MAX_MAP_VERTEX_BYTES,
        ) {
            Ok(allocated_bytes) => {
                self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(allocated_bytes);
                self.shadow_map_key = Some(key);
                ShadowUploadStatus::Ready
            }
            Err(()) => ShadowUploadStatus::Failed,
        }
    }

    pub(super) fn release_shadow_map_casters(&mut self) {
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(self.shadow_gpu.release_map_casters());
        self.shadow_map_key = None;
        self.shadow_publish_ready = false;
        self.shadow_upload_status = ShadowUploadStatus::Pending;
    }
}
