//! The client's dice, as animation rolls them: idles that pick a variant now and then, and
//! effects that play with some probability and a varied intensity.
//!
//! The numbers come from the client's one generator ([`Random`]), which every body shares with
//! everything else in the program that draws: what one body draws moves what the next draws.

use auberean_world::chargen::Random;

/// A float between two bounds, either order: the span times a draw plus the low bound, at double
/// precision, rounded once. Equal bounds give the bound without drawing.
pub fn roll_float(dice: &mut Random, a: f32, b: f32) -> f32 {
    dice.roll_float(a, b)
}

/// A whole number between two bounds, either order, both included.
pub fn roll_int(dice: &mut Random, a: i32, b: i32) -> i32 {
    dice.roll_int(a, b)
}
