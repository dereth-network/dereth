// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Cryptography/ISAAC.cs
//! `ACE.Common.Cryptography.ISAAC`: Jenkins' ISAAC with the Turbine seeding (the 32-bit key is
//! loaded into `a`, `b` and `c`, the table is initialised from zeros) and a result pointer that
//! walks down from 255.
//!
//! The generator itself is the shared transport core's (`dereth_transport::Isaac`): ACE's
//! stream is the client's, draw for draw, which ACE's own output confirms (the `isaac` rows of
//! `tests/fixtures/ace_common.tsv`, 600 draws over five seeds). ACE refills the table right after
//! handing out its last entry and the shared generator right before handing out the next one;
//! the values drawn are the same. What stays here is ACE's shape: the key taken from a byte array
//! and `ReleaseResources`.

use dereth_transport::isaac::Isaac as Generator;

// ACE: ISAAC
/// The ISAAC generator as ACE exposes it.
#[derive(Clone)]
pub struct Isaac {
    generator: Generator,
    is_released: bool,
}

impl std::fmt::Debug for Isaac {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Isaac")
            .field("is_released", &self.is_released)
            .finish_non_exhaustive()
    }
}

impl Isaac {
    // ACE: ISAAC.ISAAC, ISAAC.Initialize, ISAAC.Shuffle
    /// `new ISAAC(seed)`: the key is the first four bytes, little-endian.
    ///
    /// # Panics
    /// When `seed` is shorter than four bytes (`ArgumentException` from `BitConverter`).
    #[must_use]
    pub fn new(seed: &[u8]) -> Self {
        let key: [u8; 4] = seed
            .get(..4)
            .and_then(|b| b.try_into().ok())
            .expect("ArgumentException: seed shorter than 4 bytes");
        Self {
            generator: Generator::new(u32::from_le_bytes(key)),
            is_released: false,
        }
    }

    // ACE: ISAAC.ReleaseResources
    /// After this, [`next`](Self::next) returns 0.
    pub fn release_resources(&mut self) {
        self.is_released = true;
    }

    // ACE: ISAAC.Next, ISAAC.IsaacScramble
    /// The next value; the result table is read from index 255 down, then refilled.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        if self.is_released {
            return 0;
        }
        self.generator.next()
    }
}
