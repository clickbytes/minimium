//! Three asynchronous readbacks; lack of timestamp support is reported explicitly.
use std::{
    cell::Cell,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

struct Slot {
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    busy: Arc<AtomicBool>,
}

pub(in crate::renderer) struct CloudTiming {
    queries: Option<wgpu::QuerySet>,
    slots: Vec<Slot>,
    active: Cell<Option<usize>>,
    latest: Arc<AtomicU64>,
    period: f64,
}

impl CloudTiming {
    pub(super) const BYTES: u64 = 3 * (256 + 16) + 6 * 8;
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let supported = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let queries = supported.then(|| {
            device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("cloud GPU timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: 6,
            })
        });
        let slots = (0..if supported { 3 } else { 0 })
            .map(|_| Slot {
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("cloud timing resolve"),
                    size: 256,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("cloud timing readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                busy: Arc::new(AtomicBool::new(false)),
            })
            .collect();
        Self {
            queries,
            slots,
            active: Cell::new(None),
            latest: Arc::new(AtomicU64::new(0)),
            period: f64::from(queue.get_timestamp_period()),
        }
    }

    pub(in crate::renderer) fn begin(&self) {
        self.active.set(self.slots.iter().position(|slot| {
            slot.busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        }));
    }
    pub(in crate::renderer) fn writes(
        &self,
        start: bool,
    ) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let index = self.active.get()? as u32 * 2;
        Some(wgpu::RenderPassTimestampWrites {
            query_set: self.queries.as_ref()?,
            beginning_of_pass_write_index: start.then_some(index),
            end_of_pass_write_index: (!start).then_some(index + 1),
        })
    }
    pub(in crate::renderer) fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some(index) = self.active.take() else {
            return;
        };
        let slot = &self.slots[index];
        encoder.resolve_query_set(
            self.queries.as_ref().unwrap(),
            index as u32 * 2..index as u32 * 2 + 2,
            &slot.resolve,
            0,
        );
        encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.readback, 0, 16);
        let readback = slot.readback.clone();
        let busy = slot.busy.clone();
        let latest = self.latest.clone();
        let period = self.period;
        encoder.map_buffer_on_submit(&slot.readback, wgpu::MapMode::Read, 0..16, move |result| {
            if result.is_ok() {
                if let Ok(bytes) = readback.get_mapped_range(0..16) {
                    let start = u64::from_le_bytes(bytes[..8].try_into().unwrap());
                    let end = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
                    if end >= start {
                        latest.store(
                            ((end - start) as f64 * period / 1e6).to_bits(),
                            Ordering::Release,
                        );
                    }
                }
                readback.unmap();
            }
            busy.store(false, Ordering::Release);
        });
    }
    pub(in crate::renderer) fn status(&self) -> String {
        if self.queries.is_none() {
            return "GPU timing unavailable (timestamp queries unsupported)".into();
        }
        let value = self.latest.load(Ordering::Acquire);
        if value == 0 {
            "GPU timing awaiting asynchronous sample".into()
        } else {
            format!(
                "cloud GPU {:.3} ms (instances, aggregates, composite)",
                f64::from_bits(value)
            )
        }
    }
}
