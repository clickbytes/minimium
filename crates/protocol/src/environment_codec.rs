use crate::{models::ProtocolError, reading::Reader};
use minimium_environment::{
    time::{
        ClockId, ClockMode, ClockRevision, ClockSample, MonotonicMs, OfflinePolicy, SessionNonce,
        TimeScalePpm, WorldTimeMs,
    },
    EnvironmentDescriptor,
};

pub(crate) const CLOCK_SAMPLE_BYTES: usize = 83;
pub(crate) const ENVIRONMENT_DESCRIPTOR_BYTES: usize = 49;
pub(crate) const ENVIRONMENT_SUFFIX_BYTES: usize =
    CLOCK_SAMPLE_BYTES + ENVIRONMENT_DESCRIPTOR_BYTES;

pub(crate) fn write_pair(
    sample: ClockSample,
    descriptor: EnvironmentDescriptor,
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    if sample.boundary().calendar_epoch() > sample.revision() {
        return Err(ProtocolError::InvalidPayload);
    }
    if descriptor.clock_id() != sample.clock_id() {
        return Err(ProtocolError::UnsupportedEnvironment);
    }
    let start = output.len();
    output.extend_from_slice(&sample.clock_id().get().to_le_bytes());
    output.extend_from_slice(&sample.revision().get().to_le_bytes());
    output.extend_from_slice(sample.session_nonce().as_bytes());
    output.extend_from_slice(&sample.server_monotonic_ms().get().to_le_bytes());
    output.extend_from_slice(&sample.world_utc_ms().get().to_le_bytes());
    output.extend_from_slice(&sample.boundary().tick().to_le_bytes());
    output.extend_from_slice(&sample.boundary().calendar_epoch().get().to_le_bytes());
    output.extend_from_slice(&sample.achieved_ppm().to_le_bytes());
    output.push(sample.limit() as u8);
    output.push(sample.mode() as u8);
    output.extend_from_slice(&sample.rate_ppm().get().to_le_bytes());
    output.push(sample.offline_policy() as u8);

    output.extend_from_slice(&descriptor.clock_id().get().to_le_bytes());
    output.push(descriptor.profile() as u8);
    output.extend_from_slice(&descriptor.astronomy_model_revision().get().to_le_bytes());
    output.extend_from_slice(&descriptor.catalogue_id().get().to_le_bytes());
    output.extend_from_slice(descriptor.catalogue_sha256().as_bytes());
    debug_assert_eq!(output.len() - start, ENVIRONMENT_SUFFIX_BYTES);
    Ok(())
}

pub(crate) fn read_pair(
    reader: &mut Reader<'_>,
) -> Result<(ClockSample, EnvironmentDescriptor), ProtocolError> {
    let clock_id = ClockId::try_from(reader.u64()?).map_err(|_| ProtocolError::InvalidPayload)?;
    let revision =
        ClockRevision::try_from(reader.u64()?).map_err(|_| ProtocolError::InvalidPayload)?;
    let nonce = SessionNonce::new(reader.fixed()?);
    let monotonic = MonotonicMs::new(reader.u64()?);
    let world_time =
        WorldTimeMs::try_from(reader.i64()?).map_err(|_| ProtocolError::InvalidPayload)?;
    let tick = reader.u64()?;
    let epoch = ClockRevision::new(reader.u64()?).map_err(|_| ProtocolError::InvalidPayload)?;
    let boundary = minimium_environment::time::SimulationBoundary::new(tick, epoch)
        .map_err(|_| ProtocolError::InvalidPayload)?;
    if epoch > revision {
        return Err(ProtocolError::InvalidPayload);
    }
    let achieved = reader.u64()?;
    let limit = minimium_environment::time::SimulationLimit::from_code(reader.byte()?)
        .ok_or(ProtocolError::InvalidPayload)?;
    let mode = ClockMode::try_from(reader.byte()?).map_err(|_| ProtocolError::InvalidPayload)?;
    let rate = TimeScalePpm::try_from(reader.u64()?).map_err(|_| ProtocolError::InvalidPayload)?;
    let offline =
        OfflinePolicy::try_from(reader.byte()?).map_err(|_| ProtocolError::InvalidPayload)?;
    let sample = ClockSample::new(
        clock_id, revision, nonce, monotonic, world_time, mode, rate, offline,
    )
    .map_err(|_| ProtocolError::InvalidPayload)?;

    let descriptor = EnvironmentDescriptor::from_codes(
        reader.u64()?,
        reader.byte()?,
        reader.u32()?,
        reader.u32()?,
        reader.fixed()?,
    )
    .map_err(|_| ProtocolError::UnsupportedEnvironment)?;
    if descriptor.validate_clock(clock_id).is_err() {
        return Err(ProtocolError::UnsupportedEnvironment);
    }
    Ok((
        sample
            .with_boundary(boundary)
            .with_progress(achieved, limit),
        descriptor,
    ))
}
