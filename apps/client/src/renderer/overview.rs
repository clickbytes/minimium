use super::*;
use minimium_procgen::SectorGeometry;

impl Renderer {
    pub(super) fn is_earth_world(&self) -> bool {
        self.application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
            .is_some_and(|manifest| manifest.planet().is_some_and(|planet| planet.mode == 1))
    }

    pub(super) fn toggle_overview(&mut self) {
        self.application.input.clear_held();
        if self.application.overview().is_open() {
            self.application.overview_close();
            self.gpu.overview_active = false;
            self.gpu.overview_planet = false;
            self.last_render_position = [f32::INFINITY; 3];
            self.application.resolve_map_teleport();
            return;
        }
        let Some(manifest) = self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
            .copied()
        else {
            return;
        };
        let Some(map) = self
            .application
            .world_catalog
            .get(&self.application.game.current_instance)
        else {
            return;
        };
        let earth = manifest.planet().is_some_and(|planet| planet.mode == 1);
        self.gpu.overview_planet = manifest.topology() == WorldTopology::Planet;
        let vertices = if self.gpu.overview_planet {
            Vec::new()
        } else {
            sector_geometry(&SectorGeometry {
                boxes: map.boxes().to_vec(),
                triangles: map.triangles().to_vec(),
            })
        };
        self.last_render_position = [f32::INFINITY; 3];
        let marker_size = if manifest.topology() == WorldTopology::Planet {
            let radius = minimium_procgen::planet_surface_for(manifest).radius_mm() as f32 / 1000.0;
            [radius * self.marker_scale(earth); 3]
        } else {
            [1.0; 3]
        };
        let mut marker_vertices = Vec::new();
        cube(
            &mut marker_vertices,
            (glam::Vec3::from_array(if earth {
                let radial = glam::Vec3::from_array(
                    self.application.game.gameplay.prediction.character.position,
                )
                .normalize_or_zero();
                let radius =
                    minimium_procgen::planet_surface_for(manifest).radius_mm() as f32 / 1000.0;
                (radial * radius * (1.0 + self.marker_height())).to_array()
            } else {
                self.application.game.gameplay.prediction.character.position
            }) - glam::Vec3::from_array(self.gpu.render_origin))
            .to_array(),
            marker_size,
            [0.95, 0.2, 0.1],
        );
        self.gpu.install_overview(&vertices);
        self.gpu.install_overview_marker(&marker_vertices);
        self.metrics.frame_stats.set_gpu_bytes(
            self.gpu.fixed_gpu_bytes
                + u64::from(self.gpu.size.width.max(1))
                    * u64::from(self.gpu.size.height.max(1))
                    * 4,
        );
        let instance = self.application.game.current_instance;
        self.application.overview_open(instance);
        self.gpu.overview_active = true;
    }

    fn marker_scale(&self, earth: bool) -> f32 {
        if earth {
            ((self.application.overview().zoom() - 1.0) * 0.025).clamp(0.002, 0.035)
        } else {
            0.035
        }
    }

    pub(super) fn overview_uses_zoom(&self) -> bool {
        self.application.overview().is_open()
            && self
                .application
                .game
                .world_manifests
                .get(&self.application.game.current_instance)
                .is_some_and(|manifest| manifest.topology() == WorldTopology::Planet)
    }

    pub(super) fn set_overview_zoom(&mut self, zoom: f32) {
        self.application.overview_set_zoom(zoom);
        self.refresh_earth_marker();
        self.last_render_position = [f32::INFINITY; 3];
    }

    fn marker_height(&self) -> f32 {
        ((self.application.overview().zoom() - 1.0) * 0.25).min(0.045)
    }

    pub(super) fn refresh_earth_marker(&mut self) {
        if !self.application.overview().is_open() || !self.is_earth_world() {
            return;
        }
        let Some(manifest) = self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
        else {
            return;
        };
        let radius = minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0;
        let radial =
            glam::Vec3::from_array(self.application.game.gameplay.prediction.character.position)
                .normalize_or_zero();
        let mut vertices = Vec::new();
        cube(
            &mut vertices,
            (radial * radius * (1.0 + self.marker_height())
                - glam::Vec3::from_array(self.gpu.render_origin))
            .to_array(),
            [radius * self.marker_scale(true); 3],
            [0.95, 0.2, 0.1],
        );
        self.gpu.install_overview_marker(&vertices);
        self.metrics.frame_stats.set_gpu_bytes(
            self.gpu.fixed_gpu_bytes
                + u64::from(self.gpu.size.width.max(1))
                    * u64::from(self.gpu.size.height.max(1))
                    * 4,
        );
    }

    pub(super) fn arm_map_teleport(&mut self) {
        if self.application.map_teleport_pending() {
            self.application.ui.console.push_ui_error_message(
                minimium_localization::UiMessage::Notice(
                    minimium_localization::UiNotice::MapTeleportBusy,
                ),
                &self.localizer,
                self.application.settings.locale,
            );
            return;
        }
        let earth = self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
            .is_some_and(|manifest| manifest.planet().is_some_and(|planet| planet.mode == 1));
        if !earth || self.application.connection.session().is_none() || !self.gpu.scene_visible {
            self.application.ui.console.push_ui_error_message(
                minimium_localization::UiMessage::Notice(
                    minimium_localization::UiNotice::MapTeleportRequiresEarth,
                ),
                &self.localizer,
                self.application.settings.locale,
            );
            return;
        }
        if !self.application.overview().is_open() {
            self.toggle_overview();
        }
        self.application.ui.console.open = false;
        self.application.input.clear_held();
        if !self.application.arm_map_teleport() {
            return;
        }
        self.application.status = crate::application::ClientStatus::Status(
            minimium_localization::UiStatus::EarthClickLand,
        );
    }

