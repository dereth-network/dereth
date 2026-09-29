// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Cryptography/CryptoSystem.cs
//! `ACE.Common.Cryptography.CryptoSystem`: the ISAAC key stream with ACE's out-of-order tolerance
//! (a set of skipped keys that later packets may still use).
//!
//! Not ACE's: the server does not check client packets with this.
//! The retail client and server shared one receive check: an encrypted packet carries the next
//! key of a strictly positional stream, and the keys of sequences skipped over are parked for
//! their retransmissions. ACE instead accepts any key up to 256 draws ahead and remembers the ones
//! it skipped (a duplicate makes it park 256 keys; see `crypto_system_search_follows_ace` in
//! empyrean-net). Sessions use the shared window (`dereth_transport::session::SequenceWindow`); this
//! type stays as the port of ACE's, and [`CryptoSystem::MAXIMUM_EFFORT_LEVEL`] still bounds how
//! far ahead of the gap a retransmit request may reach. The key stream itself is the shared
//! generator, through [`Isaac`].

use super::isaac::Isaac;
use crate::dotnet::DotNetHashSet;

/// `CryptoSystem : ISAAC`.
#[derive(Debug, Clone)]
pub struct CryptoSystem {
    isaac: Isaac,
    /// `xors`: keys skipped over by [`search`](Self::search) and not yet consumed. `None` after
    /// [`release_resources`](Self::release_resources) (`null` in ACE).
    pub xors: Option<DotNetHashSet<u32>>,
    /// `CurrentKey`.
    pub current_key: u32,
}

impl CryptoSystem {
    // ACE: CryptoSystem.MaximumEffortLevel
    /// How far ahead [`search`](Self::search) looks, counting the keys already skipped.
    pub const MAXIMUM_EFFORT_LEVEL: i32 = 256;

    // ACE: CryptoSystem.CryptoSystem(uint)
    /// `new CryptoSystem(seed)`: `BitConverter.GetBytes(seed)` (little-endian), then the first key.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self::from_seed_bytes(&seed.to_le_bytes())
    }

    // ACE: CryptoSystem.CryptoSystem(byte[])
    /// `new CryptoSystem(seedBytes)`.
    #[must_use]
    pub fn from_seed_bytes(seed: &[u8]) -> Self {
        let mut isaac = Isaac::new(seed);
        let current_key = isaac.next();
        Self {
            isaac,
            xors: Some(DotNetHashSet::new()),
            current_key,
        }
    }

    /// `ISAAC.Next()` (inherited).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.isaac.next()
    }

    fn xors_mut(&mut self) -> &mut DotNetHashSet<u32> {
        self.xors
            .as_mut()
            .expect("NullReferenceException: xors after ReleaseResources")
    }

    // ACE: CryptoSystem.ConsumeKey
    /// Uses up key `x`: advances when it is the current key, otherwise forgets it from `xors`.
    pub fn consume_key(&mut self, x: u32) {
        if self.current_key == x {
            self.current_key = self.isaac.next();
        } else {
            self.xors_mut().remove(&x);
        }
    }

    // ACE: CryptoSystem.Search
    /// Whether `x` is the current key, a skipped key, or one of the next keys within the effort
    /// budget (skipping the keys it passes into `xors`).
    pub fn search(&mut self, x: u32) -> bool {
        if self.current_key == x {
            return true;
        }
        if self.xors_mut().contains(&x) {
            return true;
        }
        let g = i32::try_from(self.xors_mut().len()).unwrap_or(i32::MAX);
        for _ in 0..(Self::MAXIMUM_EFFORT_LEVEL - g) {
            let key = self.current_key;
            self.xors_mut().insert(key);
            self.consume_key(key);
            if self.current_key == x {
                return true;
            }
        }
        false
    }

    // ACE: CryptoSystem.ReleaseResources
    /// Clears and drops `xors`, then releases the generator.
    pub fn release_resources(&mut self) {
        if let Some(x) = self.xors.as_mut() {
            x.clear();
        }
        self.xors = None;
        self.isaac.release_resources();
    }
}
