pub(crate) use crate::environment::weather::clouds::settings::{
    CloudCommand, CloudDebug, CloudLighting, CloudMethod, CloudQuality,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EffectiveCloudSettings {
    pub(super) scale: f32,
    pub(super) instances: u32,
    pub(super) lod: f64,
    pub(super) bins: u32,
    pub(super) neighbors: u32,
    pub(super) aggregates: usize,
    pub(super) debug: CloudDebug,
    pub(super) method: CloudMethod,
    pub(super) normal_detail: f32,
}

pub(super) struct CloudRenderSettings {
    pub(super) quality: CloudQuality,
    scale: Option<u8>,
    instances: Option<u32>,
    lod: Option<u32>,
    lighting: Option<CloudLighting>,
    method: CloudMethod,
    debug: CloudDebug,
}

impl EffectiveCloudSettings {
    pub(super) fn same_representation(self, other: Self) -> bool {
        self.instances == other.instances
            && self.lod == other.lod
            && self.aggregates == other.aggregates
    }
}

impl Default for CloudRenderSettings {
    fn default() -> Self {
        Self {
            quality: CloudQuality::Sky,
            scale: None,
            instances: None,
            lod: None,
            lighting: None,
            method: CloudMethod::Volumetric,
            debug: CloudDebug::Off,
        }
    }
}

impl CloudRenderSettings {
    pub(super) fn apply(&mut self, command: CloudCommand) {
        match command {
            CloudCommand::Quality(quality) => {
                *self = Self {
                    quality,
                    ..Self::default()
                };
            }
            CloudCommand::Reset => *self = Self::default(),
            CloudCommand::Scale(value) => self.scale = Some(value),
            CloudCommand::Instances(value) => self.instances = Some(value),
            CloudCommand::Lod(value) => self.lod = Some(value),
            CloudCommand::Method(method) => self.method = method,
            CloudCommand::Lighting(value) => self.lighting = Some(value),
            CloudCommand::Debug(value) => self.debug = value,
            CloudCommand::Status => {}
        }
    }

    pub(super) fn effective(
        &self,
        standard_sky: bool,
        limits: &wgpu::Limits,
        precise: bool,
    ) -> EffectiveCloudSettings {
        let quality = match self.quality {
            CloudQuality::Sky if standard_sky => CloudQuality::Standard,
            CloudQuality::Sky => CloudQuality::Low,
            value => value,
        };
        let (scale, instances, lod, lighting, aggregates) = match quality {
            CloudQuality::Low => (0.5, 2048, 48.0, CloudLighting::Low, 128),
            CloudQuality::High => (0.75, 16384, 12.0, CloudLighting::High, 512),
            _ => (0.5, 8192, 24.0, CloudLighting::Standard, 256),
        };
        let lighting = self.lighting.unwrap_or(lighting);
        let capacity = (limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size)
            / std::mem::size_of::<super::cloud_instances::CloudInstanceGpu>() as u64)
            .min(32768) as u32;
        let mut effective = EffectiveCloudSettings {
            scale: self.scale.map_or(scale, |v| f32::from(v) / 4.0),
            instances: self.instances.unwrap_or(instances).min(capacity),
            lod: self.lod.map_or(lod, f64::from),
            bins: if lighting == CloudLighting::Low { 2 } else { 4 },
            neighbors: match lighting {
                CloudLighting::Low => 0,
                CloudLighting::Standard => 4,
                CloudLighting::High => 8,
            },
            aggregates,
            debug: self.debug,
            method: self.method,
            normal_detail: match lighting {
                CloudLighting::Low => 0.35,
                CloudLighting::Standard => 0.65,
                CloudLighting::High => 0.90,
            },
        };
        if !precise {
            effective.scale = effective.scale.min(0.5);
            effective.instances = effective.instances.min(2048);
            effective.lod = effective.lod.max(48.0);
            effective.bins = 2;
            effective.neighbors = 0;
            effective.aggregates = effective.aggregates.min(128);
        }
        effective
    }

    pub(super) fn description(&self, effective: EffectiveCloudSettings) -> String {
        format!("clouds requested {:?} method={:?} overrides(scale={:?}, instances={:?}, lod={:?}, lighting={:?}) | effective scale={} limit={} lod={}px bins={} neighbors={} aggregates={} detail={} debug={:?}",
            self.quality, effective.method, self.scale.map(|v| f32::from(v) / 4.0), self.instances, self.lod, self.lighting,
            effective.scale, effective.instances, effective.lod, effective.bins, effective.neighbors, effective.aggregates, effective.normal_detail, effective.debug)
    }
}
