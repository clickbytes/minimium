#[path = "bindings.generated.rs"]
mod bindings;

pub(crate) use bindings::{MinimiumAeEphemeris, MinimiumAeLocalFrame};

pub(crate) fn ephemeris(ut_days: f64) -> Result<MinimiumAeEphemeris, i32> {
    let mut output = MinimiumAeEphemeris::default();
    let status = unsafe { bindings::minimium_ae_calculate_ephemeris(ut_days, &mut output) };
    if status == 0 {
        Ok(output)
    } else {
        Err(status)
    }
}

pub(crate) fn terrestrial_days(ut_days: f64) -> Result<f64, i32> {
    let mut output = 0.0;
    let status = unsafe { bindings::minimium_ae_terrestrial_days(ut_days, &mut output) };
    if status == 0 {
        Ok(output)
    } else {
        Err(status)
    }
}

pub(crate) fn local_frame(
    ut_days: f64,
    latitude_degrees: f64,
    longitude_degrees: f64,
    height_meters: f64,
) -> Result<MinimiumAeLocalFrame, i32> {
    let mut output = MinimiumAeLocalFrame::default();
    let status = unsafe {
        bindings::minimium_ae_calculate_local_frame(
            ut_days,
            latitude_degrees,
            longitude_degrees,
            height_meters,
            &mut output,
        )
    };
    if status == 0 {
        Ok(output)
    } else {
        Err(status)
    }
}

pub(crate) fn refraction(altitude_degrees: f64) -> Result<f64, i32> {
    let mut correction = 0.0;
    let status =
        unsafe { bindings::minimium_ae_refraction_correction(altitude_degrees, &mut correction) };
    if status == 0 {
        Ok(correction)
    } else {
        Err(status)
    }
}
