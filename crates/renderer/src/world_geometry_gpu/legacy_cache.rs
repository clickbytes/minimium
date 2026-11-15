use super::*;

impl WorldGeometryGpu {
    pub fn cache_scene(
        &mut self,
        device: &wgpu::Device,
        instance: WorldInstanceId,
        generation: GenerationRevision,
        content_revision: WorldContentRevision,
        sectors: &[(SectorKey, u8, Arc<SectorGeometry>)],
        render_origin: [f32; 3],
        terrain: Option<&minimium_procgen::PlanetTerrain>,
        origin_generation: u64,
    ) -> GeometryCacheResult {
        let mut edges = crate::terrain_edges::EdgeConformance::default();
        let conformed = terrain.map(|t| edges.prepare(sectors.to_vec(), t));
        let sectors = conformed.as_deref().unwrap_or(sectors);
        let mut released_bytes = 0_u64;
        let version = (generation, content_revision);
        if self
            .publication
            .as_ref()
            .is_some_and(|publication| publication.stamp.instance == instance)
        {
            self.publication = None;
        }
        if self
            .versions
            .get(&instance)
            .is_some_and(|cached| *cached != version)
        {
            released_bytes += self.release_world(instance);
        }
        self.versions.insert(instance, version);
        let required: HashSet<_> = sectors.iter().map(|(key, lod, _)| (*key, *lod)).collect();
        let mut allocated_bytes = 0_u64;
        for (key, lod, geometry) in sectors {
            let catalog_key = (instance, *key, *lod);
            if self.catalog.contains_key(&catalog_key) {
                continue;
            }
            let vertices = geometry::sector_geometry_relative(geometry, render_origin);
            if let Some(terrain) = terrain {
                let surface = crate::terrain_geometry::vertices(geometry, terrain, render_origin);
                let ocean = edges.ocean(*key, *lod).map_or_else(Vec::new, |g| {
                    crate::prepared_sector::clipped_ocean_vertices(g, render_origin, None)
                });
                let object_count = geometry.boxes.len() * 36;
                let make_buffer = |label, bytes: &[u8]| {
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(label),
                        contents: if bytes.is_empty() { &[0; 4] } else { bytes },
                        usage: self.usage(),
                    })
                };
                let buffers = TerrainBuffers {
                    surface: make_buffer("terrain-vertices", bytemuck::cast_slice(&surface)),
                    surface_count: surface.len() as u32,
                    ocean: make_buffer("ocean-vertices", bytemuck::cast_slice(&ocean)),
                    ocean_count: ocean.len() as u32,
                    objects: make_buffer(
                        "terrain-objects",
                        bytemuck::cast_slice(&vertices[..object_count]),
                    ),
                    object_count: object_count as u32,
                };
                allocated_bytes += buffers.bytes();
                self.terrain.insert(catalog_key, buffers);
            }
            let vertices = if terrain.is_some() {
                crate::terrain_geometry::shadow_vertices(geometry, vertices)
            } else {
                vertices
            };
            let empty_vertices = [0.0_f32; 9];
            let contents = if vertices.is_empty() {
                bytemuck::cast_slice(&empty_vertices)
            } else {
                bytemuck::cast_slice(&vertices)
            };
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("world-sector-geometry"),
                contents,
                usage: self.usage(),
            });
            allocated_bytes = allocated_bytes.saturating_add(buffer.size());
            self.catalog
                .insert(catalog_key, (buffer, vertices.len() as u32));
            self.catalog_origin_generation
                .insert(catalog_key, origin_generation);
            if let Some(bounds) = crate::shadow_geometry::geometry_bounds(geometry) {
                self.bounds.insert(catalog_key, bounds);
            }
        }
        self.catalog.retain(|(owner, key, lod), (buffer, _)| {
            if *owner != instance || required.contains(&(*key, *lod)) {
                true
            } else {
                released_bytes = released_bytes.saturating_add(buffer.size());
                false
            }
        });
        self.catalog_origin_generation
            .retain(|key, _| self.catalog.contains_key(key));
        self.bounds
            .retain(|(owner, key, lod), _| *owner != instance || required.contains(&(*key, *lod)));
        self.terrain.retain(|(owner, key, lod), buffers| {
            let retain = *owner != instance || required.contains(&(*key, *lod));
            if !retain {
                released_bytes += buffers.bytes();
            }
            retain
        });
        let ready = sectors
            .iter()
            .all(|(key, lod, _)| self.catalog.contains_key(&(instance, *key, *lod)));
        GeometryCacheResult {
            ready,
            allocated_bytes,
            released_bytes,
        }
    }
}
