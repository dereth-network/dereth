//! The helper the tests hold the ports to the client's own answers with.

/// A case: the operation it asks (a short label), its inputs and the client's answer, as the
/// bits of 32-bit floats (or of whole numbers, where an answer is one).
pub type Case = (&'static str, &'static [u32], &'static [u32]);

/// Runs every case through `port` and fails listing each answer that differs from the
/// client's in any bit (two NaNs counting as the same); `port` answers `None` for a label it
/// does not know, which also fails.
pub fn check(cases: &[Case], port: impl Fn(&str, &[f32], &[u32]) -> Option<Vec<u32>>) {
    let f = f32::from_bits;
    let mut wrong: Vec<String> = Vec::new();
    for &(label, inputs, want) in cases {
        let x: Vec<f32> = inputs.iter().map(|&b| f(b)).collect();
        let Some(got) = port(label, &x, inputs) else {
            wrong.push(format!("{label}: no such operation"));
            continue;
        };
        let same = |a: u32, b: u32| a == b || (f(a).is_nan() && f(b).is_nan());
        if got.len() != want.len() || !got.iter().zip(want).all(|(&a, &b)| same(a, b)) {
            wrong.push(format!("{label} {x:?}: client {want:x?}, port {got:x?}"));
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
