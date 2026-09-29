// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/PHashTable.cs
//! Port of `Source/ACE.Server/Network/Structure/PHashTable.cs`.

use empyrean_common::dotnet::CsCast;

use crate::network::game_messages::game_message::BinaryWriter;

// ACE: PHashTable.WriteHeader
/// Deprecated in ACE (nothing calls it). The bucket count is `1 << (GetNumBits(count) - 1)`.
#[allow(clippy::cast_possible_truncation)] // C# `(ushort)` narrowing
pub fn write_header(writer: &mut Vec<u8>, count: u32) {
    let num_bits = get_num_bits(count);
    // `1 << ((int)numBits - 1)`: C# masks an `int` shift count to its low five bits.
    let num_buckets = 1i32.wrapping_shl(CsCast::<i32>::cs_cast(num_bits).wrapping_sub(1) as u32);

    writer.write_u16(count as u16);
    writer.write_u16(num_buckets as u16);
}

// ACE: PHashTable.GetNumBits
/// The number of bits required to store `num`: `(uint)Math.Log(num, 2) + 1`. `Math.Log(a, 2)` is
/// `Log(a) / Log(2)` in .NET, so an exact power of two can land just under its integer; the
/// `(uint)` cast of `Log(0)` (negative infinity) saturates to 0 on net10.
#[must_use]
pub fn get_num_bits(num: u32) -> u32 {
    let log = empyrean_common::math::log(f64::from(num)) / empyrean_common::math::log(2.0);
    CsCast::<u32>::cs_cast(log).wrapping_add(1)
}
