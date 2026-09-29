// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/PackableHashTable.cs
//! Port of `Source/ACE.Server/Network/Structure/PackableHashTable.cs`.
//!
//! The header of a packable hash table (`ushort` count, `ushort` bucket count), and the fill
//! component table of the player description, which ACE writes in bucket order with a stable sort.

use empyrean_store::models::shard::CharacterPropertiesFillCompBook;

use crate::network::game_messages::game_message::BinaryWriter;

// ACE: PackableHashTable.WriteHeader
/// `WriteHeader(writer, int count, int numBuckets)`: both narrowed to `ushort`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // C# `(ushort)` of an int
pub fn write_header(writer: &mut Vec<u8>, count: i32, num_buckets: i32) {
    writer.write_u16(count as u16);
    writer.write_u16(num_buckets as u16);
}

/// The bucket byte of a dereth-protocol `PHash` header whose four bytes are the ones
/// [`write_header`] writes for `num_buckets`: the header's high word is `(ushort)numBuckets`, and a
/// bucket count that is a multiple of 256 below 65,536 makes that word's high byte the `PHash`
/// bucket byte over a 24-bit count (for a count below 65,536).
///
/// # Panics
/// When `num_buckets` is not such a multiple.
#[must_use]
pub fn bucket_byte(num_buckets: i32) -> u8 {
    let buckets = u16::try_from(num_buckets).expect("a ushort bucket count");
    assert!(
        buckets & 0xFF == 0,
        "{num_buckets} buckets have no PHash header of the same bytes"
    );
    buckets.to_be_bytes()[0]
}

// ACE: PackableHashTable.WriteOld
/// `writer.WriteOld(List<CharacterPropertiesFillCompBook> fillComps)`: 256 buckets (a constant
/// from retail pcaps); the entries ordered by `SpellComponentId % 256` with LINQ's stable
/// `OrderBy`, each written as two `int`s.
pub fn write_old(writer: &mut Vec<u8>, fill_comps: &[CharacterPropertiesFillCompBook]) {
    let record = fill_comps_record(fill_comps);
    crate::network::game_messages::game_message::write_record(writer, &[], |w| {
        w.packed_hash(&record, |w, k, v| {
            w.u32(*k);
            w.i32(*v);
            Ok(())
        })
    });
}

/// The fill component table [`write_old`] writes, as dereth-protocol's record.
#[must_use]
pub fn fill_comps_record(
    fill_comps: &[CharacterPropertiesFillCompBook],
) -> dereth_protocol::archive::PackedHash<u32, i32> {
    const NUM_BUCKETS: i32 = 256; // constant from retail pcaps

    let _ = super::packable_list::count(fill_comps.len());

    // `OrderBy` is stable; C#'s `%` keeps the dividend's sign, as Rust's does.
    let mut sorted: Vec<&CharacterPropertiesFillCompBook> = fill_comps.iter().collect();
    sorted.sort_by_key(|i| i.spell_component_id % NUM_BUCKETS);

    dereth_protocol::archive::PackedHash {
        table_size: NUM_BUCKETS.cast_unsigned(),
        entries: sorted
            .into_iter()
            .map(|f| (f.spell_component_id.cast_unsigned(), f.quantity_to_rebuy))
            .collect(),
    }
}
