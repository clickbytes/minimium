use super::{ClientSession, QueueError, SessionState};
use crate::environment::{EnvironmentFrameInput, SynchronizationQuality};
use estimate::{
    estimate_key, estimate_welcome_time, estimate_world_time, rounded_time, EstimateKey,
    OffsetSample,
};
use minimium_core::{environment::EnvironmentDescriptor, time::ClockSample};
use minimium_protocol::ClientMessage;
use smoothing::SmoothInput;
use std::{collections::VecDeque, time::Instant};

mod estimate;
mod smoothing;

impl ClientSession {
    pub(super) fn queue_due_time_probe(&mut self, now_ms: u64) {
        if self.state() != SessionState::Connected {
            return;
        }
        let Some(sequence) = self.clock.due_probe_sequence(now_ms) else {
            return;
        };
        // Capture t1 immediately before the probe is encoded and queued.
        let sent_at_ms = self.clock.local_monotonic_ms();
        let result = self.queue(&ClientMessage::TimeProbe {
            seq: sequence,
            client_send_monotonic_ms: sent_at_ms,
        });
        match result {
            Ok(()) => self.clock.probe_queued(sequence, sent_at_ms),
            Err(error) => self.clock.defer_probe_after_queue_error(now_ms, error),
        }
    }
}

const BOOTSTRAP_INTERVAL_MS: u64 = 1_000;
const NORMAL_INTERVAL_MS: u64 = 5_000;
const PROBE_TIMEOUT_MS: u64 = 2_000;
const MAX_FILTER_AGE_MS: u64 = 60_000;
const STALE_AGE_MS: u64 = 15_000;
const UNAVAILABLE_AGE_MS: u64 = 60_000;
const MAX_PROBE_RTT_MS: i128 = 2_000;
const MAX_OFFSET_SAMPLES: usize = 8;

#[derive(Clone, Copy, Debug)]
struct InflightProbe {
    sequence: u32,
    sent_at_ms: u64,
}

/// Owns all per-connection monotonic clock estimation and render-date smoothing.
pub(super) struct ClientClock {
    epoch: Instant,
    session_nonce: Option<minimium_core::time::SessionNonce>,
    probe_baseline_at_ms: Option<u64>,
    sample_received_at_ms: Option<u64>,
    last_poll_at_ms: Option<u64>,
    next_probe_at_ms: Option<u64>,
    next_sequence: u32,
    inflight: Option<InflightProbe>,
    valid_probe_count: u32,
    last_valid_probe_at_ms: Option<u64>,
    offsets: VecDeque<OffsetSample>,
    previous_input: Option<EstimateKey>,
    previous_revision: Option<u64>,
    previous_frame_at_ms: Option<u64>,
    previous_render_world_ms: Option<f64>,
    smoothing_bias_ms: f64,
    smoothing_started_at_ms: Option<u64>,
    resync_stale_until_ms: Option<u64>,
    last_diagnostic_at_ms: Option<u64>,
    frame: EnvironmentFrameInput,
}

impl ClientClock {
    pub(super) fn new(epoch: Instant) -> Self {
        Self {
            epoch,
            session_nonce: None,
            probe_baseline_at_ms: None,
            sample_received_at_ms: None,
            last_poll_at_ms: None,
            next_probe_at_ms: None,
            next_sequence: 1,
            inflight: None,
            valid_probe_count: 0,
            last_valid_probe_at_ms: None,
            offsets: VecDeque::with_capacity(MAX_OFFSET_SAMPLES),
            previous_input: None,
            previous_revision: None,
            previous_frame_at_ms: None,
            previous_render_world_ms: None,
            smoothing_bias_ms: 0.0,
            smoothing_started_at_ms: None,
            resync_stale_until_ms: None,
            last_diagnostic_at_ms: None,
            frame: EnvironmentFrameInput::unavailable(),
        }
    }

    pub(super) fn frame(&self) -> EnvironmentFrameInput {
        self.frame
    }

