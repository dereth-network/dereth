// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/PackableList.cs
//! Port of `Source/ACE.Server/Network/Structure/PackableList.cs`.
//!
//! A `PackableList<uint>`: an `int` count followed by the items.

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};

// ACE: PackableList.ReadListUInt32
/// `reader.ReadListUInt32()`: a `uint` count, then that many `uint`s. `Err` is .NET's
/// `EndOfStreamException`.
pub fn read_list_u_int32(reader: &mut BinaryReader<'_>) -> Result<Vec<u32>, ReadError> {
    let size = reader.read_u32()?;

    let mut list = Vec::new();

    // `for (var i = 0; i < size; i++)`: `i` is an `int` compared with a `uint`, so both widen to
    // `long` and the loop runs `size` times.
    for _ in 0..size {
        list.push(reader.read_u32()?);
    }

    Ok(list)
}

// ACE: PackableList.Write
/// `writer.Write(List<uint> list)`: `list.Count` as an `int`, then each item.
#[allow(clippy::ptr_arg)]
pub fn write(writer: &mut Vec<u8>, list: &[u32]) {
    crate::network::game_messages::game_message::write_record(writer, &[], |w| {
        w.packed_list(list, |w, item| {
            w.u32(*item);
            Ok(())
        })
    });
}

/// `ICollection.Count` as the `int` C# writes. No collection here exceeds `int.MaxValue` items.
#[must_use]
pub fn count(len: usize) -> i32 {
    i32::try_from(len).unwrap_or(i32::MAX)
}
