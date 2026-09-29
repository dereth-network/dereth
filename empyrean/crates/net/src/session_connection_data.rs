// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/SessionConnectionData.cs

use dereth_transport::isaac::Isaac;
use dereth_transport::session::SequenceWindow;

/// The random source for seeds and cookies.
///
/// DIVERGE: ACE builds a fresh unseeded `System.Random` per session. Here one SplitMix64 stream,
/// seeded once by the server (from entropy in the binary, from a constant in tests), feeds every
/// session, so a test run is reproducible. The values are secrets either way; only their
/// unpredictability to a third party matters, and the protocol cannot tell the difference.
#[derive(Debug, Clone)]
pub struct SessionRandom {
    state: u64,
}

impl SessionRandom {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// SplitMix64.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `rand.NextBytes(new byte[4])` read back as a little-endian `uint`.
    pub fn next_u32(&mut self) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        let v = (self.next_u64() >> 32) as u32;
        v
    }
}

/// The `UIntSequence` behaviour `SessionConnectionData.PacketSequence` relies on.
///
/// `Network/Sequence` is world-level and not ported in this crate; this is the two members the
/// transport uses, with ACE's semantics: `CurrentValue`, and `NextValue`, which wraps from
/// `uint.MaxValue` to 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketSequence {
    pub current_value: u32,
}

impl PacketSequence {
    /// `new UIntSequence(false)`: not client-primed, so it starts at `uint.MaxValue` and its first
    /// `NextValue` is 0 (the `ConnectRequest`'s sequence).
    #[must_use]
    pub const fn not_client_primed() -> Self {
        Self {
            current_value: u32::MAX,
        }
    }

    /// `new UIntSequence(startingValue)`.
    #[must_use]
    pub const fn starting_at(starting_value: u32) -> Self {
        Self {
            current_value: starting_value,
        }
    }

    /// `NextValue`.
    pub fn next_value(&mut self) -> u32 {
        if self.current_value == u32::MAX {
            self.current_value = 0;
            return 0;
        }
        self.current_value += 1;
        self.current_value
    }
}

/// ACE `SessionConnectionData`.
#[derive(Debug, Clone)]
pub struct SessionConnectionData {
    /// Random shared 64-bit secret the client must echo in its `ConnectResponse`.
    pub connection_cookie: u64,
    /// Initial value of the client-to-server checksum key stream; `None` once discarded.
    pub client_seed: Option<u32>,
    /// Initial value of the server-to-client checksum key stream; `None` once discarded.
    pub server_seed: Option<u32>,
    pub packet_sequence: PacketSequence,
    pub fragment_sequence: u32,
    /// Client -> server checksum keys.
    ///
    /// Not ACE's: ACE's `CryptoSystem` accepts any key up to 256 draws
    /// ahead and remembers the ones it skipped. This is the shared retail receive window instead:
    /// each sequence takes the next key in order, a skipped sequence has its key drawn and parked,
    /// its retransmission must carry exactly that key, and a duplicate is dropped without drawing.
    pub crypto_client: SequenceWindow,
    /// Server -> client stream cipher.
    pub issac_server: Isaac,
}

impl SessionConnectionData {
    // ACE: SessionConnectionData.SessionConnectionData
    #[must_use]
    pub fn new(rand: &mut SessionRandom) -> Self {
        let client_seed = rand.next_u32();
        let server_seed = rand.next_u32();
        let connection_cookie = rand.next_u64();
        Self {
            connection_cookie,
            client_seed: Some(client_seed),
            server_seed: Some(server_seed),
            packet_sequence: PacketSequence::not_client_primed(),
            fragment_sequence: 0,
            // The client numbers its first sequenced packet 2, as the ACE receive order expects.
            crypto_client: SequenceWindow {
                highest_id_received: 1,
                ..SequenceWindow::new(client_seed)
            },
            issac_server: Isaac::new(server_seed),
        }
    }

    // ACE: SessionConnectionData.DiscardSeeds
    pub fn discard_seeds(&mut self) {
        self.client_seed = None;
        self.server_seed = None;
    }

    // ACE: SessionConnectionData.ToString
    #[must_use]
    pub fn to_log_string(&self) -> String {
        let hex =
            |s: Option<u32>| s.map_or_else(String::new, |v| format!("{:08X}", v.swap_bytes()));
        format!(
            "Seeds: [Client {}, Server {}]",
            hex(self.client_seed),
            hex(self.server_seed)
        )
    }
}
