//! Optional nonblocking GPU timing; no queue waits or synchronous readbacks.
use std::time::Instant;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};
pub struct AtmosphereTiming {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    period: f32,
    pending: Cell<bool>,
    encoded: Cell<bool>,
    ready: Arc<AtomicU8>,
    history: RefCell<VecDeque<[f64; 3]>>,
    sampled_lut: Cell<bool>,
    last_sample: Cell<Option<Instant>>,
    submitted_at: Cell<Option<Instant>>,
}
impl AtmosphereTiming {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("atmosphere timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: 6,
        });
        let make = |usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("atmosphere timestamp buffer"),
                size: 256,
                usage,
                mapped_at_creation: false,
            })
        };
        Some(Self {
            queries,
            resolve: make(wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC),
            readback: make(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ),
            period: queue.get_timestamp_period(),
            pending: Cell::new(false),
            encoded: Cell::new(false),
            ready: Arc::new(AtomicU8::new(0)),
            history: RefCell::new(VecDeque::with_capacity(120)),
            sampled_lut: Cell::new(true),
            last_sample: Cell::new(None),
            submitted_at: Cell::new(None),
        })
    }
    pub fn compute(&self, index: u32) -> wgpu::ComputePassTimestampWrites<'_> {
        wgpu::ComputePassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(index * 2),
            end_of_pass_write_index: Some(index * 2 + 1),
        }
    }
    pub fn render(&self) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(4),
            end_of_pass_write_index: Some(5),
        }
    }
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder, lut_updated: bool) {
        if self.pending.get() {
            return;
        }
        self.sampled_lut.set(lut_updated);
        encoder.resolve_query_set(&self.queries, 0..6, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, 48);
        self.encoded.set(true);
    }
    pub fn after_submit(&self) {
        if !self.encoded.replace(false) {
            return;
        }
        self.pending.set(true);
        self.submitted_at.set(Some(Instant::now()));
        let ready = self.ready.clone();
        self.readback
            .slice(0..48)
            .map_async(wgpu::MapMode::Read, move |result| {
                ready.store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
            });
    }
    pub fn poll(&self, device: &wgpu::Device) {
        if !self.pending.get() {
            return;
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let state = self.ready.swap(0, Ordering::AcqRel);
        if state == 0 {
            return;
        }
        if state == 1 {
            if let Ok(bytes) = self.readback.slice(0..48).get_mapped_range() {
                let times: Vec<_> = bytes
                    .chunks_exact(8)
                    .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                    .collect();
                let values = std::array::from_fn(|i| {
                    if i < 2 && !self.sampled_lut.get() {
                        0.0
                    } else {
                        times[i * 2 + 1].saturating_sub(times[i * 2]) as f64 * self.period as f64
                            / 1e6
                    }
                });
                let mut history = self.history.borrow_mut();
                if history.len() == 120 {
                    history.pop_front();
                }
                history.push_back(values);
                self.last_sample.set(self.submitted_at.take());
            }
            self.readback.unmap();
        }
        self.pending.set(false);
        self.submitted_at.set(None);
    }
    pub fn status(&self) -> String {
        let Some((median, p95, samples, _age_ms)) = self.summary() else {
            return "GPU timings: preparing asynchronous samples".into();
        };
        format!("atmosphere GPU median={median:.3}ms p95={p95:.3}ms samples={samples} (sky LUT + aerial LUT + composite)")
    }

    pub fn summary(&self) -> Option<(f64, f64, usize, u64)> {
        let history = self.history.borrow();
        if history.is_empty() {
            return None;
        }
        let mut totals: Vec<_> = history.iter().map(|s| s.iter().sum::<f64>()).collect();
        totals.sort_by(f64::total_cmp);
        Some((
            totals[totals.len() / 2],
            totals[((totals.len() - 1) as f64 * 0.95).ceil() as usize],
            totals.len(),
            self.last_sample
                .get()?
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        ))
    }
}
