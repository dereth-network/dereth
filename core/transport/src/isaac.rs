//! ISAAC as the client implements it, and the `CryptoSystem` wrapper around it.
//!
//! The client's generator is Bob Jenkins' ISAAC with a 256-word state and exactly one Turbine
//! modification: the 32-bit seed is loaded into `randa`, `randb` **and** `randc` *before*
//! `randinit`, and `randinit` runs with `flag = TRUE` so it does not clear them. A stock ISAAC
//! seeded through `randrsl` produces a completely different stream, so this is the whole of the
//! difference between talking to a server and being silently dropped by one.
//!
//! One value is drawn per encrypted packet, in sequence order, on both sides. The stream is
//! therefore **positional**: see `docs/CORRECTIONS.md` —
//! ACE's `CryptoSystem.Search`, which scans up to 256 values forward, is an approximation of this
//! and must not be ported.
//!
//! See `docs/networking/01-packet-format.md` §5.4.

/// Jenkins' golden ratio constant, the initial value of all eight `shuffle` variables.
const GOLDEN: u32 = 0x9E37_79B9;

/// The state size. `RANDSIZL = 8`, so `RANDSIZ = 256`.
const SIZE: usize = 256;

/// The generator's shuffle — the standard Jenkins `mix`.
///
/// Transcribed shift for shift; the asymmetric mix of `<<` and `>>` is the point of the function
/// and any "tidying" of it changes the stream.
fn shuffle(x: &mut [u32; 8]) {
    x[0] ^= x[1] << 0x0B;
    x[3] = x[3].wrapping_add(x[0]);
    x[1] = x[1].wrapping_add(x[2]);
    x[1] ^= x[2] >> 0x02;
    x[4] = x[4].wrapping_add(x[1]);
    x[2] = x[2].wrapping_add(x[3]);
    x[2] ^= x[3] << 0x08;
    x[5] = x[5].wrapping_add(x[2]);
    x[3] = x[3].wrapping_add(x[4]);
    x[3] ^= x[4] >> 0x10;
    x[6] = x[6].wrapping_add(x[3]);
    x[4] = x[4].wrapping_add(x[5]);
    x[4] ^= x[5] << 0x0A;
    x[7] = x[7].wrapping_add(x[4]);
    x[5] = x[5].wrapping_add(x[6]);
    x[5] ^= x[6] >> 0x04;
    x[0] = x[0].wrapping_add(x[5]);
    x[6] = x[6].wrapping_add(x[7]);
    x[6] ^= x[7] << 0x08;
    x[1] = x[1].wrapping_add(x[6]);
    x[7] = x[7].wrapping_add(x[0]);
    x[7] ^= x[0] >> 0x09;
    x[2] = x[2].wrapping_add(x[7]);
    x[0] = x[0].wrapping_add(x[1]);
}

/// The raw generator: 256-word ISAAC over 32-bit words.
///
/// Public only so a test can dump its state; the protocol always reaches it through
/// [`CryptoSystem`].
#[derive(Clone)]
pub struct Isaac {
    randrsl: [u32; SIZE],
    randmem: [u32; SIZE],
    randa: u32,
    randb: u32,
    randc: u32,
    /// Counts *down* through `randrsl`. Starts at 256 so the first draw is `randrsl[255]`.
    randcnt: u32,
}

// The state is 2 KiB of derived noise; printing it is never what a debug format is for.
impl std::fmt::Debug for Isaac {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Isaac")
            .field("randa", &format_args!("{:08X}", self.randa))
            .field("randb", &format_args!("{:08X}", self.randb))
            .field("randc", &format_args!("{:08X}", self.randc))
            .field("randcnt", &self.randcnt)
            .finish_non_exhaustive()
    }
}

