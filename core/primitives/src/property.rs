//! The type tags of the property system: which kind of value a property holds.
//!
//! The property table in the portal dat stores one of these per property, and both readers of
//! property values -- the dat's UI layouts and master property table, and the gameplay-options
//! collection on the wire -- recover a value's layout from it, because the value itself is written
//! with no tag. The tag vocabulary is shared here; each reader keeps its own value types and codec.

/// The kind of value a property holds, by the number the property table stores for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BasePropertyType {
    Invalid = 0,
    Bool = 1,
    Integer = 2,
    LongInteger = 3,
    Float = 4,
    Vector = 5,
    Color = 6,
    String = 7,
    StringInfo = 8,
    Enum = 9,
    DataFile = 10,
    Waveform = 11,
    InstanceId = 12,
    Position = 13,
    TimeStamp = 14,
    Bitfield32 = 15,
    Bitfield64 = 16,
    Array = 17,
    Struct = 18,
    StringToken = 19,
    PropertyName = 20,
    TriState = 21,
}

impl BasePropertyType {
    /// The tag a stored number names, or `None` for a number past the last tag.
    #[must_use]
    pub const fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            0 => Self::Invalid,
            1 => Self::Bool,
            2 => Self::Integer,
            3 => Self::LongInteger,
            4 => Self::Float,
            5 => Self::Vector,
            6 => Self::Color,
            7 => Self::String,
            8 => Self::StringInfo,
            9 => Self::Enum,
            10 => Self::DataFile,
            11 => Self::Waveform,
            12 => Self::InstanceId,
            13 => Self::Position,
            14 => Self::TimeStamp,
            15 => Self::Bitfield32,
            16 => Self::Bitfield64,
            17 => Self::Array,
            18 => Self::Struct,
            19 => Self::StringToken,
            20 => Self::PropertyName,
            21 => Self::TriState,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::BasePropertyType;

    #[test]
    fn every_tag_number_names_its_own_tag_and_none_past_the_last() {
        for v in 0..=21u32 {
            let t = BasePropertyType::from_u32(v).expect("tags 0..=21 are all named");
            assert_eq!(t as u32, v);
        }
        assert_eq!(BasePropertyType::from_u32(22), None);
        assert_eq!(BasePropertyType::from_u32(u32::MAX), None);
    }
}
