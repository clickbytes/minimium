use super::*;
use std::io::Write;

pub(super) fn parse_location(fields: &mut std::str::Split<'_, char>) -> io::Result<WorldLocation> {
    let instance = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing location instance"))?
        .parse::<u128>()
        .ok()
        .and_then(WorldInstanceId::new)
        .ok_or_else(|| invalid_world_snapshot("invalid location instance"))?;
    let mut position = [0_i64; 3];
    for coordinate in &mut position {
        *coordinate = fields
            .next()
            .ok_or_else(|| invalid_world_snapshot("missing quantized coordinate"))?
            .parse::<i64>()
            .map_err(|_| invalid_world_snapshot("invalid quantized coordinate"))?;
        if coordinate.unsigned_abs() > MAX_SAVED_POSITION_MM {
            return Err(invalid_world_snapshot(
                "quantized coordinate is out of range",
            ));
        }
    }
    let mut orientation = [0_i32; 4];
    for component in &mut orientation {
        *component = fields
            .next()
            .ok_or_else(|| invalid_world_snapshot("missing location orientation"))?
            .parse::<i32>()
            .map_err(|_| invalid_world_snapshot("invalid location orientation"))?;
    }
    let orientation = WorldOrientationQ30::new(orientation)
        .ok_or_else(|| invalid_world_snapshot("invalid location orientation"))?;
    Ok(WorldLocation::new(
        instance,
        Position3Mm(position),
        orientation,
    ))
}

pub(super) fn write_location(file: &mut impl Write, location: WorldLocation) -> io::Result<()> {
    write!(file, "\t{}", location.instance().get())?;
    for coordinate in location.position().coordinates() {
        write!(file, "\t{coordinate}")?;
    }
    for component in location.orientation().components() {
        write!(file, "\t{component}")?;
    }
    Ok(())
}

pub(super) fn legacy_location(instance: WorldInstanceId, position: [f32; 3]) -> WorldLocation {
    let millimeters = position.map(|value| (f64::from(value) * 1000.0).round() as i64);
    WorldLocation::new(
        instance,
        Position3Mm(millimeters),
        WorldOrientationQ30::IDENTITY,
    )
}

pub(super) fn position_from_location(location: WorldLocation) -> [f32; 3] {
    location
        .position()
        .coordinates()
        .map(|value| value as f32 / 1000.0)
}
