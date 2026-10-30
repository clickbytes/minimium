use super::*;
use minimium_procgen::{materialize_planet_overview, PlanetTerrain, SectorGeometry};
use wgpu::util::DeviceExt;

impl Renderer {
    pub(super) fn is_earth_world(&self) -> bool {
        self.world_manifests
            .get(&self.current_instance)
            .is_some_and(|manifest| manifest.planet().is_some_and(|planet| planet.mode == 1))
    }

    pub(super) fn toggle_overview(&mut self) {
        if self.overview_open {
            self.overview_open = false;
            self.gpu.overview_active = false;
            self.gpu.overview_globe = false;
            self.overview_dragging = false;
            self.overview_slider_dragging = false;
            self.map_teleport_armed = false;
            return;
        }
        let Some(manifest) = self.world_manifests.get(&self.current_instance).copied() else {
            return;
        };
        let Some(map) = self.world_catalog.get(&self.current_instance) else {
            return;
        };
        let earth = manifest.planet().is_some_and(|planet| planet.mode == 1);
        self.gpu.overview_globe = earth;
        if earth && self.gpu.globe.is_none() {
            self.connection_status = "Earth: przygotowanie globu".into();
        }
        let mut vertices = if earth {
            if self.gpu.globe.is_none() && self.globe_loading.is_none() {
                let planet = manifest.planet().expect("Earth definition exists");
                let (width, height) = self.gpu.texture_dimensions();
                let (sender, receiver) = std::sync::mpsc::sync_channel(1);
                std::thread::spawn(move || {
                    let pixels = super::globe::pixels(
                        planet.height_scale_permille,
                        planet.sea_level_mm,
                        width,
                        height,
                    );
                    let _ = sender.send(pixels);
                });
                self.globe_loading = Some(receiver);
            }
            Vec::new()
        } else if manifest.topology() == WorldTopology::Planet {
            let surface = minimium_procgen::planet_surface_for(manifest);
            let Some(mut terrain) = PlanetTerrain::new(manifest.generation(), surface) else {
                return;
            };
            if let Some(definition) = manifest.planet() {
                terrain = terrain.with_definition(definition);
            }
            let geometry = SectorGeometry {
                boxes: Vec::new(),
                triangles: materialize_planet_overview(terrain),
            };
            sector_geometry(&geometry)
        } else {
            let geometry = SectorGeometry {
                boxes: map.boxes().to_vec(),
                triangles: map.triangles().to_vec(),
            };
            sector_geometry(&geometry)
        };
        let marker_size = if manifest.topology() == WorldTopology::Planet {
            let radius = minimium_procgen::planet_surface_for(manifest).radius_mm() as f32 / 1000.0;
            [radius * self.marker_scale(earth); 3]
        } else {
            [1.0; 3]
        };
        cube(
            &mut vertices,
            if earth {
                let radial =
                    glam::Vec3::from_array(self.game.character.position).normalize_or_zero();
                let radius =
                    minimium_procgen::planet_surface_for(manifest).radius_mm() as f32 / 1000.0;
                (radial * radius * (1.0 + self.marker_height())).to_array()
            } else {
                self.game.character.position
            },
            marker_size,
            [0.95, 0.2, 0.1],
        );
        self.gpu.install_overview(&vertices);
        self.metrics.frame_stats.set_gpu_bytes(
            self.gpu.fixed_gpu_bytes
                + u64::from(self.gpu.size.width.max(1))
                    * u64::from(self.gpu.size.height.max(1))
                    * 4,
        );
        self.overview_instance = Some(self.current_instance);
        self.overview_open = true;
        self.gpu.overview_active = true;
    }

    fn marker_scale(&self, earth: bool) -> f32 {
        if earth {
            ((self.overview_zoom - 1.0) * 0.025).clamp(0.002, 0.035)
        } else {
            0.035
        }
    }

    pub(super) fn overview_uses_zoom(&self) -> bool {
        self.overview_open
            && self
                .world_manifests
                .get(&self.current_instance)
                .is_some_and(|manifest| manifest.topology() == WorldTopology::Planet)
    }

    pub(super) fn set_overview_zoom(&mut self, zoom: f32) {
        self.overview_zoom = zoom.clamp(crate::console::MIN_ZOOM, crate::console::MAX_ZOOM);
        self.refresh_earth_marker();
        self.refresh_earth_globe_lod();
    }

    fn marker_height(&self) -> f32 {
        ((self.overview_zoom - 1.0) * 0.25).min(0.045)
    }

    pub(super) fn refresh_earth_marker(&mut self) {
        if !self.overview_open || !self.is_earth_world() {
            return;
        }
        let Some(manifest) = self.world_manifests.get(&self.current_instance) else {
            return;
        };
        let radius = minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0;
        let radial = glam::Vec3::from_array(self.game.character.position).normalize_or_zero();
        let mut vertices = Vec::new();
        cube(
            &mut vertices,
            (radial * radius * (1.0 + self.marker_height())).to_array(),
            [radius * self.marker_scale(true); 3],
            [0.95, 0.2, 0.1],
        );
        self.gpu.install_overview(&vertices);
    }

    pub(super) fn refresh_earth_globe_lod(&mut self) {
        if !self.overview_open || !self.is_earth_world() {
            return;
        }
        let Some(manifest) = self.world_manifests.get(&self.current_instance) else {
            return;
        };
        let radius = minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0;
        self.gpu.refresh_globe_lod(radius, self.overview_zoom);
        self.metrics.frame_stats.set_gpu_bytes(
            self.gpu.fixed_gpu_bytes
                + u64::from(self.gpu.size.width.max(1))
                    * u64::from(self.gpu.size.height.max(1))
                    * 4,
        );
    }