impl Isaac {
    /// The ISAAC constructor, followed by
    /// `randinit` with `flag = TRUE`.
    ///
    /// Step 2 — `randa = randb = randc = seed` **before** `randinit` — is the Turbine modification.
    /// Because `randinit` is called with `flag = TRUE` it does not clear them, so the seed survives
    /// into the first `isaac()` round. `randrsl` is all zero at that point, so it contributes
    /// nothing to the mixing passes and the entire stream hangs off those three words.
    ///
    /// See `docs/networking/01-packet-format.md` §5.4.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        let mut this = Self {
            randrsl: [0; SIZE],
            randmem: [0; SIZE],
            randa: seed,
            randb: seed,
            randc: seed,
            randcnt: 0,
        };
        this.randinit();
        this
    }

    fn randinit(&mut self) {
        let mut x = [GOLDEN; 8];
        for _ in 0..4 {
            shuffle(&mut x);
        }
        // Pass 1 mixes in randrsl (all zero at construction).
        for i in (0..SIZE).step_by(8) {
            for (xk, rk) in x.iter_mut().zip(&self.randrsl[i..i + 8]) {
                *xk = xk.wrapping_add(*rk);
            }
            shuffle(&mut x);
            self.randmem[i..i + 8].copy_from_slice(&x);
        }
        // Pass 2 mixes in what pass 1 just wrote.
        for i in (0..SIZE).step_by(8) {
            for (xk, mk) in x.iter_mut().zip(&self.randmem[i..i + 8]) {
                *xk = xk.wrapping_add(*mk);
            }
            shuffle(&mut x);
            self.randmem[i..i + 8].copy_from_slice(&x);
        }
        self.isaac();
        self.randcnt = 256;
    }

    /// One round, refilling all 256 results.
    fn isaac(&mut self) {
        let mut a = self.randa;
        self.randc = self.randc.wrapping_add(1);
        let mut b = self.randb.wrapping_add(self.randc);

        for i in 0..SIZE {
            let x = self.randmem[i];
            a ^= match i & 3 {
                0 => a << 0x0D,
                1 => a >> 0x06,
                2 => a << 0x02,
                _ => a >> 0x10,
            };
            a = a.wrapping_add(self.randmem[(i + 128) & 0xFF]);
            let y = self.randmem[((x >> 2) & 0xFF) as usize]
                .wrapping_add(a)
                .wrapping_add(b);
            self.randmem[i] = y;
            b = self.randmem[((y >> 10) & 0xFF) as usize].wrapping_add(x);
            self.randrsl[i] = b;
        }

        self.randa = a;
        self.randb = b;
    }

    /// One draw, matching the identical inline draw in the encrypt path:
    ///
    /// ```text
    /// old = randcnt; randcnt = old - 1;
    /// if (old == 0) { isaac(); randcnt = 0xFF; return randrsl[0xFF]; }
    /// return randrsl[randcnt];
    /// ```
    ///
    /// The stream is `randrsl[255]`, `randrsl[254]`, ... `randrsl[0]`, then a fresh round and
    /// `randrsl[255]` again: 256 values per round, no repeats and no gaps. Transcribed as written
    /// Note that the counter momentarily wraps to `0xFFFF_FFFF` on the refill path and is
    /// immediately overwritten with `0xFF`, which is why the decrement is unconditional here too.
    // Named for the client's draw, not for `Iterator`: the stream is infinite and stateful, and
    // making it an iterator would invite `.take(n)` in places that must draw exactly once per
    // encrypted packet.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        let old = self.randcnt;
        self.randcnt = old.wrapping_sub(1);
        if old == 0 {
            self.isaac();
            self.randcnt = 0xFF;
            return self.randrsl[0xFF];
        }
        self.randrsl[self.randcnt as usize]
    }
}

/// `CryptoSystem` — the ISAAC stream as the transport uses it.
///
/// One value per encrypted packet, in sequence order. The draw counter starts at 1 and decrements
/// on every draw; nothing in the client
/// reads it back, but it is carried here so a state dump matches.
#[derive(Debug, Clone)]
pub struct CryptoSystem {
    isaac: Isaac,
    last_iter: u32,
}

