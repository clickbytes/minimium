use crate::{reading::Reader, ProtocolError};
use minimium_environment::climate::{ClimateOffset, ClimateRegion, ClimateScenario};

pub(crate) fn write(scenario: &ClimateScenario, out: &mut Vec<u8>) -> Result<(), ProtocolError> {
    scenario
        .validate()
        .map_err(|_| ProtocolError::InvalidPayload)?;
    for value in [
        scenario.start_utc,
        scenario.end_utc,
        scenario.global.temperature_c,
        scenario.global.moisture,
        scenario.relief_influence,
    ] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.push(scenario.regions.len() as u8);
    for region in &scenario.regions {
        for value in [
            region.center[0],
            region.center[1],
            region.center[2],
            region.radius_radians,
            region.offset.temperature_c,
            region.offset.moisture,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    Ok(())
}

pub(crate) fn read(reader: &mut Reader<'_>) -> Result<ClimateScenario, ProtocolError> {
    let float = |reader: &mut Reader<'_>| -> Result<f64, ProtocolError> {
        Ok(f64::from_le_bytes(reader.fixed()?))
    };
    let mut scenario = ClimateScenario {
        start_utc: float(reader)?,
        end_utc: float(reader)?,
        global: ClimateOffset {
            temperature_c: float(reader)?,
            moisture: float(reader)?,
        },
        relief_influence: float(reader)?,
        regions: Vec::new(),
    };
    let count = reader.byte()?;
    if count > 32 {
        return Err(ProtocolError::InvalidPayload);
    }
    for _ in 0..count {
        scenario.regions.push(ClimateRegion {
            center: [float(reader)?, float(reader)?, float(reader)?],
            radius_radians: float(reader)?,
            offset: ClimateOffset {
                temperature_c: float(reader)?,
                moisture: float(reader)?,
            },
        });
    }
    scenario
        .validate()
        .map_err(|_| ProtocolError::InvalidPayload)?;
    Ok(scenario)
}