    pub(super) fn arm_map_teleport(&mut self) {
        if self.map_teleport_pending {
            self.ui
                .console
                .push_error("map_telepo: wait for the previous destination");
            return;
        }
        let earth = self
            .world_manifests
            .get(&self.current_instance)
            .is_some_and(|manifest| manifest.planet().is_some_and(|planet| planet.mode == 1));
        if !earth || self.connection.session().is_none() || !self.gpu.scene_visible {
            self.ui
                .console
                .push_error("map_telepo requires a connected Earth world");
            return;
        }
        if !self.overview_open {
            self.toggle_overview();
        }
        self.ui.console.open = false;
        self.map_teleport_armed = true;
        self.connection_status = "Earth: kliknij ląd na globie (Esc anuluje)".into();
    }

    pub(super) fn click_map_teleport(&mut self) {
        if !self.map_teleport_armed {
            return;
        }
        if self.gpu.globe.is_none() {
            self.connection_status = "Earth: poczekaj na teksturę globu".into();
            return;
        }
        let Some((x, y)) = self.input.last_cursor else {
            return;
        };
        let Some(manifest) = self.world_manifests.get(&self.current_instance) else {
            return;
        };
        let radius = minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0;
        let (mut view, far) = self.overview_camera();
        let width = self.gpu.size.width.max(1) as f32;
        let height = self.gpu.size.height.max(1) as f32;
        if self.is_earth_world() {
            view = glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0)) * view;
        }
        let near = radius * ((self.overview_zoom - 1.0) * 0.08).max(0.001);
        let projection = glam::Mat4::perspective_rh(70_f32.to_radians(), width / height, near, far);
        let inverse = (projection * view).inverse();
        let ndc = glam::Vec2::new(2.0 * x as f32 / width - 1.0, 1.0 - 2.0 * y as f32 / height);
        let near_point = inverse.project_point3(glam::Vec3::new(ndc.x, ndc.y, 0.0));
        let far_point = inverse.project_point3(glam::Vec3::new(ndc.x, ndc.y, 1.0));
        let ray = (far_point - near_point).normalize();
        let eye = view.inverse().transform_point3(glam::Vec3::ZERO);
        let b = eye.dot(ray);
        let discriminant = b * b - (eye.length_squared() - radius * radius);
        if discriminant < 0.0 {
            self.connection_status = "Earth: kliknij powierzchnię globu".into();
            return;
        }
        let distance = -b - discriminant.sqrt();
        if distance <= 0.0 {
            return;
        }
        let direction = (eye + ray * distance).normalize().to_array();
        match self
            .connection
            .queue(&ClientMessage::MapTeleport { direction })
        {
            Ok(()) => {
                self.map_teleport_pending = true;
                self.connection_status = "Earth: przenoszenie...".into();
                self.toggle_overview();
            }
            Err(error) => self.ui.console.push_error(format!("map_telepo: {error:?}")),
        }
    }

    pub(super) fn poll_globe(&mut self) {
        let Some(receiver) = &self.globe_loading else {
            return;
        };
        if let Ok(pixels) = receiver.try_recv() {
            if let Some(manifest) = self
                .world_manifests
                .get(&self.current_instance)
                .filter(|manifest| manifest.planet().is_some_and(|planet| planet.mode == 1))
            {
                let radius =
                    minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0;
                let (width, height) = self.gpu.texture_dimensions();
                self.gpu
                    .install_globe(&pixels, width, height, radius, self.overview_zoom);
                if self.settings.require_earth && self.gpu.scene_visible {
                    self.connection_status = "Earth: połączono".into();
                }
            }
            self.globe_loading = None;
        }
    }

    pub(super) fn overview_camera(&self) -> (glam::Mat4, f32) {
        let topology = self
            .world_manifests
            .get(&self.current_instance)
            .map(|manifest| manifest.topology());
        if topology == Some(WorldTopology::Planet) {
            let radial = glam::Vec3::from_array(self.game.character.position).normalize_or_zero();
            let radial = if radial.length_squared() > 0.0 {
                radial
            } else {
                glam::Vec3::Y
            };
            let up = glam::Vec3::Y;
            let radius = self
                .world_manifests
                .get(&self.current_instance)
                .map(|manifest| {
                    minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0
                })
                .unwrap_or(10_000.0);
            let latitude = radial.y.asin() + self.overview_pitch;
            let longitude = radial.z.atan2(radial.x) + self.overview_yaw;
            let latitude = latitude.clamp(-1.48, 1.48);
            let direction = glam::Vec3::new(
                latitude.cos() * longitude.cos(),
                latitude.sin(),
                latitude.cos() * longitude.sin(),
            );
            (
                glam::Mat4::look_at_rh(
                    direction * radius * self.overview_zoom,
                    glam::Vec3::ZERO,
                    up,
                ),
                radius * (self.overview_zoom + 2.0),
            )
        } else {
            let Some(map) = self.world_catalog.get(&self.current_instance) else {
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
        if let Some((old, _)) = self.overview_buffer.take() {
            self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(old.size());
        }
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("world-overview-geometry"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(buffer.size());
        self.overview_buffer = Some((buffer, vertices.len() as u32));
    }
}
