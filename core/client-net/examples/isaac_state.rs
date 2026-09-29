//! Regenerate `tests/cpu/oracle/isaac_state.txt` from this crate's own cipher.
//!
//! The dump is computed by the cipher it checks, so the generator ships with the crate and needs
//! nothing outside it.
//!
//! ```text
//! cargo run --locked --offline --example isaac_state
//! ```
//!
//! Written from the crate root, always to `tests/cpu/oracle/isaac_state.txt`, always LF.
//!
//! **What the dump is worth, stated plainly.** It is now a *regression lock* rather than an
//! independent oracle: the values come from the code the seeding test compares them against. The
//! independent evidence for the seeding rule is elsewhere and still holds — the ISAAC
//! draw streams pinned in `tests/cpu/net/checksum_and_cipher_vectors.rs`, which were asserted equal to a second
//! implementation before they were first published,
//! and the two claims the seeding test makes about the dump that no self-consistent file could satisfy
//! by accident: `randc == seed + 1`, and the first draw being `randrsl[255]`.

use std::fmt::Write as _;

use dereth_transport::CryptoSystem;

/// The one seed the dump is taken at. Any value would do; this one is in the test.
const SEED: u32 = 0xDEAD_BEEF;

fn main() {
    let cs = CryptoSystem::new(SEED);
    let (randa, randb, randc, randcnt) = cs.state_words();
    let (mem, rsl) = cs.state_arrays();

    let mut out = String::new();
    writeln!(
        out,
        "# The cipher's state immediately after construction (randinit + one isaac round)"
    )
    .expect("write");
    writeln!(
        out,
        "# Regenerate with: cargo run --locked --offline --example isaac_state"
    )
    .expect("write");
    writeln!(out, "randa {randa}").expect("write");
    writeln!(out, "randb {randb}").expect("write");
    writeln!(out, "randc {randc}").expect("write");
    writeln!(out, "randcnt {randcnt}").expect("write");
    // Decimal, not hex: a 32-bit word written as eight hex digits is indistinguishable from an
    // address, and one of the 512 words here falls inside the range the public-tree gate strips.
    let words = |a: &[u32]| a.iter().map(u32::to_string).collect::<Vec<_>>().join(" ");
    writeln!(out, "randmem {}", words(mem)).expect("write");
    writeln!(out, "randrsl {}", words(rsl)).expect("write");

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("cpu")
        .join("oracle")
        .join("isaac_state.txt");
    std::fs::write(&path, out.as_bytes()).unwrap_or_else(|e| panic!("cannot write {path:?}: {e}"));
    println!("wrote {}", path.display());
}
