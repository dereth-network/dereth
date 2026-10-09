//! Noise of this presentation's own, the same on the CPU and in its shaders. It never draws from
//! the game's random sequence.

/// One step of the PCG hash: a well-mixed 32-bit value from `v`.
#[must_use]
pub const fn pcg(v: u32) -> u32 {
    let state = v.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277_803_737);
    (word >> 22) ^ word
}
