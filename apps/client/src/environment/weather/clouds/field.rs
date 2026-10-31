use super::{
    atmosphere::Atlas,
    coordinates::{Cell, CLUSTER_LEVEL},
    population, shapes,
    spatial::{SpatialIndex, EXTENT_M},
    CloudCluster, CloudLobe,
};
use crate::environment::weather::{
    transport::{self, FlowFrame},
    wind::EARTH_RADIUS,
};
use glam::DVec3;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

type CacheKey = (usize, Cell);
#[derive(Default)]
struct DescriptorCache {
    entries: HashMap<CacheKey, Arc<Vec<CloudCluster>>>,
    order: VecDeque<CacheKey>,
}

pub(crate) struct CloudFieldSnapshot {
    pub(crate) seed: u128,
    pub(crate) revision: u64,
    pub(crate) clock_revision: Option<u64>,
    pub(crate) seconds: f64,
    pub(crate) atmosphere: Arc<Atlas>,
    pub(crate) blend: f32,
    pub(super) flow: Arc<FlowFrame>,
    spatial: SpatialIndex,
    cache: Mutex<DescriptorCache>,
}

impl CloudFieldSnapshot {
    pub(crate) fn new(
        seed: u128,
        seconds: f64,
        clock_revision: Option<u64>,
        atmosphere: Arc<Atlas>,
        blend: f32,
    ) -> Self {
        let flow = transport::frame(seconds, seed);
        let spatial = SpatialIndex::new(&flow);
        Self {
            seed,
            revision: atmosphere.revision,
            clock_revision,
            seconds,
            atmosphere,
            blend,
            flow,
            spatial,
            cache: Mutex::new(DescriptorCache::default()),
        }
    }

    /// Physical spherical-cap query, independent of render selection and budgets.
    pub(crate) fn clusters(&self, up: DVec3, radius_m: f64) -> Vec<CloudCluster> {
        let radius = radius_m / EARTH_RADIUS;
        let mut result = Vec::new();
        for index in self.spatial.query(up, radius) {
            self.visit(
                index,
                self.spatial.roots[index].cell,
                up,
                radius,
                &mut result,
            );
        }
        result.sort_unstable_by_key(|c| c.id);
        result.dedup_by_key(|c| c.id);
        result
    }

    fn visit(
        &self,
        root: usize,
        cell: Cell,
        up: DVec3,
        radius: f64,
        result: &mut Vec<CloudCluster>,
    ) {
        if !self.spatial.roots[root].intersects(cell, up, radius) {
            return;
        }
        if cell.level < CLUSTER_LEVEL {
            for child in cell.children() {
                self.visit(root, child, up, radius, result);
            }
            return;
        }
        let key = (root, cell);
        let cached = self.cache.lock().unwrap().entries.get(&key).cloned();
        let clusters = cached.unwrap_or_else(|| {
            let clusters = Arc::new(
                (0..2)
                    .filter_map(|candidate| {
                        population::cluster(self, &self.spatial.roots[root], cell, candidate)
                    })
                    .collect::<Vec<_>>(),
            );
            let mut cache = self.cache.lock().unwrap();
            if !cache.entries.contains_key(&key) {
                while cache.order.len() >= 4096 {
                    if let Some(old) = cache.order.pop_front() {
                        cache.entries.remove(&old);
                    }
                }
                cache.entries.insert(key, clusters.clone());
                cache.order.push_back(key);
            }
            clusters
        });
        let cosine = (radius + EXTENT_M / EARTH_RADIUS)
            .min(std::f64::consts::PI)
            .cos();
        result.extend(clusters.iter().filter(|c| c.up.dot(up) >= cosine).cloned());
    }

    pub(crate) fn lobes(&self, up: DVec3, radius_m: f64) -> Vec<CloudLobe> {
        self.clusters(up, radius_m)
            .iter()
            .flat_map(|c| shapes::lobes(c, shapes::DETAIL_DEPTH))
            .collect()
    }

    pub(crate) fn bytes(&self) -> usize {
        self.spatial.bytes()
            + self
                .cache
                .lock()
                .unwrap()
                .entries
                .values()
                .map(|v| v.capacity() * std::mem::size_of::<CloudCluster>())
                .sum::<usize>()
    }
}
