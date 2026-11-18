use super::ClientClock;

const MAX_FRAME_DELTA_S: f64 = 0.25;
const SMOOTHING_TAU_S: f64 = 2.0;
const MAX_SMOOTHING_EPISODE_MS: u64 = 30_000;
const HALF_TIMESTAMP_TICK_MS: f64 = 0.5;

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
        // Both monotonic timestamps and published world UTC have millisecond
        // resolution. Half a probe timestamp tick spans 500 world ms at 1000x.
        let settle_world_ms = HALF_TIMESTAMP_TICK_MS * rate.max(1.0);
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
                    if error.abs() > settle_world_ms && self.smoothing_started_at_ms.is_none() {
                        self.smoothing_started_at_ms = Some(now_ms);
                    }
                } else {
                    self.smoothing_bias_ms = 0.0;
                    self.smoothing_started_at_ms = None;
                    self.resync_stale_until_ms = Some(now_ms.saturating_add(2_000));
                    self.log_event(
                        "client_runtime.clock_resync",
                        &[
                            ("revision", revision.to_string()),
                            ("reason", "large_correction".to_owned()),
                            ("rate_ppm", rate_ppm.to_string()),
                            ("error_world_ms", format!("{error:.1}")),
                        ],
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
        let residual = self.smoothing_bias_ms.abs();
        if residual > settle_world_ms
            && self
                .smoothing_started_at_ms
                .is_some_and(|started| now_ms.saturating_sub(started) >= MAX_SMOOTHING_EPISODE_MS)
        {
            let error = -self.smoothing_bias_ms;
            self.smoothing_bias_ms = 0.0;
            self.smoothing_started_at_ms = None;
            self.resync_stale_until_ms = Some(now_ms.saturating_add(2_000));
            self.log_event(
                "client_runtime.clock_resync",
                &[
                    ("revision", revision.to_string()),
                    ("reason", "smoothing_timeout".to_owned()),
                    ("rate_ppm", rate_ppm.to_string()),
                    ("error_world_ms", format!("{error:.1}")),
                ],
            );
            return raw_world_ms;
        }
        // Drain the unresolvable tail at the normal slew limit. Snapping the
        // entire tail would move a fast render clock on a zero-delta poll.
        let correction = if residual <= settle_world_ms {
            self.smoothing_bias_ms
        } else {
            self.smoothing_bias_ms * (1.0 - (-dt_s / SMOOTHING_TAU_S).exp())
        };
        let maximum_step = 0.1 * rate * 1_000.0 * dt_s;
        let applied = correction.abs().min(maximum_step) * correction.signum();
        self.smoothing_bias_ms -= applied;
        if self.smoothing_bias_ms.abs() <= settle_world_ms {
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
