// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Sequence/SequenceType.cs
//! Port of `Source/ACE.Server/Network/Sequence/SequenceType.cs`.

// ACE: SequenceType
/// ACE enum `SequenceType` (underlying `int`); the values are the high half of a
/// `SequenceManager` key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(u32)]
pub enum SequenceType {
    ObjectPosition = 0,
    ObjectMovement = 1,
    ObjectState = 2,
    ObjectVector = 3,
    ObjectTeleport = 4,
    ObjectServerControl = 5,
    ObjectForcePosition = 6,
    ObjectVisualDesc = 7,
    ObjectInstance = 8,

    Motion,

    UpdatePropertyInt,
    UpdatePropertyInt64,
    UpdatePropertyBool,
    UpdatePropertyDouble,
    UpdatePropertyDataID,
    UpdatePropertyInstanceID,
    UpdatePropertyString,
    UpdateRestrictionDB,

    UpdateAttribute,
    UpdateAttribute2ndLevel,
    UpdatePosition,
    UpdateSkill,
}
