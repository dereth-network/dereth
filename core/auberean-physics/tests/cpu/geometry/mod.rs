//! The geometry and math every other part works in: matrices, rotations, frames, boxes, planes,
//! polygons and positions, each operation held to the September 2013 client's own answers.
//! Fixture: tables of cases, each the inputs given and the bits the client's own code answered.

mod boxes;
mod boxes_cases;
mod frame;
mod frame_cases;
mod material;
mod matrix;
mod matrix_cases;
mod misc;
mod misc_cases;
mod plane;
mod plane_cases;
mod polygon;
mod polygon_cases;
mod position;
mod position_cases;
mod sphere;
mod sphere_cases;
mod text;
mod tree;
mod tree_cases;
mod workpoly;
mod workpoly_cases;

/// A case: the operation it asks (a short label), its inputs and the client's answer, as the bits
/// of 32-bit floats (or of whole numbers, where an answer is one).
pub type Case = (&'static str, &'static [u32], &'static [u32]);

pub fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}

pub fn bits(xs: &[f32]) -> Vec<u32> {
    xs.iter().map(|x| x.to_bits()).collect()
}

/// Runs every case through `port` and fails listing each answer that differs from the client's
/// in any bit (two NaNs counting as the same answer); `port` answers `None` for a label it does
/// not know, which also fails.
pub fn check(cases: &[Case], port: impl Fn(&str, &[f32], &[u32]) -> Option<Vec<u32>>) {
    let mut wrong = Vec::new();
    for &(label, inputs, want) in cases {
        let x: Vec<f32> = inputs.iter().map(|&b| f(b)).collect();
        let Some(got) = port(label, &x, inputs) else {
            wrong.push(format!("{label}: no such operation"));
            continue;
        };
        let same = |a: u32, b: u32| a == b || (f(a).is_nan() && f(b).is_nan());
        if got.len() != want.len() || !got.iter().zip(want).all(|(&a, &b)| same(a, b)) {
            wrong.push(format!(
                "{label} {x:?}: client {:?}, port {:?}",
                want.iter().map(|&b| f(b)).collect::<Vec<_>>(),
                got.iter().map(|&b| f(b)).collect::<Vec<_>>()
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {}:\n{}",
        wrong.len(),
        cases.len(),
        wrong.join("\n")
    );
}
