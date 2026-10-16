//! Where the world's water lies and the level it lies at, as the scene hands it over with the
//! world's landscape vertices: a value pair for every vertex, which the shaders interpolate.
//!
//! - **How far inside the water**, meters: the water's edge runs halfway between a water vertex
//!   and the land beside it, rounded about lone water vertices; negative on land.
//! - **The water's level**, as how far below the vertex it lies: nought at a water vertex, whose
//!   own height retail draws its water at; on land beside water, as far as the water about it
//!   lies below, so the water stays level up to a rising bank.
//!
//! The scene works the pair out once, from the same vertices, and holds each value as the half
//! float the shaders read, so what it stands in the water or on the banks is drawn where the
//! ground is drawn.

/// A landscape cell's side, meters.
pub const CELL: f32 = 24.0;
/// The farthest inside or outside the water the field tells apart, meters: past it every value is
/// the same.
pub const REACH: f32 = 960.0;

/// The field as the shaders read it: two half floats a vertex, `rg16float`.
#[must_use]
pub fn field_bytes(field: &[[f32; 2]]) -> Vec<u8> {
    use crate::shared::terrain_field::f16_bits;
    field
        .iter()
        .flat_map(|[a, b]| {
            let [a0, a1] = f16_bits(*a).to_le_bytes();
            let [b0, b1] = f16_bits(*b).to_le_bytes();
            [a0, a1, b0, b1]
        })
        .collect()
}

/// The made-up depth of the bed below the water's surface `inside` meters inside the water: none
/// at its edge, falling away steepest there and leveling out at `depth` meters `shelf` meters in.
/// The shaders' own arithmetic.
#[must_use]
pub fn bed_depth(inside: f32, depth: f32, shelf: f32) -> f32 {
    if inside <= 0.0 {
        return 0.0;
    }
    let t = 1.0 - (inside / shelf.max(1e-3)).min(1.0);
    depth * (1.0 - t * t)
}
