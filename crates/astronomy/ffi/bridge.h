#ifndef MINIMIUM_ASTRONOMY_BRIDGE_H
#define MINIMIUM_ASTRONOMY_BRIDGE_H

#include <stdint.h>

typedef struct {
    double sun_geocentric_m[3];
    double moon_geocentric_m[3];
    double earth_heliocentric_m[3];
    double moon_north_eqj[3];
    double moon_v_magnitude;
    double ecliptic_phase_degrees;
    double tt_days;
} minimium_ae_ephemeris_t;

typedef struct {
    double observer_geocentric_eqj_m[3];
    double eqj_to_hor[3][3];
    double tt_days;
} minimium_ae_local_frame_t;

int32_t minimium_ae_calculate_ephemeris(
    double ut_days,
    minimium_ae_ephemeris_t *output
);

int32_t minimium_ae_terrestrial_days(double ut_days, double *tt_days);

int32_t minimium_ae_calculate_local_frame(
    double ut_days,
    double latitude_degrees,
    double longitude_degrees,
    double height_meters,
    minimium_ae_local_frame_t *output
);

int32_t minimium_ae_refraction_correction(
    double altitude_degrees,
    double *correction_degrees
);

#endif
