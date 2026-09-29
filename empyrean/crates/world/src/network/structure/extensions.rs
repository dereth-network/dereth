// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/Extensions.cs
//! Port of `Source/ACE.Server/Network/Structure/Extensions.cs`.

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_entity::{Quaternion, Vector3};

// ACE: Extensions.ReadVector3
pub fn read_vector3(reader: &mut BinaryReader<'_>) -> Result<Vector3, ReadError> {
    let x = reader.read_f32()?;
    let y = reader.read_f32()?;
    let z = reader.read_f32()?;
    Ok(Vector3::new(x, y, z))
}

// ACE: Extensions.ReadQuaternion
/// Note that AC sends quaternions with the W component first.
pub fn read_quaternion(reader: &mut BinaryReader<'_>) -> Result<Quaternion, ReadError> {
    let w = reader.read_f32()?;
    let x = reader.read_f32()?;
    let y = reader.read_f32()?;
    let z = reader.read_f32()?;
    Ok(Quaternion::new(x, y, z, w))
}
