//! Creation preview framing for each interface and body shape.
use crate::DisplayVariant;

/// Camera position for the selected body and face/body framing.
#[must_use]
pub fn camera(heritage: u32, face: bool, variant: DisplayVariant) -> [f32; 3] {
    match (heritage, face) {
        (12, true) => [0., -1.85, 1.85],
        (13, true) => [0., -3.05, 2.75],
        (7, true) => [0., -0.85, 1.65],
        (_, true) => [0., -0.55, 1.65],
        (12, false) => [0., -3.8, 1.15],
        (13, false) => [0., -5.7, 1.65],
        (_, false) => match variant {
            DisplayVariant::Classic => [0., -2.2, 1.1],
            DisplayVariant::Modern => [0., -2.5, 0.95],
        },
    }
}
