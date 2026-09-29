// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/FellowshipLockData.cs
//! Port of `Source/ACE.Server/Network/Structure/FellowshipLockData.cs`.

use empyrean_common::dotnet::{CsCast, DotNetDict};

use crate::network::game_messages::game_message::{ace_str, write_record};

// ACE: FellowshipLockData
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FellowshipLockData {
    // ACE: FellowshipLockData.Unknown_1
    pub unknown_1: u32,
    // ACE: FellowshipLockData.Unknown_2
    pub unknown_2: u32,
    // ACE: FellowshipLockData.Unknown_3
    pub unknown_3: u32,
    // ACE: FellowshipLockData.Timestamp
    pub timestamp: u32,
    // ACE: FellowshipLockData.Sequence
    pub sequence: u32,
}

impl FellowshipLockData {
    // ACE: FellowshipLockData.FellowshipLockData
    /// `new FellowshipLockData(double timestamp)`: sequence 1.
    #[must_use]
    pub fn new(timestamp: f64) -> Self {
        Self {
            unknown_1: 0x0, // always 0 in pcaps
            unknown_2: 0x0, // with Unknown_3, one double on the wire: minus the lock's age in seconds
            unknown_3: 0x0,
            timestamp: timestamp.cs_cast(),
            sequence: 1,
        }
    }

    // ACE: FellowshipLockData.UpdateTimestamp
    pub fn update_timestamp(&mut self, timestamp: f64) {
        self.timestamp = timestamp.cs_cast();
        self.sequence = self.sequence.wrapping_add(1);
    }
}

// ACE: FellowshipLockDataExtensions.NumBuckets
/// A `static ushort` field in ACE that nothing reassigns.
pub const NUM_BUCKETS: u16 = 32;

// ACE: FellowshipLockDataExtensions.Write
/// `writer.Write(FellowshipLockData fellowshipLockData)`.
pub fn write(writer: &mut Vec<u8>, fellowship_lock_data: &FellowshipLockData) {
    write_record(writer, &[], |w| record(fellowship_lock_data).write(w));
}

// ACE: FellowshipLockDataExtensions.Write
/// `writer.Write(Dictionary<string, FellowshipLockData> fellowshipLocks)`: a header, then the
/// entries. Not ACE's (retail, V279; the retail captures): the entries go in retail's
/// hash-bucket order (the name's string hash mod [`NUM_BUCKETS`], dictionary order within a
/// bucket), not ACE's dictionary order; dereth-protocol's lock-table writer orders them.
pub fn write_locks(
    writer: &mut Vec<u8>,
    fellowship_locks: &DotNetDict<String, FellowshipLockData>,
) {
    let keys: Vec<&str> = fellowship_locks.iter().map(|(k, _)| k.as_str()).collect();
    write_record(writer, &keys, |w| locks_record(fellowship_locks).write(w));
}

/// The dereth-protocol record the lock `Write` extension writes, field for field.
#[must_use]
pub fn record(
    fellowship_lock_data: &FellowshipLockData,
) -> dereth_protocol::social::FellowshipLock {
    dereth_protocol::social::FellowshipLock {
        unknown_1: fellowship_lock_data.unknown_1,
        // ACE's second and third dwords are, on the wire, one double: its low and high halves.
        age: f64::from_bits(
            (u64::from(fellowship_lock_data.unknown_3) << 32)
                | u64::from(fellowship_lock_data.unknown_2),
        ),
        timestamp: fellowship_lock_data.timestamp,
        sequence: fellowship_lock_data.sequence,
    }
}

/// The lock table as dereth-protocol's record: [`NUM_BUCKETS`] buckets, the entries held in dictionary
/// order. The retail server sent this table at the end of every full update, empty unless the
/// fellowship was locked (V256). Not ACE's (retail, V279; the retail captures): the record's
/// writer puts the entries in retail's bucket order (the name's string hash mod 32), not ACE's
/// insertion order.
#[must_use]
pub fn locks_record(
    fellowship_locks: &DotNetDict<String, FellowshipLockData>,
) -> dereth_protocol::social::FellowshipLocks {
    dereth_protocol::social::FellowshipLocks(dereth_protocol::archive::PackedHash {
        table_size: u32::from(NUM_BUCKETS),
        entries: fellowship_locks
            .iter()
            .map(|(k, v)| (ace_str(k.as_str()), record(v)))
            .collect(),
    })
}