    pub(super) fn click_map_teleport(&mut self) {
        if !self.application.map_teleport_armed() {
            return;
        }
        let Some((x, y)) = self.input.last_cursor else {
            return;
        };
        let Some(manifest) = self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
        else {
            return;
        };
        let radius = minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0;
        let (mut view, far) = self.overview_camera();
        let width = self.gpu.size.width.max(1) as f32;
        let height = self.gpu.size.height.max(1) as f32;
        if self.is_earth_world() {
            view = glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0)) * view;
        }
        let near = radius * ((self.application.overview().zoom() - 1.0) * 0.08).max(0.001);
        let projection =
            glam::Mat4::perspective_rh(self.fov_degrees.to_radians(), width / height, near, far);
        let inverse = (projection * view).inverse();
        let ndc = glam::Vec2::new(2.0 * x as f32 / width - 1.0, 1.0 - 2.0 * y as f32 / height);
        let near_point = inverse.project_point3(glam::Vec3::new(ndc.x, ndc.y, 0.0));
        let far_point = inverse.project_point3(glam::Vec3::new(ndc.x, ndc.y, 1.0));
        let ray = (far_point - near_point).normalize();
        let eye = view.inverse().transform_point3(glam::Vec3::ZERO);
        let b = eye.dot(ray);
        let discriminant = b * b - (eye.length_squared() - radius * radius);
        if discriminant < 0.0 {
            self.application.status = crate::application::ClientStatus::Status(
                minimium_localization::UiStatus::EarthClickSurface,
            );
            return;
        }
        let distance = -b - discriminant.sqrt();
        if distance <= 0.0 {
            return;
        }
        let direction = (eye + ray * distance).normalize().to_array();
        match self
            .application
            .connection
            .queue_world_input(minimium_protocol::ClientWorldInput::MapTeleport { direction })
        {
            Ok(_) => {
                self.application.request_map_teleport();
                self.application.status = crate::application::ClientStatus::Status(
                    minimium_localization::UiStatus::EarthMoving,
                );
                self.toggle_overview();
            }
            Err(error) => self.application.ui.console.push_client_text(
                crate::messages::ClientText::CouldNotQueueCommand(error.into()),
                true,
                &self.localizer,
                self.application.settings.locale,
            ),
        }
    }

    pub(super) fn terrain_view(&self) -> minimium_renderer::terrain_view::TerrainView {
        if self.application.overview().is_open() && self.gpu.overview_planet {
            let position = self
                .overview_camera()
                .0
                .inverse()
                .transform_point3(glam::Vec3::ZERO)
                .to_array();
            return minimium_renderer::terrain_view::TerrainView::overview(
                position,
                self.fov_degrees.to_radians(),
                [self.gpu.size.width, self.gpu.size.height],
            );
        }
        minimium_renderer::terrain_view::TerrainView::new(
            self.application.game.gameplay.prediction.character.position,
            self.fov_degrees.to_radians(),
            self.gpu.size.height,
        )
    }

    pub(super) fn overview_camera(&self) -> (glam::Mat4, f32) {
        let topology = self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
            .map(|manifest| manifest.topology());
        if topology == Some(WorldTopology::Planet) {
            let radial = glam::Vec3::from_array(
                self.application.game.gameplay.prediction.character.position,
            )
            .normalize_or_zero();
            let radial = if radial.length_squared() > 0.0 {
                radial
            } else {
                glam::Vec3::Y
            };
            let up = glam::Vec3::Y;
            let radius = self
                .application
                .game
                .world_manifests
                .get(&self.application.game.current_instance)
                .map(|manifest| {
                    minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0
                })
                .unwrap_or(10_000.0);
            let latitude = radial.y.asin() + self.application.overview().pitch();
            let longitude = radial.z.atan2(radial.x) + self.application.overview().yaw();
            let latitude = latitude.clamp(-1.48, 1.48);
            let direction = glam::Vec3::new(
                latitude.cos() * longitude.cos(),
                latitude.sin(),
                latitude.cos() * longitude.sin(),
            );
            (
                glam::Mat4::look_at_rh(
                    direction * radius * self.application.overview().zoom(),
                    glam::Vec3::ZERO,
                    up,
                ),
                // The physical Moon remains about 60 Earth radii away.
                radius * 80.0,
            )
        } else {
            let Some(map) = self
                .application
                .world_catalog
                .get(&self.application.game.current_instance)
            else {
                return (glam::Mat4::IDENTITY, 1000.0);
            };
            let mut min = glam::Vec3::splat(f32::INFINITY);
            let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
            for triangle in map.triangles() {
                for point in triangle.vertices {
                    let point = glam::Vec3::from_array(point);
                    min = min.min(point);
                    max = max.max(point);
                }
            }
            for map_box in map.boxes() {
                let center = glam::Vec3::from_array(map_box.center);
                let half = glam::Vec3::from_array(map_box.size) * 0.5;
                min = min.min(center - half);
                max = max.max(center + half);
            }
            let center = (min + max) * 0.5;
            let span = (max - min).max_element().max(10.0);
            let eye = center + glam::Vec3::Y * (span * 1.5 + 10.0);
            (
                glam::Mat4::look_at_rh(eye, center, glam::Vec3::Z),
                span * 4.0 + 100.0,
            )
        }
    }
}

impl GpuResources {
    fn install_overview(&mut self, vertices: &[Vertex]) {
        let (old_size, new_size) = self.overview_buffers.install_world(&self.device, vertices);
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(old_size)
            .saturating_add(new_size);
    }

    fn install_overview_marker(&mut self, vertices: &[Vertex]) {
        let (old_size, new_size) = self.overview_buffers.install_marker(&self.device, vertices);
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(old_size)
            .saturating_add(new_size);
    }
}
