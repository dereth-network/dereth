// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/CAllIterationList.cs
//! Port of `Source/ACE.Server/Network/Structure/CAllIterationList.cs`.

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};

use super::p_tagged_iteration_list::{read_p_tagged_iteration_list, PTaggedIterationList};

// ACE: CAllIterationList, CAllIterationList.CAllIterationList
/// `new CAllIterationList()` is `Default`.
/// The iteration lists of every dat, sent by the client with the DDD interrogation response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CAllIterationList {
    // ACE: CAllIterationList.Lists
    pub lists: Vec<PTaggedIterationList>,
}

// ACE: CAllIterationListExtensions.ReadCAllIterationList
/// An `int` count, then that many lists (none for a count below 1).
pub fn read_c_all_iteration_list(
    reader: &mut BinaryReader<'_>,
) -> Result<CAllIterationList, ReadError> {
    let mut obj = CAllIterationList::default();
    let num_elements = reader.read_i32()?;
    for _ in 0..num_elements.max(0) {
        obj.lists.push(read_p_tagged_iteration_list(reader)?);
    }
    Ok(obj)
}