    pub(super) fn local_monotonic_ms(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    pub(super) fn begin_session(&mut self, sample: ClockSample) {
        let epoch = self.epoch;
        *self = Self::new(epoch);
        let now_ms = self.local_monotonic_ms();
        self.session_nonce = Some(sample.session_nonce());
        self.probe_baseline_at_ms = Some(now_ms);
        self.sample_received_at_ms = Some(now_ms);
        self.last_poll_at_ms = Some(now_ms);
        self.next_probe_at_ms = Some(now_ms);
    }

    pub(super) fn note_sample(&mut self, _sample: ClockSample) {
        self.sample_received_at_ms = Some(self.local_monotonic_ms());
    }

    pub(super) fn session_nonce_matches(&self, nonce: minimium_core::time::SessionNonce) -> bool {
        self.session_nonce == Some(nonce)
    }

    pub(super) fn observe_poll(&mut self, now_ms: u64) {
        if self
            .last_poll_at_ms
            .is_some_and(|last| now_ms.saturating_sub(last) > 10_000)
        {
            self.offsets.clear();
            self.inflight = None;
            self.valid_probe_count = 0;
            self.last_valid_probe_at_ms = None;
            self.probe_baseline_at_ms = Some(now_ms);
            self.next_probe_at_ms = Some(now_ms);
            self.reset_smoothing();
            self.diagnostic(now_ms, "poll_gap_reset");
        }
        self.last_poll_at_ms = Some(now_ms);
        if self
            .inflight
            .is_some_and(|probe| now_ms.saturating_sub(probe.sent_at_ms) >= PROBE_TIMEOUT_MS)
        {
            self.inflight = None;
            self.next_probe_at_ms = Some(now_ms.saturating_add(self.probe_interval_ms()));
            self.diagnostic(now_ms, "probe_timeout");
        }
        self.prune_offsets(now_ms);
    }

    pub(super) fn due_probe_sequence(&self, now_ms: u64) -> Option<u32> {
        if self.session_nonce.is_none()
            || self.inflight.is_some()
            || !self.next_probe_at_ms.is_some_and(|due| now_ms >= due)
        {
            return None;
        }
        Some(self.next_sequence)
    }

    pub(super) fn probe_queued(&mut self, sequence: u32, sent_at_ms: u64) {
        self.inflight = Some(InflightProbe {
            sequence,
            sent_at_ms,
        });
        self.next_sequence = sequence.wrapping_add(1);
        self.next_probe_at_ms = Some(sent_at_ms.saturating_add(self.probe_interval_ms()));
    }

    pub(super) fn defer_probe_after_queue_error(&mut self, now_ms: u64, error: QueueError) {
        if error == QueueError::BufferFull {
            // Retry later without occupying the one in-flight slot.
            self.next_probe_at_ms = Some(now_ms.saturating_add(100));
        }
    }

    pub(super) fn accept_probe_reply(
        &mut self,
        sequence: u32,
        echoed_client_send_ms: u64,
        server_receive_ms: u64,
        server_send_ms: u64,
        received_at_ms: u64,
    ) {
        let Some(inflight) = self.inflight else {
            self.diagnostic(received_at_ms, "stale_reply_without_probe");
            return;
        };
        if sequence != inflight.sequence {
            self.diagnostic(received_at_ms, "stale_reply_sequence");
            return;
        }
        if echoed_client_send_ms != inflight.sent_at_ms {
            self.diagnostic(received_at_ms, "probe_echo_mismatch");
            return;
        }
        if received_at_ms < inflight.sent_at_ms || server_send_ms < server_receive_ms {
            self.finish_invalid_probe(received_at_ms, "invalid_probe_timestamps");
            return;
        }

        let Some(round_trip_ms) = i128::from(received_at_ms)
            .checked_sub(i128::from(inflight.sent_at_ms))
            .and_then(|outbound| {
                i128::from(server_send_ms)
                    .checked_sub(i128::from(server_receive_ms))
                    .and_then(|processing| outbound.checked_sub(processing))
            })
        else {
            self.finish_invalid_probe(received_at_ms, "probe_rtt_overflow");
            return;
        };
        if !(0..=MAX_PROBE_RTT_MS).contains(&round_trip_ms) {
            self.finish_invalid_probe(received_at_ms, "probe_rtt_out_of_range");
            return;
        }
        let Some(offset_twice_ms) = i128::from(server_receive_ms)
            .checked_sub(i128::from(inflight.sent_at_ms))
            .and_then(|first| {
                i128::from(server_send_ms)
                    .checked_sub(i128::from(received_at_ms))
                    .and_then(|second| first.checked_add(second))
            })
        else {
            self.finish_invalid_probe(received_at_ms, "probe_offset_overflow");
            return;
        };
        self.inflight = None;
        self.valid_probe_count = self.valid_probe_count.saturating_add(1);
        self.last_valid_probe_at_ms = Some(received_at_ms);
        if self.valid_probe_count == 4 {
            self.next_probe_at_ms = Some(received_at_ms.saturating_add(NORMAL_INTERVAL_MS));
        }
        self.offsets.push_back(OffsetSample {
            offset_twice_ms,
            rtt_ms: round_trip_ms as u64,
            received_at_ms,
        });
        while self.offsets.len() > MAX_OFFSET_SAMPLES {
            self.offsets.pop_front();
        }
    }

    pub(super) fn cancel_inflight_for_transition(&mut self) {
        if self.inflight.take().is_some() {
            let now_ms = self.local_monotonic_ms();
            let retry_after_timeout = now_ms.saturating_add(PROBE_TIMEOUT_MS);
            self.next_probe_at_ms = Some(
                self.next_probe_at_ms
                    .unwrap_or(retry_after_timeout)
                    .max(retry_after_timeout),
            );
        }
    }

    pub(super) fn reset(&mut self) {
        *self = Self::new(Instant::now());
    }

    pub(super) fn update_frame(
        &mut self,
        sample: Option<ClockSample>,
        descriptor: Option<EnvironmentDescriptor>,
        context: Option<minimium_protocol::WorldContext>,
        now_ms: u64,
    ) {
        let (Some(sample), Some(descriptor), Some(nonce)) =
            (sample, descriptor, self.session_nonce)
        else {
            self.reset_frame_only();
            return;
        };
        if sample.session_nonce() != nonce || descriptor.clock_id() != sample.clock_id() {
            self.frame = EnvironmentFrameInput::unavailable();
            self.reset_smoothing();
            return;
        }

        self.prune_offsets(now_ms);
        let offset = self.best_offset();
        let mut quality = self.quality(sample, descriptor, now_ms);
        if quality == SynchronizationQuality::Unavailable {
            self.frame = EnvironmentFrameInput {
                estimated_world_utc_ms: None,
                render_world_utc_ms: None,
                descriptor: Some(descriptor),
                context,
                revision: Some(sample.revision().get()),
                rate_ppm: Some(sample.rate_ppm().get()),
                quality,
            };
            self.reset_smoothing();
            return;
        }
        let sample_received_at_ms = self.sample_received_at_ms.unwrap_or(now_ms);
        let (raw_world_ms, before_anchor) = if let Some(offset) = offset {
            match estimate_world_time(sample, now_ms, offset) {
                Some(estimate) => estimate,
                None => {
                    self.frame = EnvironmentFrameInput {
                        estimated_world_utc_ms: None,
                        render_world_utc_ms: None,
                        descriptor: Some(descriptor),
                        context,
                        revision: Some(sample.revision().get()),
                        rate_ppm: Some(sample.rate_ppm().get()),
                        quality: SynchronizationQuality::Unavailable,
                    };
                    self.reset_smoothing();
                    return;
                }
            }
        } else if let Some(estimate) = estimate_welcome_time(sample, now_ms, sample_received_at_ms)
        {
            (estimate, false)
        } else {
            self.frame = EnvironmentFrameInput {
                estimated_world_utc_ms: None,
                render_world_utc_ms: None,
                descriptor: Some(descriptor),
                context,
                revision: Some(sample.revision().get()),
                rate_ppm: Some(sample.rate_ppm().get()),
                quality: SynchronizationQuality::Unavailable,
            };
            self.reset_smoothing();
            return;
        };
        if before_anchor && quality != SynchronizationQuality::Stale {
            quality = SynchronizationQuality::Initializing;
        }

        let revision = sample.revision().get();
        let key = estimate_key(sample, offset);
        let changed = self.previous_input != Some(key);
        let is_new_revision = self
            .previous_revision
            .is_some_and(|previous| revision > previous);
        if self
            .previous_revision
            .is_some_and(|previous| revision < previous)
        {
            self.diagnostic(now_ms, "stale_clock_revision");
            self.frame = EnvironmentFrameInput::unavailable();
            self.reset_smoothing();
            return;
        }
        let dt_s = self.frame_delta_seconds(now_ms);
        let rendered = self.smooth_date(SmoothInput {
            raw_world_ms,
            rate_ppm: sample.rate_ppm().get(),
            revision,
            changed,
            is_new_revision,
            before_anchor,
            now_ms,
            dt_s: if changed && !is_new_revision {
                0.0
            } else {
                dt_s
            },
        });
        if self
            .resync_stale_until_ms
            .is_some_and(|until| now_ms < until)
        {
            quality = SynchronizationQuality::Stale;
        }
        self.previous_input = Some(key);
        self.previous_revision = Some(revision);
        self.previous_frame_at_ms = Some(now_ms);
        self.previous_render_world_ms = Some(rendered);
        let (Some(estimated_world_utc_ms), Some(render_world_utc_ms)) =
            (rounded_time(raw_world_ms), rounded_time(rendered))
        else {
            self.frame = EnvironmentFrameInput::unavailable();
            self.reset_smoothing();
            return;
        };
        self.frame = EnvironmentFrameInput {
            estimated_world_utc_ms: Some(estimated_world_utc_ms),
            render_world_utc_ms: Some(render_world_utc_ms),
            descriptor: Some(descriptor),
            context,
            revision: Some(revision),
            rate_ppm: Some(sample.rate_ppm().get()),
            quality,
        };
    }

    fn quality(
        &self,
        sample: ClockSample,
        descriptor: EnvironmentDescriptor,
        now_ms: u64,
    ) -> SynchronizationQuality {
        let sample_age = now_ms.saturating_sub(self.sample_received_at_ms.unwrap_or(now_ms));
        let probe_at = self.last_valid_probe_at_ms.or(self.probe_baseline_at_ms);
        let probe_age = now_ms.saturating_sub(probe_at.unwrap_or(now_ms));
        if descriptor.clock_id() != sample.clock_id()
            || sample_age > UNAVAILABLE_AGE_MS
            || probe_age > UNAVAILABLE_AGE_MS
            || self.last_valid_probe_at_ms.is_some() && self.best_offset().is_none()
        {
            return SynchronizationQuality::Unavailable;
        }
        if sample_age > STALE_AGE_MS || probe_age > STALE_AGE_MS {
            return SynchronizationQuality::Stale;
        }
        if self.valid_probe_count >= 4 && self.best_offset().is_some() {
            SynchronizationQuality::Synchronized
        } else {
            SynchronizationQuality::Initializing
        }
    }

    fn best_offset(&self) -> Option<OffsetSample> {
        self.offsets.iter().copied().min_by(|left, right| {
            left.rtt_ms
                .cmp(&right.rtt_ms)
                .then_with(|| right.received_at_ms.cmp(&left.received_at_ms))
        })
    }

    fn prune_offsets(&mut self, now_ms: u64) {
        self.offsets
            .retain(|sample| now_ms.saturating_sub(sample.received_at_ms) <= MAX_FILTER_AGE_MS);
    }

    fn probe_interval_ms(&self) -> u64 {
        if self.valid_probe_count < 4 {
            BOOTSTRAP_INTERVAL_MS
        } else {
            NORMAL_INTERVAL_MS
        }
    }

    fn finish_invalid_probe(&mut self, now_ms: u64, reason: &'static str) {
        self.inflight = None;
        self.next_probe_at_ms = Some(now_ms.saturating_add(self.probe_interval_ms()));
        self.diagnostic(now_ms, reason);
    }

    fn diagnostic(&mut self, now_ms: u64, reason: &'static str) {
        let log_allowed = self
            .last_diagnostic_at_ms
            .is_none_or(|previous| now_ms.saturating_sub(previous) >= 1_000);
        if log_allowed {
            eprintln!("clock_probe_ignored reason={reason}");
            self.last_diagnostic_at_ms = Some(now_ms);
        }
    }

    fn reset_frame_only(&mut self) {
        self.frame = EnvironmentFrameInput::unavailable();
        self.reset_smoothing();
    }
}