impl CryptoSystem {
    /// The crypto system's constructor.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self {
            isaac: Isaac::new(seed),
            last_iter: 1,
        }
    }

    /// Draw the next key.
    ///
    /// Every call to this is a commitment: the peer draws in lockstep, so a draw the peer did not
    /// make desynchronises the connection permanently. In particular a duplicate encrypted packet
    /// must be dropped **without** calling this.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.last_iter = self.last_iter.wrapping_add(1);
        self.isaac.next()
    }

    /// XOR every **whole** dword of `data` with one key.
    ///
    /// A trailing 1-3 bytes are left untouched, which is faithful and also irrelevant: the only
    /// call site passes a length of 4, the four bytes of the checksum the receiver decrypts. Kept in this shape so the behaviour is the original's
    /// if anything ever passes a longer buffer.
    pub fn encrypt_in_place(data: &mut [u8], key: u32) {
        let (words, _tail) = data.as_chunks_mut::<4>();
        for word in words {
            *word = (u32::from_le_bytes(*word) ^ key).to_le_bytes();
        }
    }

    /// The last iteration, exposed for state comparison only. Nothing in the client reads it.
    #[must_use]
    pub fn last_iter(&self) -> u32 {
        self.last_iter
    }

    /// The generator's `randa`/`randb`/`randc`/`randcnt`, for the state-dump test that proves the
    /// seed reached all three words before `randinit`.
    #[must_use]
    pub fn state_words(&self) -> (u32, u32, u32, u32) {
        (
            self.isaac.randa,
            self.isaac.randb,
            self.isaac.randc,
            self.isaac.randcnt,
        )
    }

    /// `randmem` and `randrsl`, for the state-dump test.
    #[must_use]
    pub fn state_arrays(&self) -> (&[u32; SIZE], &[u32; SIZE]) {
        (&self.isaac.randmem, &self.isaac.randrsl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the independent packet-format calculation, which asserts
    /// `ac_isaac.ClientISAAC == ACE's ISAAC.cs` for 1000 draws across five seeds and prints these
    /// eight values. Also quoted in `docs/networking/01-packet-format.md` §6.
    #[test]
    fn deadbeef_first_eight() {
        let mut cs = CryptoSystem::new(0xDEAD_BEEF);
        let got: Vec<u32> = (0..8).map(|_| cs.next()).collect();
        assert_eq!(
            got,
            vec![
                0x5DA2_2D96,
                0xDB3B_A3B6,
                0x9FD9_67F9,
                0x0748_7047,
                0x0A8E_4664,
                0x7480_3C1F,
                0xEEFD_EC2C,
                0xA4E4_FB92,
            ]
        );
    }

    /// Oracle: the same. `last_iter` starts at 1 and counts draws.
    #[test]
    fn last_iter_starts_at_one_and_counts_draws() {
        let mut cs = CryptoSystem::new(0);
        assert_eq!(cs.last_iter(), 1);
        cs.next();
        cs.next();
        assert_eq!(cs.last_iter(), 3);
    }

    /// The stream walks `randrsl` downward and refills at the bottom: draw 256 must be the first
    /// word of a *fresh* round, not a repeat of draw 0.
    ///
    /// The expected second round comes from a separately initialized generator.
    #[test]
    fn draws_walk_down_and_refill() {
        let mut cs = CryptoSystem::new(0xDEAD_BEEF);
        let first_round: Vec<u32> = (0..256).map(|_| cs.next()).collect();
        // Reading the arrays after 256 draws shows the *second* round's results, so compare
        // against a fresh instance's first round instead.
        let mut fresh = CryptoSystem::new(0xDEAD_BEEF);
        let (_, rsl) = fresh.state_arrays();
        let expected: Vec<u32> = (0..256).rev().map(|i| rsl[i]).collect();
        assert_eq!(first_round, expected);

        // Draw 256 comes from a new round, so it is not in the first round's set at the same
        // position, and the counter has been reloaded.
        let next = fresh_after(&mut fresh, 256);
        assert_ne!(next, first_round[0]);
    }

    fn fresh_after(cs: &mut CryptoSystem, n: usize) -> u32 {
        for _ in 0..n {
            cs.next();
        }
        cs.next()
    }

    /// `encrypt_in_place` XORs whole dwords only; a ragged tail survives. The only real call site passes
    /// exactly four bytes.
    #[test]
    fn encrypt_covers_whole_dwords_only() {
        let mut buf = [0xAAu8, 0xAA, 0xAA, 0xAA, 0x55, 0x55];
        CryptoSystem::encrypt_in_place(&mut buf, 0xFFFF_FFFF);
        assert_eq!(buf, [0x55, 0x55, 0x55, 0x55, 0x55, 0x55]);
    }

    /// Encryption is its own inverse, which is what makes the receiver's recovery work.
    #[test]
    fn encrypt_is_an_involution() {
        let mut buf = 0x1234_5678u32.to_le_bytes();
        CryptoSystem::encrypt_in_place(&mut buf, 0x5DA2_2D96);
        CryptoSystem::encrypt_in_place(&mut buf, 0x5DA2_2D96);
        assert_eq!(u32::from_le_bytes(buf), 0x1234_5678);
    }
}
