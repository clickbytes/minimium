//! Bounded local water-presentation capture. Values are read from published owners.
use super::super::render_queries::record;
use super::super::{
    sector_scene::{SceneInterestOwner, SceneInterestPresentation},
    DesktopHost,
};
use minimium_commands::operation::{CommandValue as V, FiniteValue};
use minimium_procgen::SectorContentVersion;
use minimium_world::world::WorldInstanceId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::renderer) struct WaterSceneKey {
    pub owner: SceneInterestOwner,
    pub instance: WorldInstanceId,
    pub origin_generation: u64,
    pub terrain_version: SectorContentVersion,
    pub fluid_revision: u64,
    pub water_level_hash: u64,
    pub source_revision: u64,
    pub catalog_digest: [u8; 32],
    pub center_m: [f64; 3],
    pub view_position_m: [f32; 3],
    pub interface_cells: usize,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::renderer) struct WaterInstalledSummary {
    pub key: WaterSceneKey,
    pub vertices: usize,
}

fn digest(bytes: [u8; 32]) -> V {
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    V::Text(hex)
}

fn version(v: SectorContentVersion) -> V {
    record([
        ("edits", V::U64(v.edits)),
        ("remnants", V::U64(v.remnants)),
        ("volume", V::U64(v.volume)),
    ])
}

fn xyz_i64(values: [i32; 3]) -> V {
    record([
        ("x", V::I64(i64::from(values[0]))),
        ("y", V::I64(i64::from(values[1]))),
        ("z", V::I64(i64::from(values[2]))),
    ])
}

fn xyz_m(values: [f64; 3]) -> V {
    let finite = |value| FiniteValue::new(value).map_or(V::Unit, V::Finite);
    record([
        ("x", finite(values[0])),
        ("y", finite(values[1])),
        ("z", finite(values[2])),
    ])
}

fn context(owner: SceneInterestOwner) -> V {
    owner.context().map_or(V::Unit, |context| {
        record([
            ("instance_id", V::U128(context.instance().get())),
            ("membership_epoch", V::U64(context.membership_epoch().get())),
            ("content_revision", V::U64(context.content_revision().get())),
        ])
    })
}

fn key(k: WaterSceneKey) -> V {
    record([
        ("instance_id", V::U128(k.instance.get())),
        ("context", context(k.owner)),
        ("origin_generation", V::U64(k.origin_generation)),
        ("terrain_version", version(k.terrain_version)),
        ("fluid_revision", V::U64(k.fluid_revision)),
        ("water_level_hash", V::U64(k.water_level_hash)),
        ("source_revision", V::U64(k.source_revision)),
        ("catalog_digest", digest(k.catalog_digest)),
        ("center_m", xyz_m(k.center_m)),
        ("view_position_m", xyz_m(k.view_position_m.map(f64::from))),
        ("interface_cells", V::U64(k.interface_cells as u64)),
        ("radius_m", V::U64(96)),
        ("interface_cell_limit", V::U64(4096)),
    ])
}

fn received(r: minimium_client_runtime::WaterReplicaSummary) -> V {
    let region = r.region.map_or(V::Unit, |(min, max)| {
        record([
            ("min_cell", xyz_i64(min)),
            ("max_exclusive_cell", xyz_i64(max)),
        ])
    });
    record([
        ("instance_id", V::U128(r.context.instance().get())),
        ("context", context(SceneInterestOwner::Active(r.context))),
        ("sync_id", r.sync_id.map_or(V::Unit, V::U64)),
        ("fluid_revision", V::U64(r.fluid_revision)),
        (
            "terrain_version",
            r.terrain_version.map_or(V::Unit, version),
        ),
        ("catalog_digest", r.catalog_digest.map_or(V::Unit, digest)),
        ("region", region),
    ])
}

impl DesktopHost {
    pub(in crate::renderer) fn water_render_report(&self) -> V {
        let capture = std::time::Instant::now();
        let capture_id = self
            .renderer
            .water_capture_sequence
            .try_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |id| id.checked_add(1),
            )
            .unwrap_or(u64::MAX);
        let session = self
            .application
            .connection_view()
            .session()
            .filter(|session| session.state() == minimium_client_runtime::SessionState::Connected);
        let active = session
            .and_then(minimium_client_runtime::ClientSession::active_world_context)
            .filter(|context| self.application.core_view().context == Some(*context));
        let manifest = self.application.core_view().manifest;
        let replica =
            session.and_then(minimium_client_runtime::ClientSession::water_replica_summary);
        let requested = active.and_then(|context| {
            self.sector_cache
                .presentation
                .worker
                .water_request_summary(SceneInterestOwner::Active(context))
        });
        let installed = self.renderer.water_installed;
        let publication = self.renderer.world_geometry.publication();
        let installed_current = installed.is_some_and(|s| {
            active == s.key.owner.context()
                && self.renderer.scene_visible
                && publication.is_some_and(|p| {
                    p.stamp.instance == s.key.instance
                        && p.stamp.origin_generation == s.key.origin_generation
                        && p.stage == s.key.owner.geometry_stage()
                })
        });
        let state = if manifest.is_some_and(|m| m.planet().is_none()) {
            "not_applicable"
        } else if active.is_none() && installed.is_some() {
            "stale"
        } else if active.is_none() {
            "no_snapshot"
        } else if installed.is_some() && !installed_current {
            "stale"
        } else if replica.is_none() && self.application.is_loading() {
            "loading"
        } else if replica.is_none() {
            "no_snapshot"
        } else if requested.as_ref().is_some_and(|(_, failed)| *failed) {
            "failed"
        } else if let (Some((requested, _)), Some(installed)) = (requested, installed) {
            if installed_current && requested == installed.key {
                "available"
            } else {
                "loading"
            }
        } else {
            "loading"
        };
        let requested_value = requested.map_or(V::Unit, |(key, failed)| {
            record([("key", key_value(key)), ("failed", V::Bool(failed))])
        });
        let installed_value = installed.map_or(V::Unit, |summary| {
            record([
                ("key", key_value(summary.key)),
                ("vertices", V::U64(summary.vertices as u64)),
                ("triangles", V::U64((summary.vertices / 3) as u64)),
                ("active", V::Bool(installed_current)),
            ])
        });
        record([
            ("source", V::Enum("local_renderer".into())),
            ("capture_id", V::U64(capture_id)),
            (
                "capture_monotonic_ms",
                V::U64(
                    capture
                        .duration_since(self.renderer.water_diagnostics_started)
                        .as_millis() as u64,
                ),
            ),
            ("state", V::Enum(state.into())),
            ("received", replica.map_or(V::Unit, received)),
            ("requested", requested_value),
            ("installed", installed_value),
        ])
    }
}

fn key_value(k: WaterSceneKey) -> V {
    key(k)
}
