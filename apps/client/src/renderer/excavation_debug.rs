use super::*;
use minimium_localization::{ToolMessage, ToolNotice, UiMessage};
use minimium_protocol::{ExcavationMessage, WorldContext};
use minimium_world::{
    items::{ActionRejection, ActionResult, ToolKind},
    terrain::{SurfaceHit, SurfaceTarget, TerrainRead},
};

#[derive(Default)]
pub(super) struct ExcavationDebug {
    context: Option<WorldContext>,
    last_result: Option<ActionResult>,
}

impl DesktopHost {
    pub(super) fn observe_excavation(
        &mut self,
        context: WorldContext,
        message: &ExcavationMessage,
    ) {
        if Some(context) != self.application.game_view().session_context() {
            return;
        }
        if self.excavation_debug.context != Some(context) {
            self.excavation_debug = ExcavationDebug {
                context: Some(context),
                last_result: None,
            };
        }
        let message = match message {
            ExcavationMessage::Inventory { inventory } => Some(ToolMessage::Heading {
                tool: self.tool_notice(tool_name(inventory.tool())),
            }),
            ExcavationMessage::ActionResult { result } => {
                self.excavation_debug.last_result = Some(*result);
                Some(self.tool_result_message(*result))
            }
            _ => None,
        };
        if let Some(message) = message {
            self.application.ui.console.push_ui_message(
                UiMessage::Tool(message),
                &self.localizer,
                self.application.settings.locale,
            );
        }
    }

    fn tool_notice(&self, notice: ToolNotice) -> String {
        self.localizer.format(
            self.application.settings.locale,
            &UiMessage::Tool(ToolMessage::Notice(notice)),
        )
    }

    fn tool_result_message(&self, result: ActionResult) -> ToolMessage {
        let (action, notice) = match result {
            ActionResult::Committed { action_id, .. } => (action_id, ToolNotice::Committed),
            ActionResult::Rejected { action_id, reason } => (action_id, rejection_notice(reason)),
        };
        ToolMessage::Result {
            action,
            outcome: self.tool_notice(notice),
        }
    }

    pub(super) fn tool_overlay(
        &self,
        view: &minimium_renderer::RenderViewFrame,
    ) -> Option<crate::console::ToolOverlay> {
        let application_view = self.application.desktop_render_view();
        let session = self.application.connection_view().session()?;
        let selected_definition = application_view.equipped_tool_profile;
        if !application_view.has_tools
            || self.application.overview().is_open()
            || self.application.ui.console.open
        {
            return None;
        }
        let ready = self.renderer.scene_visible
            && self.application.game_view().gameplay.sectors_ready
            && session.excavation_ready();
        let query_reach = selected_definition
            .map_or(application_view.max_tool_reach_m, |definition| {
                definition.reach_m()
            });
        let hit = ready.then(|| self.preview_tool_hit(query_reach)).flatten();
        let (notice, color) = if !ready {
            (ToolNotice::Synchronizing, [1.0, 0.75, 0.25])
        } else if session.tool_action_pending() {
            (ToolNotice::Pending, [1.0, 0.75, 0.25])
        } else if selected_definition.is_some() {
            match hit {
                None => (ToolNotice::NoTarget, [0.85, 0.65, 0.35]),
                Some(hit) if !matches!(hit.target, SurfaceTarget::Terrain(_)) => {
                    (ToolNotice::Obstructed, [1.0, 0.32, 0.26])
                }
                Some(hit)
                    if !selected_definition.is_some_and(|definition| {
                        definition.allowed_materials().contains(hit.material)
                    }) =>
                {
                    (ToolNotice::WrongMaterial, [1.0, 0.32, 0.26])
                }
                Some(_) => (ToolNotice::Ready, [0.25, 0.95, 0.45]),
            }
        } else {
            (ToolNotice::NotEquipped, [0.85, 0.65, 0.35])
        };
        let format = |message| {
            self.localizer
                .format(self.application.settings.locale, &UiMessage::Tool(message))
        };
        let target = hit.map_or_else(
            || self.tool_notice(ToolNotice::NoTarget),
            |hit| {
                let material = if matches!(hit.target, SurfaceTarget::Terrain(_)) {
                    use minimium_world::terrain::TerrainMaterial;
                    match hit.material {
                        TerrainMaterial::Soil => ToolNotice::Soil,
                        TerrainMaterial::Rock => ToolNotice::Rock,
                        TerrainMaterial::Empty => ToolNotice::Empty,
                    }
                } else {
                    ToolNotice::Obstacle
                };
                format(ToolMessage::Target {
                    material: self.tool_notice(material),
                    distance: format!("{:.2}", hit.distance_m),
                })
            },
        );
        Some(crate::console::ToolOverlay {
            heading: format(ToolMessage::Heading {
                tool: self.tool_notice(tool_name(application_view.equipped_tool)),
            }),
            target,
            status: self.tool_notice(notice),
            color,
            controls: self.tool_notice(ToolNotice::Controls),
            preview: self.tool_notice(ToolNotice::Preview),
            result: (self.excavation_debug.context
                == self.application.game_view().session_context())
            .then_some(self.excavation_debug.last_result)
            .flatten()
            .map(|result| format(self.tool_result_message(result))),
            outline: hit.map_or_else(Vec::new, |hit| {
                outline(
                    hit,
                    view,
                    selected_definition.map_or(application_view.max_tool_radius_m, |definition| {
                        definition.radius_m()
                    }),
                )
            }),
        })
    }

