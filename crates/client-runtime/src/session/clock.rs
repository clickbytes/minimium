use super::{ClientSession, QueueError, SessionState};
use crate::environment::{EnvironmentFrameInput, SynchronizationQuality};
use crate::{ClientTime, ClientTimeError};
use diagnostics::clock_diagnostics;
#[derive(Clone, Copy, Debug)]
struct OffsetSample {
    offset_twice_ms: i128,
    rtt_ms: u64,
    received_at_ms: u64,
}
use minimium_environment::{time::ClockSample, EnvironmentDescriptor};
use minimium_protocol::ClientMessage;
use std::collections::VecDeque;

mod diagnostics;
mod time_input;

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
        let result = self.queue_current(&ClientMessage::TimeProbe {
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

pub(super) struct ClientClock {
    diagnostic_logger: Option<minimium_diagnostics::DomainLogger>,
    epoch: ClientTime,
    now_ms: u64,
    session_nonce: Option<minimium_environment::time::SessionNonce>,
    probe_baseline_at_ms: Option<u64>,
    sample_received_at_ms: Option<u64>,
    last_poll_at_ms: Option<u64>,
    next_probe_at_ms: Option<u64>,
    next_sequence: u32,
    inflight: Option<InflightProbe>,
    valid_probe_count: u32,
    accepted_probe_count: u64,
    rejected_probe_count: u64,
    last_valid_probe_at_ms: Option<u64>,
    offsets: VecDeque<OffsetSample>,
    segment: Option<PresentationSegment>,
    last_diagnostic_at_ms: Option<u64>,
    frame: EnvironmentFrameInput,
}

impl ClientClock {
    pub(super) fn server_monotonic_ms(&self) -> Option<u64> {
        let offset = self.best_offset()?.offset_twice_ms / 2;
        u64::try_from(i128::from(self.local_monotonic_ms()).checked_add(offset)?).ok()
    }
    pub(super) fn new(
        epoch: ClientTime,
        diagnostic_logger: Option<minimium_diagnostics::DomainLogger>,
    ) -> Self {
        Self {
            diagnostic_logger,
            epoch,
            now_ms: 0,
            session_nonce: None,
            probe_baseline_at_ms: None,
            sample_received_at_ms: None,
            last_poll_at_ms: None,
            next_probe_at_ms: None,
            next_sequence: 1,
            inflight: None,
            valid_probe_count: 0,
            accepted_probe_count: 0,
            rejected_probe_count: 0,
            last_valid_probe_at_ms: None,
            offsets: VecDeque::with_capacity(MAX_OFFSET_SAMPLES),
            segment: None,
            last_diagnostic_at_ms: None,
            frame: EnvironmentFrameInput::unavailable(),
        }
    }

    pub(super) fn set_logger(
        &mut self,
        diagnostic_logger: Option<minimium_diagnostics::DomainLogger>,
    ) {
        self.diagnostic_logger = diagnostic_logger;
    }

    pub(super) fn log_event(&self, code: &'static str, fields: &[(&str, String)]) {
        crate::diagnostics::emit(
            self.diagnostic_logger.as_ref(),
            minimium_diagnostics::Level::Warn,
            code,
            fields
                .iter()
                .map(|(key, value)| crate::diagnostics::field(key, value)),
        );
    }

    pub(super) fn frame(&self) -> EnvironmentFrameInput {
        self.frame
    }

    pub(super) fn begin_session(&mut self, sample: ClockSample) {
        let epoch = self.epoch;
        let diagnostic_logger = self.diagnostic_logger.clone();
        let now_ms = self.now_ms;
        *self = Self::new(epoch, diagnostic_logger);
        self.now_ms = now_ms;
        self.session_nonce = Some(sample.session_nonce());
        self.note_sample(sample);
        self.probe_baseline_at_ms = Some(now_ms);
        self.sample_received_at_ms = Some(now_ms);
        self.last_poll_at_ms = Some(now_ms);
        self.next_probe_at_ms = Some(now_ms);
    }

    pub(super) fn note_sample(&mut self, sample: ClockSample) {
        let at = self.local_monotonic_ms();
        self.sample_received_at_ms = Some(at);
        let target = sample.boundary().simulation_ms();
        let segment = match self.segment {
            Some(old) if sample.rate_ppm().get() != 0 && !old.paused && target >= old.target => {
                if old.target == target {
                    return;
                }
                let span = sample
                    .server_monotonic_ms()
                    .get()
                    .saturating_sub(old.server_at)
                    .max(1);
                PresentationSegment {
                    start: old.at(at),
                    target,
                    span,
                    received: at,
                    server_at: sample.server_monotonic_ms().get(),
                    paused: false,
                }
            }
            _ => PresentationSegment {
                start: target,
                target,
                span: 1,
                received: at,
                server_at: sample.server_monotonic_ms().get(),
                paused: sample.rate_ppm().get() == 0,
            },
        };
        self.segment = Some(segment);
    }

    pub(super) fn session_nonce_matches(
        &self,
        nonce: minimium_environment::time::SessionNonce,
    ) -> bool {
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
            self.segment = None;
            self.diagnostic(now_ms, "poll_gap_reset");
        }
        self.last_poll_at_ms = Some(now_ms);
        if self
            .inflight
            .is_some_and(|probe| now_ms.saturating_sub(probe.sent_at_ms) >= PROBE_TIMEOUT_MS)
        {
            self.rejected_probe_count = self.rejected_probe_count.saturating_add(1);
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
            self.rejected_probe_count = self.rejected_probe_count.saturating_add(1);
            self.diagnostic(received_at_ms, "stale_reply_without_probe");
            return;
        };
        if sequence != inflight.sequence {
            self.rejected_probe_count = self.rejected_probe_count.saturating_add(1);
            self.diagnostic(received_at_ms, "stale_reply_sequence");
            return;
        }
        if echoed_client_send_ms != inflight.sent_at_ms {
            self.rejected_probe_count = self.rejected_probe_count.saturating_add(1);
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
        self.accepted_probe_count = self.accepted_probe_count.saturating_add(1);
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
            self.segment = None;
            return;
        }

        self.prune_offsets(now_ms);
        let clock_diagnostics = Some(clock_diagnostics(self, sample, now_ms));
        let quality = self.quality(sample, descriptor, now_ms);
        let Some(segment) = self.segment else {
            self.frame = EnvironmentFrameInput::unavailable();
            return;
        };
        let sim_ms = segment.at(now_ms);
        let lag = segment.target - sim_ms;
        self.frame = EnvironmentFrameInput {
            committed: Some(sample),
            estimated_world_utc_ms: Some(sample.world_utc_ms().get()),
            render_world_utc_ms: Some(
                sample
                    .world_utc_ms()
                    .get()
                    .saturating_sub(lag.try_into().unwrap_or(i64::MAX))
                    .max(minimium_environment::time::MIN_WORLD_TIME_MS),
            ),
            simulation_ms: Some(sim_ms),
            achieved_rate: if sim_ms < segment.target {
                (segment.target - segment.start) as f64 / segment.span as f64
            } else {
                0.0
            },
            descriptor: Some(descriptor),
            context,
            revision: Some(sample.boundary().calendar_epoch().get()),
            rate_ppm: Some(sample.rate_ppm().get()),
            quality,
            clock_diagnostics,
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
        self.rejected_probe_count = self.rejected_probe_count.saturating_add(1);
        self.inflight = None;
        self.next_probe_at_ms = Some(now_ms.saturating_add(self.probe_interval_ms()));
        self.diagnostic(now_ms, reason);
    }

    fn diagnostic(&mut self, now_ms: u64, reason: &'static str) {
        let log_allowed = self
            .last_diagnostic_at_ms
            .is_none_or(|previous| now_ms.saturating_sub(previous) >= 1_000);
        if log_allowed {
            self.log_event(
                "client_runtime.clock_probe_ignored",
                &[("reason", reason.to_owned())],
            );
            self.last_diagnostic_at_ms = Some(now_ms);
        }
    }

    fn reset_frame_only(&mut self) {
        self.frame = EnvironmentFrameInput::unavailable();
    }
}

#[derive(Clone, Copy)]
struct PresentationSegment {
    start: u64,
    target: u64,
    span: u64,
    received: u64,
    server_at: u64,
    paused: bool,
}

impl PresentationSegment {
    fn at(self, now: u64) -> u64 {
        let elapsed = now.saturating_sub(self.received).min(self.span);
        self.start
            + (u128::from(self.target - self.start) * u128::from(elapsed) / u128::from(self.span))
                as u64
    }
}
