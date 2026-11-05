use super::{dot3, triangle_normal, vector_subtract};

pub(super) fn segment_triangle_distance_squared(
    start: [f32; 3],
    end: [f32; 3],
    [a, b, c]: [[f32; 3]; 3],
) -> f32 {
    let direction = vector_subtract(end, start);
    let normal = triangle_normal([a, b, c]);
    let denominator = dot3(normal, direction);
    if denominator.abs() > 1.0e-8 {
        let fraction = dot3(normal, vector_subtract(a, start)) / denominator;
        if (0.0..=1.0).contains(&fraction) {
            let point = [
                start[0] + direction[0] * fraction,
                start[1] + direction[1] * fraction,
                start[2] + direction[2] * fraction,
            ];
            if point_in_triangle(point, [a, b, c]) {
                return 0.0;
            }
        }
    }
    let mut minimum = point_triangle_distance_squared(start, [a, b, c])
        .min(point_triangle_distance_squared(end, [a, b, c]));
    for (first, second) in [(a, b), (b, c), (c, a)] {
        minimum = minimum.min(segment_segment_distance_squared(start, end, first, second));
    }
    minimum
}

fn point_triangle_distance_squared(point: [f32; 3], [a, b, c]: [[f32; 3]; 3]) -> f32 {
    let normal = triangle_normal([a, b, c]);
    let normal_squared = dot3(normal, normal);
    if normal_squared <= 1.0e-12 {
        return f32::INFINITY;
    }
    let signed = dot3(normal, vector_subtract(point, a)) / normal_squared;
    let projected = [
        point[0] - normal[0] * signed,
        point[1] - normal[1] * signed,
        point[2] - normal[2] * signed,
    ];
    if point_in_triangle(projected, [a, b, c]) {
        return signed * signed * normal_squared;
    }
    point_segment_distance_squared(point, a, b)
        .min(point_segment_distance_squared(point, b, c))
        .min(point_segment_distance_squared(point, c, a))
}

fn point_segment_distance_squared(point: [f32; 3], start: [f32; 3], end: [f32; 3]) -> f32 {
    let direction = vector_subtract(end, start);
    let length_squared = dot3(direction, direction);
    let fraction = if length_squared <= 1.0e-12 {
        0.0
    } else {
        (dot3(vector_subtract(point, start), direction) / length_squared).clamp(0.0, 1.0)
    };
    let closest = [
        start[0] + direction[0] * fraction,
        start[1] + direction[1] * fraction,
        start[2] + direction[2] * fraction,
    ];
    let difference = vector_subtract(point, closest);
    dot3(difference, difference)
}

fn segment_segment_distance_squared(
    first_start: [f32; 3],
    first_end: [f32; 3],
    second_start: [f32; 3],
    second_end: [f32; 3],
) -> f32 {
    let first = vector_subtract(first_end, first_start);
    let second = vector_subtract(second_end, second_start);
    let offset = vector_subtract(first_start, second_start);
    let first_length = dot3(first, first);
    let second_length = dot3(second, second);
    let cross_dot = dot3(first, second);
    let first_offset = dot3(first, offset);
    let second_offset = dot3(second, offset);
    let denominator = first_length * second_length - cross_dot * cross_dot;
    let mut first_fraction = if denominator > 1.0e-12 {
        ((cross_dot * second_offset - first_offset * second_length) / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut second_fraction =
        (cross_dot * first_fraction + second_offset) / second_length.max(1.0e-12);
    if second_fraction < 0.0 {
        second_fraction = 0.0;
        first_fraction = (-first_offset / first_length.max(1.0e-12)).clamp(0.0, 1.0);
    } else if second_fraction > 1.0 {
        second_fraction = 1.0;
        first_fraction = ((cross_dot - first_offset) / first_length.max(1.0e-12)).clamp(0.0, 1.0);
    }
    let first_point = [
        first_start[0] + first[0] * first_fraction,
        first_start[1] + first[1] * first_fraction,
        first_start[2] + first[2] * first_fraction,
    ];
    let second_point = [
        second_start[0] + second[0] * second_fraction,
        second_start[1] + second[1] * second_fraction,
        second_start[2] + second[2] * second_fraction,
    ];
    let difference = vector_subtract(first_point, second_point);
    dot3(difference, difference)
}

fn point_in_triangle(point: [f32; 3], [a, b, c]: [[f32; 3]; 3]) -> bool {
    let first = vector_subtract(b, a);
    let second = vector_subtract(c, a);
    let relative = vector_subtract(point, a);
    let d00 = dot3(first, first);
    let d01 = dot3(first, second);
    let d11 = dot3(second, second);
    let d20 = dot3(relative, first);
    let d21 = dot3(relative, second);
    let denominator = d00 * d11 - d01 * d01;
    if denominator.abs() <= 1.0e-12 {
        return false;
    }
    let u = (d11 * d20 - d01 * d21) / denominator;
    let v = (d00 * d21 - d01 * d20) / denominator;
    u >= -1.0e-5 && v >= -1.0e-5 && u + v <= 1.00001
}