    fn preview_tool_hit(&self, reach_m: f64) -> Option<SurfaceHit> {
        let gameplay = &self.application.game_view().gameplay;
        let character = gameplay.prediction.character;
        let feet = gameplay
            .prediction
            .position_mm
            .map_or(character.position.map(f64::from), |p| {
                p.0.map(|v| v as f64 / 1000.0)
            });
        let geometry = CharacterGeometry::new(feet, character.gravity)?;
        let eye = geometry.eye_position_m();
        let direction =
            camera::camera_direction_from_geometry(geometry, self.input.yaw, self.input.pitch)
                .as_dvec3()
                .to_array();
        let blockers = gameplay
            .reconciliation
            .tracks()
            .iter()
            .flatten()
            .filter_map(|track| {
                let feet_m = track.displayed_position_mm.map(|v| v as f64 / 1000.0);
                CharacterGeometry::new(feet_m, character.gravity)
                    .map(|geometry| minimium_simulation::ToolRayCapsule { geometry })
            });
        let mut hit = gameplay.collision_index.tool_surface_hit(
            self.application.game_view().current_instance,
            eye,
            direction,
            reach_m,
            blockers,
        )?;
        if let SurfaceTarget::Terrain(_) = hit.target {
            let session = self.application.connection_view().session()?;
            let manifest = *self
                .application
                .game_view()
                .world_manifests
                .get(&self.application.game_view().current_instance)?;
            let inputs = session.terrain_inputs();
            let store = inputs.volumes.clone()?;
            let terrain = minimium_procgen::PlanetTerrain::for_manifest(manifest, inputs).ok()?;
            let base = terrain.volume_base()?;
            let behind = std::array::from_fn(|i| hit.position_m[i] - hit.normal[i] * 0.01);
            if let Some(sample) = store.view(&base).sample(behind) {
                hit.material = sample.material;
                hit.revision = sample.revision;
            }
        }
        Some(hit)
    }
}

fn tool_name(tool: Option<ToolKind>) -> ToolNotice {
    match tool {
        Some(ToolKind::Shovel) => ToolNotice::Shovel,
        Some(ToolKind::Pickaxe) => ToolNotice::Pickaxe,
        None => ToolNotice::Stowed,
    }
}

fn rejection_notice(reason: ActionRejection) -> ToolNotice {
    match reason {
        ActionRejection::NotEquipped => ToolNotice::NotEquipped,
        ActionRejection::InvalidIntent => ToolNotice::InvalidIntent,
        ActionRejection::OutOfReach => ToolNotice::NoTarget,
        ActionRejection::Obstructed => ToolNotice::Obstructed,
        ActionRejection::UnsafeGeometry => ToolNotice::UnsafeGeometry,
        ActionRejection::WrongMaterial => ToolNotice::WrongMaterial,
        ActionRejection::Busy => ToolNotice::Busy,
        ActionRejection::RateLimited => ToolNotice::RateLimited,
        ActionRejection::Stale => ToolNotice::Stale,
        ActionRejection::Capacity => ToolNotice::Capacity,
        ActionRejection::Storage => ToolNotice::Storage,
    }
}

fn outline(
    hit: SurfaceHit,
    view: &minimium_renderer::RenderViewFrame,
    radius_m: f64,
) -> Vec<Option<[f32; 2]>> {
    let center = glam::DVec3::from_array(hit.position_m);
    let mut normal = glam::DVec3::from_array(hit.normal).normalize_or_zero();
    if normal == glam::DVec3::ZERO {
        normal = (glam::DVec3::from_array(view.camera_eye_world_m) - center).normalize_or_zero();
    }
    let axis = if normal.x.abs() < 0.8 {
        glam::DVec3::X
    } else {
        glam::DVec3::Y
    };
    let tangent = normal.cross(axis).normalize_or_zero();
    let bitangent = normal.cross(tangent);
    let matrix = glam::Mat4::from_cols_array_2d(&view.view_projection);
    (0..=32)
        .map(|index| {
            let angle = index as f64 * std::f64::consts::TAU / 32.0;
            let point = center
                + normal * 0.01
                + (tangent * angle.cos() + bitangent * angle.sin()) * radius_m;
            let relative = (point - glam::DVec3::from_array(view.render_origin_m)).as_vec3();
            let clip = matrix * relative.extend(1.0);
            if !clip.is_finite() || clip.w <= 0.0 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            (ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z))
                .then_some([ndc.x, ndc.y])
        })
        .collect()
}
