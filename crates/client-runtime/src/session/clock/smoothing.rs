use super::ClientClock;

const MAX_FRAME_DELTA_S: f64 = 0.25;
const SMOOTHING_TAU_S: f64 = 2.0;
const MAX_SMOOTHING_EPISODE_MS: u64 = 30_000;

pub(super) struct SmoothInput {
    pub(super) raw_world_ms: f64,
    pub(super) rate_ppm: u64,
    pub(super) revision: u64,
    pub(super) changed: bool,
    pub(super) is_new_revision: bool,
    pub(super) before_anchor: bool,
    pub(super) now_ms: u64,
    pub(super) dt_s: f64,
}

impl ClientClock {
    pub(super) fn smooth_date(&mut self, input: SmoothInput) -> f64 {
        let SmoothInput {
            raw_world_ms,
            rate_ppm,
            revision,
            changed,
            is_new_revision,
            before_anchor,
            now_ms,
            dt_s,
        } = input;
        let rate = rate_ppm as f64 / 1_000_000.0;
        if is_new_revision || self.previous_revision.is_none() {
            self.smoothing_bias_ms = 0.0;
            self.smoothing_started_at_ms = None;
            self.resync_stale_until_ms = None;
            return raw_world_ms;
        }
        if before_anchor {
            return self.previous_render_world_ms.unwrap_or(raw_world_ms);
        }
        if changed {
            if let (Some(previous_render), Some(previous_at)) =
                (self.previous_render_world_ms, self.previous_frame_at_ms)
            {
                let elapsed_ms = now_ms.saturating_sub(previous_at) as f64;
                let predicted = previous_render + elapsed_ms * rate;
                let error = raw_world_ms - predicted;
                let threshold = 1_000.0_f64.max(2_000.0 * rate);
                if error.abs() <= threshold {
                    self.smoothing_bias_ms = predicted - raw_world_ms;
                    if self.smoothing_bias_ms.abs() > 0.5 && self.smoothing_started_at_ms.is_none()
                    {
                        self.smoothing_started_at_ms = Some(now_ms);
                    }
                } else {
                    self.smoothing_bias_ms = 0.0;
                    self.smoothing_started_at_ms = None;
                    self.resync_stale_until_ms = Some(now_ms.saturating_add(2_000));
                    eprintln!(
                        "clock_resync revision={revision} error_world_ms={:.1}",
                        error
                    );
                    return raw_world_ms;
                }
            }
        }
        if rate == 0.0 {
            self.smoothing_bias_ms = 0.0;
            self.smoothing_started_at_ms = None;
            return raw_world_ms;
        }
        if self
            .smoothing_started_at_ms
            .is_some_and(|started| now_ms.saturating_sub(started) >= MAX_SMOOTHING_EPISODE_MS)
            && self.smoothing_bias_ms.abs() > 0.5
        {
            self.smoothing_bias_ms = 0.0;
            self.smoothing_started_at_ms = None;
            self.resync_stale_until_ms = Some(now_ms.saturating_add(2_000));
            eprintln!("clock_resync revision={revision} reason=smoothing_timeout");
            return raw_world_ms;
        }
        let correction = self.smoothing_bias_ms * (1.0 - (-dt_s / SMOOTHING_TAU_S).exp());
        let maximum_step = 0.1 * rate * 1_000.0 * dt_s;
        let applied = correction.abs().min(maximum_step) * correction.signum();
        self.smoothing_bias_ms -= applied;
        if self.smoothing_bias_ms.abs() <= 0.5 {
            self.smoothing_bias_ms = 0.0;
            self.smoothing_started_at_ms = None;
        }
        raw_world_ms + self.smoothing_bias_ms
    }

    pub(super) fn frame_delta_seconds(&self, now_ms: u64) -> f64 {
        self.previous_frame_at_ms.map_or(0.0, |previous| {
            (now_ms.saturating_sub(previous) as f64 / 1_000.0).min(MAX_FRAME_DELTA_S)
        })
    }

    pub(super) fn reset_smoothing(&mut self) {
        self.previous_input = None;
        self.previous_revision = None;
        self.previous_frame_at_ms = None;
        self.previous_render_world_ms = None;
        self.smoothing_bias_ms = 0.0;
        self.smoothing_started_at_ms = None;
        self.resync_stale_until_ms = None;
    }
}
