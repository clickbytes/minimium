#include "bridge.h"

#include <math.h>
#include <string.h>

#include "astronomy.h"

#define BRIDGE_INVALID_TIME (-1)
#define BRIDGE_NONFINITE (-2)
#define BRIDGE_INVALID_ARGUMENT (-3)
#define AU_TO_METERS (KM_PER_AU * 1000.0)

static int32_t copy_vector_meters(const astro_vector_t *vector, double output[3])
{
    if (vector->status != ASTRO_SUCCESS)
        return (int32_t)vector->status;
    if (!isfinite(vector->x) || !isfinite(vector->y) || !isfinite(vector->z))
        return BRIDGE_NONFINITE;
    output[0] = vector->x * AU_TO_METERS;
    output[1] = vector->y * AU_TO_METERS;
    output[2] = vector->z * AU_TO_METERS;
    if (!isfinite(output[0]) || !isfinite(output[1]) || !isfinite(output[2]))
        return BRIDGE_NONFINITE;
    return (int32_t)ASTRO_SUCCESS;
}

static int32_t create_time(double ut_days, astro_time_t *time)
{
    if (!isfinite(ut_days))
        return BRIDGE_INVALID_TIME;
    *time = Astronomy_TimeFromDays(ut_days);
    if (!isfinite(time->ut) || !isfinite(time->tt))
        return BRIDGE_INVALID_TIME;
    return (int32_t)ASTRO_SUCCESS;
}

int32_t minimium_ae_terrestrial_days(double ut_days, double *tt_days)
{
    astro_time_t time;
    int32_t status;
    if (tt_days == NULL)
        return BRIDGE_INVALID_ARGUMENT;
    status = create_time(ut_days, &time);
    if (status != (int32_t)ASTRO_SUCCESS)
        return status;
    *tt_days = time.tt;
    return isfinite(*tt_days) ? (int32_t)ASTRO_SUCCESS : BRIDGE_NONFINITE;
}

int32_t minimium_ae_calculate_ephemeris(
    double ut_days,
    minimium_ae_ephemeris_t *output
)
{
    astro_time_t time;
    astro_vector_t sun;
    astro_vector_t moon;
    astro_vector_t earth;
    astro_axis_t axis;
    astro_illum_t illumination;
    astro_angle_result_t phase;
    int32_t status;

    if (output == NULL)
        return BRIDGE_INVALID_ARGUMENT;
    memset(output, 0, sizeof(*output));
    status = create_time(ut_days, &time);
    if (status != (int32_t)ASTRO_SUCCESS)
        return status;

    sun = Astronomy_GeoVector(BODY_SUN, time, ABERRATION);
    status = copy_vector_meters(&sun, output->sun_geocentric_m);
    if (status != (int32_t)ASTRO_SUCCESS)
        return status;

    moon = Astronomy_GeoMoon(time);
    status = copy_vector_meters(&moon, output->moon_geocentric_m);
    if (status != (int32_t)ASTRO_SUCCESS)
        return status;

    earth = Astronomy_HelioVector(BODY_EARTH, time);
    status = copy_vector_meters(&earth, output->earth_heliocentric_m);
    if (status != (int32_t)ASTRO_SUCCESS)
        return status;

    axis = Astronomy_RotationAxis(BODY_MOON, &time);
    if (axis.status != ASTRO_SUCCESS)
        return (int32_t)axis.status;
    if (axis.north.status != ASTRO_SUCCESS)
        return (int32_t)axis.north.status;
    output->moon_north_eqj[0] = axis.north.x;
    output->moon_north_eqj[1] = axis.north.y;
    output->moon_north_eqj[2] = axis.north.z;

    illumination = Astronomy_Illumination(BODY_MOON, time);
    if (illumination.status != ASTRO_SUCCESS)
        return (int32_t)illumination.status;
    output->moon_v_magnitude = illumination.mag;

    phase = Astronomy_MoonPhase(time);
    if (phase.status != ASTRO_SUCCESS)
        return (int32_t)phase.status;
    output->ecliptic_phase_degrees = phase.angle;
    output->tt_days = time.tt;

    if (!isfinite(output->moon_north_eqj[0])
        || !isfinite(output->moon_north_eqj[1])
        || !isfinite(output->moon_north_eqj[2])
        || !isfinite(output->moon_v_magnitude)
        || !isfinite(output->ecliptic_phase_degrees)
        || !isfinite(output->tt_days))
        return BRIDGE_NONFINITE;
    return (int32_t)ASTRO_SUCCESS;
}

int32_t minimium_ae_calculate_local_frame(
    double ut_days,
    double latitude_degrees,
    double longitude_degrees,
    double height_meters,
    minimium_ae_local_frame_t *output
)
{
    astro_time_t time;
    astro_observer_t observer;
    astro_vector_t position;
    astro_rotation_t rotation;
    int32_t status;
    int row;
    int column;

    if (output == NULL)
        return BRIDGE_INVALID_ARGUMENT;
    memset(output, 0, sizeof(*output));
    if (!isfinite(latitude_degrees) || !isfinite(longitude_degrees) || !isfinite(height_meters))
        return BRIDGE_NONFINITE;
    if (latitude_degrees < -90.0 || latitude_degrees > 90.0
        || longitude_degrees < -180.0 || longitude_degrees >= 180.0
        || height_meters < -12000.0 || height_meters > 100000.0)
        return BRIDGE_INVALID_ARGUMENT;
    status = create_time(ut_days, &time);
    if (status != (int32_t)ASTRO_SUCCESS)
        return status;

    observer = Astronomy_MakeObserver(latitude_degrees, longitude_degrees, height_meters);
    position = Astronomy_ObserverVector(&time, observer, EQUATOR_J2000);
    status = copy_vector_meters(&position, output->observer_geocentric_eqj_m);
    if (status != (int32_t)ASTRO_SUCCESS)
        return status;

    rotation = Astronomy_Rotation_EQJ_HOR(&time, observer);
    if (rotation.status != ASTRO_SUCCESS)
        return (int32_t)rotation.status;
    for (row = 0; row < 3; ++row)
    {
        for (column = 0; column < 3; ++column)
        {
            if (!isfinite(rotation.rot[row][column]))
                return BRIDGE_NONFINITE;
            /* Astronomy Engine indexes [input][output]; Rust uses [row][column]. */
            output->eqj_to_hor[row][column] = rotation.rot[column][row];
        }
    }
    output->tt_days = time.tt;
    return isfinite(output->tt_days) ? (int32_t)ASTRO_SUCCESS : BRIDGE_NONFINITE;
}

int32_t minimium_ae_refraction_correction(
    double altitude_degrees,
    double *correction_degrees
)
{
    if (correction_degrees == NULL)
        return BRIDGE_INVALID_ARGUMENT;
    if (!isfinite(altitude_degrees))
        return BRIDGE_NONFINITE;
    if (altitude_degrees < -90.0 || altitude_degrees > 90.0)
        return BRIDGE_INVALID_ARGUMENT;
    *correction_degrees = Astronomy_Refraction(REFRACTION_NORMAL, altitude_degrees);
    return isfinite(*correction_degrees) ? (int32_t)ASTRO_SUCCESS : BRIDGE_NONFINITE;
}
