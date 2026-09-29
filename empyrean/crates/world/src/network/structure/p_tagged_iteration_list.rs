// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/PTaggedIterationList.cs
//! Port of `Source/ACE.Server/Network/Structure/PTaggedIterationList.cs`.

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};

use super::c_mostly_consecutive_int_set::{
    read_c_mostly_consecutive_int_set, CMostlyConsecutiveIntSet,
};

// ACE: PTaggedIterationList, PTaggedIterationList.PTaggedIterationList
/// `new PTaggedIterationList()` is `Default`.
/// One dat's iteration list, as the client reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PTaggedIterationList {
    // ACE: PTaggedIterationList.DatFileType
    pub dat_file_type: i32,
    // ACE: PTaggedIterationList.DatFileId
    pub dat_file_id: i32,
    // ACE: PTaggedIterationList.List
    pub list: CMostlyConsecutiveIntSet,
}

impl std::fmt::Display for PTaggedIterationList {
    // ACE: PTaggedIterationList.ToString
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut str = String::new();

        str += &format!("DatFileType: {}\r\n", self.dat_file_type);
        str += &format!("DatFileId:   {}\r\n", self.dat_file_id);
        str += &self.list.to_string();

        f.write_str(&str)
    }
}

// ACE: PTaggedIterationListExtensions.ReadPTaggedIterationList
pub fn read_p_tagged_iteration_list(
    reader: &mut BinaryReader<'_>,
) -> Result<PTaggedIterationList, ReadError> {
    let dat_file_type = reader.read_i32()?;
    let dat_file_id = reader.read_i32()?;
    let list = read_c_mostly_consecutive_int_set(reader)?;
    Ok(PTaggedIterationList {
        dat_file_type,
        dat_file_id,
        list,
    })
}
