//! Read-only presentation diagnostics. No sampling or GPU readback on this path.
use super::DesktopHost;

impl DesktopHost {
    pub(in crate::renderer) fn terrain_report(
        &self,
    ) -> Option<minimium_commands::operation::CommandValue> {
        use super::render_queries::record;
        use minimium_commands::operation::CommandValue as V;
        let publication = self.renderer.world_geometry.publication()?;
        let instance = self.application.core_view().world_instance;
        let context = self.render_environment_frame.context?;
        if publication.stamp.instance != instance
            || publication.stamp.content_revision != context.content_revision()
        {
            return None;
        }
        let surfaces = self.renderer.world_geometry.terrain();
        let mut surface_vertices = 0u64;
        let mut ocean_vertices = 0u64;
        let mut bytes = 0u64;
        for (key, level) in &publication.keys {
            if let Some(buffer) = surfaces.get(&(instance, *key, *level)) {
                surface_vertices += u64::from(buffer.surface_count);
                ocean_vertices += u64::from(buffer.ocean_count);
                bytes += buffer.surface.size() + buffer.ocean.size() + buffer.objects.size();
            }
        }
        Some(record([
            ("patches", V::U64(publication.keys.len() as u64)),
            (
                "geometry_revision",
                V::U64(publication.stamp.geometry_revision),
            ),
            (
                "patch_budget",
                V::U64(self.terrain_view().patch_budget() as u64),
            ),
            ("surface_triangles", V::U64(surface_vertices / 3)),
            ("ocean_triangles", V::U64(ocean_vertices / 3)),
            ("resident_geometry_bytes", V::U64(bytes)),
        ]))
    }
}
